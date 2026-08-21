# 実機必須の統合テストに #[ignore] を付けて Raspberry Pi 以外で cargo test が panic するのを防ぐ

- Created: 2026-08-21
- Completed:
- Branch: feature/fix-add-ignore-to-hardware-dependent-tests
- Polished:

## 目的

`tests/test_converter.rs` の `test_converter_pipeline_all_mmap` / `test_converter_pipeline_mixed_dmabuf` は V4L2 M2M デバイス（`/dev/video10` / `/dev/video11` / `/dev/video12`）を必要とする実機テストだが、`#[ignore]` が付いていない。`prek` は Linux aarch64 以外で cargo test をスキップするが、それ以外の環境で `cargo test` を直接叩くと `Device::open("/dev/video12")` の `.expect(...)` で panic する。開発者体験を毀損する上、CI 拡張時のトラブル源になるため修正する。

## 現状

- `tests/test_converter.rs` に 2 つの `#[test]` 関数（`test_converter_pipeline_all_mmap` / `test_converter_pipeline_mixed_dmabuf`）
- どちらも `#[ignore]` 未指定
- `prek.toml::cargo-test` は `sh -c 'case "$(uname -sm)" in Darwin*) ...; Linux aarch64) cargo test --workspace;; *) ...'` で macOS と非 aarch64 Linux をスキップするが、この分岐は Linux aarch64 の非 Raspberry Pi 環境（QEMU / dev VM）を素通しする
- 直接 `cargo test` を叩ける環境では `Device::open("/dev/video12")` が失敗し `.expect("ImageConverter の作成に失敗しました")` で panic する

## 設計方針

- `#[ignore = "requires bcm2835-codec (Raspberry Pi)"]` を 2 つの `#[test]` に付与する
- 実機テストは `cargo test -- --ignored` で明示的に走らせる運用にする
- `prek.toml::cargo-test` を `cargo test --workspace -- --ignored --include-ignored` に変更するか、`--ignored` フラグを渡す構成にする
- CI 側の Raspberry Pi self-hosted runner でも `cargo test` を回すかを検討する（現状は example バイナリ実行のみ）
- 加えて `tests/test_format.rs` は実機不要だが、そのまま残す

## 完了条件

- Raspberry Pi 以外で `cargo test --workspace` が panic せず全テストが pass する
- Raspberry Pi 実機で `cargo test --workspace -- --ignored` を実行すると従来通り 2 つのテストが走る
- `prek run cargo-test` が意図した環境で意図した組で走る

## 変更対象

- `tests/test_converter.rs`（`#[ignore]` 追加）
- `prek.toml`（cargo-test フック）
- 必要に応じて `.github/workflows/ci.yml`（`cargo test -- --ignored` ステップ追加検討）
