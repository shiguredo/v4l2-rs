//! H.264 ハードウェアエンコーダー。
//!
//! V4L2 M2M デバイス (`/dev/video11`) を使用して
//! I420 フレームを H.264 にエンコードする。

use std::collections::VecDeque;
use std::os::fd::RawFd;
use std::sync::{Arc, Mutex, MutexGuard};

use crate::buffer::BufferSet;
use crate::device::Device;
use crate::format::{PixelFormat, Resolution};
use crate::poller::{PollEvent, Poller, PollerConfig};
use crate::queue::{CaptureQueue, OutputQueue};
use crate::sys;

/// H.264 プロファイル。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum H264Profile {
    Baseline,
    ConstrainedBaseline,
    Main,
    High,
}

impl H264Profile {
    /// V4L2 定数に変換する。
    pub fn to_v4l2(self) -> i32 {
        match self {
            H264Profile::Baseline => sys::V4L2_MPEG_VIDEO_H264_PROFILE_BASELINE,
            H264Profile::ConstrainedBaseline => {
                sys::V4L2_MPEG_VIDEO_H264_PROFILE_CONSTRAINED_BASELINE
            }
            H264Profile::Main => sys::V4L2_MPEG_VIDEO_H264_PROFILE_MAIN,
            H264Profile::High => sys::V4L2_MPEG_VIDEO_H264_PROFILE_HIGH,
        }
    }

    /// V4L2 定数から変換する。
    pub fn from_v4l2(value: i32) -> Option<Self> {
        match value {
            sys::V4L2_MPEG_VIDEO_H264_PROFILE_BASELINE => Some(H264Profile::Baseline),
            sys::V4L2_MPEG_VIDEO_H264_PROFILE_CONSTRAINED_BASELINE => {
                Some(H264Profile::ConstrainedBaseline)
            }
            sys::V4L2_MPEG_VIDEO_H264_PROFILE_MAIN => Some(H264Profile::Main),
            sys::V4L2_MPEG_VIDEO_H264_PROFILE_HIGH => Some(H264Profile::High),
            _ => None,
        }
    }
}

/// H.264 レベル。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum H264Level {
    Level3_0,
    Level3_1,
    Level3_2,
    Level4_0,
    Level4_1,
    Level4_2,
    Level5_0,
    Level5_1,
}

impl H264Level {
    /// V4L2 定数に変換する。
    pub fn to_v4l2(self) -> i32 {
        match self {
            H264Level::Level3_0 => sys::V4L2_MPEG_VIDEO_H264_LEVEL_3_0,
            H264Level::Level3_1 => sys::V4L2_MPEG_VIDEO_H264_LEVEL_3_1,
            H264Level::Level3_2 => sys::V4L2_MPEG_VIDEO_H264_LEVEL_3_2,
            H264Level::Level4_0 => sys::V4L2_MPEG_VIDEO_H264_LEVEL_4_0,
            H264Level::Level4_1 => sys::V4L2_MPEG_VIDEO_H264_LEVEL_4_1,
            H264Level::Level4_2 => sys::V4L2_MPEG_VIDEO_H264_LEVEL_4_2,
            H264Level::Level5_0 => sys::V4L2_MPEG_VIDEO_H264_LEVEL_5_0,
            H264Level::Level5_1 => sys::V4L2_MPEG_VIDEO_H264_LEVEL_5_1,
        }
    }

    /// V4L2 定数から変換する。
    pub fn from_v4l2(value: i32) -> Option<Self> {
        match value {
            sys::V4L2_MPEG_VIDEO_H264_LEVEL_3_0 => Some(H264Level::Level3_0),
            sys::V4L2_MPEG_VIDEO_H264_LEVEL_3_1 => Some(H264Level::Level3_1),
            sys::V4L2_MPEG_VIDEO_H264_LEVEL_3_2 => Some(H264Level::Level3_2),
            sys::V4L2_MPEG_VIDEO_H264_LEVEL_4_0 => Some(H264Level::Level4_0),
            sys::V4L2_MPEG_VIDEO_H264_LEVEL_4_1 => Some(H264Level::Level4_1),
            sys::V4L2_MPEG_VIDEO_H264_LEVEL_4_2 => Some(H264Level::Level4_2),
            sys::V4L2_MPEG_VIDEO_H264_LEVEL_5_0 => Some(H264Level::Level5_0),
            sys::V4L2_MPEG_VIDEO_H264_LEVEL_5_1 => Some(H264Level::Level5_1),
            _ => None,
        }
    }
}

/// エンコーダーへの入力メモリ方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMemory {
    /// mmap によるコピー入力。
    Mmap,
    /// DMABUF によるゼロコピー入力。
    DmaBuf,
}

/// エンコーダーの設定。
pub struct EncoderConfig {
    /// デバイスパス。デフォルト: "/dev/video11"。
    pub device_path: String,
    /// 入力映像の幅。
    pub width: u32,
    /// 入力映像の高さ。
    pub height: u32,
    /// 入力映像の stride。0 の場合は width と同じ。
    pub stride: u32,
    /// H.264 プロファイル。デフォルト: High。
    pub profile: H264Profile,
    /// H.264 レベル。デフォルト: 4.2。
    pub level: H264Level,
    /// I フレーム間隔 (フレーム数)。デフォルト: 500。
    pub i_period: u32,
    /// SPS/PPS を各キーフレームに付加するか。デフォルト: true。
    pub repeat_sequence_header: bool,
    /// ビットレート (bps)。
    pub bitrate_bps: u32,
    /// OUTPUT バッファ数。デフォルト: 4。
    pub output_buffer_count: u32,
    /// CAPTURE バッファ数。デフォルト: 4。
    pub capture_buffer_count: u32,
    /// 入力メモリ方式。デフォルト: Mmap。
    pub input_memory: InputMemory,
    /// 入力ピクセルフォーマット。デフォルト: Yuv420 (I420)。
    pub pixel_format: PixelFormat,
}

impl EncoderConfig {
    /// デフォルト設定を作成する。解像度とビットレートは必須。
    pub fn new(width: u32, height: u32, bitrate_bps: u32) -> Self {
        EncoderConfig {
            device_path: "/dev/video11".to_string(),
            width,
            height,
            stride: 0,
            profile: H264Profile::High,
            level: H264Level::Level4_2,
            i_period: 500,
            repeat_sequence_header: true,
            bitrate_bps,
            output_buffer_count: 4,
            capture_buffer_count: 4,
            input_memory: InputMemory::Mmap,
            pixel_format: PixelFormat::Yuv420,
        }
    }
}

/// エンコーダーへの入力フレーム。
pub enum InputFrame<'a> {
    /// I420 データのスライス。
    I420(&'a [u8]),
    /// NV12 データのスライス。
    NV12(&'a [u8]),
    /// DMABUF ファイルディスクリプタ。
    DmaBuf {
        fd: RawFd,
        bytesused: u32,
        length: u32,
    },
}

/// エンコードされた H.264 フレーム。
pub struct EncodedFrame<'a> {
    /// H.264 NAL データ。
    pub data: &'a [u8],
    /// キーフレームかどうか。
    pub is_keyframe: bool,
    /// タイムスタンプ (マイクロ秒)。
    pub timestamp_us: i64,
}

/// エンコーダーのコールバック出力。
pub enum EncodeCallbackOutput<'a, T> {
    /// エンコードされたフレーム。
    Frame { frame: EncodedFrame<'a>, value: T },
}

type EncoderCallback<T> =
    dyn for<'a> FnMut(crate::error::Result<EncodeCallbackOutput<'a, T>>) + Send;

#[derive(Debug, Clone, Copy)]
struct ConfiguredOutputFormat {
    width: u32,
    height: u32,
    stride: u32,
}

struct EncoderState<T> {
    output_queue: OutputQueue,
    capture_queue: CaptureQueue,
    resolution: Resolution,
    output_memory: u32,
    started: bool,
    pending_values: VecDeque<T>,
    callback: Box<EncoderCallback<T>>,
}

impl<T> EncoderState<T> {
    fn handle_event(&mut self, event: PollEvent) {
        match event {
            PollEvent::OutputDequeued { index } => {
                self.output_queue.return_buffer(index);
            }
            PollEvent::CaptureDequeued {
                index,
                bytesused,
                flags,
                timestamp,
            } => {
                let value = match self.pending_values.pop_front() {
                    Some(value) => value,
                    None => {
                        (self.callback)(Err(crate::error::Error::NoAvailableBuffer));
                        let _ = self.capture_queue.enqueue(index);
                        return;
                    }
                };

                let data = match self.capture_queue.buffers().mmap_slice(index, 0) {
                    Some(data) => data,
                    None => {
                        (self.callback)(Err(crate::error::Error::NoAvailableBuffer));
                        let _ = self.capture_queue.enqueue(index);
                        return;
                    }
                };

                let bytesused = bytesused as usize;
                if bytesused > data.len() {
                    (self.callback)(Err(crate::error::Error::InputTooLarge {
                        size: bytesused,
                        capacity: data.len(),
                    }));
                    let _ = self.capture_queue.enqueue(index);
                    return;
                }

                let frame = EncodedFrame {
                    data: &data[..bytesused],
                    is_keyframe: flags & sys::V4L2_BUF_FLAG_KEYFRAME != 0,
                    timestamp_us: timestamp.tv_sec * 1_000_000 + timestamp.tv_usec,
                };

                (self.callback)(Ok(EncodeCallbackOutput::Frame { frame, value }));

                if let Err(err) = self.capture_queue.enqueue(index) {
                    (self.callback)(Err(err));
                }
            }
            PollEvent::Error(err) => {
                (self.callback)(Err(err));
            }
            PollEvent::SourceChanged => {
                // エンコーダーでは発生しない。
            }
        }
    }
}

/// H.264 ハードウェアエンコーダー。
///
/// フィールド宣言順序は Drop 順序に影響する。
/// `device` (fd) はキューやポーラーより後に Drop されなければならない。
pub struct H264Encoder<T> {
    poller: Option<Poller>,
    state: Arc<Mutex<EncoderState<T>>>,
    device: Device,
}

impl<T: Send + 'static> H264Encoder<T> {
    /// エンコーダーを初期化する。
    pub fn new<F>(config: EncoderConfig, callback: F) -> crate::error::Result<Self>
    where
        F: for<'a> FnMut(crate::error::Result<EncodeCallbackOutput<'a, T>>) + Send + 'static,
    {
        let device = Device::open(&config.device_path)?;
        let fd = device.raw_fd();

        let stride = if config.stride == 0 {
            config.width
        } else {
            config.stride
        };

        // ピクセルフォーマットのバリデーション
        match config.pixel_format {
            PixelFormat::Yuv420 | PixelFormat::Nv12 => {}
            _ => {
                return Err(crate::error::Error::InvalidFormat {
                    reason: format!("encoder input does not support {:?}", config.pixel_format),
                });
            }
        }

        // H.264 コントロール設定
        Self::set_controls(fd, &config)?;

        // OUTPUT フォーマット設定
        let output_memory = match config.input_memory {
            InputMemory::Mmap => sys::V4L2_MEMORY_MMAP,
            InputMemory::DmaBuf => sys::V4L2_MEMORY_DMABUF,
        };

        let output_format =
            Self::set_output_format(fd, config.width, config.height, stride, config.pixel_format)?;

        // CAPTURE フォーマット設定
        Self::set_capture_format(fd, output_format.width, output_format.height)?;

        // OUTPUT バッファ確保
        let output_buffers = BufferSet::allocate(
            fd,
            sys::V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE,
            output_memory,
            config.output_buffer_count,
            false,
        )?;
        let output_queue = OutputQueue::new(
            fd,
            sys::V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE,
            output_memory,
            output_buffers,
        );

        // CAPTURE バッファ確保
        let capture_buffers = BufferSet::allocate(
            fd,
            sys::V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE,
            sys::V4L2_MEMORY_MMAP,
            config.capture_buffer_count,
            false,
        )?;
        let mut capture_queue = CaptureQueue::new(
            fd,
            sys::V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE,
            sys::V4L2_MEMORY_MMAP,
            capture_buffers,
        );

        // 全 CAPTURE バッファを QBUF
        capture_queue.enqueue_all()?;

        let resolution = Resolution {
            width: output_format.width,
            height: output_format.height,
            stride: output_format.stride,
        };

        let state = EncoderState {
            output_queue,
            capture_queue,
            resolution,
            output_memory,
            started: false,
            pending_values: VecDeque::new(),
            callback: Box::new(callback),
        };

        Ok(H264Encoder {
            poller: None,
            state: Arc::new(Mutex::new(state)),
            device,
        })
    }

    /// フレームをエンキューする。
    pub fn encode(
        &mut self,
        frame: InputFrame<'_>,
        timestamp_us: i64,
        force_keyframe: bool,
        value: T,
    ) -> crate::error::Result<()> {
        let fd = self.device.raw_fd();

        // キーフレーム強制
        if force_keyframe {
            self.force_keyframe()?;
        }

        let mut needs_start = false;
        {
            let mut state = self.lock_state()?;

            let output_index = state
                .output_queue
                .dequeue_available()
                .ok_or(crate::error::Error::NoAvailableBuffer)?;

            let enqueue_result = match frame {
                InputFrame::I420(data) | InputFrame::NV12(data) => {
                    state.output_queue.enqueue(output_index, data, timestamp_us)
                }
                InputFrame::DmaBuf {
                    fd: dmabuf_fd,
                    bytesused,
                    length,
                } => state.output_queue.enqueue_dmabuf(
                    output_index,
                    dmabuf_fd,
                    bytesused,
                    length,
                    timestamp_us,
                ),
            };

            if let Err(err) = enqueue_result {
                state.output_queue.return_buffer(output_index);
                return Err(err);
            }

            state.pending_values.push_back(value);

            if !state.started {
                // 順序: OUTPUT QBUF → OUTPUT STREAMON → CAPTURE STREAMON
                sys::ioctl_streamon(fd, sys::V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE)?;
                sys::ioctl_streamon(fd, sys::V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE)?;
                state.started = true;
                needs_start = true;
            }
        }

        if needs_start {
            self.start_poller();
        }

        Ok(())
    }

    /// ビットレートを変更する。
    pub fn set_bitrate(&mut self, bitrate_bps: u32) -> crate::error::Result<()> {
        let value = i32::try_from(bitrate_bps).map_err(|_| crate::error::Error::InvalidFormat {
            reason: format!("bitrate exceeds i32 maximum: {bitrate_bps}"),
        })?;
        let ctrl = sys::v4l2_control {
            id: sys::V4L2_CID_MPEG_VIDEO_BITRATE,
            value,
        };
        sys::ioctl_s_ctrl(self.device.raw_fd(), &ctrl)
    }

    /// 次のフレームを強制的にキーフレームにする。
    pub fn force_keyframe(&mut self) -> crate::error::Result<()> {
        let ctrl = sys::v4l2_control {
            id: sys::V4L2_CID_MPEG_VIDEO_FORCE_KEY_FRAME,
            value: 1,
        };
        sys::ioctl_s_ctrl(self.device.raw_fd(), &ctrl)
    }

    /// 現在の解像度を取得する。
    pub fn resolution(&self) -> Resolution {
        match self.state.lock() {
            Ok(state) => state.resolution,
            Err(_) => Resolution {
                width: 0,
                height: 0,
                stride: 0,
            },
        }
    }

    fn lock_state(&self) -> crate::error::Result<MutexGuard<'_, EncoderState<T>>> {
        self.state
            .lock()
            .map_err(|_| crate::error::Error::PollerAborted)
    }

    fn start_poller(&mut self) {
        if self.poller.is_some() {
            return;
        }

        let fd = self.device.raw_fd();
        let output_memory = match self.state.lock() {
            Ok(state) => state.output_memory,
            Err(_) => return,
        };

        let state = self.state.clone();
        self.poller = Some(Poller::start(
            PollerConfig {
                fd,
                output_buf_type: sys::V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE,
                output_memory,
                capture_buf_type: sys::V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE,
                capture_memory: sys::V4L2_MEMORY_MMAP,
                subscribe_events: false,
            },
            move |event| {
                if let Ok(mut state) = state.lock() {
                    state.handle_event(event);
                }
            },
        ));
    }

    fn set_controls(fd: RawFd, config: &EncoderConfig) -> crate::error::Result<()> {
        // 各コントロールはデバイスによってサポートされない場合があるため非致命的に設定する

        // プロファイル
        let _ = sys::ioctl_s_ctrl(
            fd,
            &sys::v4l2_control {
                id: sys::V4L2_CID_MPEG_VIDEO_H264_PROFILE,
                value: config.profile.to_v4l2(),
            },
        );

        // レベル
        let _ = sys::ioctl_s_ctrl(
            fd,
            &sys::v4l2_control {
                id: sys::V4L2_CID_MPEG_VIDEO_H264_LEVEL,
                value: config.level.to_v4l2(),
            },
        );

        // I フレーム間隔
        let _ = sys::ioctl_s_ctrl(
            fd,
            &sys::v4l2_control {
                id: sys::V4L2_CID_MPEG_VIDEO_H264_I_PERIOD,
                value: config.i_period as i32,
            },
        );

        // SPS/PPS 繰り返し
        let _ = sys::ioctl_s_ctrl(
            fd,
            &sys::v4l2_control {
                id: sys::V4L2_CID_MPEG_VIDEO_REPEAT_SEQ_HEADER,
                value: if config.repeat_sequence_header { 1 } else { 0 },
            },
        );

        // ビットレート
        if let Ok(value) = i32::try_from(config.bitrate_bps) {
            let _ = sys::ioctl_s_ctrl(
                fd,
                &sys::v4l2_control {
                    id: sys::V4L2_CID_MPEG_VIDEO_BITRATE,
                    value,
                },
            );
        }

        Ok(())
    }

    fn set_output_format(
        fd: RawFd,
        width: u32,
        height: u32,
        stride: u32,
        pixel_format: PixelFormat,
    ) -> crate::error::Result<ConfiguredOutputFormat> {
        let mut fmt = sys::zeroed_format(sys::V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE);
        let pix_mp = unsafe { &mut fmt.fmt.pix_mp };
        pix_mp.width = width;
        pix_mp.height = height;
        pix_mp.pixelformat = pixel_format.to_fourcc();
        pix_mp.field = sys::V4L2_FIELD_ANY;
        pix_mp.colorspace = sys::V4L2_COLORSPACE_DEFAULT;
        pix_mp.num_planes = 1;
        pix_mp.plane_fmt[0].bytesperline = stride;
        pix_mp.plane_fmt[0].sizeimage = Resolution {
            width,
            height,
            stride,
        }
        .yuv420_size() as u32;

        sys::ioctl_s_fmt(fd, &mut fmt)?;

        let pix_mp = unsafe { &fmt.fmt.pix_mp };
        Ok(ConfiguredOutputFormat {
            width: pix_mp.width,
            height: pix_mp.height,
            stride: pix_mp.plane_fmt[0].bytesperline,
        })
    }

    fn set_capture_format(fd: RawFd, width: u32, height: u32) -> crate::error::Result<()> {
        let mut fmt = sys::zeroed_format(sys::V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE);
        let pix_mp = unsafe { &mut fmt.fmt.pix_mp };
        pix_mp.width = width;
        pix_mp.height = height;
        pix_mp.pixelformat = sys::V4L2_PIX_FMT_H264;
        pix_mp.num_planes = 1;
        pix_mp.plane_fmt[0].sizeimage = 512 * 1024; // 512KB

        sys::ioctl_s_fmt(fd, &mut fmt)
    }
}

impl<T> Drop for H264Encoder<T> {
    fn drop(&mut self) {
        // Poller を先に停止
        if let Some(ref mut poller) = self.poller {
            poller.stop();
        }
        self.poller = None;

        let fd = self.device.raw_fd();
        if let Ok(mut state) = self.state.lock()
            && state.started
        {
            let _ = sys::ioctl_streamoff(fd, sys::V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE);
            let _ = sys::ioctl_streamoff(fd, sys::V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE);
            state.started = false;
        }
    }
}
