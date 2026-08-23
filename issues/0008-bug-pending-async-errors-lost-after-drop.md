# pending_async_errors が Drop 後に配送されず silently 失われる問題を修正する

- Created: 2026-08-21
- Completed:
- Branch: feature/fix-pending-async-errors-lost-after-drop
- Polished: 2026-08-23

## 目的

`H264Encoder` / `H264Decoder` / `ImageConverter` は `Arc<Mutex<VecDeque<Error>>>` として `pending_async_errors` を持ち、`RequeueToken::requeue` の失敗を保持する設計だが、これを配送するのは Poller の `handle_event` 冒頭のみ。Drop は先に `Poller::stop` を呼ぶため、コーデック Drop 後にフレームが Drop されて追加された `requeue` 失敗は永久に配送されない。フレームを外部スレッドに配布したケースで発生する silent なエラー消失を修正する。

## 現状

- `src/encoder.rs::EncoderShared::drain_pending_async_errors` は `pending_async_errors` を drain するのみ
- 配送タイミングは `H264Encoder::handle_event` の冒頭 1 箇所のみ（`decoder.rs` / `converter.rs` も同型）
- `H264Encoder::drop` は先に `poller.stop()` を呼び、そのあと `pending_async_errors` の drain は行っていない
- コーデック Drop 後にフレームが別スレッドで Drop されて `RequeueToken::requeue` が失敗を積んでも、ハンドラーへ配送されず握りつぶされる（`issues/0003-bug-frame-outlives-codec-fd.md` の別角度でもある）

## 設計方針

コーデック Drop 後はハンドラーが存在しない（`start_poller` が `handler.take()` で消費し、poller 停止とともに破棄される）ため、Drop 後の `requeue` 失敗はハンドラーへ配送できない。よって `RequeueToken::requeue` の失敗時に直接ログ出力する。

- `RequeueToken::requeue` は enqueue 失敗時に、既存どおり `pending_async_errors` へ push する（コーデック生存中は `handle_event` 冒頭でユーザーへ配信される既存経路）
- これに加えて、enqueue 失敗時に `eprintln!` でログ出力する（メッセージは英語）。これにより、コーデック Drop 後に配信不能になってもエラーが silent に消失しない
- `eprintln!` を選ぶのは依存追加を避けるため（`src/lib.rs` に log / tracing クレート依存を持ち込む判断は別途 issue にする。ログは shiguredo-rust 規約で tracing が指定されているが、本 issue では既存の依存を増やさない）
- コーデック Drop 側（`H264Encoder::drop` 等）の変更は不要。Drop 時点で drain しても、その後に積まれるエラーは捕捉できないため
- `flush_errors(&mut H)` のような明示 API は不採用。handler は `start_poller` の `handler.take()` で poller スレッドのクロージャに移動済みであり、ユーザーが `&mut H` を渡せないため
- `issues/0003-bug-frame-outlives-codec-fd.md` は案 A（`CaptureQueue` / `OutputQueue` / `BufferSet` の fd を `Arc<OwnedFd>` に変更して共有）で確定済み（案 B の `!Send` 化、案 C の無効化フラグは不採用）。0003 適用後、コーデック Drop 後の `requeue` は open 済み fd への QBUF になり「成功するか EINVAL」を返す。成功時はエラーが発生せず、EINVAL 時のみ本 issue の対象（push + ログ出力）となる

## 完了条件

- コーデック Drop 後にフレームが Drop されて `requeue` が失敗した場合でも、エラーが `eprintln!` により標準エラー出力に出力され、silent に消失しない
- 通常運用でパフォーマンスへの影響が無視できる
- `cargo test --workspace` および `cargo clippy --workspace --all-targets -- -D warnings` が成功する

## 変更対象

- `src/encoder.rs`（`RequeueToken::requeue` へのログ出力追加）
- `src/decoder.rs`（同上）
- `src/converter.rs`（同上）
- `CHANGES.md`（`[FIX]` として追加）
