# ImageConverter::frame_size が奇数解像度で必要バイト数と乖離しオーバーフローする問題を修正する

- Created: 2026-08-21
- Completed:
- Branch: feature/fix-converter-frame-size-odd-resolution
- Polished: 2026-08-23

## 目的

`src/converter.rs` の `ImageConverter::frame_size` は `(width as usize) * (height as usize) * 3 / 2` の単純計算で、`src/format.rs` の `Resolution::yuv420_size` に施された「奇数 stride / height でも平面分割と整合するように `div_ceil(2)` で計算する」修正から取り残されている。加えて素の乗算で保護がないため `width` / `height` が大きいと release で wrap、debug で panic する。converter に奇数解像度を渡した際の `sizeimage` 過小と、公開 API 経由での panic 経路を修正する。

## 現状

- `src/converter.rs` の `ImageConverter::frame_size` は width / height を素で乗算する古い計算式
- `src/converter.rs` の `set_output_format` / `set_capture_format` から `plane_fmt[0].sizeimage` に渡している
- `src/format.rs` の `Resolution::yuv420_size` は `chroma_stride = stride.div_ceil(2)` / `chroma_height = height.div_ceil(2)` で計算し、さらに `saturating_mul` / `saturating_add` で保護されている
- `CHANGES.md` の `## develop` の `[FIX] yuv420_size() が奇数 stride / height で平面分割と必要なバイト数が一致しない問題を修正する` に該当する修正が converter 側では未反映
- 例: `width=5, height=5` の場合、`frame_size` は `5*5*3/2 = 37` を返すが、`yuv420_size` は `25 + 3*3*2 = 43` で **6 バイト不足**
- `ImageConverter::new` は公開 API で任意の `u32` を受けるため、`u32::MAX` 級の値で乗算オーバーフローが起こる

## 設計方針

- `ImageConverter::frame_size` を削除し、呼び出し箇所を `Resolution { width, height, stride: width }.yuv420_size()` を `u32::try_from` で変換する形に置き換える
- NV12 も 4:2:0 で plane 合計サイズは I420 と同じため、この置き換えで両フォーマット対応可能
- `sizeimage` が `u32` に収まらない場合（`u32::MAX` 級の入力）は `u32::try_from` の失敗で `Error::InvalidFormat` を返す。`as u32` による切り詰めは完了条件「panic せず適切にエラーを返す」を満たさないため使用しない
- `Resolution::yuv420_size` に対する既存の PBT (`pbt/tests/prop_format.rs::yuv420_size_matches_plane_split`) が引き続き適用される

## 完了条件

- `ImageConverter::frame_size` が削除されるか `Resolution::yuv420_size` に統一される
- 奇数解像度（例: 3x3 / 5x5）で converter を初期化しても sizeimage が過小にならない
- `u32::MAX` 級の入力で `ImageConverter::new` が panic せず適切にエラーを返す
- `cargo test --workspace` および `cargo clippy --workspace --all-targets -- -D warnings` が成功する

## 変更対象

- `src/converter.rs`（`ImageConverter::frame_size` の削除、呼び出し箇所の置き換え）
- `CHANGES.md`（`[FIX]` として追加）
