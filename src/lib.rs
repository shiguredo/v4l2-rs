//! V4L2 bindings。
//!
//! Raspberry Pi 向け V4L2 M2M (Memory-to-Memory) デバイスへのバインディング。

mod buffer;
mod device;
mod poller;
mod queue;

mod converter;
mod decoder;
mod encoder;
mod error;
mod format;
pub(crate) mod sys;

/// V4L2 M2M (Memory-to-Memory) を使った H.264 エンコード/デコード。
///
/// Raspberry Pi の `/dev/video11` (エンコーダー) と `/dev/video10` (デコーダー) を
/// 操作するための汎用的な V4L2 M2M ラッパー。WebRTC には依存しない。
pub mod v4l2_m2m {
    pub use crate::converter::{
        ConvertInput, ConvertOutput, ConverterConfig, ConverterMemory, ImageConverter,
    };
    pub use crate::decoder::{DecodeCallbackOutput, DecodedFrame, DecoderConfig, H264Decoder};
    pub use crate::encoder::{
        EncodeCallbackOutput, EncodedFrame, EncoderConfig, H264Encoder, H264Level, H264Profile,
        InputFrame, InputMemory,
    };
    pub use crate::error::{Error, Result};
    pub use crate::format::{PixelFormat, Resolution};
}
