# rust-toolchain.toml を stable から MSRV の 1.93 に固定して MSRV 検知漏れを防ぐ

- Created: 2026-08-21
- Completed:
- Branch: feature/change-pin-rust-toolchain-to-msrv
- Polished:

## 目的

`rust-toolchain.toml::channel = "stable"` は toolchain 更新のたびに新機能を取り込みうるため、Cargo.toml の `rust-version = "1.93"` より新しい機能を使ったコードがコミットされても発見できない。加えて CI の `rustup update stable` (ci.yml) と組み合わさると、CI 上の rust バージョンも変動し MSRV 検知の再現性が下がる。stable 固定を止めて MSRV 検知を CI に含める。

## 現状

- `rust-toolchain.toml`:
  ```toml
  [toolchain]
  channel = "stable"
  components = ["rustfmt", "clippy"]
  ```
- `.github/workflows/ci.yml::build::steps` の `rustup update stable` で最新 stable にする
- Cargo.toml の `rust-version = "1.93"` は宣言のみで、実際のビルドは常に最新 stable で行われる
- MSRV より新しい機能を使ったコードがコミットされても CI・ローカルとも検知不能

## 設計方針

- 案 A: `rust-toolchain.toml::channel` を `"1.93"` に固定する
  - devcontainer / CI / ローカル全てが同じ toolchain で走るため MSRV 遵守が保証される
  - 一方、stable の最新機能を使いたい場合は Cargo.toml の rust-version と合わせて明示更新する必要がある
- 案 B: `rust-toolchain.toml` はそのままにし、CI に MSRV 検証ステップを追加する
  - `.github/workflows/ci.yml` に `rustup install 1.93 && cargo +1.93 check --workspace` を追加
  - stable での最新機能開発と MSRV 検証の両方を回せる
- どちらの方針も一長一短。MSRV の位置付け（ビルドが通る最低バージョン vs. 実際にサポートするバージョン）で判断する
- shiguredo-rust 規約と併せて決定する

## 完了条件

- MSRV より新しい機能を使ったコードが CI で検知される
- 通常の開発・リリースフローが引き続き動作する
- shiguredo-rust 規約に沿った運用になる

## 変更対象

- `rust-toolchain.toml`（案 A）
- `.github/workflows/ci.yml`（案 B）
