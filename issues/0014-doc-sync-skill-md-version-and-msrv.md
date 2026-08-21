# skills/shiguredo-v4l2/SKILL.md のバージョンと MSRV を実装と同期する

- Created: 2026-08-21
- Completed:
- Branch: feature/update-skill-md-version-and-msrv
- Polished:

## 目的

`skills/shiguredo-v4l2/SKILL.md` の「バージョン: 2026.1.0 / 最小 Rust バージョン: 1.88」が Cargo.toml の実態（`version = "2026.2.0-canary.2"` / `rust-version = "1.93"`）と食い違い、CHANGES.md の `[CHANGE] MSRV (rust-version) を 1.93 に上げる` にも追従していない。ユーザーが `rustc 1.88` で始めて突然ビルド失敗する誤誘導になるため同期する。

## 現状

- `skills/shiguredo-v4l2/SKILL.md` の「バージョン情報」節:
  - `- バージョン: 2026.1.0`
  - `- 最小 Rust バージョン: 1.88`
- `Cargo.toml`:
  - `version = "2026.2.0-canary.2"`
  - `rust-version = "1.93"`
- `CHANGES.md` `## develop` の `[CHANGE] MSRV (rust-version) を 1.93 に上げる` が既に反映済み

## 設計方針

- SKILL.md のバージョン情報節を Cargo.toml の値と揃える
- 「バージョン」は canary サフィックスを含めるか、あるいは「開発中 (次期: 2026.2.0)」のような表現にするかを実装時に判断する（SKILL.md はスキルの利用者が読む前提のため）
- MSRV は Cargo.toml の値と一致させる（`1.93`）
- 今後 canary 更新のたびに手動同期が必要になるため、`canary.py` から自動更新する余地も別途 issue 化検討する（本 issue のスコープ外）

## 完了条件

- SKILL.md のバージョン情報が Cargo.toml と一致する
- ユーザーが SKILL.md を見て `rustc 1.88` を使い、ビルド失敗するミスリードが無くなる

## 変更対象

- `skills/shiguredo-v4l2/SKILL.md`（バージョン情報節）
