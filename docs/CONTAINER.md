# Container

macOS 上で Apple Container (`container` CLI) を使い、CI と同等の clippy 検証をローカルで実行する方法です。

## 動作確認環境

- macOS 26.5.1
- container CLI 1.0.0 (Homebrew)

## 準備

Homebrew で container をインストールし、サービスを起動します。

```bash
brew install container
brew services start container
```

## 検証用イメージのビルド

```bash
make build-container
```

内部では `container build -t v4l2-ci-check -f Dockerfile.check .` を実行します。
イメージにはソースコードを焼き込まず、aarch64 用 sysroot とクロスツールチェーンのみが含まれます。
ソースを編集してもイメージの再ビルドは不要で、実行時にホストの作業ツリーをマウントして使います。

## clippy の実行

```bash
container run --rm -v "$(pwd):/workspace" -w /workspace v4l2-ci-check
```

デフォルトで `cargo clippy --workspace --target aarch64-unknown-linux-gnu -- -D warnings` が実行されます。

## その他のコマンドを実行する

```bash
container run --rm -v "$(pwd):/workspace" -w /workspace v4l2-ci-check cargo check --workspace --target aarch64-unknown-linux-gnu
container run --rm -v "$(pwd):/workspace" -w /workspace v4l2-ci-check cargo test --workspace --target aarch64-unknown-linux-gnu
```

## prek フックからの利用

`prek.toml` の `cargo-clippy` フックは `command -v container` で判定し、container CLI があれば
このイメージ経由で aarch64 向け clippy を実行し、無ければホストでそのまま clippy を実行します。
イメージのビルド / 再ビルドは `make build-container` で行います。

## 注意事項

- `cargo test` は `/dev/video12` などの V4L2 デバイスを必要とするテストを除いて実行可能です
- `brew services start container` を実行しないと `container run` が失敗することがあります
- コンテナ側のビルド成果物はホストの `target/container/` に出力されます
  （`CARGO_TARGET_DIR=/workspace/target/container`）。
  ホストの `cargo` が使う `target/{debug,release,aarch64-unknown-linux-gnu}` 等とは分離されます
