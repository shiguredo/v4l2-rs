//! V4L2 ポーリングスレッド。
//!
//! C++ の `V4L2Runner` に相当する。
//! `poll()` でイベントを監視し、`mpsc::channel` で通知する。

use std::os::fd::RawFd;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};

use crate::sys;

/// タイムスタンプ情報。
#[derive(Debug, Clone, Copy)]
pub(crate) struct Timestamp {
    pub tv_sec: i64,
    pub tv_usec: i64,
}

/// ポーリングスレッドからのイベント。
pub(crate) enum PollEvent {
    /// OUTPUT バッファがデキューされた。
    OutputDequeued { index: u32 },
    /// CAPTURE バッファがデキューされた。
    CaptureDequeued {
        index: u32,
        bytesused: u32,
        flags: u32,
        timestamp: Timestamp,
    },
    /// ソース変更イベント (解像度変更)。
    SourceChanged,
    /// エラーが発生した。
    Error(crate::error::Error),
}

/// ポーリングスレッドの設定。
pub(crate) struct PollerConfig {
    pub fd: RawFd,
    pub output_buf_type: u32,
    pub output_memory: u32,
    pub capture_buf_type: u32,
    pub capture_memory: u32,
    pub subscribe_events: bool,
}

/// ポーリングスレッド。
pub(crate) struct Poller {
    thread: Option<JoinHandle<()>>,
    event_rx: mpsc::Receiver<PollEvent>,
    abort: Arc<AtomicBool>,
}

impl Poller {
    /// ポーリングスレッドを起動する。
    pub fn start(config: PollerConfig) -> Self {
        let (event_tx, event_rx) = mpsc::channel();
        let abort = Arc::new(AtomicBool::new(false));
        let abort_clone = abort.clone();

        let thread = thread::spawn(move || {
            Self::poll_loop(config, event_tx, abort_clone);
        });

        Poller {
            thread: Some(thread),
            event_rx,
            abort,
        }
    }

    /// イベントを受信する (タイムアウト付き)。
    pub fn recv_timeout(&self, timeout: std::time::Duration) -> Option<PollEvent> {
        self.event_rx.recv_timeout(timeout).ok()
    }

    /// ポーリングスレッドを停止する。
    pub fn stop(&mut self) {
        self.abort.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }

    fn poll_loop(config: PollerConfig, event_tx: mpsc::Sender<PollEvent>, abort: Arc<AtomicBool>) {
        let mut poll_events = libc::POLLIN;
        if config.subscribe_events {
            poll_events |= libc::POLLPRI;
        }

        loop {
            if abort.load(Ordering::Acquire) {
                return;
            }

            let mut pollfd = libc::pollfd {
                fd: config.fd,
                events: poll_events,
                revents: 0,
            };

            let ret = unsafe { libc::poll(&mut pollfd, 1, 500) };

            if abort.load(Ordering::Acquire) {
                return;
            }

            if ret < 0 {
                let err = std::io::Error::last_os_error();
                if err.kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                let _ = event_tx.send(PollEvent::Error(crate::error::Error::Poll { source: err }));
                return;
            }

            if ret == 0 {
                // タイムアウト
                continue;
            }

            // イベント処理 (POLLPRI)
            if pollfd.revents & libc::POLLPRI != 0 {
                let mut event: sys::v4l2_event = unsafe { std::mem::zeroed() };
                if sys::ioctl_dqevent(config.fd, &mut event).is_ok()
                    && event.r#type == sys::V4L2_EVENT_SOURCE_CHANGE
                {
                    // event.u の先頭 4 バイトが v4l2_event_src_change.changes
                    let changes = u32::from_ne_bytes([
                        event.u.data[0],
                        event.u.data[1],
                        event.u.data[2],
                        event.u.data[3],
                    ]);
                    if changes & sys::V4L2_EVENT_SRC_CH_RESOLUTION != 0
                        && event_tx.send(PollEvent::SourceChanged).is_err()
                    {
                        return;
                    }
                }
            }

            // データ処理 (POLLIN)
            if pollfd.revents & libc::POLLIN != 0 {
                // OUTPUT バッファのデキュー
                Self::try_dequeue_output(&config, &event_tx, &abort);

                // CAPTURE バッファのデキュー
                Self::try_dequeue_capture(&config, &event_tx, &abort);
            }
        }
    }

    fn try_dequeue_output(
        config: &PollerConfig,
        event_tx: &mpsc::Sender<PollEvent>,
        abort: &AtomicBool,
    ) {
        if abort.load(Ordering::Acquire) {
            return;
        }

        let mut plane = sys::v4l2_plane {
            bytesused: 0,
            length: 0,
            m: sys::v4l2_plane_m { mem_offset: 0 },
            data_offset: 0,
            reserved: [0; 11],
        };

        let mut buf = sys::zeroed_buffer(config.output_buf_type, config.output_memory);
        buf.length = 1;
        buf.m = sys::v4l2_buffer_m {
            planes: &mut plane as *mut _,
        };

        if sys::ioctl_dqbuf(config.fd, &mut buf).is_ok() {
            let _ = event_tx.send(PollEvent::OutputDequeued { index: buf.index });
        }
    }

    fn try_dequeue_capture(
        config: &PollerConfig,
        event_tx: &mpsc::Sender<PollEvent>,
        abort: &AtomicBool,
    ) {
        if abort.load(Ordering::Acquire) {
            return;
        }

        let mut plane = sys::v4l2_plane {
            bytesused: 0,
            length: 0,
            m: sys::v4l2_plane_m { mem_offset: 0 },
            data_offset: 0,
            reserved: [0; 11],
        };

        let mut buf = sys::zeroed_buffer(config.capture_buf_type, config.capture_memory);
        buf.length = 1;
        buf.m = sys::v4l2_buffer_m {
            planes: &mut plane as *mut _,
        };

        if sys::ioctl_dqbuf(config.fd, &mut buf).is_ok() {
            let _ = event_tx.send(PollEvent::CaptureDequeued {
                index: buf.index,
                bytesused: plane.bytesused,
                flags: buf.flags,
                timestamp: Timestamp {
                    tv_sec: buf.timestamp.tv_sec,
                    tv_usec: buf.timestamp.tv_usec,
                },
            });
        }
    }
}

impl Drop for Poller {
    fn drop(&mut self) {
        self.stop();
    }
}
