//! H.264 ハードウェアデコーダー。
//!
//! V4L2 M2M デバイス (`/dev/video10`) を使用して
//! H.264 データを I420 フレームにデコードする。

use std::collections::VecDeque;
use std::os::fd::RawFd;
use std::sync::{Arc, Mutex, MutexGuard};

use crate::buffer::BufferSet;
use crate::device::Device;
use crate::format::Resolution;
use crate::poller::{PollEvent, Poller, PollerConfig, Timestamp};
use crate::queue::{CaptureQueue, OutputQueue};
use crate::sys;

/// デコーダーの設定。
pub struct DecoderConfig {
    /// デバイスパス。デフォルト: "/dev/video10"。
    pub device_path: String,
    /// CAPTURE バッファを DMABUF としてエクスポートするか。デフォルト: false。
    pub export_dmabuf: bool,
    /// OUTPUT バッファ数。デフォルト: 4。
    pub output_buffer_count: u32,
    /// CAPTURE バッファ数。デフォルト: 4。
    pub capture_buffer_count: u32,
}

impl DecoderConfig {
    /// デフォルト設定を作成する。
    pub fn new() -> Self {
        DecoderConfig {
            device_path: "/dev/video10".to_string(),
            export_dmabuf: false,
            output_buffer_count: 4,
            capture_buffer_count: 4,
        }
    }
}

impl Default for DecoderConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// デコードされた YUV420 フレーム。
pub struct DecodedFrame<'a> {
    /// YUV420 データ。DMABUF の場合は空スライス。
    pub data: &'a [u8],
    /// 内部バッファインデックス (参照用途)。
    pub index: u32,
    /// DMABUF ファイルディスクリプタ。Mmap の場合は None。
    pub dmabuf_fd: Option<RawFd>,
    /// タイムスタンプ (マイクロ秒)。
    pub timestamp_us: i64,
}

/// デコーダーのコールバック出力。
pub enum DecodeCallbackOutput<'a, T> {
    /// デコードされたフレーム。
    Frame { frame: DecodedFrame<'a>, value: T },
    /// 解像度が変更された。
    ResolutionChanged { resolution: Resolution },
}

type DecoderCallback<T> =
    dyn for<'a> FnMut(crate::error::Result<DecodeCallbackOutput<'a, T>>) + Send;

struct DecoderState<T> {
    fd: RawFd,
    output_queue: OutputQueue,
    capture_queue: Option<CaptureQueue>,
    resolution: Option<Resolution>,
    export_dmabuf: bool,
    capture_buffer_count: u32,
    started: bool,
    pending_values: VecDeque<T>,
    callback: Box<DecoderCallback<T>>,
}

impl<T> DecoderState<T> {
    fn handle_event(&mut self, event: PollEvent) {
        match event {
            PollEvent::OutputDequeued { index } => {
                self.output_queue.return_buffer(index);
            }
            PollEvent::SourceChanged => {
                if let Err(err) = self.handle_source_change() {
                    (self.callback)(Err(err));
                }
            }
            PollEvent::CaptureDequeued {
                index,
                bytesused,
                flags: _,
                timestamp,
            } => {
                self.handle_capture(index, bytesused, timestamp);
            }
            PollEvent::Error(err) => {
                (self.callback)(Err(err));
            }
        }
    }

    fn handle_capture(&mut self, index: u32, bytesused: u32, timestamp: Timestamp) {
        let value = match self.pending_values.pop_front() {
            Some(value) => value,
            None => {
                (self.callback)(Err(crate::error::Error::NoAvailableBuffer));
                self.requeue_capture(index);
                return;
            }
        };

        let capture_queue = match self.capture_queue.as_ref() {
            Some(capture_queue) => capture_queue,
            None => {
                (self.callback)(Err(crate::error::Error::NotStarted));
                return;
            }
        };

        let timestamp_us = timestamp.tv_sec * 1_000_000 + timestamp.tv_usec;

        let data = match capture_queue.buffers().mmap_slice(index, 0) {
            Some(slice) => {
                let bytesused = bytesused as usize;
                if bytesused > slice.len() {
                    (self.callback)(Err(crate::error::Error::InputTooLarge {
                        size: bytesused,
                        capacity: slice.len(),
                    }));
                    self.requeue_capture(index);
                    return;
                }
                &slice[..bytesused]
            }
            None => &[],
        };

        let dmabuf_fd = capture_queue.buffers().dmabuf_fd(index, 0);

        let frame = DecodedFrame {
            data,
            index,
            dmabuf_fd,
            timestamp_us,
        };
        (self.callback)(Ok(DecodeCallbackOutput::Frame { frame, value }));

        self.requeue_capture(index);
    }

    fn requeue_capture(&mut self, index: u32) {
        if let Some(capture_queue) = self.capture_queue.as_mut()
            && let Err(err) = capture_queue.enqueue(index)
        {
            (self.callback)(Err(err));
        }
    }

    fn handle_source_change(&mut self) -> crate::error::Result<()> {
        // CAPTURE ストリームを停止 (開始済みの場合)
        if self.started {
            let _ = sys::ioctl_streamoff(self.fd, sys::V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE);
        }

        // 古い CAPTURE バッファを解放
        self.capture_queue = None;

        // G_FMT で新しい解像度を取得
        let mut fmt = sys::zeroed_format(sys::V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE);
        sys::ioctl_g_fmt(self.fd, &mut fmt)?;

        let (width, height, stride) = unsafe {
            (
                fmt.fmt.pix_mp.width,
                fmt.fmt.pix_mp.height,
                fmt.fmt.pix_mp.plane_fmt[0].bytesperline,
            )
        };

        self.resolution = Some(Resolution {
            width,
            height,
            stride,
        });

        // 新しい CAPTURE バッファを確保
        let capture_buffers = BufferSet::allocate(
            self.fd,
            sys::V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE,
            sys::V4L2_MEMORY_MMAP,
            self.capture_buffer_count,
            self.export_dmabuf,
        )?;
        let mut capture_queue = CaptureQueue::new(
            self.fd,
            sys::V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE,
            sys::V4L2_MEMORY_MMAP,
            capture_buffers,
        );

        // 全 CAPTURE バッファを QBUF
        capture_queue.enqueue_all()?;
        self.capture_queue = Some(capture_queue);

        // CAPTURE STREAMON
        sys::ioctl_streamon(self.fd, sys::V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE)?;
        self.started = true;

        (self.callback)(Ok(DecodeCallbackOutput::ResolutionChanged {
            resolution: Resolution { width, height, stride },
        }));
        Ok(())
    }
}

/// H.264 ハードウェアデコーダー。
///
/// フィールド宣言順序は Drop 順序に影響する。
/// `device` (fd) はキューやポーラーより後に Drop されなければならない。
pub struct H264Decoder<T> {
    poller: Option<Poller>,
    state: Arc<Mutex<DecoderState<T>>>,
    device: Device,
}

impl<T: Send + 'static> H264Decoder<T> {
    /// デコーダーを初期化する。
    pub fn new<F>(config: DecoderConfig, callback: F) -> crate::error::Result<Self>
    where
        F: for<'a> FnMut(crate::error::Result<DecodeCallbackOutput<'a, T>>) + Send + 'static,
    {
        let device = Device::open(&config.device_path)?;
        let fd = device.raw_fd();

        // OUTPUT フォーマット設定 (H.264 入力)
        Self::set_output_format(fd)?;

        // OUTPUT バッファ確保
        let output_buffers = BufferSet::allocate(
            fd,
            sys::V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE,
            sys::V4L2_MEMORY_MMAP,
            config.output_buffer_count,
            false,
        )?;
        let output_queue = OutputQueue::new(
            fd,
            sys::V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE,
            sys::V4L2_MEMORY_MMAP,
            output_buffers,
        );

        // OUTPUT STREAMON
        sys::ioctl_streamon(fd, sys::V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE)?;

        // SOURCE_CHANGE イベントを購読 (非対応のデバイスでは無視)
        let sub = sys::v4l2_event_subscription {
            r#type: sys::V4L2_EVENT_SOURCE_CHANGE,
            id: 0,
            flags: 0,
            reserved: [0; 5],
        };
        let subscribe_events = sys::ioctl_subscribe_event(fd, &sub).is_ok();

        let state = Arc::new(Mutex::new(DecoderState {
            fd,
            output_queue,
            capture_queue: None,
            resolution: None,
            export_dmabuf: config.export_dmabuf,
            capture_buffer_count: config.capture_buffer_count,
            started: false,
            pending_values: VecDeque::new(),
            callback: Box::new(callback),
        }));

        let poller_state = state.clone();
        let poller = Poller::start(
            PollerConfig {
                fd,
                output_buf_type: sys::V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE,
                output_memory: sys::V4L2_MEMORY_MMAP,
                capture_buf_type: sys::V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE,
                capture_memory: sys::V4L2_MEMORY_MMAP,
                subscribe_events,
            },
            move |event| {
                if let Ok(mut state) = poller_state.lock() {
                    state.handle_event(event);
                }
            },
        );

        Ok(H264Decoder {
            poller: Some(poller),
            state,
            device,
        })
    }

    /// H.264 データをデコードキューへ投入する。
    pub fn decode(&mut self, data: &[u8], timestamp_us: i64, value: T) -> crate::error::Result<()> {
        let mut state = self.lock_state()?;

        let output_index = state
            .output_queue
            .dequeue_available()
            .ok_or(crate::error::Error::NoAvailableBuffer)?;

        if let Err(err) = state.output_queue.enqueue(output_index, data, timestamp_us) {
            state.output_queue.return_buffer(output_index);
            return Err(err);
        }

        state.pending_values.push_back(value);
        Ok(())
    }

    /// 現在の解像度を取得する。
    pub fn resolution(&self) -> Option<Resolution> {
        match self.state.lock() {
            Ok(state) => state.resolution,
            Err(_) => None,
        }
    }

    fn lock_state(&self) -> crate::error::Result<MutexGuard<'_, DecoderState<T>>> {
        self.state
            .lock()
            .map_err(|_| crate::error::Error::PollerAborted)
    }

    fn set_output_format(fd: RawFd) -> crate::error::Result<()> {
        let mut fmt = sys::zeroed_format(sys::V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE);
        let pix_mp = unsafe { &mut fmt.fmt.pix_mp };
        pix_mp.pixelformat = sys::V4L2_PIX_FMT_H264;
        pix_mp.num_planes = 1;
        pix_mp.plane_fmt[0].sizeimage = 512 * 1024; // 512KB

        sys::ioctl_s_fmt(fd, &mut fmt)
    }
}

impl<T> Drop for H264Decoder<T> {
    fn drop(&mut self) {
        // Poller を先に停止
        if let Some(ref mut poller) = self.poller {
            poller.stop();
        }
        self.poller = None;

        let fd = self.device.raw_fd();

        if let Ok(mut state) = self.state.lock() {
            // CAPTURE キューを解放
            if state.started {
                let _ = sys::ioctl_streamoff(fd, sys::V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE);
            }
            state.capture_queue = None;
            state.started = false;
        }

        let _ = sys::ioctl_streamoff(fd, sys::V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE);
    }
}
