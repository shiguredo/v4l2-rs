# ImageConverter に入力 crop 機能を追加する

- Created: 2026-08-19
- Completed: {YYYY-MM-DD}
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
- `ConvertInput::DmaBuf` に `crop: Option<Crop>` フィールドを追加する（後方互換のない変更であり、既存の構築箇所の更新を伴う。Mmap 入力には crop を指定できない）
- `ImageConverter::convert` は crop が前回適用値と異なる場合のみ S_SELECTION を発行し、同一 crop が連続する場合は ioctl を発行しない。前回適用値の初期状態は `None` とし、初回の `crop: None` では S_SELECTION を発行しない（デバイスの初期 crop 矩形は入力解像度全体である。実機確認済み）。前回適用値は G_SELECTION による反映確認が成功した場合のみ更新する
- `crop: None` はフルフレーム（crop なし）を意味する。crop 付きフレームの後に `crop: None` を渡した場合は、入力解像度全体の矩形へ戻す S_SELECTION を発行する
- S_SELECTION は初回を STREAMON 前に発行し、以降の crop 変更はストリーミング中に発行する。ストリーミング中の変更をドライバが拒否した場合は S_SELECTION がエラーを返す
- S_SELECTION 後に G_SELECTION で反映結果を確認し、返却された矩形が指定と一致しない場合はエラーを返して静かに歪んだ出力を流さない
- 反映失敗のエラーは既存の `Error` バリアントでは表現できないため、`src/error.rs` に新バリアントを追加する

### ハードウェア制約

Raspberry Pi の `/dev/video12`（bcm2835-codec ISP）は `V4L2_SEL_TGT_CROP` を「(0,0) 起点のみ」でサポートし、`left` / `top` を強制的に 0 にする（実機確認済み）。一方、libwebrtc の `AdaptedVideoTrackSource::AdaptFrame` は `crop_x = (width - crop_width) / 2` / `crop_y = (height - crop_height) / 2` のように中央基準の crop を返す。このため x / y オフセットを忠実に反映できない。オフセット付き crop の扱いは本 issue のスコープ外（sora-rust-sdk 側で対応）だが、crop が静かに無視されないよう上記の反映検証を必須とする。

また、bcm2835-codec の ISP は幅を広げる crop 遷移を受け付けず、指定幅を現在の crop 幅へクランプする（高さは入力高さまで拡大できる。実機確認済み）。一度狭い crop を適用した後に `crop: None`（フルフレーム復帰）を渡しても入力解像度全体には戻らず、G_SELECTION 検証がエラーになる。このため幅非増加の crop 遷移のみが安全に適用できる。フルフレームへの復帰が必要な場合はストリームの再構築等を sora-rust-sdk 側で検討する（本 issue のスコープ外）。

実機検証は Raspberry Pi 4 Model B（カーネル 6.12）で行った。他の機種・カーネルでは挙動が異なる可能性があるため、前提と異なる挙動を G_SELECTION 検証でエラーとして検知できるようにする。

## 完了条件

- `ConvertInput::DmaBuf` に `crop` を指定でき、指定した領域が出力解像度へ拡大縮小される
- `crop: None` の場合の挙動が従来どおりである
- crop 指定がドライバに反映されない場合にエラーで検知できる
- `cargo test --workspace` が成功する

## 変更対象

- `src/converter.rs`（`ConvertInput` / `ImageConverter`）
- `src/format.rs`（`Crop`）
- `src/sys.rs`（V4L2 selection API）
- `src/error.rs`（反映失敗のエラーバリアント）
- `src/lib.rs`（re-export）
- `tests/test_converter.rs`（`ConvertInput::DmaBuf` 使用箇所の更新）
- `pbt/tests/prop_error.rs`（`arb_error()` への新バリアント追加）
- `skills/shiguredo-v4l2/SKILL.md`（公開 API の記述の更新）
- `CHANGES.md`（後方互換のない変更として `[CHANGE]` に分類）
- `Cargo.toml`（バージョン更新）
