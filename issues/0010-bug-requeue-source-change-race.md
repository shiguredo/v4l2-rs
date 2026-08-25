# RequeueToken::requeue と handle_source_change の間の race で stale な QBUF / REQBUFS が新規バッファ再確保を壊す問題を修正する

- Created: 2026-08-21
- Completed:
- Branch: feature/fix-requeue-source-change-race
- Polished: 2026-08-23

## 目的

`src/decoder.rs::H264Decoder::handle_source_change` は STREAMOFF → `runtime.capture_queue = None` → 新規 REQBUFS の順で処理を進めるが、この途中でユーザーが古い `DecodedFrame`（古い `Arc<CaptureQueue>` を保持）を drop すると、`RequeueToken::requeue` は runtime ロックを取らずに古い `CaptureQueue::enqueue(index)` → `ioctl_qbuf(fd)` を呼ぶ。この stale な QBUF が新規 REQBUFS(count>0) 成功後に挟まると、同じ index の新規バッファが QUEUED 状態になり、`enqueue_all` の該当 QBUF が EINVAL で失敗する。さらに、requeue が `self` を消費する際に最後の `Arc<CaptureQueue>` 参照なら `BufferSet::drop` の `REQBUFS(count=0)` が発行され、同じ fd / buf_type の新規バッファを解放してしまう。これらの race で SOURCE_CHANGE が失敗する経路を修正する。

## 現状

- `src/decoder.rs::RequeueToken::requeue` は `capture_queue.enqueue(self.index)` を無ロックで呼ぶ
- `src/queue.rs::CaptureQueue::enqueue` は `sys::ioctl_qbuf(self.fd, ...)` を発行する
- `H264Decoder::handle_source_change` は runtime ロック内で `runtime.capture_queue = None`（Arc drop、他 Arc 保持者がいれば実体は残る）を実行後、新規 `BufferSet::allocate` で REQBUFS を発行する
- 古い CaptureQueue はまだ `Arc<CaptureQueue>` として RequeueToken から到達可能なため、drop されずに残る
- タイミング次第で「STREAMOFF 済み → 古い CaptureQueue に QBUF → 新規 REQBUFS(count>0) → enqueue_all」の順が発生し、stale な QBUF が新規 REQBUFS 後に新規バッファを QUEUED 状態にして、`enqueue_all` の該当 QBUF が EINVAL で失敗する（Linux vb2 は QUEUED 状態のバッファへの再 QBUF を EINVAL で拒否する。REQBUFS 自体は queued buffer を回収するため EBUSY にはならない）
- また、古い `DecodedFrame` の drop が最後の `Arc<CaptureQueue>` 参照だった場合、`RequeueToken::requeue` の `self` 消費で `BufferSet::drop` → `REQBUFS(count=0)` が発行され、同じ fd / buf_type の新規バッファを解放してしまう
- 加えて `issues/0003-bug-frame-outlives-codec-fd.md` とも関連するが、こちらは fd 生存中の CaptureQueue race で異なる問題

## 設計方針

`RequeueToken::requeue` が runtime ロックを取り、対応する `CaptureQueue` が現在の active queue であることを確認してから enqueue する。加えて、stale な `BufferSet::drop` の自動 `REQBUFS(count=0)` を廃止し、バッファ解放を意図したタイミングでの明示的な発行に限定する。

- `RequeueToken::requeue` は runtime ロックを取得し、`self.capture_queue` が `runtime.capture_queue` と同一の `Arc` である場合のみ `enqueue` する（`Arc::ptr_eq` で判定）。active でない場合は enqueue をスキップする（古いキューへの stale QBUF を発行しない）
- enqueue 失敗時は既存どおり `pending_async_errors` へ push し `eprintln!` でログ出力する（`issues/0008-bug-pending-async-errors-lost-after-drop.md` の確定設計）
- `BufferSet::drop` が自動的に `REQBUFS(count=0)` を発行する設計をやめる（stale なキュー解放が新規バッファを解放する race の根本原因の除去）。バッファ解放が必要な箇所（`handle_source_change` の旧バッファ解放）では、意図したタイミングで明示的に `ioctl_reqbufs(count=0)` を呼ぶ。encoder / converter 側は 0004 のロールバックが実装されず closed となったため、明示的な `ioctl_reqbufs(count=0)` の対象外とし、fd close によるカーネル側解放に任せる
- 明示的な `ioctl_reqbufs(count=0)` は、古いキューにユーザーフレーム等の外部参照（`Arc<CaptureQueue>`）が残っていない場合のみ発行する。外部参照が残っている場合は `REQBUFS(count=0)` を発行せず、最後の参照解放時（`BufferSet::drop`）の munmap と fd close によるカーネル側解放に任せる。これにより、SOURCE_CHANGE をまたいで生存する古い `DecodedFrame` の `data()` / `dmabuf_fd()` が、明示的 `REQBUFS(count=0)` によって生存中に無効化されない
- 古いキューに外部参照が残っている間に新規 `REQBUFS(count>0)` を発行すると、V4L2 仕様上 EBUSY になり得る（mapped / exported バッファが残っているため。`V4L2_BUF_CAP_SUPPORTS_ORPHANED_BUFS` 非対応時）。このため `handle_source_change` は、古いキューへの外部参照が残っている場合に新規 REQBUFS が EBUSY を返したら SOURCE_CHANGE を失敗として扱い、次の SOURCE_CHANGE で再試行する（古いフレームは通常すぐに drop されるため、再試行時に成功する）。なお REQBUFS の capabilities で `V4L2_BUF_CAP_SUPPORTS_ORPHANED_BUFS` を確認し、対応している場合は古いバッファを orphan 化して新規 REQBUFS が成功する
- コーデック Drop 時は fd close でカーネルが全バッファを解放するため、自動 `REQBUFS(count=0)` の廃止による影響はない
- `issues/0003-bug-frame-outlives-codec-fd.md` は案 A（`Arc<OwnedFd>` 共有）で確定済み（案 C の無効化フラグは不採用）。本 issue の requeue skip と 0003 の fd 共有は独立に両立する
- `issues/0009-bug-handle-source-change-long-lock-hold.md` は本 issue を前提とし、実装順序は本 issue（0010）を先に完了させる。0009 のフェーズ 1 で `runtime.capture_queue = None` になるため、requeue の active 確認は古いキューをスキップし、フェーズ 3 で新キューが設置された後は新キューとの `Arc` 同一性で判定する。なお 0009 のフェーズ 2「明示的に drop する（`BufferSet::drop` の `REQBUFS(count=0)` が STREAMOFF 後に実行されるため EBUSY にならない）」は自動 `REQBUFS(count=0)` に依存しており、本 issue の適用後は 0009 実装時に明示的な `ioctl_reqbufs(count=0)` の発行に置き換える必要がある

## 完了条件

- SOURCE_CHANGE 処理中に古い `DecodedFrame` が drop されても、古いキューへの stale QBUF が発行されない
- 古いフレームが新規 `REQBUFS(count>0)` の発行前に解放された場合、新規 REQBUFS / enqueue_all / STREAMON が成功する。古いフレームが発行後に解放される場合は V4L2 仕様上 EBUSY になり得る（`V4L2_BUF_CAP_SUPPORTS_ORPHANED_BUFS` 非対応時）が、その場合は次の SOURCE_CHANGE で再試行され、バッファ破壊は発生しない
- stale な `BufferSet::drop` が新規に確保したバッファを解放しない
- 明示的な `ioctl_reqbufs(count=0)` によって、SOURCE_CHANGE をまたいで生存する古い `DecodedFrame` の `data()` / `dmabuf_fd()` が無効化されない（古いキューに外部参照が残る間はカーネルバッファを解放しない）
- 通常運用でパフォーマンスへの影響が無視できる
- `cargo test --workspace` および `cargo clippy --workspace --all-targets -- -D warnings` が成功する

## 変更対象

- `src/decoder.rs`（`RequeueToken::requeue` / `H264Decoder::handle_source_change`）
- `src/buffer.rs`（`BufferSet::drop` の自動 `REQBUFS(count=0)` 廃止。解放が必要な箇所は明示的な `ioctl_reqbufs(count=0)` に置き換え）
- `CHANGES.md`（`[FIX]` として追加）
