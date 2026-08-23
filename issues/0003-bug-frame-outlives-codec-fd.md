# EncodedFrame / DecodedFrame / ConvertedFrame がコーデック本体より長寿命化するとクローズ済み fd に ioctl を撃つ問題を修正する

- Created: 2026-08-21
- Completed:
- Branch: feature/fix-frame-outlives-codec-fd
- Polished: 2026-08-23

## 目的

`EncodedFrame<T>` / `DecodedFrame<T>` は `T: Send` の場合に、`ConvertedFrame` は無条件で自動導出により `Send` となり、ユーザーが別スレッドへ move できる。この経路で `H264Encoder` / `H264Decoder` / `ImageConverter` 本体が Drop された後にフレームが Drop されると、`RequeueToken::requeue` が既にクローズされた fd に対して `VIDIOC_QBUF` を発行する。fd 番号は Linux 上で即座に再利用されるため、直後に別スレッドが開いた無関係の fd（TCP ソケット、別ファイル等）に対して V4L2 ioctl が届く原理的な use-after-free 経路であり、致命的なので修正する。

## 現状

- `src/encoder.rs` の `RequeueToken` / `EncodedFrame<T>` に `!Send` マーカー（`PhantomData<*const ()>` 等）も明示的な `Send`/`Sync` impl も無く、`T: Send` なら auto derive で `Send` になる。`Send` のフィールド (`Option<RequeueToken>`, `u32`, `bool`, `i64`, `T`) しか持たないため。
- `RequeueToken` は `Arc<CaptureQueue>` を保持しており、`src/queue.rs` の `CaptureQueue` は `fd: RawFd` を生値で持つ。`RawFd = i32` は当然 `Send + Sync`。
- fd の所有権は `src/device.rs` の `Device { fd: OwnedFd }` に集中しており、`CaptureQueue` / `OutputQueue` / `BufferSet` はすべて生の `RawFd` を値として保持しているのみ。Rust の型システムで所有関係が表現されていない。
- コーデック本体（`H264Encoder` 等）の Drop は「Poller 停止 → STREAMOFF → Device close (OwnedFd の Drop)」の順で進むが、`Arc<CaptureQueue>` の強参照カウントが 0 でない限り `CaptureQueue` は解放されないため、`Device` が先に close される。
- 以降にフレームが別スレッドで Drop されると、`RequeueToken::requeue` → `CaptureQueue::enqueue(index)` → `sys::ioctl_qbuf(self.fd, ...)` が実行される。閉じた直後は EBADF で失敗するだけだが、fd 番号が再利用された後は無関係の fd に `VIDIOC_QBUF` を送信する。
- さらに、最後の Frame が Drop されて `Arc<CaptureQueue>` の強参照が 0 になった時点で `BufferSet::drop` が実行され、`src/buffer.rs` の `Drop` impl が保持する `RawFd` に対して `VIDIOC_REQBUFS`（count=0）を発行する。この経路も同様に fd 番号再利用後の無関係デバイスへ届き得る。
- Poller はコーデック Drop 時点で停止済みのため、`pending_async_errors` に積まれた失敗も配送されず silent に消失する（`issues/0008-bug-pending-async-errors-lost-after-drop.md` とは別経路）。
- `tests/test_converter.rs` の `EncoderValue` / `DecoderValue` / `ConverterOutValue` は「フレームを持ち回して drop を遅延させる」設計になっており、この使い方はユーザーからも普通に選択され得る。

同じ問題が 3 モジュールに複製されている:

- `src/encoder.rs` の `RequeueToken` / `EncodedFrame<T>` / その `Drop`
- `src/decoder.rs` の `RequeueToken` / `DecodedFrame<T>` / その `Drop`
- `src/converter.rs` の `RequeueToken` / `ConvertedFrame` / その `Drop`

## 設計方針

案 A で確定する。`CaptureQueue` / `OutputQueue` / `BufferSet` が保持する fd を `Arc<OwnedFd>` に変更し、コーデック本体（`H264Encoder` / `H264Decoder` / `ImageConverter`）と共有する。

- `src/device.rs` の `Device` が内部に `Arc<OwnedFd>` を保持する構成に変更し、`raw_fd()` は従来どおり生の `RawFd` を返す API を維持する
- `src/queue.rs` の `CaptureQueue` / `OutputQueue` と `src/buffer.rs` の `BufferSet` の `fd: RawFd` フィールドを `Arc<OwnedFd>` に変更し、ioctl は `.as_raw_fd()` で取得する
- Frame が保持する `Arc<CaptureQueue>` が生きている限り `Arc<OwnedFd>` も生きるため、fd は close されず番号も再利用されない。コーデック Drop 後に Frame を Drop しても、`RequeueToken::requeue` の `VIDIOC_QBUF` と `BufferSet::drop` の `VIDIOC_REQBUFS` はいずれも正しい fd に対して発行される
- コーデック Drop 後は STREAMOFF 済みのため、`requeue` の `VIDIOC_QBUF` は成功するか EINVAL を返すだけであり、未定義動作や無関係 fd への ioctl は発生しない
- 案 B（`Frame` を `!Send` にする）は不採用。`tests/test_converter.rs` の `EncoderValue` / `DecoderValue` / `ConverterOutValue` のようにフレームを `user_data` として持ち回す正当な使い方（`UserData: Send + 'static` 制約に依存）が compile error になるため
- 案 C（コーデック Drop 時に共有フラグを立てて `requeue` を no-op にする）は不採用。`RequeueToken::requeue` の no-op 化だけでは `BufferSet::drop` の `VIDIOC_REQBUFS` 経路を塞げず、fd 再利用後の無関係デバイスへの ioctl が残るため

## 完了条件

- `H264Encoder` / `H264Decoder` / `ImageConverter` を Drop した後、対応する `Frame` を Drop しても、閉じた fd や再利用された fd に対して V4L2 ioctl が発行されない
- `tests/test_converter.rs` の `EncoderValue` / `DecoderValue` / `ConverterOutValue` パターン（フレームを `user_data` として持ち回す使い方）が無変更で動作し続ける
- `cargo test --workspace` および `cargo clippy --workspace --all-targets -- -D warnings` が成功する
- 修正は 3 モジュール（encoder / decoder / converter）で一貫して適用する

## 変更対象

- `src/device.rs`（`Device` が内部に `Arc<OwnedFd>` を保持する構成への変更）
- `src/queue.rs`（`CaptureQueue` / `OutputQueue` の fd 保持を `Arc<OwnedFd>` に変更）
- `src/buffer.rs`（`BufferSet` の fd 保持を `Arc<OwnedFd>` に変更）
- `src/encoder.rs`（`RequeueToken` / `EncodedFrame` / `H264Encoder::drop`）
- `src/decoder.rs`（`RequeueToken` / `DecodedFrame` / `H264Decoder::drop`）
- `src/converter.rs`（`RequeueToken` / `ConvertedFrame` / `ImageConverter::drop`）
- `CHANGES.md`（`[FIX]` として追加）
