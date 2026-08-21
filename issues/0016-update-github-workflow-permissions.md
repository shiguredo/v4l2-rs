# .github/workflows/ci.yml と release.yml に permissions を明示する

- Created: 2026-08-21
- Completed:
- Branch: feature/update-github-workflow-permissions
- Polished:

## 目的

`.github/workflows/ci.yml` 全体と `.github/workflows/release.yml` の `github-release` ジョブに `permissions:` ブロックが無く、`GITHUB_TOKEN` にリポジトリのデフォルト権限が付与される。最小権限原則違反であり、リポジトリ設定変更で壊れる可能性もあるため明示する。

## 現状

- `.github/workflows/ci.yml` 全体で `permissions:` の指定なし
- `.github/workflows/release.yml`:
  - `github-release` ジョブに `permissions:` なし（`gh release create` は `contents: write` を必要とする）
  - `publish` ジョブは `permissions: id-token: write` のみ明示（`contents: read` を明示していないため、publish の checkout は他ジョブに依存）
  - `slack_notify` ジョブは `permissions: actions: read, contents: read` を明示
- デフォルト権限がリポジトリ設定変更で `restricted` に切り替わると全ワークフローが壊れる

## 設計方針

- `.github/workflows/ci.yml`:
  - ワークフロー全体または各ジョブに `permissions: contents: read` を明示する（artifact upload は `actions: write` / `attestations: write` などが必要な場合は追加検討）
- `.github/workflows/release.yml`:
  - `github-release` ジョブに `permissions: contents: write` を明示（`gh release create` の要求）
  - `publish` ジョブに `permissions: contents: read, id-token: write` を明示
  - `slack_notify` ジョブは現状のまま
- `shiguredo-github-actions` スキルを参照して規約に沿った書き方に統一する

## 完了条件

- 全ワークフローの各ジョブに必要最小限の `permissions:` が明示される
- CI / リリースフローが引き続き正常に動作する

## 変更対象

- `.github/workflows/ci.yml`
- `.github/workflows/release.yml`
