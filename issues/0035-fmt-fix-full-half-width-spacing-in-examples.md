# examples/v4l2_m2m_libcamera_encode の全角と半角の間の半角スペース欠落を修正する

- Created: 2026-08-21
- Completed:
- Branch: feature/fix-full-half-width-spacing-in-examples
- Polished:

## 目的

`examples/v4l2_m2m_libcamera_encode/src/main.rs` の `println!` メッセージ 2 箇所で `{}` (半角) と `秒` / `秒間` (全角) の間に半角スペースが無く、AGENTS.md「全角と半角の間には半角スペースを入れること」規約に違反する。同ファイル内で他は正しく空けているのに書き漏れによる不整合を修正する。

## 現状

- `examples/v4l2_m2m_libcamera_encode/src/main.rs` の `fn main` 冒頭:
  - `"設定: {}x{}, {}kbps, {}秒, 出力: {}"` （`{}秒` の間にスペース無し）
- `camera.start` 直後:
  - `println!("キャプチャ開始 ({}秒間) ...", args.duration)` （`{}秒間` の間にスペース無し）
- 同ファイル内 `"エンコード完了: {frame_count} フレーム"` は `} フレーム` と正しく空けている
- 他の example ファイル（`v4l2_m2m_check` / `v4l2_m2m_encode`）にも同種の違反がないか要確認

## 設計方針

- 該当 2 箇所を `{} 秒` / `{} 秒間` に修正する
- 加えて 3 examples を通しで grep して全角半角違反を洗い出す:
  - `grep -nP '\{[^}]*\}[^\s\p{Latin}]' examples/*/src/main.rs`
  - `grep -nP '[^\s\p{Latin}]\{[^}]*\}' examples/*/src/main.rs`
- src / tests / CHANGES.md / SKILL.md / docs も同様の grep で違反がないか確認するかは本 issue のスコープ判断（3 examples に絞るのが妥当）

## 完了条件

- `examples/v4l2_m2m_libcamera_encode/src/main.rs` の該当 2 箇所が修正される
- 3 examples を通しで全角半角違反が無い
- 実行時の出力メッセージが AGENTS.md 規約通り

## 変更対象

- `examples/v4l2_m2m_libcamera_encode/src/main.rs`
- 必要に応じて他 examples
