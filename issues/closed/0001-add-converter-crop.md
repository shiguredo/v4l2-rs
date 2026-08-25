# ImageConverter に入力 crop 機能を追加する

- Created: 2026-08-19
- Completed: 2026-08-19
- Branch: feature/add-converter-crop
- Polished: 2026-08-19

## 目的

sora-rust-sdk の libcamera エンコードパイプライン（libcamera native パス）で、libwebrtc の `AdaptedVideoTrackSource::AdaptFrame` が返す crop 情報を V4L2 の HWA で反映するための基盤として、`shiguredo_v4l2` の `ImageConverter` に入力 crop 機能を追加する。

## 現状

`ImageConverter`（`src/converter.rs`）の `ConvertInput::DmaBuf` は `fd` / `bytesused` / `length` のみを持ち、入力映像の一部分を切り出して出力解像度へ拡大縮小する手段がない。

`src/sys.rs` に V4L2 selection API（`VIDIOC_S_SELECTION` / `VIDIOC_G_SELECTION` / `V4L2_SEL_TGT_CROP` / `v4l2_selection`）の定義がない。

## 設計方針

crop の指定は V4L2 selection API の `V4L2_SEL_TGT_CROP` で行う。`ImageConverter` の入力は OUTPUT 側なので、selection は OUTPUT 側に対して発行し、crop 領域が出力解像度（CAPTURE 側の S_FMT サイズ）へ拡大縮小されるようにする。`v4l2_selection.type` のバッファ型は MPLANE 型（`V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE`）で実装する（実機で MPLANE 型と非 MPLANE 型（`V4L2_BUF_TYPE_VIDEO_OUTPUT`）の両方が受理されることを確認済み。既存コードが MPLANE で統一されているため）。

`v4l2_plane.data_offset` による指定は、入力バッファが Y/U/V を 1 つの連続 DMA-BUF に詰め込んだ単一プレーンであり、任意の部分矩形 crop は行ごとに不連続になるため表現できない。`V4L2_SEL_TGT_CROP` は bcm2835-codec ドライバが ISP ロールでサポートしており、crop 領域を出力解像度へ拡大縮小する標準的な方法である（ISP の対応は実機で確認済み）。

- `src/format.rs` に `Crop`（`x` / `y` / `width` / `height`。座標・サイズは OUTPUT 側 S_FMT で確定した入力解像度 `input_resolution()` 基準、`x` / `y` は `v4l2_rect` の `left` / `top` に対応）構造体を追加し、`src/lib.rs` で re-export する
- `src/sys.rs` に `VIDIOC_S_SELECTION` / `VIDIOC_G_SELECTION` / `V4L2_SEL_TGT_CROP` / `v4l2_rect` / `v4l2_selection` / `ioctl_s_selection` / `ioctl_g_selection` を追加する
- `ConverterConfig` に `crop: Option<Crop>` フィールドを追加する（後方互換のない変更であり、`Crop` は変換器の静的設定として Mmap / DMABUF どちらの入力でも有効）
- `ImageConverter::new` は S_FMT 確定後に `crop: Some(...)` の場合のみ S_SELECTION を 1 回発行する。`crop: None` では S_SELECTION を発行しない（デバイスの初期 crop 矩形は入力解像度全体である。実機確認済み）
- S_SELECTION は STREAMON 前に発行する必要があるため、`new()` 内で適用する。ストリーミング開始後の crop 変更はできない
- S_SELECTION 後に G_SELECTION で反映結果を確認し、返却された矩形が指定と一致しない場合はエラーを返して静かに歪んだ出力を流さない
- 反映失敗のエラーは既存の `Error` バリアントでは表現できないため、`src/error.rs` に新バリアントを追加する

### ハードウェア制約

Raspberry Pi の `/dev/video12`（bcm2835-codec ISP）は `V4L2_SEL_TGT_CROP` を「(0,0) 起点のみ」でサポートし、`left` / `top` を強制的に 0 にする（実機確認済み）。一方、libwebrtc の `AdaptedVideoTrackSource::AdaptFrame` は `crop_x = (width - crop_width) / 2` / `crop_y = (height - crop_height) / 2` のように中央基準の crop を返す。このため x / y オフセットを忠実に反映できない。オフセット付き crop の扱いは本 issue のスコープ外（sora-rust-sdk 側で対応）だが、crop が静かに無視されないよう上記の反映検証を必須とする。

また、本機ではストリーミング中の S_SELECTION は幅の増減に関わらず EINVAL で拒否される（実機確認済み）。このため crop は STREAMON 前（`new()` 時）にのみ適用でき、ストリーミング開始後の crop 変更が必要な場合はストリームの再構築等を sora-rust-sdk 側で検討する（本 issue のスコープ外）。

実機検証は Raspberry Pi 4 Model B（カーネル 6.12）で行った。他の機種・カーネルでは挙動が異なる可能性があるため、前提と異なる挙動を G_SELECTION 検証でエラーとして検知できるようにする。

## 完了条件

- `ConverterConfig` に `crop` を指定でき、指定した領域が出力解像度へ拡大縮小される
- `crop: None` の場合の挙動が従来どおりである
- crop 指定がドライバに反映されない場合にエラーで検知できる
- `cargo test --workspace` が成功する

## 変更対象

- `src/converter.rs`（`ConverterConfig` / `ImageConverter`）
- `src/format.rs`（`Crop`）
- `src/sys.rs`（V4L2 selection API）
- `src/error.rs`（反映失敗のエラーバリアント）
- `src/lib.rs`（re-export）
- `tests/test_converter.rs`（crop テストの追加）
- `pbt/tests/prop_error.rs`（`arb_error()` への新バリアント追加）
- `skills/shiguredo-v4l2/SKILL.md`（公開 API の記述の更新）
- `CHANGES.md`（後方互換のない変更として `[CHANGE]` に分類）
- `Cargo.toml`（バージョン更新）

## 解決方法

実機 (Raspberry Pi 4 Model B / kernel 6.18.39) での検証の結果、本 issue の前提が成立せず、crop 機能としての価値が低いと判断したため closed にする。

- `V4L2_SEL_TGT_CROP` は bcm2835-codec ISP でオフセット (`left` / `top`) が (0,0) に強制されるが、サイズは維持されることを実機で確認した (例: requested (100,100,960,540) → actual (0,0,960,540))。よってオフセット付き crop は表現できない
- さらに sora-rust-sdk 側の実機検証 (実 PeerConnection + エンコーダ wants) で、`AdaptedVideoTrackSource::adapt_frame` はアスペクト比不一致でも常にフルフレームの crop (`crop_x = 0` / `crop_y = 0`) を返すことが判明した。libwebrtc M150 の `VideoStreamEncoder` が `OnOutputFormatRequest` を呼ばないため、crop が発生する場面がそもそも無い
- そのため本 issue の crop 機能が実際に使われる場面が無く、基盤としての価値も低いと判断した
- 実装は `feature/add-converter-crop` ブランチにコミット済みであり、将来 (例: 他のカメラパイプラインでオフセット付き crop が必要になった場合) 再利用できるよう残す
