# EncodedFrame / DecodedFrame / ConvertedFrame がコーデック本体より長寿命化するとクローズ済み fd に ioctl を撃つ問題を修正する

- Created: 2026-08-21
- Completed:
- Branch: feature/fix-frame-outlives-codec-fd
- Polished:

## 目的

`EncodedFrame<T>` / `DecodedFrame<T>` / `ConvertedFrame` は `T: Send` の場合に自動導出で `Send` となり、ユーザーが別スレッドへ move できる。この経路で `H264Encoder` / `H264Decoder` / `ImageConverter` 本体が Drop された後にフレームが Drop されると、`RequeueToken::requeue` が既にクローズされた fd に対して `VIDIOC_QBUF` を発行する。fd 番号は Linux 上で即座に再利用されるため、直後に別スレッドが開いた無関係の fd（TCP ソケット、別ファイル等）に対して V4L2 ioctl が届く原理的な use-after-free 経路であり、致命的なので修正する。

## 現状

- `src/encoder.rs` の `RequeueToken` / `EncodedFrame<T>` に `!Send` マーカー（`PhantomData<*const ()>` 等）も明示的な `Send`/`Sync` impl も無く、`T: Send` なら auto derive で `Send` になる。`Send` のフィールド (`Option<RequeueToken>`, `u32`, `bool`, `i64`, `T`) しか持たないため。
- `RequeueToken` は `Arc<CaptureQueue>` を保持しており、`src/queue.rs` の `CaptureQueue` は `fd: RawFd` を生値で持つ。`RawFd = i32` は当然 `Send + Sync`。
- fd の所有権は `src/device.rs` の `Device { fd: OwnedFd }` に集中しており、`CaptureQueue` / `OutputQueue` / `BufferSet` はすべて `RawFd` を借用しているのみ。Rust の型システムで所有関係が表現されていない。
- コーデック本体（`H264Encoder` 等）の Drop は「Poller 停止 → STREAMOFF → Device close (OwnedFd の Drop)」の順で進むが、`Arc<CaptureQueue>` の強参照カウントが 0 でない限り `CaptureQueue` は解放されないため、`Device` が先に close される。
- 以降にフレームが別スレッドで Drop されると、`RequeueToken::requeue` → `CaptureQueue::enqueue(index)` → `sys::ioctl_qbuf(self.fd, ...)` が実行される。閉じた直後は EBADF で失敗するだけだが、fd 番号が再利用された後は無関係の fd に `VIDIOC_QBUF` を送信する。
- Poller はコーデック Drop 時点で停止済みのため、`pending_async_errors` に積まれた失敗も配送されず silent に発火する（`issues/0002-bug-poller-handle-poll-error-events.md` とは別経路）。
- `tests/test_converter.rs` の `EncoderValue` / `DecoderValue` / `ConverterOutValue` は「フレームを持ち回して drop を遅延させる」設計になっており、この使い方はユーザーからも普通に選択され得る。

同じ問題が 3 モジュールに複製されている:

- `src/encoder.rs` の `RequeueToken` / `EncodedFrame<T>` / その `Drop`
- `src/decoder.rs` の `RequeueToken` / `DecodedFrame<T>` / その `Drop`
- `src/converter.rs` の `RequeueToken` / `ConvertedFrame` / その `Drop`

## 設計方針

以下の 3 案から選択する。実装着手時に advisor 相談 / `/polish-issue` で確定させる。

- 案 A: `CaptureQueue` が `Arc<OwnedFd>` を保持し、`Device` と共有する
  - fd の生存期間を Rust の型で保証する。`Frame` から到達可能な限り fd は close されない。
  - `Device` の代わりに `H264Encoder` / `H264Decoder` / `ImageConverter` が `Arc<OwnedFd>` を共有し、`CaptureQueue` / `OutputQueue` / `BufferSet` も `Arc<OwnedFd>` を借用する構成に変える。
  - フレーム側は最も自然だが、既存の `Device: OwnedFd` 中心の設計を大きく変える。
- 案 B: `Frame` を `!Send` にする
  - `PhantomData<*const ()>` などで `Send` を除去し、`Frame` をコーデックと同じスレッドでしか drop できないようにする。
  - 副作用がユーザー API の制約になり、非同期ランタイム（Tokio 等）と組み合わせる場合に扱いにくくなる。
- 案 C: コーデック Drop 時に共有フラグ（`Arc<AtomicBool>`）を立てて `RequeueToken::requeue` を no-op にする
  - フラグを Drop 時に `SeqCst` で立て、`requeue` 冒頭でチェック。`Arc<OwnedFd>` を共有しないので既存構造への影響は小さい。
  - ただし「無効化フラグを立てる」だけでは、フレームが `data()` / `dmabuf_fd()` 経由で `capture_queue.buffers()` 内の `MmapRegion` / `OwnedFd` を触るケースも同時に無効化する設計が必要。`data()` は `&data[..bytesused]` を返すため、`MmapRegion` 本体が生きていればスライスは有効。とはいえ fd close 後の mmap 領域アクセスの安全性は V4L2 では未定義であり、この案でも `data()` を無効化する追加設計が必要。

上記の判定は `/polish-issue` で advisor に相談してから確定する。

## 完了条件

- `H264Encoder` / `H264Decoder` / `ImageConverter` を Drop した後、別スレッドで対応する `Frame` を Drop しても、閉じた fd や再利用された fd に対して V4L2 ioctl が発行されない
- フレームを外部スレッドへ move して長時間保持するユースケース（例: `tests/test_converter.rs` の `EncoderValue` パターン）が動作し続ける、または適切に compile error になる（案 B の場合）
- `cargo test --workspace` および `cargo clippy --workspace --all-targets -- -D warnings` が成功する
- 修正は 3 モジュール（encoder / decoder / converter）で一貫して適用する

## 変更対象

- `src/encoder.rs`（`RequeueToken` / `EncodedFrame` / `H264Encoder::drop`）
- `src/decoder.rs`（`RequeueToken` / `DecodedFrame` / `H264Decoder::drop`）
- `src/converter.rs`（`RequeueToken` / `ConvertedFrame` / `ImageConverter::drop`）
- `src/queue.rs`（`CaptureQueue` の fd 保持を `Arc<OwnedFd>` 化する場合）
- `src/buffer.rs`（同上、`BufferSet` の fd 保持）
- `src/device.rs`（`Device` を `Arc<OwnedFd>` で共有する場合）
- `CHANGES.md`（`[FIX]` として追加）
