# tests/*.rs / pbt/tests/*.rs にファイル冒頭 docstring を追加する

- Created: 2026-08-21
- Completed:
- Branch: feature/update-add-test-file-docstrings
- Polished:

## 目的

`tests/test_converter.rs` / `tests/test_format.rs` / `pbt/tests/prop_error.rs` / `pbt/tests/prop_format.rs` のいずれもファイル冒頭に module docstring が無い。AGENTS.md「テストはコメントを重視すること」に照らして、ファイル全体の目的・カバー範囲を明記する。

## 現状

- `tests/test_converter.rs` は `use` から始まる。個別テスト直前のブロックコメントはあるが、ファイル全体の目的（4 段パイプラインのラウンドトリップ結合テスト）を説明する冒頭コメントが無い
- `tests/test_format.rs` は `use` から始まり、境界値テスト 1 個のみ
- `pbt/tests/prop_error.rs` / `prop_format.rs` も冒頭が即 `use`

## 設計方針

- 各ファイル冒頭に module docstring を追加する:
  - `tests/test_converter.rs`: 4 段パイプライン (I420 → NV12 → H.264 → I420 → I420) のラウンドトリップ結合テスト。実機必須の旨、`#[ignore]` の方針を書く
  - `tests/test_format.rs`: `Resolution::yuv420_size` の境界値テスト。PBT (`prop_format.rs`) との棲み分けを書く
  - `pbt/tests/prop_error.rs`: `Error` の Display / Debug / source の網羅性検証
  - `pbt/tests/prop_format.rs`: `PixelFormat` / `H264Profile` / `H264Level` / `Resolution::yuv420_size` の property テスト
- コメントは日本語（`//` 単行 or `/*!` module doc は integration test では使えないので `//` で書く）
- テスト関数の直前コメントも既存分を確認して補強する

## 完了条件

- 4 ファイルに冒頭 module docstring が入る
- テストの目的・カバー範囲・実行前提が読み取れる

## 変更対象

- `tests/test_converter.rs`
- `tests/test_format.rs`
- `pbt/tests/prop_error.rs`
- `pbt/tests/prop_format.rs`
