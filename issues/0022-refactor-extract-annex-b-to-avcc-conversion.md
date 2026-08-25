# examples の Annex B → AVCC 変換 3 関数と Error boilerplate の複製を共通化する

- Created: 2026-08-21
- Completed:
- Branch: feature/refactor-extract-annex-b-to-avcc-conversion
- Polished:

## 目的

`examples/v4l2_m2m_encode/src/main.rs` と `examples/v4l2_m2m_libcamera_encode/src/main.rs` は Annex B → AVCC 変換の 3 関数（`split_nal_units` / `nals_to_avcc` / `create_sample_entry`）と `Error` enum の boilerplate（`Args` / `V4l2` / `Mp4` / `Io` / `Message` バリアントと Display / From impl）を 100% 同一で複製している（合計約 200 行）。今後 example を追加するたびに複製が増えるため共通化する。

## 現状

- `examples/v4l2_m2m_encode/src/main.rs` に 3 関数と Error enum が定義
- `examples/v4l2_m2m_libcamera_encode/src/main.rs` にも同一の 3 関数と Error enum が定義
- 差分は 0 行（NAL type 定数、関数シグネチャ、Display / From impl すべて同一）
- どちらも `shiguredo_mp4` を使って MP4 に mux する共通用途

## 設計方針

- 案 A: `examples/common/` に workspace member として共通クレートを追加し、`split_nal_units` / `nals_to_avcc` / `create_sample_entry` を pub 関数として提供
  - Error enum は Display / From impl が固有バリアントを含むので共通化しにくい。共通クレートには変換 3 関数のみを置き、Error は各 example 側で定義する形が現実的
- 案 B: `shiguredo_v4l2` の公開 API に Annex B → AVCC 変換を追加
  - ライブラリのスコープを広げるため、AGENTS.md「Premature Optimization is the Root of All Evil」との兼ね合いで慎重に判断する
  - 将来的に本ライブラリを「H.264 デコード / エンコードのラッパー」として汎化するなら妥当
- 実装時に advisor 相談で方針を確定させる（デフォルトは案 A）

## 完了条件

- 3 関数の複製が 1 箇所にまとまる
- 2 example が引き続き独立にビルド・実行できる
- `cargo test --workspace` および `cargo clippy --workspace --all-targets -- -D warnings` が成功する

## 変更対象

- 新規: `examples/common/`（案 A の場合）
- `examples/v4l2_m2m_encode/src/main.rs`
- `examples/v4l2_m2m_libcamera_encode/src/main.rs`
- 関連する `Cargo.toml`（新 workspace member 追加）
- `CHANGES.md`（記載するかは変更の可視性次第。examples の内部リファクタなら記載不要も検討）
