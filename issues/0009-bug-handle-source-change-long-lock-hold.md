# H264Decoder::handle_source_change が runtime lock を数十〜数百 ms 保持し decode を長時間ブロックする問題を修正する

- Created: 2026-08-21
- Completed:
- Branch: feature/fix-handle-source-change-long-lock-hold
- Polished:

## 目的

`src/decoder.rs::H264Decoder::handle_source_change` は `runtime.lock()` を取得したまま `ioctl_streamoff` / `ioctl_g_fmt` / `BufferSet::allocate`（REQBUFS + N 回の QUERYBUF + N 回の mmap または EXPBUF）/ `CaptureQueue::enqueue_all`（N 回の QBUF）/ `ioctl_streamon` を連続実行する。この間、`H264Decoder::decode` は完全にブロックされ、back-pressure の中核 API が数十〜数百 ms 単位で応答しなくなる問題を修正する。

## 現状

- `src/decoder.rs::H264Decoder::handle_source_change` はロック取得後、STREAMOFF → G_FMT → BufferSet 確保 → QBUF 全数 → STREAMON をロック内で実行
- BufferSet::allocate は REQBUFS + `capture_buffer_count` 回の QUERYBUF + `capture_buffer_count` 回の mmap または EXPBUF を発行するため、バッファ数 8〜12 の環境で ioctl だけで 20〜30 回相当
- ユーザーが `decode()` を呼ぶと `H264Decoder::lock_runtime` で待たされる（`decoder.rs::lock_runtime`）
- ライブストリーミング用途では back-pressure が数百 ms 単位で消えるため、上位のフレーム供給スケジューリングが崩れる

## 設計方針

- `handle_source_change` の ioctl 群をロック外で実施する構造に組み直す
- 状態遷移フェーズを 3 段階に分ける:
  1. 短時間ロック: 現在の状態を確認し、STREAMOFF 対象を決定する（capture_queue を退避）
  2. ロック外で長時間の ioctl 群を実行（STREAMOFF / G_FMT / BufferSet::allocate / enqueue_all / STREAMON）
  3. 短時間ロック: 新しい状態（capture_queue, resolution, capture_started）を書き込む
- ロック外実行中に `decode()` が呼ばれた場合、`capture_queue == None` かつ `capture_started == false` の中間状態で受理する挙動を設計する（現在も handle_source_change 中は capture が使えないのでユーザー影響は本質的に変わらない）
- 途中で失敗した場合の state ロールバックは `issues/0004-bug-streamon-partial-failure-state-corruption.md` の設計と協調する
- ロック外 ioctl 中に `RequeueToken::requeue` が古い CaptureQueue を触るリスクは `issues/0010-bug-requeue-source-change-race.md` の設計と協調する

## 完了条件

- SOURCE_CHANGE 処理中に `decode()` が数十 ms 以上ブロックされない
- SOURCE_CHANGE 処理そのものは既存と同等の順序で成功する
- `cargo test --workspace` および `cargo clippy --workspace --all-targets -- -D warnings` が成功する
- 実機で解像度変更を含むデコードシナリオが引き続き動作する

## 変更対象

- `src/decoder.rs`（`H264Decoder::handle_source_change`、`DecoderRuntime` の状態遷移設計）
- `CHANGES.md`（`[FIX]` として追加）
