# v4l2-rs

[![shiguredo_v4l2](https://img.shields.io/crates/v/shiguredo_v4l2.svg)](https://crates.io/crates/shiguredo_v4l2)
[![Documentation](https://docs.rs/shiguredo_v4l2/badge.svg)](https://docs.rs/shiguredo_v4l2)
[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

## About Shiguredo's open source software

We will not respond to PRs or issues that have not been discussed on Discord. Also, Discord is only available in Japanese.

Please read <https://github.com/shiguredo/oss> before use.

## 時雨堂のオープンソースソフトウェアについて

利用前に <https://github.com/shiguredo/oss> をお読みください。

## 概要

Raspberry Pi 向け V4L2 M2M (Memory-to-Memory) デバイスへの Rust バインディングです。
bcm2835-codec を利用した H.264 ハードウェアエンコード/デコードを提供します。

現時点では Raspberry Pi 専用ですが、将来的には汎用的な V4L2 ラッパーライブラリを目指しています。

## 特徴

- Raspberry Pi の bcm2835-codec デバイスを利用したハードウェアエンコード/デコード
- H.264 エンコード (`/dev/video11`)
- H.264 デコード (`/dev/video10`)
- 画像変換 (`/dev/video12`) - スケーリングと I420 ⇔ NV12 相互変換
- I420 (YUV420 planar) / NV12 (YUV420 semi-planar) 入力対応
- DMABUF によるゼロコピー入出力対応 (入力は DMABUF の直接 import、出力は EXPBUF でエクスポートした DMABUF fd を取得可能)
- 依存ライブラリは `libc` のみ

## 対応環境

- Raspberry Pi (bcm2835-codec が利用可能な環境)
- Linux

## 使い方

### H.264 エンコード

```rust
use std::sync::mpsc;
use std::time::Duration;

use shiguredo_v4l2::v4l2_m2m::{EncodeInput, EncoderConfig, FnEncodeHandler, H264Encoder};

// エンコーダーを作成する (1280x720, 2Mbps)
let config = EncoderConfig::new(1280, 720, 2_000_000);
let (tx, rx) = mpsc::channel::<(usize, bool, i64, u64)>();
let mut encoder = H264Encoder::new(config, FnEncodeHandler::new(move |result| match result {
    Ok(frame) => {
        // frame のバッファは frame が Drop されるまで有効
        if let Some(data) = frame.data() {
            let _ = tx.send((
                data.len(),
                frame.is_keyframe(),
                frame.timestamp_us(),
                *frame.user_data(),
            ));
        }
    }
    Err(err) => {
        eprintln!("encode error: {err}");
    }
}))?;

// I420 フレームをエンキューする (初回 encode 呼び出し時に自動で STREAMON される)
let timestamp_us = 0;
let force_keyframe = false;
let user_data = 123_u64;
encoder.encode(
    EncodeInput::Mmap(&mut |buf, _resolution, _user_data| {
        let size = yuv_data.len();
        buf[..size].copy_from_slice(&yuv_data);
        Some(size)
    }),
    timestamp_us,
    force_keyframe,
    user_data,
)?;

let (bytes, keyframe, out_ts, value) = rx.recv_timeout(Duration::from_secs(1))?;
println!("encoded: {bytes} bytes, keyframe: {keyframe}, ts={out_ts}, value={value}");
```

### H.264 デコード

デコーダーは動的解像度 (SOURCE_CHANGE) に対応しています。`H264Decoder::new` 時点では CAPTURE バッファは
未確保のまま `VIDIOC_SUBSCRIBE_EVENT` (SOURCE_CHANGE) を購読し、解像度が確定したタイミングで
`on_resolution_changed` が呼ばれます (購読に失敗したデバイスではイベント購読なしで動作するため、
このコールバックは呼ばれません)。

```rust
use std::sync::mpsc;
use std::time::Duration;

use shiguredo_v4l2::v4l2_m2m::{
    DecodeInput, DecoderConfig, FnDecodeHandler, H264Decoder,
};

// デコーダーを作成する
let config = DecoderConfig::new();
let (tx, rx) = mpsc::channel::<String>();
let mut decoder = H264Decoder::new(
    config,
    FnDecodeHandler::new(
        move |result| match result {
            Ok(frame) => {
                if let Some(data) = frame.data() {
                    let _ = tx.send(format!(
                        "decoded: {} bytes, ts={}, user_data={}",
                        data.len(),
                        frame.timestamp_us(),
                        frame.user_data(),
                    ));
                }
            }
            Err(err) => {
                eprintln!("decode error: {err}");
            }
        },
        |resolution| {
            let _ = tx.send(format!(
                "resolution changed: {}x{}",
                resolution.width, resolution.height
            ));
        },
    ),
)?;

// H.264 データをデコードキューへ投入する
let timestamp_us = 0;
let user_data = 456_u64;
decoder.decode(
    DecodeInput::Mmap(&mut |buf, _user_data| {
        let size = h264_data.len();
        buf[..size].copy_from_slice(&h264_data);
        Some(size)
    }),
    timestamp_us,
    user_data,
)?;

if let Ok(message) = rx.recv_timeout(Duration::from_secs(1)) {
    println!("{message}");
}
```

### DMABUF 入出力

libcamera 等からの DMABUF を直接エンコーダーに渡せます。入力側は `input_memory = Memory::DmaBuf` と設定し、
`EncodeInput::DmaBuf` を渡します。出力側は `output_memory = Memory::DmaBuf` と設定すると、CAPTURE バッファが
EXPBUF でエクスポートされ、`dmabuf_fd()` で DMABUF fd を取得できます (`data()` は `None`)。

```rust
use shiguredo_v4l2::v4l2_m2m::{EncodeInput, EncoderConfig, FnEncodeHandler, H264Encoder, Memory};

let mut config = EncoderConfig::new(1920, 1080, 4_000_000);
config.input_memory = Memory::DmaBuf;

let mut encoder = H264Encoder::new(config, FnEncodeHandler::<u64>::new(|_| {}))?;

encoder.encode(
    EncodeInput::DmaBuf {
        fd: dmabuf_fd,
        bytesused: frame_size as u32,
        length: buffer_size as u32,
    },
    timestamp_us,
    false,
    999_u64,
)?;
```

### エンコーダー設定

```rust
use shiguredo_v4l2::v4l2_m2m::{EncoderConfig, H264Profile, H264Level, Memory, PixelFormat};

let mut config = EncoderConfig::new(1920, 1080, 4_000_000);
config.profile = H264Profile::High;
config.level = H264Level::Level4_2;
config.pixel_format = PixelFormat::Nv12;   // NV12 入力 (デフォルト: Yuv420)
config.i_period = 500;                     // I フレーム間隔
config.repeat_sequence_header = true;      // 各キーフレームに SPS/PPS を付加する
config.output_buffer_count = 4;            // OUTPUT バッファ数
config.capture_buffer_count = 4;           // CAPTURE バッファ数
config.input_memory = Memory::Mmap;        // または Memory::DmaBuf
config.output_memory = Memory::Mmap;       // Memory::DmaBuf にすると DMABUF 出力
```

### 画像変換

I420 入力を NV12 へ変換しつつ解像度を変更します。

```rust
use shiguredo_v4l2::v4l2_m2m::{
    ConvertCallbackOutput, ConvertInput, ConverterConfig, ImageConverter, PixelFormat,
};

// 変換器を作成する (1920x1080 の I420 を 1280x720 の NV12 へ変換)
let mut config = ConverterConfig::new(1920, 1080, 1280, 720);
config.input_pixel_format = PixelFormat::Yuv420;
config.output_pixel_format = PixelFormat::Nv12;

let mut converter = ImageConverter::<u64>::new(config, |result| match result {
    Ok(ConvertCallbackOutput::Frame { frame, value }) => {
        // frame のバッファは frame が Drop されるまで有効
        if let Some(data) = frame.data() {
            println!("converted: {} bytes, user_value={}", data.len(), value);
        }
    }
    Err(err) => {
        eprintln!("convert error: {err}");
    }
})?;

// I420 フレームを変換キューへ投入する
let timestamp_us = 0;
let user_value = 789_u64;
converter.convert(
    ConvertInput::Mmap(&mut |buf, _resolution, _user_value| {
        let size = i420_data.len();
        buf[..size].copy_from_slice(&i420_data);
        Some(size)
    }),
    timestamp_us,
    user_value,
)?;
```

## サンプル

引数のライブラリには [noargs](https://github.com/sile/noargs) を利用しています。

### v4l2_m2m_check

エンコーダー/デコーダーデバイスの存在確認、権限確認、初期化確認を行います。

```bash
cargo run -p v4l2_m2m_check
```

### v4l2_m2m_encode

テストパターン (YUV420) を生成してエンコードし、MP4 ファイルに保存します。
MP4 の Mux には [shiguredo_mp4](https://github.com/shiguredo/mp4-rs) を利用しています。

```bash
cargo run -p v4l2_m2m_encode -- --output output.mp4
```

### v4l2_m2m_libcamera_encode

Raspberry Pi カメラから映像をキャプチャし、H.264 エンコードして MP4 に保存します。
カメラのキャプチャには [shiguredo_libcamera](https://github.com/shiguredo/libcamera-rs) を利用しています。

```bash
cargo run -p v4l2_m2m_libcamera_encode -- --output output.mp4
```

## ライセンス

Apache License 2.0

```text
Copyright 2026-2026, Shiguredo Inc.

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
```
