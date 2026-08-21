# .github/workflows/ci.yml の paths フィルタに tests / Cargo.lock / rust-toolchain.toml 等を追加する

- Created: 2026-08-21
- Completed:
- Branch: feature/update-ci-paths-filter-coverage
- Polished:

## 目的

`.github/workflows/ci.yml` の `paths` フィルタが `src/**`, `examples/**`, `pbt/**`, `sysroot/**`, `Cargo.toml`, `.github/workflows/ci.yml` の 6 パターンのみで、`tests/**` / `Cargo.lock` / `rust-toolchain.toml` / `fuzz/**` / `Dockerfile.check` / `prek.toml` / `Makefile` などの変更で CI が起動しない。統合テストの追加や toolchain / lock file の更新で CI ゲートが素通りする穴を塞ぐ。

## 現状

- `.github/workflows/ci.yml:5-11` の `paths`:
  - `src/**`
  - `examples/**`
  - `pbt/**`
  - `sysroot/**`
  - `Cargo.toml`
  - `.github/workflows/ci.yml`
- 抜けているパス:
  - `tests/**`（統合テストの変更で CI が走らない）
  - `Cargo.lock`（依存ロックの更新が検証されない）
  - `rust-toolchain.toml`（toolchain channel 変更が検証されない）
  - `fuzz/**`（fuzz ターゲット変更が検証されない。ただし CI で fuzzing 自体は走っていないため優先度低）
  - `Dockerfile.check` / `prek.toml`（container 経由の clippy に影響）
  - `.github/workflows/release.yml`（release ワークフローの変更が ci で検証されない）

## 設計方針

- 必要最小限を追加する:
  - `tests/**`
  - `Cargo.lock`
  - `rust-toolchain.toml`
- 検討余地:
  - `.github/workflows/**` の glob に置き換えて release.yml もカバーする
  - `Dockerfile.check` / `prek.toml`（container ビルドの検証をどこで走らせるか次第）
- `paths` フィルタの代わりに `paths-ignore` に切り替える案も検討する（`docs/**` / `.markdownlint.jsonc` / `README.md` などを ignore する形）
- どちらの方針も CI 実行回数と検証範囲のトレードオフがあるため、実装時に方針を確定させる

## 完了条件

- `tests/**` / `Cargo.lock` / `rust-toolchain.toml` の変更で CI が起動する
- 現状トリガーされる変更（`src/**` 等）が引き続き起動する
- CI 実行回数が過剰に増えない

## 変更対象

- `.github/workflows/ci.yml`（`paths` フィルタの拡張または `paths-ignore` への置き換え）
