# Dockerfile.check の apt install に --no-install-recommends と apt lists クリアを追加する

- Created: 2026-08-21
- Completed:
- Branch: feature/update-dockerfile-check-apt-hygiene
- Polished:

## 目的

`Dockerfile.check` の apt install ステップは `--no-install-recommends` と `rm -rf /var/lib/apt/lists/*` を含まず、`.devcontainer/Dockerfile` と流儀が食い違っている。CI 用イメージが不要に大きくなり、apt キャッシュ由来の再現性劣化リスクもあるため、`.devcontainer/Dockerfile` に合わせる。

## 現状

- `.devcontainer/Dockerfile` の apt install は `--no-install-recommends` を付け、末尾で `rm -rf /var/lib/apt/lists/*` を実行
- `Dockerfile.check` の apt install はどちらも無し
- 結果、`Dockerfile.check` からビルドされる CI 用イメージ (`v4l2-ci-check`) が不要に大きい
- 同一プロジェクト内で apt 使用方針が食い違うのは可読性・保守性の面でも問題

## 設計方針

- `Dockerfile.check` の apt install を以下の形に修正する:
  ```dockerfile
  RUN apt-get update && apt-get install -y --no-install-recommends \
      <packages...> \
      && rm -rf /var/lib/apt/lists/*
  ```
- `.devcontainer/Dockerfile` と統一するため、パッケージリストと install オプションの差分を精査する
- 依存パッケージが `--no-install-recommends` で不足しないか、build ステップで確認する

## 完了条件

- `Dockerfile.check` の apt install に `--no-install-recommends` が付く
- `rm -rf /var/lib/apt/lists/*` が末尾に入る
- `make container-build` で `v4l2-ci-check` イメージがビルドできる
- `prek run cargo-clippy` が引き続き成功する

## 変更対象

- `Dockerfile.check`
