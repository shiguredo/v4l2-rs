# prek.toml / Dockerfile.check / docs/CONTAINER.md の CARGO_TARGET_DIR を統一する

- Created: 2026-08-21
- Completed:
- Branch: feature/update-unify-cargo-target-dir
- Polished:

## 目的

`prek.toml` の cargo-clippy フック、`Dockerfile.check` の環境変数、`docs/CONTAINER.md` の説明で `CARGO_TARGET_DIR` の値が三者三様になっており、prek 経由でコンテナ clippy を実行するたびにビルドキャッシュが破棄されて再ビルドが激遅化する。3 者を統一する。

## 現状

- `prek.toml::cargo-clippy` のエントリで `-e CARGO_TARGET_DIR=/tmp/cargo-target` を container run に渡している
- `Dockerfile.check` は `ENV CARGO_TARGET_DIR=/workspace/target/container` を設定
- `docs/CONTAINER.md` は「ホストの `target/container/` に成果物が出る」と説明
- prek 経由の実行では `-e CARGO_TARGET_DIR=/tmp/cargo-target` が優先されるため、コンテナ終了と同時にビルドキャッシュが消滅し次回実行が全再ビルドになる
- Dockerfile.check の ENV 設定は上書きされて意味を持たない
- ドキュメントに書いた「ホストの target/container に成果物が出る」実装と食い違う

## 設計方針

- 3 者を `target/container` に統一する:
  - `prek.toml::cargo-clippy` の `-e CARGO_TARGET_DIR=/workspace/target/container` に変更（ホストの `target/container` に bind mount 経由で書き込む）
  - `Dockerfile.check` はそのまま
  - `docs/CONTAINER.md` はそのまま
- あるいは高速化目的で `/tmp` を使う判断が正しいなら、Dockerfile.check の ENV を削除し、CONTAINER.md にその方針を明記する

## 完了条件

- prek 経由の container clippy がホストの `target/container` にキャッシュを残し、2 回目以降のビルドが高速化する
- 3 者の記述が一致する
- 通常運用（macOS でのローカル clippy、Linux での直接 clippy）に影響が無い

## 変更対象

- `prek.toml`
- 必要に応じて `Dockerfile.check`
- 必要に応じて `docs/CONTAINER.md`
