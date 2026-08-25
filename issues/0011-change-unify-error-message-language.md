# Error::Display の日本語と InvalidFormat.reason の英語が混在するのを統一する

- Created: 2026-08-21
- Completed:
- Branch: feature/change-unify-error-message-language
- Polished:

## 目的

`src/error.rs::Error` の `Display` 実装は全 12 バリアントとも日本語文言だが、`Error::InvalidFormat { reason }` の `reason` フィールドを組み立てる呼び出し側（converter / encoder / decoder / queue）は英語文字列を渡しており、実際のエラー出力が「不正なフォーマットです: `converter is configured for DMABUF input`」のように日本語と英語が混在する。ユーザーが受け取るエラー文言の一貫性を確保するため、日英どちらかに統一する。

## 現状

- `src/error.rs::Error` の `Display` は全バリアント日本語（例: 「デバイスのオープンに失敗しました: {path}: {source}」）
- `Error::InvalidFormat { reason }` の reason を組み立てる箇所は全て英語:
  - `src/converter.rs` の `ConvertInput::Mmap` 分岐（`"converter is configured for DMABUF input"`）
  - `src/converter.rs` の `ConvertInput::DmaBuf` 分岐（`"converter is configured for MMAP input"`）
  - `src/converter.rs::ImageConverter::validate_pixel_format`（`format!("converter {name} does not support {:?}", fmt)`）
  - `src/converter.rs::ImageConverter::set_output_format` / `set_capture_format`（`format!("{name} is invalid after VIDIOC_S_FMT")`）
  - `src/encoder.rs` の `EncodeInput::Mmap` / `EncodeInput::DmaBuf` 分岐、`H264Encoder::set_bitrate` 内の `format!("bitrate exceeds i32 maximum: {bitrate_bps}")`
  - `src/decoder.rs::H264Decoder::decode` の `DecodeInput::Mmap` / `DecodeInput::DmaBuf` 分岐
  - `src/queue.rs::OutputQueue::enqueue`（`"output queue does not provide MMAP buffer"`）
- 加えて `AGENTS.md` に「ログメッセージは全て英語にすること」の規約がある。エラーメッセージがログメッセージに含まれるかは解釈次第だが、Rust エコシステムでは `Error::Display` は英語が慣例

## 設計方針

- AGENTS.md 規約と Rust 慣例に合わせて **英語に統一** する方向を推奨する
- 対応内容:
  - `src/error.rs::Error::Display` を英語で書き直す
  - `reason` を組み立てる箇所はそのまま英語のため変更不要
  - 「後方互換のない変更」として `CHANGES.md` の `[CHANGE]` に分類する
- あるいは日本語に統一する場合は、`reason` 文字列を組み立てる 9 箇所以上を書き換える必要がある
- 実装時に advisor 相談で方向を確定させる。ユーザー影響が大きい変更なので Cargo.toml のバージョンとの兼ね合いも検討

## 完了条件

- `Error::Display` の出力が日英混在しなくなる
- 選択した言語で全バリアントが統一される
- `cargo test --workspace` および `cargo clippy --workspace --all-targets -- -D warnings` が成功する
- `pbt/tests/prop_error.rs::error_display_non_empty` が引き続き成功する

## 変更対象

- `src/error.rs`（`Error::Display` の書き換え）
- 日本語に寄せる場合は `src/converter.rs` / `src/encoder.rs` / `src/decoder.rs` / `src/queue.rs` の reason 組み立て箇所
- `CHANGES.md`（`[CHANGE]` として追加）
