//! H.264 ハードウェアデコーダー。
//!
//! V4L2 M2M デバイス (`/dev/video10`) を使用して
//! H.264 データを I420 フレームにデコードする。

use std::time::Duration;

use crate::buffer::BufferSet;
use crate::device::Device;
use crate::format::Resolution;
use crate::poller::{PollEvent, Poller, PollerConfig};
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

/// デコード結果。
pub enum DecodeOutput<'a> {
    /// デコードされたフレーム。
    Frame(DecodedFrame<'a>),
    /// 解像度が変更された。
    ResolutionChanged { width: u32, height: u32 },
    /// データがバッファリング中で、まだフレームが出力されていない。
    Pending,
}

/// デコードされた YUV420 フレーム。
pub struct DecodedFrame<'a> {
    /// YUV420 データ。DMABUF の場合は空スライス。
    pub data: &'a [u8],
    /// バッファインデックス (release_buffer() で使用)。
    pub index: u32,
    /// DMABUF ファイルディスクリプタ。Mmap の場合は None。
    pub dmabuf_fd: Option<std::os::fd::RawFd>,
    /// タイムスタンプ (マイクロ秒)。
    pub timestamp_us: i64,
}

/// H.264 ハードウェアデコーダー。
///
/// フィールド宣言順序は Drop 順序に影響する。
/// `device` (fd) はキューやポーラーより後に Drop されなければならない。
pub struct H264Decoder {
    poller: Option<Poller>,
    output_queue: OutputQueue,
    capture_queue: Option<CaptureQueue>,
    resolution: Option<Resolution>,
    export_dmabuf: bool,
    capture_buffer_count: u32,
    started: bool,
    device: Device,
}

impl H264Decoder {
    /// デコーダーを初期化する。
    pub fn new(config: DecoderConfig) -> crate::error::Result<Self> {
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

        // Poller を起動
        let poller = Poller::start(PollerConfig {
            fd,
            output_buf_type: sys::V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE,
            output_memory: sys::V4L2_MEMORY_MMAP,
            capture_buf_type: sys::V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE,
            capture_memory: sys::V4L2_MEMORY_MMAP,
            subscribe_events,
        });

        Ok(H264Decoder {
            poller: Some(poller),
            output_queue,
            capture_queue: None,
            resolution: None,
            export_dmabuf: config.export_dmabuf,
            capture_buffer_count: config.capture_buffer_count,
            started: false,
            device,
        })
    }

    /// H.264 データをデコードする。
    pub fn decode(
        &mut self,
        data: &[u8],
        timestamp_us: i64,
    ) -> crate::error::Result<DecodeOutput<'_>> {
        // 利用可能な OUTPUT バッファを取得
        let output_index = self
            .output_queue
            .dequeue_available()
            .ok_or(crate::error::Error::NoAvailableBuffer)?;

        // H.264 データを OUTPUT バッファに投入
        self.output_queue
            .enqueue(output_index, data, timestamp_us)?;

        // Poller からのイベントを待機
        let poller = self
            .poller
            .as_ref()
            .ok_or(crate::error::Error::NotStarted)?;

        match poller.recv_timeout(Duration::from_secs(5)) {
            Some(event) => match event {
                PollEvent::OutputDequeued { index } => {
                    self.output_queue.return_buffer(index);
                    // 追加のイベントを確認
                    self.try_recv_more()
                }
                PollEvent::SourceChanged => self.handle_source_change(),
                PollEvent::CaptureDequeued {
                    index,
                    bytesused,
                    flags: _,
                    timestamp,
                } => self.make_decode_output(index, bytesused, &timestamp),
                PollEvent::Error(err) => Err(err),
            },
            None => Ok(DecodeOutput::Pending),
        }
    }

    /// バッファを解放する (CAPTURE バッファを再投入)。
    pub fn release_buffer(&mut self, index: u32) -> crate::error::Result<()> {
        if let Some(ref mut capture_queue) = self.capture_queue {
            capture_queue.enqueue(index)?;
        }
        Ok(())
    }

    /// 現在の解像度を取得する。
    pub fn resolution(&self) -> Option<Resolution> {
        self.resolution
    }

    fn try_recv_more(&mut self) -> crate::error::Result<DecodeOutput<'_>> {
        let poller = self
            .poller
            .as_ref()
            .ok_or(crate::error::Error::NotStarted)?;

        match poller.recv_timeout(Duration::from_millis(100)) {
            Some(event) => match event {
                PollEvent::OutputDequeued { index } => {
                    self.output_queue.return_buffer(index);
                    Ok(DecodeOutput::Pending)
                }
                PollEvent::SourceChanged => self.handle_source_change(),
                PollEvent::CaptureDequeued {
                    index,
                    bytesused,
                    flags: _,
                    timestamp,
                } => self.make_decode_output(index, bytesused, &timestamp),
                PollEvent::Error(err) => Err(err),
            },
            None => Ok(DecodeOutput::Pending),
        }
    }

    fn handle_source_change(&mut self) -> crate::error::Result<DecodeOutput<'_>> {
        let fd = self.device.raw_fd();

        // CAPTURE ストリームを停止 (開始済みの場合)
        if self.started {
            let _ = sys::ioctl_streamoff(fd, sys::V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE);
        }

        // 古い CAPTURE バッファを解放
        self.capture_queue = None;

        // G_FMT で新しい解像度を取得
        let mut fmt = sys::zeroed_format(sys::V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE);
        sys::ioctl_g_fmt(fd, &mut fmt)?;

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
            fd,
            sys::V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE,
            sys::V4L2_MEMORY_MMAP,
            self.capture_buffer_count,
            self.export_dmabuf,
        )?;
        let mut capture_queue = CaptureQueue::new(
            fd,
            sys::V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE,
            sys::V4L2_MEMORY_MMAP,
            capture_buffers,
        );

        // 全 CAPTURE バッファを QBUF
        capture_queue.enqueue_all()?;
        self.capture_queue = Some(capture_queue);

        // CAPTURE STREAMON
        sys::ioctl_streamon(fd, sys::V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE)?;
        self.started = true;

        Ok(DecodeOutput::ResolutionChanged { width, height })
    }

    fn make_decode_output(
        &self,
        index: u32,
        bytesused: u32,
        timestamp: &crate::poller::Timestamp,
    ) -> crate::error::Result<DecodeOutput<'_>> {
        let capture_queue = self
            .capture_queue
            .as_ref()
            .ok_or(crate::error::Error::NotStarted)?;

        let timestamp_us = timestamp.tv_sec * 1_000_000 + timestamp.tv_usec;

        let data = capture_queue
            .buffers()
            .mmap_slice(index, 0)
            .map(|slice| &slice[..bytesused as usize])
            .unwrap_or(&[]);

        let dmabuf_fd = capture_queue.buffers().dmabuf_fd(index, 0);

        Ok(DecodeOutput::Frame(DecodedFrame {
            data,
            index,
            dmabuf_fd,
            timestamp_us,
        }))
    }

    fn set_output_format(fd: std::os::fd::RawFd) -> crate::error::Result<()> {
        let mut fmt = sys::zeroed_format(sys::V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE);
        let pix_mp = unsafe { &mut fmt.fmt.pix_mp };
        pix_mp.pixelformat = sys::V4L2_PIX_FMT_H264;
        pix_mp.num_planes = 1;
        pix_mp.plane_fmt[0].sizeimage = 512 * 1024; // 512KB

        sys::ioctl_s_fmt(fd, &mut fmt)
    }
}

impl Drop for H264Decoder {
    fn drop(&mut self) {
        // Poller を先に停止
        if let Some(ref mut poller) = self.poller {
            poller.stop();
        }
        self.poller = None;

        let fd = self.device.raw_fd();

        // CAPTURE キューを解放
        if self.started {
            let _ = sys::ioctl_streamoff(fd, sys::V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE);
        }
        self.capture_queue = None;

        let _ = sys::ioctl_streamoff(fd, sys::V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE);
    }
}
