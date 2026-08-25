//! フォーマット変換 API のパニック耐性を検証する fuzz ターゲット。
//!
//! 任意のバイト列から fourcc / H.264 プロファイル・レベル値 / 解像度を構成し、
//! 公開 API に渡してもパニックしないことを検証する。

#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_v4l2::v4l2_m2m::{H264Level, H264Profile, PixelFormat, Resolution};

fuzz_target!(|data: &[u8]| {
    // 入力が 4 バイト未満の場合は何もしない
    if data.len() < 4 {
        return;
    }

    let fourcc = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    // 破損データでも Result で失敗するだけでパニックしないこと
    let _ = PixelFormat::from_fourcc(fourcc);

    if data.len() < 8 {
        return;
    }
    let profile = i32::from_le_bytes([data[4], data[5], data[6], data[7]]);
    let _ = H264Profile::from_v4l2(profile);

    if data.len() < 12 {
        return;
    }
    let level = i32::from_le_bytes([data[8], data[9], data[10], data[11]]);
    let _ = H264Level::from_v4l2(level);

    // 残り 12 バイトを width / height / stride として扱う
    if data.len() < 24 {
        return;
    }
    let width = u32::from_le_bytes([data[12], data[13], data[14], data[15]]);
    let height = u32::from_le_bytes([data[16], data[17], data[18], data[19]]);
    let stride = u32::from_le_bytes([data[20], data[21], data[22], data[23]]);
    let _ = Resolution { width, height, stride }.yuv420_size();
});
