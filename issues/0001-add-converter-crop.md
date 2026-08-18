# ImageConverter に入力 crop 機能を追加する

- Created: 2026-08-19
- Completed: {YYYY-MM-DD}
- Branch: feature/add-converter-crop
- Polished: {YYYY-MM-DD}

## 目的

sora-rust-sdk の libcamera エンコードパイプライン（libcamera native パス）で、`AdaptedVideoTrackSource::adapt_frame` が返す crop 情報を V4L2 の HWA で反映するための基盤として、`shiguredo_v4l2` の `ImageConverter` に入力 crop 機能を追加する。

## 現状

`ImageConverter`（`src/converter.rs`）の `ConvertInput::DmaBuf` は `fd` / `bytesused` / `length` のみを持ち、入力映像の一部分を切り出して出力解像度へ拡大縮小する手段がない。

`src/sys.rs` に V4L2 selection API（`VIDIOC_S_SELECTION` / `V4L2_SEL_TGT_CROP` / `v4l2_selection`）の定義がない。

## 設計方針

crop の指定は V4L2 selection API の `V4L2_SEL_TGT_CROP` で行う。

`v4l2_plane.data_offset` による指定は、入力バッファが Y/U/V を 1 つの連続 DMA-BUF に詰め込んだ単一プレーンであり、任意の部分矩形 crop は行ごとに不連続になるため表現できない。`V4L2_SEL_TGT_CROP` は bcm2835-codec ドライバが ISP ロールでサポートしており、crop 領域を出力解像度へ拡大縮小する標準的な方法である。

- `src/format.rs` に `Crop`（x / y / width / height）構造体を追加し、`src/lib.rs` で re-export する
- `src/sys.rs` に `VIDIOC_S_SELECTION` / `V4L2_SEL_TGT_CROP` / `v4l2_rect` / `v4l2_selection` / `ioctl_s_selection` を追加する
- `ConvertInput::DmaBuf` に `crop: Option<Crop>` フィールドを追加する
- `ImageConverter::convert` は crop が前回適用値と異なる場合のみ S_SELECTION を発行し、同一 crop が連続する場合は ioctl を発行しない
- `crop: None` はフルフレーム（crop なし）を意味し、従来どおり動作する
- S_SELECTION 後に G_SELECTION で反映結果を確認し、ドライバが指定を無視した場合はエラーを返して静かに歪んだ出力を流さない

### ハードウェア制約

Raspberry Pi の `/dev/video12`（bcm2835-codec ISP）は `V4L2_SEL_TGT_CROP` を「(0,0) 起点のみ」でサポートし、`left` / `top` を強制的に 0 にする。一方、libwebrtc の `AdaptedVideoTrackSource::AdaptFrame` は `crop_x = (width - crop_width) / 2` / `crop_y = (height - crop_height) / 2` のように中央基準の crop を返す。このため x / y オフセットを忠実に反映できない。オフセット付き crop の扱いは本 issue のスコープ外（sora-rust-sdk 側で対応）だが、crop が静かに無視されないよう上記の反映検証を必須とする。

## 完了条件

- `ConvertInput::DmaBuf` に `crop` を指定でき、指定した領域が出力解像度へ拡大縮小される
- `crop: None` の場合の挙動が従来どおりである
- crop 指定がドライバに反映されない場合にエラーで検知できる
- `cargo test --workspace` が成功する
- production log は英語、コメントとテストの assertion message は日本語にする

## 変更対象

- `src/converter.rs`（`ConvertInput` / `ImageConverter`）
- `src/format.rs`（`Crop`）
- `src/sys.rs`（V4L2 selection API）
- `src/lib.rs`（re-export）
- `tests/test_converter.rs`（`ConvertInput::DmaBuf` 使用箇所の更新）
- `CHANGES.md`
- `Cargo.toml`（バージョン更新）
