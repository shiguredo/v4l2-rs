# canary.py の cargo update no-op と (Y/n) UX の不整合を修正する

- Created: 2026-08-21
- Completed:
- Branch: feature/fix-canary-py-issues
- Polished:

## 目的

`canary.py` は canary リリースのバージョン更新スクリプトだが、以下の 2 つの問題がある。

1. `cargo update shiguredo_v4l2` は workspace root package に対して no-op（path 依存で自身を更新できない）
2. UX 表示は `(Y/n)`（デフォルト Yes）だが、実装は `if confirmation != "y"` で空 Enter を全てキャンセルにする

これらを修正する。

## 現状

- `canary.py::run_cargo_update` は `subprocess.run(["cargo", "update", "shiguredo_v4l2"], check=True)` を実行
  - `shiguredo_v4l2` は workspace root package。`cargo update <pkg>` は依存 crate 用で、workspace root には `P is set as dependency by path, would not update` の警告が出るだけで実質 no-op
- `canary.py::update_version` の確認プロンプト:
  - 表示: `input("Do you want to update the version? (Y/n): ")`（(Y/n) はデフォルト Yes 表記）
  - 実装: `if confirmation != "y": print("Version update canceled.")` （空 Enter は `""`、大文字 `Y` は `.lower()` で `y` になるので OK だが、それ以外は全てキャンセル）

## 設計方針

- `run_cargo_update` を `cargo update --workspace` あるいは `cargo update` に変更する（workspace 全体のロック更新が意図なら `-w`）
  - 意図が「Cargo.lock の canary バージョンを反映する」ことなら、`cargo update -p shiguredo_v4l2 --precise <new_version>` の形も検討
  - 実装時に canary スクリプトの意図を確認する（`--dry-run` 挙動から推測すると単に Cargo.lock を更新したい）
- 確認プロンプトを以下に修正:
  ```python
  confirmation = input("Do you want to update the version? (Y/n): ").strip().lower()
  if confirmation not in ("", "y", "yes"):
      print("Version update canceled.")
      return None
  ```
- あるいは `(y/N)` に表示を変えて空 Enter をキャンセル扱いにする方針もあり得るが、canary バージョン更新は通常 yes が期待値なのでデフォルト Yes が自然
- PEP 723 対応（uv 経由の inline metadata）は shiguredo-python 規約の推奨であり、別 issue で検討する（本 issue のスコープ外）

## 完了条件

- `run_cargo_update` が実質的な no-op でなくなる（Cargo.lock の canary バージョンが正しく反映される）
- 確認プロンプトで空 Enter が Yes として扱われる
- 通常の canary リリース手順（`--dry-run` および実行）が引き続き正常に動く

## 変更対象

- `canary.py`
