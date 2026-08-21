# RequeueToken / drain_pending_async_errors / Frame 型の 3 モジュール複製を共通化する

- Created: 2026-08-21
- Completed:
- Branch: feature/refactor-unify-frame-and-requeue-token
- Polished:

## 目的

`RequeueToken`（フィールドと `requeue()` メソッド）、`drain_pending_async_errors`、`EncodedFrame` / `DecodedFrame` / `ConvertedFrame` の Drop 実装、Frame 型の共通メソッド（`data()` / `dmabuf_fd()` / `index()` / `bytesused()` / `length()` / `timestamp_us()`）が `src/encoder.rs` / `src/decoder.rs` / `src/converter.rs` の 3 モジュールにほぼ一字一句同一で複製されている。この複製により、致命的バグ 3 件（`issues/0003` / `0004`）が 3 モジュール独立に発生しており、修正時の一貫性を確保するのが困難。共通化する。

## 現状

- `RequeueToken` は 3 モジュールに同一の構造で存在（`capture_queue: Arc<CaptureQueue>` / `index: u32` / `pending_async_errors: Arc<Mutex<VecDeque<Error>>>`）
- `RequeueToken::requeue(self)` の実装も同一
- `EncoderShared` / `DecoderShared` / `ConverterShared` の `drain_pending_async_errors` メソッドも同一実装
- `EncodedFrame<T>` / `DecodedFrame<T>` / `ConvertedFrame` の共通フィールド（`requeue: Option<RequeueToken>` / `index: u32` / `bytesused: u32` / `timestamp_us: i64`）とメソッド（`data()` / `dmabuf_fd()` / `index()` / `bytesused()` / `length()` / `timestamp_us()`）も同一
- 差分は `is_keyframe: bool`（EncodedFrame のみ）と `user_data: T`（EncodedFrame / DecodedFrame。ConvertedFrame は `T` を型パラメータで持たず `ConvertCallbackOutput<T>::Frame` の隣で保持）
- 修正時、3 モジュール独立に同じ変更を入れる必要があり、抜け漏れのリスクが高い

## 設計方針

- 共通型を `src/queue.rs` あるいは新モジュール `src/frame.rs` に切り出す:
  - `pub(crate) struct RequeueToken`（と `requeue()`）
  - `pub(crate) struct CaptureFrameBase { requeue: Option<RequeueToken>, index: u32, bytesused: u32, timestamp_us: i64 }`
  - 共通メソッド（`data()` / `dmabuf_fd()` / `index()` / `bytesused()` / `length()` / `timestamp_us()`）を `CaptureFrameBase` の impl として提供
- 各 Frame 型は `CaptureFrameBase` を保持するラッパーとし、固有フィールド（`is_keyframe` / `user_data`）と固有メソッドだけを追加する
- `drain_pending_async_errors` は `pending_async_errors: Arc<Mutex<...>>` を扱う小さいヘルパー関数として共通化
- 公開 API（EncodedFrame / DecodedFrame / ConvertedFrame の型名とメソッド名）は変更しない
- リファクタリング単独の issue とし、バグ修正（`issues/0003` / `0004` / `0006` / `0010` 等）とは混ぜない
- ただし、これらのバグ修正が本 issue の共通化を前提とする場合、着手順序は先にこちらを完了させる

## 完了条件

- `RequeueToken` / Frame 共通部分 / `drain_pending_async_errors` が 1 箇所にまとまり、3 モジュールの重複コードが削減される
- 公開 API の型名・メソッドシグネチャに変更が無い
- 実機テスト（`tests/test_converter.rs`）が引き続き成功する
- `cargo test --workspace` および `cargo clippy --workspace --all-targets -- -D warnings` が成功する

## 変更対象

- 新規: `src/frame.rs`（あるいは `src/queue.rs` への追記）
- `src/encoder.rs`（`RequeueToken` / `EncodedFrame` / `drain_pending_async_errors` の共通化）
- `src/decoder.rs`（同上）
- `src/converter.rs`（同上）
- `src/lib.rs`（新モジュールを追加する場合の mod 宣言）
- `CHANGES.md`（記載するかは変更の可視性次第。内部リファクタなら記載不要も検討）
