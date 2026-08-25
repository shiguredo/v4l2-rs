# H264Decoder::handle_source_change が runtime lock を数十〜数百 ms 保持し decode を長時間ブロックする問題を修正する

- Created: 2026-08-21
- Completed:
- Branch: feature/fix-handle-source-change-long-lock-hold
- Polished: 2026-08-23

## 目的

`src/decoder.rs::H264Decoder::handle_source_change` は `runtime.lock()` を取得したまま `ioctl_streamoff` / `ioctl_g_fmt` / `BufferSet::allocate`（REQBUFS + N 回の QUERYBUF + N 回の mmap または EXPBUF）/ `CaptureQueue::enqueue_all`（N 回の QBUF）/ `ioctl_streamon` を連続実行する。この間、`H264Decoder::decode` は完全にブロックされ、back-pressure の中核 API が長時間（ioctl だけで 2N+4 回、N=8 で 20 回。MMAP 出力時。DMABUF 出力時は EXPBUF が加わり 3N+4 回。実機依存で数十 ms 以上になり得る。実測は要計測）応答しなくなる問題を修正する。

## 現状

- `src/decoder.rs::H264Decoder::handle_source_change` はロック取得後、STREAMOFF → G_FMT → BufferSet 確保 → QBUF 全数 → STREAMON をロック内で実行
- `BufferSet::allocate` は REQBUFS + N 回の QUERYBUF + N 回の mmap または EXPBUF を発行する（N は確保されたバッファ数）。これに STREAMOFF / G_FMT / 全バッファの QBUF / STREAMON を加えると ioctl だけで 2N+4 回（MMAP 出力時。DMABUF 出力時は EXPBUF が加わり 3N+4 回。N=8 で 20 / 28 回）に達する
- ユーザーが `decode()` を呼ぶと `H264Decoder::lock_runtime` で待たされる（`decoder.rs::lock_runtime`）
- ライブストリーミング用途では back-pressure が長時間消えるため、上位のフレーム供給スケジューリングが崩れる

## 設計方針

`handle_source_change` を 3 フェーズに分割し、長時間の ioctl 群（STREAMOFF / G_FMT / REQBUFS / QUERYBUF / mmap / QBUF / STREAMON）をロック外で実行する。`decode()` は短時間ロック（フェーズ 1・3）でのみブロックされる。

1. フェーズ 1（短時間ロック）: `capture_started` を確認して STREAMOFF 要否を決定する。古い `capture_queue` の参照をローカル変数に退避したうえで `runtime.capture_queue = None` に書き換える（ローカル変数が参照を保持するため、この時点で `Arc` は解放されない）。STREAMOFF を発行する場合は `runtime.capture_started = false` にも書き換える（カーネル側の STREAMOFF はフェーズ 2 で発行されるため一時的にフラグとカーネル状態がずれるが、フェーズ 2 失敗時は false のままで整合し、フェーズ 3 成功時に true に戻して、フラグとカーネル側の STREAMON 状態を一致させる）
2. フェーズ 2（ロック外）:
   - STREAMOFF（CAPTURE）を発行して queued バッファを回収する（フェーズ 1 で必要と判断した場合のみ）
   - 退避した古い `capture_queue` を明示的に drop する（`BufferSet::drop` の自動 `REQBUFS(count=0)` は 0010 で廃止するため依存せず、STREAMOFF をバッファ解放に先行させてから明示的に `ioctl_reqbufs(count=0)` を発行する）
   - G_FMT で新解像度を取得 → `BufferSet::allocate` で新 CAPTURE バッファを確保 → `CaptureQueue::enqueue_all` で全 QBUF → `ioctl_streamon`（CAPTURE）
   - 失敗時: STREAMOFF を発行して queued バッファを回収し、確保した新バッファを `ioctl_reqbufs(count=0)` で解放する
3. フェーズ 3（短時間ロック）: STREAMON 成功後にのみ `runtime.capture_queue = Some(new)` / `runtime.capture_started = true` / `runtime.resolution = Some(resolution)` を書き込む（STREAMON 成功後にのみ状態を更新する）

### 中間状態（フェーズ 2 実行中）での各経路の挙動

- `decode()`: `capture_queue` を参照しないため、フェーズ 2 中も OUTPUT キューへのエンキューを続行する。ただし poller が `handle_source_change` 実行中は `OutputDequeued` を処理しないため、`output_queue` の利用可能バッファが枯渇していれば `Error::NoAvailableBuffer` を返す（従来はブロックしていたが、新設計ではブロックせず即時エラーになる）
- poller の `handle_capture`: `handle_source_change` は poller スレッド上で直列実行されるため、フェーズ 2 中に `handle_capture` が走ることはなく、`capture_queue == None` を観測して `Error::NotStarted` を配送する経路は発生しない
- `RequeueToken::requeue`: フェーズ 1 で `runtime.capture_queue` が `None` になるため、`issues/0010-bug-requeue-source-change-race.md` の「requeue が active queue か確認する」設計により古いキューへの QBUF はスキップされる（0010 を本 issue の前提として参照する。実装順序は 0010 を先に完了させる）
- `H264Decoder::drop`: `poller.stop()` が poller スレッドの join でフェーズ 2 の完了を待つため、ioctl 実行中に fd が close されることはない。`capture_started` が false の間は CAPTURE STREAMOFF を発行しない

## 完了条件

- `decode()` が `handle_source_change` の長時間 ioctl 実行（フェーズ 2）中にブロックされない（短時間ロックでのみブロックされる）
- SOURCE_CHANGE 処理そのものは既存と同等の順序で成功する（STREAMOFF → 旧バッファ解放 → G_FMT → 新バッファ確保 → QBUF → STREAMON）
- `cargo test --workspace` および `cargo clippy --workspace --all-targets -- -D warnings` が成功する
- 実機で解像度変更を含むデコードシナリオが引き続き動作する

## 変更対象

- `src/decoder.rs`（`H264Decoder::handle_source_change`、`DecoderRuntime` の状態遷移設計）
- `CHANGES.md`（`[FIX]` として追加）
