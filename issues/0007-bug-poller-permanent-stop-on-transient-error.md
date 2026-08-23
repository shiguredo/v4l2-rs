# Poller が transient error でも永久停止しリカバリ経路が無い問題を修正する

- Created: 2026-08-21
- Completed:
- Branch: feature/fix-poller-permanent-stop-on-transient-error
- Polished: 2026-08-23

## 目的

`src/poller.rs` の `Poller` は `EAGAIN` のみ特別扱いし、それ以外のエラーは `PollEvent::Error(...)` を配送してループを抜ける。EIO / 2 段目以降の EINTR などは次のフレームで復帰できる transient error だが、それでも Poller が完全停止し、ユーザーは以降のフレームを一切受け取れなくなる。かつユーザーは Error 型から transient か fatal かを判別できず、コーデックを再構築するしか復旧手段がない状態を修正する。

## 現状

- `src/poller.rs` の `Poller::try_dequeue_output` / `Poller::try_dequeue_capture` は `Error::Ioctl { source, .. }` のうち `source.raw_os_error() == Some(libc::EAGAIN)` のみ `Ok(None)` を返す（`Poller::handle_source_event` も同様に EAGAIN を `Ok(())` として無視する）
- それ以外のエラー（`EIO` / `EBUSY` / `EINVAL` など）は `on_event(PollEvent::Error(err))` を配送した直後に `return` で `poll_loop` を抜ける
- 1 回の transient error（例: bcm2835-codec が一時的に EIO を返す）で poller スレッドが停止し、`H264Encoder` / `H264Decoder` / `ImageConverter` は Drop されるまで完全に無音になる
- `src/error.rs::Error` にも「transient か fatal か」を区別するフィールドやメソッドが無い

## 設計方針

- `Poller` 側で errno ベースで復帰可能／不可能を分類する:
  - silent（次の poll を待つ）: `EAGAIN` / `EINTR`（正常な一時的状況。現状の `Ok(None)` / `continue` を維持）
  - 配送して継続: `EIO`（transient なドライバエラー。`on_event(PollEvent::Error(...))` を配送してループを継続）
  - 配送して停止（fatal）: `EBADF` / `ENODEV` / `EINVAL` / `EPIPE`（fd / デバイス消失、契約違反、ストリーム終了）
  - 分類表に無い errno は fatal（停止）をデフォルトとする
- `EIO` が連続 8 回以上返る場合は transient ではなく fatal とみなして停止する（busy loop とバッファ枯渇の回避）。V4L2 仕様は「ドライバがエラーを返しながら空バッファをデキューし得る」と注記しており、EIO 連続時はバッファが失われて枯渇し得るため
- errno に応じた分類は `src/poller.rs` に閉じ込め、`Error::Ioctl` のバリアントは変更しない
- 呼び出し側（`H264Encoder::handle_event` 等）には配送対象（`EIO`）のみ既存の `on_event(PollEvent::Error(...))` 経路で渡し、Poller は継続する
- fatal error のみ従来通り `PollEvent::Error(...)` を配送してからループを抜ける
- errno ベース分類は設計として確定する。`EPIPE` は mem2mem コーデックの DQBUF でストリーム終了時に返る標準エラー（V4L2 仕様）のため fatal に分類する
- `issues/0002-bug-poller-handle-poll-error-events.md` との関係: 0002 は `POLLERR` / `POLLHUP` / `POLLNVAL`（revents）を fatal として dispatch + return する方針で確定済み。本 issue の errno 分類は ioctl エラー（`try_dequeue_*` / `handle_source_event` の戻り値）を対象とし、レイヤーが異なる。POLLERR 系のビットが同時に立っている場合は 0002 の設計どおり既存 3 分岐を処理してから POLLERR 系で停止するため、transient ioctl エラーの継続より 0002 の停止が優先される

## 完了条件

- transient error（`EIO`）発生時に poller スレッドが停止せず、次のフレームで処理を継続する
- fatal error（`EBADF` / `ENODEV` / `EINVAL` / `EPIPE` 等）では従来通り停止する
- ユーザーは復帰可能なエラー（`EIO`）を `PollEvent::Error(...)` として受け取り、コーデックを再構築せずに動作継続できる
- bcm2835-codec で `EIO` が transient として返ることを実機（Raspberry Pi + bcm2835-codec）で確認し、その際に poller が停止せず継続する
- `cargo test --workspace` および `cargo clippy --workspace --all-targets -- -D warnings` が成功する

## 変更対象

- `src/poller.rs`（`Poller::poll_loop` および `try_dequeue_output` / `try_dequeue_capture` / `handle_source_event` の errno 分類）
- `CHANGES.md`（`[FIX]` として追加）
