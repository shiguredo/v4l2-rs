# 実機必須の統合テストを feature フラグでゲートして Raspberry Pi 以外で cargo test が panic するのを防ぐ

- Created: 2026-08-21
- Completed:
- Branch: feature/fix-gate-hardware-dependent-tests
- Polished: 2026-08-23

## 目的

`tests/test_converter.rs` の `test_converter_pipeline_all_mmap` / `test_converter_pipeline_mixed_dmabuf` は V4L2 M2M デバイス（`/dev/video10` / `/dev/video11` / `/dev/video12`）を必要とする実機テストだが、コンパイル対象から外す仕組みがない。`prek` は Linux aarch64 以外で cargo test をスキップするが、それ以外の aarch64 Linux 実行可能環境（QEMU / dev VM 等）で `cargo test` を直接叩くと `Device::open("/dev/video12")` が失敗し、テストの `.expect(...)` で panic する。開発者体験を毀損する上、CI 拡張時のトラブル源になるため修正する。

## 現状

- `tests/test_converter.rs` に 2 つの `#[test]` 関数（`test_converter_pipeline_all_mmap` / `test_converter_pipeline_mixed_dmabuf`）
- `prek.toml::cargo-test` は `sh -c 'case "$(uname -sm)" in Darwin*) ...; Linux aarch64) cargo test --workspace;; *) ...'` で macOS と非 aarch64 Linux をスキップするが、この分岐は Linux aarch64 の非 Raspberry Pi 環境（QEMU / dev VM）を素通しする
- 直接 `cargo test` を叩ける aarch64 Linux 環境では `Device::open("/dev/video12")` が失敗し、`tests/test_converter.rs` 内の `.expect("...の初期化に失敗しました")` で panic する（macOS 等では `.cargo/config.toml` の `build.target` によりクロスリンクエラーで panic の前に失敗するため、本 issue の対象は aarch64 Linux 実行可能環境に限定される）

## 設計方針

shiguredo-rust 規約は「`#[ignore]` を使わないこと」と定めているため、`#[ignore]` ではなく **feature フラグ**で実機必須テストをゲートする（`examples/whip` の `--features whip_client` と同様のパターン）。

- `Cargo.toml` に `[features] hw-tests = []` を追加する
- `tests/test_converter.rs` のファイル先頭に `#![cfg(feature = "hw-tests")]` を置き、2 つの `#[test]` とそのヘルパー（`run_pipeline_test` / `build_pipeline` 等）をまとめてコンパイル対象から外す（`#[test]` に個別に cfg を付けると、feature off 時にヘルパーが未使用になり clippy の dead_code 警告で失敗するため）
- 非 RPi では `cargo test --workspace`（features なし）で実機テストがコンパイルされず、panic しない
- RPi では `cargo test --workspace --features hw-tests` で実機テストを実行する
- `prek.toml::cargo-test` は素の `cargo test --workspace` のまま変更しない（非実機テストのみ実行する。実機テストは prek の対象外）
- `.github/workflows/ci.yml` の Raspberry Pi self-hosted runner に `cargo test --workspace --features hw-tests` を実行するステップを追加し、実機テストの継続検証手段を確保する。現状の verify ジョブは `actions/checkout` を持たないため、`actions/checkout` と Rust toolchain のセットアップ（`.cargo/config.toml` が参照する aarch64 sysroot の用意を含む）も併せて追加する
- `tests/test_format.rs` は実機不要のため、そのまま残す

## 完了条件

- 非 RPi の aarch64 Linux 実行可能環境（コンテナ / QEMU / dev VM）で `cargo test --workspace` が panic せず、非実機テストが全て pass する（実機テストはコンパイルされない）
- RPi 実機で `cargo test --workspace --features hw-tests` を実行すると従来通り 2 つのテストが走る
- `prek run cargo-test` が従来どおり非実機テストを実行する
- `cargo test --workspace` および `cargo clippy --workspace --all-targets -- -D warnings` が成功する

## 変更対象

- `Cargo.toml`（`[features] hw-tests` 追加）
- `tests/test_converter.rs`（ファイル先頭に `#![cfg(feature = "hw-tests")]` を追加）
- `.github/workflows/ci.yml`（Raspberry Pi self-hosted runner に `cargo test --workspace --features hw-tests` ステップと、checkout / Rust toolchain セットアップの追加）
