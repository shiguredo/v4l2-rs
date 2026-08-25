# .markdownlint.jsonc に対応する実行フックを追加するか設定ファイルを削除する

- Created: 2026-08-21
- Completed:
- Branch: feature/add-markdownlint-hook
- Polished:

## 目的

`.markdownlint.jsonc` は MD004 / MD013 / MD024 / MD036 の設定を持つが、`prek.toml` にも `.github/workflows/ci.yml` にも markdown lint 実行フックが無く、事実上機能していない。README.md / CHANGES.md / docs/*.md / SKILL.md の書式規約が unenforced な状態を修正する。

## 現状

- `.markdownlint.jsonc` に MD004 (list marker `dash`)、MD013 (line_length 200, code_blocks false)、MD024 (siblings_only true)、MD036 (false) の設定
- `prek.toml::[[repos]]` に markdown lint フック無し
- `.github/workflows/ci.yml` にも markdown lint ステップ無し
- 設定ファイルだけ置いてあるが誰も実行しない状態

## 設計方針

以下の 2 案から選択する:

- 案 A: `markdownlint-cli2` フックを prek に追加する
  - `prek.toml::[[repos]]` に `local` フックとして `markdownlint-cli2` を追加
  - `types: [markdown]` で `.md` ファイル全てを対象にする
  - CI 側にも markdown lint ステップを追加するかは判断
- 案 B: `.markdownlint.jsonc` を削除する
  - 使わないなら設定ファイルの存在自体がノイズなので削除する

案 A が推奨（設定ファイルを残す価値がある）。実装時に方針を確定させる。

## 完了条件

- 案 A: `prek run markdownlint` あるいは同等コマンドで markdown lint が走る
- 案 B: `.markdownlint.jsonc` が削除される
- README.md / CHANGES.md / docs/*.md / SKILL.md が lint 対象になる（案 A）またはならない（案 B）

## 変更対象

- 案 A: `prek.toml`
- 案 A: 必要に応じて `.github/workflows/ci.yml`
- 案 B: `.markdownlint.jsonc` の削除
