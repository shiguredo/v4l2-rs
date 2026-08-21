# tests/test_converter.rs の英語 panic メッセージを日本語に統一する

- Created: 2026-08-21
- Completed:
- Branch: feature/fix-translate-test-panic-messages-to-japanese
- Polished:

## 目的

`tests/test_converter.rs` の flush timeout の `panic!` メッセージ 2 箇所が英語で書かれており、AGENTS.md「テストのログメッセージは全て日本語にすること」に違反する。同ファイル内の他の panic はすべて日本語（`"pipeline callback エラー: {err}"` / `"pipeline callback channel が切断されました"` 等）なので、書き漏れによる不整合を修正する。

## 現状

- `tests/test_converter.rs` の `run_pipeline` 内、`while restored.len() < FRAME_COUNT` ループで:
  - `now > deadline` 分岐の `panic!("flush timeout: restored={}, expected={}", restored.len(), FRAME_COUNT)`
  - `mpsc::RecvTimeoutError::Timeout` 分岐の `panic!("flush timeout: restored={}, expected={}", restored.len(), FRAME_COUNT)`
- 上記 2 箇所だけ英語
- 同じファイル内で日本語 panic は複数箇所（例: `panic!("pipeline callback エラー: {err}")` / `panic!("pipeline callback channel が切断されました")`）が既存

## 設計方針

- 2 箇所の英語 panic を日本語に置き換える
  - 例: `panic!("フラッシュがタイムアウトしました: 復元={}, 期待値={}", restored.len(), FRAME_COUNT)`
- 同時に `pipeline callback channel が切断されました` の "channel" は日本語規約上「チャネル」に統一するか検討する（このファイル内で他に「channel」表記があるかを確認して統一）
- 半角・全角スペース規約（AGENTS.md）も同時にチェックする

## 完了条件

- `tests/test_converter.rs` の panic メッセージが全て日本語
- 実機テスト実行時に panic メッセージが AGENTS.md 規約通り

## 変更対象

- `tests/test_converter.rs`
