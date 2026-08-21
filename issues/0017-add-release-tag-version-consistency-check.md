# release.yml でタグと Cargo.toml のバージョン一致検証を追加する

- Created: 2026-08-21
- Completed:
- Branch: feature/add-release-tag-version-consistency-check
- Polished:

## 目的

`.github/workflows/release.yml` の `publish` ジョブは `git tag <version>` を push したときに `cargo publish` を実行するが、タグ名と Cargo.toml の `version` が一致するかを検証していない。異なるバージョンで crates.io に誤って publish される事故を防ぐガードを追加する。加えて `cargo publish` に `--locked` / `--dry-run` の運用を検討する。

## 現状

- `.github/workflows/release.yml::publish` は以下だけ実行:
  ```yaml
  - uses: actions/checkout@...
  - uses: rust-lang/crates-io-auth-action@...
    id: auth
  - run: cargo publish
    env:
      CARGO_REGISTRY_TOKEN: ...
  ```
- `github-release` ジョブでタグ名を `steps.get_version.outputs.VERSION` として取得しているが、`publish` は参照していない
- `cargo publish` に `--locked` が無く、Cargo.lock を尊重しない
- `cargo publish` に `--dry-run` が事前検証されていないため、失敗しても既にタグと GitHub Release は作成済み

## 設計方針

- `publish` ジョブに以下のステップを追加する:
  1. Cargo.toml から `version` を抽出（`grep '^version = ' Cargo.toml | sed ...`）
  2. `${{ needs.github-release.outputs.VERSION }}` と一致するか比較し、不一致なら fail
- `cargo publish --locked` に変更する
- `github-release` の前段に `cargo publish --dry-run --locked` を挟む案も検討する（タグ push 前に事前検証）
- `shiguredo-github-actions` 規約を参照して書き方を統一する

## 完了条件

- タグと Cargo.toml のバージョンが不一致な状態で `cargo publish` が実行されない
- `cargo publish` が `--locked` で走り、Cargo.lock を尊重する
- 通常のリリースフローが引き続き正常に動作する

## 変更対象

- `.github/workflows/release.yml`
