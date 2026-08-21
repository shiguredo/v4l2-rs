# pending_async_errors が Drop 後に配送されず silently 失われる問題を修正する

- Created: 2026-08-21
- Completed:
- Branch: feature/fix-pending-async-errors-lost-after-drop
- Polished:

## 目的

`H264Encoder` / `H264Decoder` / `ImageConverter` は `Arc<Mutex<VecDeque<Error>>>` として `pending_async_errors` を持ち、`RequeueToken::requeue` の失敗を保持する設計だが、これを配送するのは Poller の `handle_event` 冒頭のみ。Drop は先に `Poller::stop` を呼ぶため、コーデック Drop 後にフレームが Drop されて追加された `requeue` 失敗は永久に配送されない。フレームを外部スレッドに配布したケースで発生する silent なエラー消失を修正する。

## 現状

- `src/encoder.rs::EncoderShared::drain_pending_async_errors` は `pending_async_errors` を drain するのみ
- 配送タイミングは `H264Encoder::handle_event` の冒頭 1 箇所のみ（`decoder.rs` / `converter.rs` も同型）
- `H264Encoder::drop` は先に `poller.stop()` を呼び、そのあと `pending_async_errors` の drain は行っていない
- コーデック Drop 後にフレームが別スレッドで Drop されて `RequeueToken::requeue` が失敗を積んでも、ハンドラーへ配送されず握りつぶされる（`issues/0003-bug-frame-outlives-codec-fd.md` の別角度でもある）

## 設計方針

- Drop 側で `drain_pending_async_errors` を最後にもう一度呼び、残っているエラーを何らかの経路（`eprintln!` 相当のログ、あるいは Drop 前に呼び出せる強制フラッシュ API）で外に出す
- 依存追加を避けるため `eprintln!` で標準エラー出力に吐く方向で検討する（`src/lib.rs` に log crate 依存を持ち込む判断は別途 issue にする）
- あるいは `H264Encoder::flush_errors(&mut H) -> Result<()>` のような明示 API を追加し、ユーザーが Drop 前に呼べる形にする
- どちらの方針を採るかは advisor 相談で決める。デフォルトの安全性を重視するなら Drop での自動吐き出しが妥当
- `issues/0003-bug-frame-outlives-codec-fd.md` の解決方針（fd 共有 / `!Send` 化 / 無効化フラグ）と協調させる必要がある

## 完了条件

- コーデック Drop 後にフレームが Drop されて `requeue` が失敗した場合でも、エラーが silent に消失しない
- 通常運用でパフォーマンスへの影響が無視できる
- `cargo test --workspace` および `cargo clippy --workspace --all-targets -- -D warnings` が成功する

## 変更対象

- `src/encoder.rs`（`H264Encoder::drop`）
- `src/decoder.rs`（`H264Decoder::drop`）
- `src/converter.rs`（`ImageConverter::drop`）
- 必要に応じて公開 API 追加
- `CHANGES.md`（`[FIX]` として追加）
