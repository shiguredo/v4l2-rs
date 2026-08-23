# STREAMON の 2 段呼び出しが部分失敗するとコーデックが復旧不能な状態に陥る問題を修正する

- Created: 2026-08-21
- Completed:
- Branch: feature/fix-streamon-partial-failure
- Polished: 2026-08-23

## 目的

`H264Encoder::encode` / `ImageConverter::convert` は 2 段の `ioctl_streamon` を、`H264Decoder::handle_source_change` は 1 段の `ioctl_streamon` を、いずれも `?` で順に呼び出しており、後段の STREAMON が失敗しても前段の STREAMON はカーネル側で成立したままフラグ状態が不整合になる。Drop で `runtime.started` を条件に STREAMOFF を呼ぶ現状の設計では、この不整合状態から復旧できず、以降のエンコード / 変換 / デコードが永久に失敗するため修正する。

## 現状

3 コンポーネントの該当箇所:

- `src/encoder.rs` の `H264Encoder::encode` 内、初回 STREAMON:
  1. `runtime.pending_values.push_back(user_data)` を済ませる
  2. `sys::ioctl_streamon(fd, V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE)?`
  3. `sys::ioctl_streamon(fd, V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE)?`
  4. `runtime.started = true`
- `src/converter.rs` の `ImageConverter::convert` 内、初回 STREAMON:
  1. `runtime.pending_values.push_back(value)` を済ませる
  2. `sys::ioctl_streamon(fd, V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE)?`（順番は encoder と逆）
  3. `sys::ioctl_streamon(fd, V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE)?`
  4. `runtime.started = true`
- `src/decoder.rs` の `H264Decoder::handle_source_change` 内、SOURCE_CHANGE 後の CAPTURE 起動:
  1. `runtime.capture_queue = Some(capture_queue)`
  2. `sys::ioctl_streamon(fd, V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE)?`
  3. `runtime.capture_started = true`

いずれも 2 段目 (encoder / converter) または 1 段のみ (decoder) の STREAMON が失敗した場合、次のように状態が破綻する:

- 後段 STREAMON 失敗により関数が `?` で早期リターンし、`runtime.started` (encoder / converter) や `runtime.capture_started` (decoder) は false のまま
- 前段の STREAMON はカーネル側で成立済み。encoder / converter は片方の buf_type が STREAMON、もう片方が STREAMOFF の中間状態
- decoder は `capture_queue = Some(...)` だけ残り STREAMON していない中間状態
- Drop 時、`H264Encoder::drop` / `ImageConverter::drop` は `if runtime.started` で分岐しており false 側では STREAMOFF を発行しない。`H264Decoder::drop` も `capture_started` を条件にしている
- 結果、fd を close するまで前段の STREAMON が残り、次回 `encode()` / `convert()` は再度 `ioctl_streamon`（encoder は OUTPUT、converter は CAPTURE）を発行するがドライバによっては EBUSY を返して以降永久に失敗する
- decoder は `capture_queue = Some(...)` のまま次回 SOURCE_CHANGE で古い CaptureQueue が Arc drop → `BufferSet::drop` の REQBUFS(count=0) が queued buffer に対して EBUSY で拒否される経路に入る

## 設計方針

- OUTPUT / CAPTURE の STREAMON 状態をそれぞれ独立にトラッキングする。`runtime.started: bool` を `struct StreamState { output: bool, capture: bool }` に変える（decoder は `capture_started: bool` のままでよい）。`enum StreamState { Idle, OutputOn, BothOn }` は converter（CAPTURE 先行）と decoder の状態遷移を表現できないため不採用
- 後段 STREAMON が失敗したときは、成功済みの前段を明示的に `ioctl_streamoff` でロールバックしてから `?` で戻す
- converter は CAPTURE が前段のため、ロールバックの STREAMOFF で CAPTURE バッファが未 QBUF 状態に戻る。再試行時は CAPTURE バッファを再 QBUF（`enqueue_all`）してから STREAMON するなど、CAPTURE バッファが未 QBUF のまま再試行しないようにする（v4l2_m2m ドライバは CAPTURE キューにバッファが無いとジョブを処理しない）
- STREAMON 失敗時は、OUTPUT キューの `ioctl_streamoff` を発行して queued 状態で残るバッファを回収してから、`dequeue_available` で取得済みの OUTPUT バッファ index を `return_buffer` で利用可能リストへ戻し、`pending_values` に積んだ値を取り消す。STREAMON 未成立のキューへの STREAMOFF も V4L2 仕様上 queued バッファを回収するため、再試行時の再 QBUF が EBUSY にならない（`return_buffer` だけではカーネル側の queued 状態が残り、再 QBUF が EBUSY で失敗するため、STREAMOFF による回収が必須）
- Drop 側は OUTPUT / CAPTURE のフラグを個別に見て、立っている方だけ `ioctl_streamoff` を発行する
- decoder の `handle_source_change` は `capture_queue = Some(...)` と `capture_started = true` の同期崩れを防ぐため、STREAMON 成功後にのみ両方を更新する。STREAMON が失敗した場合は、queued バッファを回収するため CAPTURE の `ioctl_streamoff` を必ず発行してから `capture_queue = None` に戻し、次の `handle_source_change` がバッファ確保からやり直せるようにする
- ロールバック処理は各モジュールに同様の実装を入れる。重複の共通化は `issues/` に別途 refactor issue を立てて対応する（本 issue では共通化しない）

## 完了条件

- 後段 (encoder / converter) または 1 段 (decoder) の STREAMON が失敗した場合に、前段の STREAMON が確実にロールバックされ、`runtime` の状態フラグとカーネル側の STREAMON 状態が一致する
- 失敗後に再度 `encode()` / `convert()` / `handle_source_change` を呼び出した場合、以前の状態を引きずらずに再度 STREAMON を試行できる
- Drop 時に不整合中間状態が残っていた場合でも、STREAMOFF が必要な buf_type すべてに対して STREAMOFF が発行される
- `cargo test --workspace` および `cargo clippy --workspace --all-targets -- -D warnings` が成功する

## 変更対象

- `src/encoder.rs`（`H264Encoder::encode` の STREAMON 順序、`EncoderRuntime` の状態フィールド、`H264Encoder::drop`）
- `src/converter.rs`（`ImageConverter::convert` の STREAMON 順序、`ConverterRuntime` の状態フィールド、`ImageConverter::drop`）
- `src/decoder.rs`（`H264Decoder::handle_source_change` の状態更新順序、`DecoderRuntime.capture_started` / `capture_queue` の関係、`H264Decoder::drop`）
- `CHANGES.md`（`[FIX]` として追加）
