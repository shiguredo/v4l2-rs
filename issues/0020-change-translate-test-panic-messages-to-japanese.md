# tests/test_converter.rs の英語 panic メッセージを日本語に統一する

- Created: 2026-08-21
- Completed:
- Branch: feature/change-translate-test-panic-messages-to-japanese
- Polished: 2026-08-23

## 目的

`tests/test_converter.rs` の flush timeout の `panic!` メッセージ 2 箇所が英語で書かれており、AGENTS.md「テストのログメッセージは全て日本語にすること」に違反する。同ファイル内の他の panic はすべて日本語（`"pipeline callback エラー: {err}"` / `"pipeline callback channel が切断されました"` 等）なので、書き漏れによる不整合を修正する。

## 現状

- `tests/test_converter.rs` の `run_pipeline_test` 内、`while restored.len() < FRAME_COUNT` ループで:
  - `now > deadline` 分岐の `panic!("flush timeout: restored={}, expected={}", restored.len(), FRAME_COUNT)`
  - `mpsc::RecvTimeoutError::Timeout` 分岐の `panic!("flush timeout: restored={}, expected={}", restored.len(), FRAME_COUNT)`
- 上記 2 箇所だけが英文メッセージ
- 同じファイル内の他の panic は日本語文（例: `panic!("pipeline callback エラー: {err}")` / `panic!("pipeline callback channel が切断されました")`）だが、日本語文に英語の一般名詞（"callback" / "channel" 等）が埋め込まれている箇所がある

## 設計方針

- 2 箇所の英語 panic を日本語に置き換える
  - 例: `panic!("フラッシュがタイムアウトしました：復元 = {}, 期待値 = {}", restored.len(), FRAME_COUNT)`（AGENTS.md の「全角と半角の間には半角スペースを入れること」に沿った形。`{restored.len()}` のような式はインライン書式で書けないため、位置引数で渡す）
- 日本語文中の英語の一般名詞はカタカナに統一する: "pipeline" →「パイプライン」、「callback」→「コールバック」、「channel」→「チャネル」（完了条件「panic メッセージが全て日本語」を満たすため）
- 日本語化の対象は本 issue で修正する panic メッセージのみ（英語 panic 2 箇所と pipeline / callback / channel を含むメッセージ）。`assert!` メッセージや `{err}` に埋め込まれるエラー文言（"frame" / "mae" / "threshold" 等）は本 issue の対象外（既存のまま）
- MMAP / DMABUF は V4L2 のメモリタイプの識別子であり、英語のままとする（カタカナ化しない）
- 半角・全角スペース規約（AGENTS.md）は、本 issue で修正する panic メッセージに限定して適用する（既存の他の panic メッセージは本 issue の対象外）
- `issues/0019-bug-gate-hardware-dependent-tests.md` との関係: 変更箇所（`run_pipeline_test` 内の panic）は 0019 が `#![cfg(feature = "hw-tests")]` でコンパイル対象から外す範囲に含まれる。実装順序はどちらが先でもよい（変更箇所が重ならない）。0019 適用後は実機テストの検証に `cargo test --workspace --features hw-tests` を要する

## 完了条件

- `tests/test_converter.rs` の本 issue で修正する panic メッセージが全て日本語（英語の一般名詞 "pipeline" / "callback" / "channel" が残らない。MMAP / DMABUF 等の技術用語の識別子と `{err}` に埋め込まれるエラー文言は除く）
- 0019 適用後は `cargo test --workspace --features hw-tests` で test_converter.rs の変更を検証できる（0019 適用前は V4L2 M2M デバイスが存在する実機環境、例えば Raspberry Pi で `cargo test --workspace`）
- `cargo clippy --workspace --all-targets -- -D warnings` が成功する

## 変更対象

- `tests/test_converter.rs`
