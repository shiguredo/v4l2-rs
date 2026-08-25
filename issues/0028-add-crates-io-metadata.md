# Cargo.toml に crates.io メタデータ (keywords / categories / documentation) を追加する

- Created: 2026-08-21
- Completed:
- Branch: feature/add-crates-io-metadata
- Polished:

## 目的

`Cargo.toml` の `[package]` セクションに `keywords` / `categories` / `documentation` の crates.io メタデータフィールドが無く、crates.io での発見性と分類が弱い。ユーザーが検索で辿り着ける情報を追加する。

## 現状

- `Cargo.toml::[package]` は `name` / `version` / `edition` / `rust-version` / `description` / `readme` / `homepage` / `repository` / `license` / `include` のみ
- `keywords` （crates.io 検索キーワード、最大 5 個）
- `categories` （crates.io カテゴリ、最大 5 個）
- `documentation` （docs.rs へのリンク）
- が全て未設定

## 設計方針

- 以下を追加する:
  ```toml
  keywords = ["v4l2", "video", "raspberry-pi", "h264"]
  categories = ["multimedia::video", "os::linux-apis"]
  documentation = "https://docs.rs/shiguredo_v4l2"
  ```
- `keywords` は crates.io の 20 文字 / 5 個までの制約に合わせる
- `categories` は https://crates.io/category_slugs から適切なものを選ぶ（`multimedia::video` / `os::linux-apis` が妥当）
- `[badges]` セクション（`maintenance = { status = "actively-developed" }`）の追加も検討する

## 完了条件

- crates.io の crate ページに keywords / categories バッジが表示される
- docs.rs へのリンクが Cargo.toml で明示される
- `cargo package --list` および `cargo publish --dry-run` が引き続き成功する

## 変更対象

- `Cargo.toml`
