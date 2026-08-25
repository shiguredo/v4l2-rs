# Poller が POLLERR / POLLHUP / POLLNVAL を処理せず busy loop に陥る問題を修正する

- Created: 2026-08-21
- Completed: 2026-08-25
- Branch: feature/fix-poller-handle-poll-error-events
- Polished: 2026-08-23

## 目的

`src/poller.rs` の `Poller::poll_loop` が `poll(2)` の `revents` として返される `POLLERR` / `POLLHUP` / `POLLNVAL` を処理していないため、これらのビットのみが立つ状況で poller スレッドが 100% CPU の busy loop に陥り、ユーザーには一切通知されない。デバイス切断時に踏むため、修正する。

## 現状

`Poller::poll_loop` は `libc::poll` を呼び出した後、`revents` を以下の 3 分岐でしか検査していない。

- `pollfd.revents & libc::POLLPRI != 0` → `Self::handle_source_event`
- `pollfd.revents & libc::POLLOUT != 0` → `Self::process_output`
- `pollfd.revents & libc::POLLIN != 0` → `Self::process_capture`

`libc::poll` の戻り値が正 (`ret > 0`) で、`POLLPRI` / `POLLOUT` / `POLLIN` のいずれも立たず、`POLLERR` / `POLLHUP` / `POLLNVAL` のみが立つケースでは 3 つの `if` を全て素通りし、次のループでも同じ `revents` を即座に受け取る。これにより poller スレッドが 100% CPU で busy loop に陥る。abort フラグは各ループ冒頭で `Acquire` で読んでいるものの、外部から `Poller::stop` が呼ばれるまで CPU を焼き続ける。

Linux man 2 poll によれば `POLLERR` / `POLLHUP` / `POLLNVAL` は "output-only" フラグで `events` に指定しなくても `revents` に立つ。主にデバイスの hot-unplug（bcm2835-codec モジュールの `modprobe -r bcm2835_codec` 相当）で `POLLERR` / `POLLHUP` が立つことで顕在化する（実機での発火条件は未検証）。

## 設計方針

- `Poller::poll_loop` の `revents` 検査に `pollfd.revents & (POLLERR | POLLHUP | POLLNVAL) != 0` の分岐を追加する
- `POLLERR` 系と既存 3 分岐（`POLLPRI` / `POLLOUT` / `POLLIN`）が同時に立った場合は、既存 3 分岐を先に処理してから `POLLERR` 系の検査で `on_event` に `PollEvent::Error(...)` を配送して `poll_loop` を抜ける（`return` する）。過去に POLLPRI/POLLIN と POLLHUP が同時に立ったケースで DQBUF/DQEVENT がまだ有効データを返す可能性があるためであり、完了条件「既存の 3 分岐の挙動が壊れない」と整合する
- エラー型は既存の `Error::Poll { source }` を再利用する方針で検討する。`std::io::Error::from(std::io::ErrorKind::ConnectionAborted)` あるいは `std::io::Error::from_raw_os_error(libc::EIO)` で意味を持たせるかを実装時に判断する
- bcm2835-codec のホットプラグを明示的に区別したい要件が実装時に固まった場合のみ、`src/error.rs` に新バリアント（例: `Error::DeviceDisconnected`）を追加する検討をする（未確定なら既存の `Error::Poll` に寄せる）

## 完了条件

- `POLLERR` / `POLLHUP` / `POLLNVAL` のみが立つ状況で busy loop に陥らず、ハンドラーへ `PollEvent::Error(...)` が 1 回配送された後に `poll_loop` が終了する
- 既存の 3 分岐（`POLLPRI` / `POLLOUT` / `POLLIN`）の挙動が壊れない
- `cargo test --workspace` および `cargo clippy --workspace --all-targets -- -D warnings` が成功する

## 変更対象

- `src/poller.rs`（`Poller::poll_loop` の `revents` 検査分岐の追加）
- 必要に応じて `src/error.rs`（新エラーバリアントを追加する場合のみ）
- `CHANGES.md`（`[FIX]` として追加）

## 解決方法

`src/poller.rs` の `Poller::poll_loop` に `revents` のエラーフラグ処理を追加した。実装にあたり当初の設計方針（`POLLERR` 系で終了する）を一次資料と実機で検証したところ、前提が一部誤っていたため修正した。

実機 (Raspberry Pi / bcm2835-codec) の converter テストで `poll(2)` の `revents` を観測した。Healthy なストリーミング中でもアイドル時に `POLLERR` のみが即座に返り続け、一度に 2 万回以上連続（約 360ms）で busy loop することを確認した。この `POLLERR` は一次資料 `refs/v4l2/Documentation/userspace-api/media/v4l/func-poll.rst` が定義する通り「まだ `VIDIOC_STREAMON` していない、または `VIDIOC_QBUF` していない」という正常な準備状態であり、デバイス障害ではない。よって「`POLLERR` で終了する」は誤りと判断した。

フラグごとの扱い:

- `POLLERR`（データ用フラグ `POLLPRI` / `POLLOUT` / `POLLIN` が同時に立たない場合）: 正常なアイドル状態として、終了せず `1ms` スリープして再ループする。これにより busy loop を防ぐ。データ用フラグと同時に立った場合は、上の DQEVENT / DQBUF で処理済みのためそのまま継続する（スリープ不要）。
- `POLLHUP` / `POLLNVAL`（切断）: `PollEvent::Error(crate::error::Error::Poll { source: ErrorKind::ConnectionAborted })` を 1 回配送して `poll_loop` を終了する。`POLLHUP` は一次資料 `refs/v4l2/drivers/media/v4l2-core/v4l2-dev.c` の既定 poll が `video_is_registered()` 偽のとき（モジュールアンロード等の切断）に `EPOLLERR | EPOLLHUP | EPOLLPRI` を返すこと、および V4L2 ユーザー空間 API ドキュメントが `POLLHUP` をベネインな再試行条件として定義していないこと（`func-poll.rst` は詳細を poll(2) マニュアルページに委ねる）に基づく。

エラー型は既存の `Error::Poll { source }` を再利用し、`src/error.rs` へのバリアント追加は行わなかった。

これに伴い完了条件の「`PollEvent::Error(...)` が 1 回配送された後に `poll_loop` が終了する」は、`POLLERR` 単独ではなく `POLLHUP` / `POLLNVAL`（切断）の場合の動作に修正された。
