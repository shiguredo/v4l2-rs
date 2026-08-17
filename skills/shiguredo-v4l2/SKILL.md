---
name: shiguredo-v4l2
description: 時雨堂の Raspberry Pi 向け V4L2 M2M バインディング shiguredo_v4l2 の機能・API リファレンス。H.264 ハードウェアエンコード/デコード、画像変換、DMABUF 入出力、ピクセルフォーマットに関する質問時に使用。
---

# shiguredo_v4l2

Raspberry Pi 向け V4L2 M2M (Memory-to-Memory) デバイスへの Rust バインディング。
bcm2835-codec を利用した H.264 ハードウェアエンコード/デコードと、フォーマット変換 (スケーリング / I420 ⇔ NV12) を提供する。

## 特徴

- **Raspberry Pi 専用**: bcm2835-codec デバイス (`/dev/video10` / `/dev/video11` / `/dev/video12`) を利用したハードウェアエンコード/デコード/変換
- **依存最小**: 外部依存は `libc` のみ
- **DMABUF ゼロコピー**: libcamera などからの DMABUF を入力としてエンコードできる
- **コールバック型 API**: ハンドラー (またはクロージャ) を渡すと、完了は内部ポーラースレッドから通知される
- **自動再キュー**: `EncodedFrame` / `DecodedFrame` / `ConvertedFrame` を `Drop` すると CAPTURE バッファが自動再キューされる
- **Drop 順序保護**: `H264Encoder` / `H264Decoder` / `ImageConverter` はフィールド宣言順で Drop され、Poller 停止 → STREAMOFF → デバイス close の順序を保証する

将来的には汎用 V4L2 ラッパーを目指すが、現時点では Raspberry Pi 専用。

## バージョン情報

- crate 名: `shiguredo_v4l2`
- バージョン: 2026.1.0
- Rust Edition: 2024
- 最小 Rust バージョン: 1.88
- ライセンス: Apache-2.0
- 依存: `libc = "0.2"`

公開 API は `shiguredo_v4l2::v4l2_m2m` モジュールから再公開される。`v4l2_m2m` は WebRTC や上位プロトコルには依存しない汎用 V4L2 M2M ラッパー。

## デバイスマッピング

| デバイス | 役割 | デフォルト設定 |
|----------|------|----------------|
| `/dev/video10` | H.264 デコーダー (`H264Decoder`) | `DecoderConfig::device_path` |
| `/dev/video11` | H.264 エンコーダー (`H264Encoder`) | `EncoderConfig::device_path` |
| `/dev/video12` | 画像変換器 (`ImageConverter`) | `ConverterConfig::device_path` |

すべて V4L2 M2M MPLANE (`V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE` / `V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE`) で動作する。

## コア API

### 共通型

| 型 | 説明 | 主要 API |
|----|------|----------|
| `Memory` | バッファのメモリ方式 | `Mmap` (カーネル mmap によるコピー入出力), `DmaBuf` (DMABUF によるゼロコピー入出力) |
| `PixelFormat` | ピクセルフォーマット | `Yuv420` (I420), `Nv12`, `H264`, `to_fourcc()`, `from_fourcc()` |
| `Resolution` | 映像解像度 (フィールド: `width`, `height`, `stride`) | `yuv420_size()` (Y plane と chroma plane 2 面の合計バイト数を算出) |
| `Error` | V4L2 操作で発生するエラー | `DeviceOpen` / `Ioctl` / `Mmap` / `Poll` / `InvalidFormat` / `NoAvailableBuffer` / `NotStarted` / `StreamOn` / `StreamOff` / `InputTooLarge` / `MmapInputNotProduced` / `PollerAborted` |
| `Result<T>` | `std::result::Result<T, Error>` のエイリアス | — |

エンコーダーは `PixelFormat::Yuv420` (I420) と `PixelFormat::Nv12` のみ入力可能。`PixelFormat::H264` を入力に指定すると `Error::InvalidFormat` を返す。

### エンコーダー (`H264Encoder<H>`)

| 型 | 説明 | 主要メソッド |
|----|------|-------------|
| `EncoderConfig` | エンコーダー設定 | `new(width, height, bitrate_bps)` (デフォルト値を構築) |
| `H264Profile` | H.264 プロファイル | `Baseline`, `ConstrainedBaseline`, `Main`, `High`, `to_v4l2()`, `from_v4l2()` |
| `H264Level` | H.264 レベル | `Level3_0` ～ `Level5_1`, `to_v4l2()`, `from_v4l2()` |
| `EncodeInput<'a, T>` | エンコード入力 | `Mmap(&mut FnMut(&mut [u8], &Resolution, &T) -> Option<usize>)`, `DmaBuf { fd, bytesused, length }` |
| `EncodedFrame<T>` | エンコード結果のハンドル (`Drop` で自動再キュー) | `data()` (MMAP 出力時のみ Some), `dmabuf_fd()` (DMABUF 出力時のみ Some), `index()`, `bytesused()`, `length()`, `is_keyframe()`, `timestamp_us()`, `user_data()` |
| `EncodeHandler` | エンコード完了通知トレイト | `type UserData`, `type Error: From<v4l2_m2m::Error>`, `on_encoded(Result<EncodedFrame<UserData>, Error>)` |
| `FnEncodeHandler<T, E>` | `FnMut` クロージャを `EncodeHandler` にするラッパー | `new(FnMut(Result<EncodedFrame<T>, E>) + Send + 'static)` |
| `H264Encoder<H: EncodeHandler>` | H.264 エンコーダー本体 | `new(EncoderConfig, H) -> Result<Self>`, `encode(EncodeInput, timestamp_us, force_keyframe, user_data) -> Result<()>`, `set_bitrate(bps) -> Result<()>`, `force_keyframe() -> Result<()>`, `resolution() -> Resolution` |

#### `EncoderConfig` のフィールド (デフォルト値)

| フィールド | 型 | デフォルト |
|------------|----|------------|
| `device_path` | `String` | `"/dev/video11"` |
| `width` / `height` | `u32` | (`new` 引数) |
| `stride` | `u32` | `0` (= `width` と同じ) |
| `profile` | `H264Profile` | `High` |
| `level` | `H264Level` | `Level4_2` |
| `i_period` | `u32` | `500` (I フレーム間隔) |
| `repeat_sequence_header` | `bool` | `true` (各キーフレームに SPS/PPS を付加) |
| `bitrate_bps` | `u32` | (`new` 引数) |
| `output_buffer_count` | `u32` | `4` (入力 YUV のバッファ数) |
| `capture_buffer_count` | `u32` | `4` (出力 H.264 のバッファ数) |
| `input_memory` | `Memory` | `Mmap` |
| `output_memory` | `Memory` | `Mmap` |
| `pixel_format` | `PixelFormat` | `Yuv420` |

V4L2 用語の OUTPUT は「カーネルへの入力 (YUV)」、CAPTURE は「カーネルからの出力 (H.264)」である点に注意。

#### `encode()` の挙動

- 初回 `encode()` 呼び出し時に内部で `STREAMON` (OUTPUT → CAPTURE) と Poller スレッド起動を行う。明示的な start API は存在しない。
- `force_keyframe = true` を渡すと `V4L2_CID_MPEG_VIDEO_FORCE_KEY_FRAME` で次のフレームを強制 I フレームにする。
- `EncodeInput::Mmap` クロージャが `None` を返すと `Error::MmapInputNotProduced`、`Some(size)` の `size` がバッファ容量を超えると `Error::InputTooLarge` を返す。
- ハンドラーへの `Ok(EncodedFrame)` 通知はポーラースレッドで非同期に行われる。

### デコーダー (`H264Decoder<H>`)

| 型 | 説明 | 主要メソッド |
|----|------|-------------|
| `DecoderConfig` | デコーダー設定 | `new()` / `Default` (デフォルト値を構築) |
| `DecodeInput<'a, T>` | デコード入力 | `Mmap(&mut FnMut(&mut [u8], &T) -> Option<usize>)`, `DmaBuf { fd, bytesused, length }` |
| `DecodedFrame<T>` | デコード結果のハンドル (`Drop` で自動再キュー) | `data()`, `dmabuf_fd()`, `index()`, `bytesused()`, `length()`, `timestamp_us()`, `user_data()` |
| `DecodeHandler` | デコード完了通知トレイト | `type UserData`, `type Error: From<v4l2_m2m::Error>`, `on_decoded(Result<DecodedFrame<UserData>, Error>)`, `on_resolution_changed(Resolution)` |
| `FnDecodeHandler<T, E>` | `FnMut` クロージャを `DecodeHandler` にするラッパー | `new(on_decoded, on_resolution_changed)` |
| `H264Decoder<H: DecodeHandler>` | H.264 デコーダー本体 | `new(DecoderConfig, H) -> Result<Self>`, `decode(DecodeInput, timestamp_us, user_data) -> Result<()>`, `resolution() -> Option<Resolution>` |

#### `DecoderConfig` のフィールド (デフォルト値)

| フィールド | 型 | デフォルト |
|------------|----|------------|
| `device_path` | `String` | `"/dev/video10"` |
| `input_memory` | `Memory` | `Mmap` |
| `output_memory` | `Memory` | `Mmap` |
| `output_buffer_count` | `u32` | `4` |
| `capture_buffer_count` | `u32` | `4` |

#### 動的解像度 (Source Change)

- `H264Decoder::new` 時点で OUTPUT (H.264 入力) のみ `STREAMON` し、CAPTURE バッファは未確保のまま `V4L2_EVENT_SOURCE_CHANGE` を購読する。
- 初回 H.264 ストリームを feed すると `SOURCE_CHANGE` イベントが上がり、内部で `G_FMT` → CAPTURE バッファ確保 → `STREAMON` → `on_resolution_changed(Resolution)` 通知を行う。
- 途中で解像度が変わった場合も同様に CAPTURE を `STREAMOFF` → 再確保 → `STREAMON` する。`on_resolution_changed` は変更が確定するたびに呼ばれる。
- デバイスが `SOURCE_CHANGE` 購読に対応しない場合は `subscribe_events = false` で動作 (この場合は呼び出し側で適切な解像度ハンドリングが必要)。
- `H264Decoder::resolution()` は CAPTURE 確保前は `None`、確保後は `Some(Resolution)`。

### 画像変換器 (`ImageConverter<T>`)

| 型 | 説明 | 主要メソッド |
|----|------|-------------|
| `ConverterConfig` | 変換器設定 | `new(input_width, input_height, output_width, output_height)` |
| `ConvertInput<'a, T>` | 変換入力 | `Mmap(&mut FnMut(&mut [u8], &Resolution, &T) -> Option<usize>)`, `DmaBuf { fd, bytesused, length }` |
| `ConvertedFrame` | 変換結果のハンドル (`Drop` で自動再キュー) | `data()`, `dmabuf_fd()`, `index()`, `bytesused()`, `length()`, `timestamp_us()` |
| `ConvertCallbackOutput<T>` | 変換コールバック出力 enum | `Frame { frame: ConvertedFrame, value: T }` |
| `ImageConverter<T: Send + 'static>` | 画像変換器本体 | `new(ConverterConfig, FnMut(Result<ConvertCallbackOutput<T>>) + Send + 'static) -> Result<Self>`, `convert(ConvertInput, timestamp_us, value) -> Result<()>`, `input_resolution() -> Resolution`, `output_resolution() -> Resolution` |

#### `ConverterConfig` のフィールド (デフォルト値)

| フィールド | 型 | デフォルト |
|------------|----|------------|
| `device_path` | `String` | `"/dev/video12"` |
| `input_width` / `input_height` | `u32` | (`new` 引数) |
| `input_memory` | `Memory` | `Mmap` |
| `input_pixel_format` | `PixelFormat` | `Yuv420` |
| `output_width` / `output_height` | `u32` | (`new` 引数) |
| `output_memory` | `Memory` | `Mmap` |
| `output_pixel_format` | `PixelFormat` | `Nv12` |
| `buffer_count` | `u32` | `4` |

変換器は `PixelFormat::Yuv420` / `PixelFormat::Nv12` のみ対応 (双方の相互変換と拡大縮小)。`PixelFormat::H264` を指定すると `Error::InvalidFormat`。
`S_FMT` 後にカーネルが確定した解像度は `input_resolution()` / `output_resolution()` で取得できる (要求値と異なる場合がある)。

## 入力種別: MMAP / DMABUF

### `Mmap` 入力 (コピー)

クロージャ `FnMut(&mut [u8], &Resolution, &T) -> Option<usize>` が、ライブラリが用意した OUTPUT バッファ (mmap 済み) に直接書き込む。`Some(size)` を返すと書き込んだバイト数を `bytesused` としてエンキューする。`None` を返すと `Error::MmapInputNotProduced`。

書き込みサイズはバッファ容量を超えてはならない (超えると `Error::InputTooLarge`)。

### `DmaBuf` 入力 (ゼロコピー)

`{ fd, bytesused, length }` を渡すと、その DMABUF を OUTPUT として import する。libcamera 等から受け取った DMABUF をそのままエンコーダーに渡せる。

設定との不整合 (`input_memory = Mmap` なのに `EncodeInput::DmaBuf` を渡す等) は `Error::InvalidFormat` を返す。

### MMAP / DMABUF 出力

`EncoderConfig::output_memory` / `DecoderConfig::output_memory` / `ConverterConfig::output_memory` で出力側 (CAPTURE) のメモリ方式を選ぶ。

- `Memory::Mmap`: フレームの `data()` がカーネル mmap 領域を参照する `&[u8]` を返す。`dmabuf_fd()` は `None`。
- `Memory::DmaBuf`: CAPTURE バッファ自体は MMAP で確保し `EXPBUF` でエクスポートする。フレームの `dmabuf_fd()` がエクスポート済み fd を返し、`data()` は `None`。

## ライフサイクルと内部スレッド

- 各コンポーネントは内部に Poller スレッド (`poll(2)` 監視) を 1 本持ち、CAPTURE/OUTPUT/EVENT を待つ。
- ハンドラー (またはクロージャ) はこの Poller スレッドから呼ばれる。`Send + 'static` が要求される。
- `H264Encoder` / `ImageConverter` は初回 `encode()` / `convert()` 時に Poller を起動する (遅延起動)。`H264Decoder` は `new()` 時点で起動する (SOURCE_CHANGE を待つため)。
- Drop 順は `poller → shared → handler/callback → device` で、Poller スレッドが停止してから `STREAMOFF` → fd close する。`H264Encoder` / `H264Decoder` / `ImageConverter` の構造体定義の宣言順がこの Drop 順を保証している (壊さないこと)。
- `EncodedFrame` / `DecodedFrame` / `ConvertedFrame` の `Drop` は CAPTURE バッファを `QBUF` で再キューする。再キュー失敗は内部の async error キューに積まれ、次のイベント通知時にハンドラーへ伝播される。

## コード例

### H.264 エンコード (MMAP 入出力)

```rust
use std::sync::mpsc;
use std::time::Duration;

use shiguredo_v4l2::v4l2_m2m::{EncodeInput, EncoderConfig, FnEncodeHandler, H264Encoder};

// 1280x720, 2Mbps のエンコーダー
let config = EncoderConfig::new(1280, 720, 2_000_000);
let (tx, rx) = mpsc::channel::<(usize, bool, i64, u64)>();
let mut encoder = H264Encoder::new(
    config,
    FnEncodeHandler::new(move |result| match result {
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
            // ここで frame が Drop され、CAPTURE バッファが自動再キューされる
        }
        Err(err) => eprintln!("encode error: {err}"),
    }),
)?;

// I420 フレームをエンキュー (初回呼び出しで自動 STREAMON)
encoder.encode(
    EncodeInput::Mmap(&mut |buf, _resolution, _user_data| {
        let size = yuv_data.len();
        buf[..size].copy_from_slice(&yuv_data);
        Some(size)
    }),
    /* timestamp_us = */ 0,
    /* force_keyframe = */ false,
    /* user_data = */ 123_u64,
)?;
```

### エンコーダー設定のカスタマイズ

```rust
use shiguredo_v4l2::v4l2_m2m::{EncoderConfig, H264Profile, H264Level, Memory, PixelFormat};

let mut config = EncoderConfig::new(1920, 1080, 4_000_000);
config.profile = H264Profile::High;
config.level = H264Level::Level4_2;
config.pixel_format = PixelFormat::Nv12;   // NV12 入力 (デフォルト: Yuv420)
config.i_period = 500;                     // I フレーム間隔
config.repeat_sequence_header = true;      // 各キーフレームに SPS/PPS を付加
config.output_buffer_count = 4;
config.capture_buffer_count = 4;
config.input_memory = Memory::Mmap;        // または Memory::DmaBuf
config.output_memory = Memory::Mmap;
```

### DMABUF 入力 (libcamera 等からのゼロコピー)

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

### ビットレート変更と強制キーフレーム

```rust
encoder.set_bitrate(3_000_000)?;  // V4L2_CID_MPEG_VIDEO_BITRATE
encoder.force_keyframe()?;        // V4L2_CID_MPEG_VIDEO_FORCE_KEY_FRAME
```

`force_keyframe()` は次のフレームに対して即時適用される。`encode()` の `force_keyframe` 引数も同等。

### H.264 デコード (動的解像度対応)

```rust
use std::sync::mpsc;
use std::time::Duration;

use shiguredo_v4l2::v4l2_m2m::{DecodeInput, DecoderConfig, FnDecodeHandler, H264Decoder};

let config = DecoderConfig::new();
let (tx, rx) = mpsc::channel::<String>();
let mut decoder = H264Decoder::new(
    config,
    FnDecodeHandler::new(
        // on_decoded
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
            Err(err) => eprintln!("decode error: {err}"),
        },
        // on_resolution_changed
        |resolution| {
            let _ = tx.send(format!(
                "resolution changed: {}x{} (stride={})",
                resolution.width, resolution.height, resolution.stride
            ));
        },
    ),
)?;

// H.264 NAL ユニットを feed
decoder.decode(
    DecodeInput::Mmap(&mut |buf, _user_data| {
        let size = h264_data.len();
        buf[..size].copy_from_slice(&h264_data);
        Some(size)
    }),
    /* timestamp_us = */ 0,
    /* user_data = */ 456_u64,
)?;
```

最初の数フレーム feed 後に `on_resolution_changed` が一度呼ばれ、それ以降 `on_decoded` で I420 フレームを受け取れる。

### 画像変換 (I420 → NV12 + 拡大縮小)

```rust
use shiguredo_v4l2::v4l2_m2m::{ConvertCallbackOutput, ConvertInput, ConverterConfig, ImageConverter, PixelFormat};

let mut config = ConverterConfig::new(1920, 1080, 1280, 720);
config.input_pixel_format = PixelFormat::Yuv420;
config.output_pixel_format = PixelFormat::Nv12;

let mut converter = ImageConverter::<u64>::new(config, |result| match result {
    Ok(ConvertCallbackOutput::Frame { frame, value }) => {
        if let Some(data) = frame.data() {
            println!("converted {} bytes (user_value = {})", data.len(), value);
        }
    }
    Err(err) => eprintln!("convert error: {err}"),
})?;

converter.convert(
    ConvertInput::Mmap(&mut |buf, _resolution, _value| {
        let size = src.len();
        buf[..size].copy_from_slice(&src);
        Some(size)
    }),
    timestamp_us,
    user_value,
)?;
```

### カスタム `EncodeHandler` 実装

`FnEncodeHandler` を使わず独自構造体にする例。`UserData` / `Error` 関連型を持てる。

```rust
use shiguredo_v4l2::v4l2_m2m::{EncodedFrame, EncodeHandler, Error as V4l2Error};

struct MyHandler {
    out: std::sync::mpsc::Sender<Vec<u8>>,
}

#[derive(Debug)]
enum MyError {
    V4l2(V4l2Error),
    Send,
}

impl From<V4l2Error> for MyError {
    fn from(err: V4l2Error) -> Self {
        MyError::V4l2(err)
    }
}

impl EncodeHandler for MyHandler {
    type UserData = ();
    type Error = MyError;

    fn on_encoded(&mut self, result: Result<EncodedFrame<()>, MyError>) {
        match result {
            Ok(frame) => {
                if let Some(data) = frame.data() {
                    let _ = self.out.send(data.to_vec());
                }
            }
            Err(err) => eprintln!("handler error: {err:?}"),
        }
    }
}
```

## エラー型 (`Error`)

| バリアント | 説明 |
|-----------|------|
| `DeviceOpen { path, source }` | デバイスファイル (`/dev/videoNN`) のオープン失敗 |
| `Ioctl { request, source }` | `VIDIOC_*` ioctl の失敗 (`request` に名称) |
| `Mmap { source }` | `mmap(2)` の失敗 |
| `Poll { source }` | `poll(2)` の失敗 |
| `InvalidFormat { reason }` | フォーマット指定の不整合 (例: 設定が DMABUF だが入力が MMAP) |
| `NoAvailableBuffer` | 利用可能な OUTPUT/CAPTURE バッファがない |
| `NotStarted` | CAPTURE がまだ確保されていない (SOURCE_CHANGE 前) |
| `StreamOn { source }` | `VIDIOC_STREAMON` の失敗 |
| `StreamOff { source }` | `VIDIOC_STREAMOFF` の失敗 |
| `InputTooLarge { size, capacity }` | 入力サイズがバッファ容量超過 |
| `MmapInputNotProduced` | `Mmap` 入力クロージャが `None` を返した |
| `PollerAborted` | Mutex の `PoisonError` 等で Poller が中断された |

`Error` は `Display` / `std::error::Error` を実装し、`source()` で原因の `io::Error` を辿れる。

## 既知の制限事項

- **Raspberry Pi 専用**: 現状は bcm2835-codec デバイス (`/dev/video10` / `/dev/video11` / `/dev/video12`) を前提とし、他の V4L2 M2M デバイスでの動作は保証しない。
- **MPLANE 前提**: シングルプレーン (`V4L2_BUF_TYPE_VIDEO_OUTPUT` / `V4L2_BUF_TYPE_VIDEO_CAPTURE`) には対応しない。
- **CAPTURE プレーン数 = 1 前提**: 内部実装は `num_planes = 1` を前提に書かれている。
- **エンコーダー入力フォーマット**: `PixelFormat::Yuv420` (I420) と `PixelFormat::Nv12` のみ受け付ける。
- **画像変換器のフォーマット**: 入力/出力ともに `Yuv420` / `Nv12` のみ。`H264` を指定すると `Error::InvalidFormat`。
- **コントロール失敗の許容**: `EncoderConfig` の各 V4L2 コントロール (Profile / Level / I-Period / Repeat SPS/PPS / Bitrate) はデバイスによって未対応の場合があり、初期化時の `S_CTRL` 失敗は無視する (非致命的)。
- **CAPTURE バッファサイズ**: H.264 出力の CAPTURE バッファは固定で 512KB の `sizeimage` を要求する。極端に大きなフレームでは `Error::InputTooLarge` の可能性がある。
- **タイムスタンプ精度**: `timestamp_us` は `struct timeval` (`tv_sec * 1_000_000 + tv_usec`) で扱うため、マイクロ秒精度。
