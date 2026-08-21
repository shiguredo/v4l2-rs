# Poller が transient error でも永久停止しリカバリ経路が無い問題を修正する

- Created: 2026-08-21
- Completed:
- Branch: feature/fix-poller-permanent-stop-on-transient-error
- Polished:

## 目的

`src/poller.rs` の `Poller` は `EAGAIN` のみ特別扱いし、それ以外のエラーは `PollEvent::Error(...)` を配送してループを抜ける。EIO / EBUSY / 2 段目以降の EINTR などは次のフレームで復帰できる transient error だが、それでも Poller が完全停止し、ユーザーは以降のフレームを一切受け取れなくなる。かつユーザーは Error 型から transient か fatal かを判別できず、コーデックを再構築するしか復旧手段がない状態を修正する。

## 現状

- `src/poller.rs::Poller::poll_loop` および `Poller::process_output` / `Poller::process_capture` / `Poller::handle_source_event` はいずれも `Error::Ioctl { source, .. }` のうち `source.raw_os_error() == Some(libc::EAGAIN)` のみ `Ok(None)` を返す
- それ以外のエラー（`EIO` / `EBUSY` / `EINVAL` など）は `on_event(PollEvent::Error(err))` を配送した直後に `return` で `poll_loop` を抜ける
- 1 回の transient error（例: bcm2835-codec が一時的に EIO を返す）で poller スレッドが停止し、`H264Encoder` / `H264Decoder` / `ImageConverter` は Drop されるまで完全に無音になる
- `src/error.rs::Error` にも「transient か fatal か」を区別するフィールドやメソッドが無い

## 設計方針

- `Poller` 側で errno ベースで復帰可能／不可能を分類する:
  - 復帰可能（retry 継続）: `EAGAIN` / `EINTR` / `EBUSY`（短期的なリソース競合）
  - 復帰可能（skip して次フレームへ）: `EIO`（transient なドライバエラー）
  - 復帰不能（stop）: `EBADF` / `ENODEV` / `EINVAL`（fd / デバイス消失、契約違反）
- ただし bcm2835-codec で `EIO` が transient として妥当かは実機検証が必要
- errno に応じた分類は `src/poller.rs` に閉じ込め、`Error::Ioctl` のバリアントは変更しない
- 呼び出し側（`H264Encoder::handle_event` 等）に「transient error」を渡す場合は既存の `on_event(PollEvent::Error(...))` 経路を使い、Poller は継続する
- fatal error のみ従来通り `PollEvent::Error(...)` を配送してからループを抜ける
- 実装時に `advisor` に相談し、errno ベース分類の網羅性を検証する

## 完了条件

- transient error 発生時に poller スレッドが停止せず、次のフレームで処理を継続する
- fatal error（fd 消失など）では従来通り停止する
- ユーザーは復帰可能なエラーとして `PollEvent::Error(...)` を受け取り、コーデックを再構築せずに動作継続できる
- `cargo test --workspace` および `cargo clippy --workspace --all-targets -- -D warnings` が成功する

## 変更対象

- `src/poller.rs`（`Poller::poll_loop` および `try_dequeue_output` / `try_dequeue_capture` / `handle_source_event` の errno 分類）
- `CHANGES.md`（`[FIX]` として追加）
