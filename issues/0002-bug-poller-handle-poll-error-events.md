# Poller が POLLERR / POLLHUP / POLLNVAL を処理せず busy loop に陥る問題を修正する

- Created: 2026-08-21
- Completed:
- Branch: feature/fix-poller-handle-poll-error-events
- Polished:

## 目的

`src/poller.rs` の `Poller::poll_loop` が `poll(2)` の `revents` として返される `POLLERR` / `POLLHUP` / `POLLNVAL` を処理していないため、これらのビットのみが立つ状況で poller スレッドが 100% CPU の busy loop に陥り、ユーザーには一切通知されない。デバイス切断や `STREAMOFF` 後の状態で確実に踏むため、修正する。

## 現状

`Poller::poll_loop` は `libc::poll` を呼び出した後、`revents` を以下の 3 分岐でしか検査していない。

- `pollfd.revents & libc::POLLPRI != 0` → `Self::handle_source_event`
- `pollfd.revents & libc::POLLOUT != 0` → `Self::process_output`
- `pollfd.revents & libc::POLLIN != 0` → `Self::process_capture`

`libc::poll` の戻り値が正 (`ret > 0`) で、`POLLPRI` / `POLLOUT` / `POLLIN` のいずれも立たず、`POLLERR` / `POLLHUP` / `POLLNVAL` のみが立つケースでは 3 つの `if` を全て素通りし、次のループでも同じ `revents` を即座に受け取る。これにより poller スレッドが 100% CPU で busy loop に陥る。abort フラグは各ループ冒頭で `Acquire` で読んでいるものの、外部から `Poller::stop` が呼ばれるまで CPU を焼き続ける。

Linux man 2 poll によれば `POLLERR` / `POLLHUP` / `POLLNVAL` は "output-only" フラグで `events` に指定しなくても `revents` に立つ。以下のシナリオで顕在化する:

- デバイスの hot-unplug（bcm2835-codec モジュールの `modprobe -r bcm2835_codec` 相当）
- `STREAMOFF` 後に fd が半 close 状態になった際の `POLLHUP`
- fd が別プロセスで invalidate されたときの `POLLNVAL`

## 設計方針

- `Poller::poll_loop` の `revents` 検査に `pollfd.revents & (POLLERR | POLLHUP | POLLNVAL) != 0` の分岐を追加する
- 該当ビットが立った場合は `on_event` に `PollEvent::Error(...)` を配送してから `poll_loop` を抜ける（`return` する）
- エラー型は既存の `Error::Poll { source }` を再利用する方針で検討する。`std::io::Error::from(std::io::ErrorKind::ConnectionAborted)` あるいは `std::io::Error::from_raw_os_error(libc::EIO)` で意味を持たせるかを実装時に判断する
- bcm2835-codec のホットプラグを明示的に区別したい要件が実装時に固まった場合のみ、`src/error.rs` に新バリアント（例: `Error::DeviceDisconnected`）を追加する検討をする（未確定なら既存の `Error::Poll` に寄せる）
- 既存の 3 分岐（`POLLPRI` / `POLLOUT` / `POLLIN`）と `POLLERR` 系が同時に立った場合、エラー検出を優先して先に配送 → `return` するか、既存分岐を先に捌いてから配送するかは実装時に決める。過去に POLLPRI/POLLIN と POLLHUP が同時に立ったケースで DQBUF/DQEVENT がまだ有効データを返す可能性があるため、既存 3 分岐を先に処理してから POLLERR 系で終了するのが素直と思われる

## 完了条件

- `POLLERR` / `POLLHUP` / `POLLNVAL` のみが立つ状況で busy loop に陥らず、ハンドラーへ `PollEvent::Error(...)` が 1 回配送された後に `poll_loop` が終了する
- 既存の 3 分岐（`POLLPRI` / `POLLOUT` / `POLLIN`）の挙動が壊れない
- `cargo test --workspace` および `cargo clippy --workspace --all-targets -- -D warnings` が成功する

## 変更対象

- `src/poller.rs`（`Poller::poll_loop` の `revents` 検査分岐の追加）
- 必要に応じて `src/error.rs`（新エラーバリアントを追加する場合のみ）
- `CHANGES.md`（`[FIX]` として追加）
