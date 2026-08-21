# RequeueToken::requeue と handle_source_change の間の race で REQBUFS が EBUSY になる問題を修正する

- Created: 2026-08-21
- Completed:
- Branch: feature/fix-requeue-source-change-race
- Polished:

## 目的

`src/decoder.rs::H264Decoder::handle_source_change` は STREAMOFF → `runtime.capture_queue = None` → 新規 REQBUFS の順で処理を進めるが、この途中でユーザーが古い `DecodedFrame`（古い `Arc<CaptureQueue>` を保持）を drop すると、`RequeueToken::requeue` は runtime ロックを取らずに古い `CaptureQueue::enqueue(index)` → `ioctl_qbuf(fd)` を呼ぶ。STREAMOFF 直後・REQBUFS(count>0) 直前に QBUF が挟まると、次の REQBUFS が queued buffer あり判定で EBUSY になり SOURCE_CHANGE が失敗する経路を修正する。

## 現状

- `src/decoder.rs::RequeueToken::requeue` は `capture_queue.enqueue(self.index)` を無ロックで呼ぶ
- `src/queue.rs::CaptureQueue::enqueue` は `sys::ioctl_qbuf(self.fd, ...)` を発行する
- `H264Decoder::handle_source_change` は runtime ロック内で `runtime.capture_queue = None`（Arc drop、他 Arc 保持者がいれば実体は残る）を実行後、新規 `BufferSet::allocate` で REQBUFS を発行する
- 古い CaptureQueue はまだ `Arc<CaptureQueue>` として RequeueToken から到達可能なため、drop されずに残る
- タイミング次第で「STREAMOFF 済み → 古い CaptureQueue に QBUF → 新規 REQBUFS」の順が発生し、REQBUFS(count>0) が EBUSY で失敗する
- 加えて `issues/0003-bug-frame-outlives-codec-fd.md` とも関連するが、こちらは fd 生存中の CaptureQueue race で異なる問題

## 設計方針

- `RequeueToken::requeue` も runtime ロックを取り、対応する CaptureQueue が現在の active queue であることを確認してから enqueue する
- 具体的な実装案:
  - RequeueToken に `generation: u64` を持たせ、CaptureQueue の generation と一致した場合のみ enqueue する
  - あるいは各 CaptureQueue に `AtomicBool disabled` フラグを持たせ、`handle_source_change` が古い queue を disable する。requeue 側は disabled ならスキップする
- 現行 encoder / converter でも同様の race は起こり得るが、それらは SOURCE_CHANGE を発火しないため実害は decoder に集中する
- `issues/0003-bug-frame-outlives-codec-fd.md` の設計方針（fd 共有 / 無効化フラグ）と協調させる

## 完了条件

- SOURCE_CHANGE 処理中に古い `DecodedFrame` が drop されても、新規 REQBUFS が EBUSY にならない
- 通常運用でパフォーマンスへの影響が無視できる
- `cargo test --workspace` および `cargo clippy --workspace --all-targets -- -D warnings` が成功する

## 変更対象

- `src/decoder.rs`（`RequeueToken::requeue` / `handle_source_change`）
- 必要に応じて `src/queue.rs::CaptureQueue`（無効化フラグ）
- `CHANGES.md`（`[FIX]` として追加）
