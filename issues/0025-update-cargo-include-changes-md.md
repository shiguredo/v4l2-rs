# Cargo.toml の include に CHANGES.md を追加して crates.io で変更履歴が見えるようにする

- Created: 2026-08-21
- Completed:
- Branch: feature/update-cargo-include-changes-md
- Polished:

## 目的

`Cargo.toml` の `include = ["/src/**/*", "LICENSE", "README.md"]` に `CHANGES.md` が含まれていないため、crates.io / docs.rs 上で変更履歴が閲覧できない。ユーザーが変更履歴を参照できるように include に追加する。

## 現状

- `Cargo.toml:11` `include = ["/src/**/*", "LICENSE", "README.md"]`
- `CHANGES.md` はリポジトリルートに存在し、`## develop` セクションで未リリースの変更を管理している
- crates.io のリリースページから変更履歴を辿る術がない

## 設計方針

- `Cargo.toml::include` に `"CHANGES.md"` を追加する:
  ```toml
  include = ["/src/**/*", "LICENSE", "README.md", "CHANGES.md"]
  ```
- 他のドキュメント（`docs/*.md`、`skills/**/*.md`）を含めるかは別途検討（本 issue のスコープ外）

## 完了条件

- crates.io にリリースされたパッケージに `CHANGES.md` が含まれる
- `cargo package --list` で `CHANGES.md` が出力される
- `cargo test --workspace` および `cargo clippy --workspace --all-targets -- -D warnings` が成功する

## 変更対象

- `Cargo.toml`
