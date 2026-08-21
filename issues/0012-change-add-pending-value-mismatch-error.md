# Error::NoAvailableBuffer の意味流用を止め pending_values 同期ズレ用の新バリアントを追加する

- Created: 2026-08-21
- Completed:
- Branch: feature/change-add-pending-value-mismatch-error
- Polished:

## 目的

`Error::NoAvailableBuffer` は「利用可能なバッファがない」という意味だが、encoder / decoder / converter の `handle_capture` では「CAPTURE 完了通知が来たが対応する `user_data` が pending_values に無い」というプロトコル異常の通知に流用されている。ユーザーが正しく原因を診断できないため、意味を分離する。

## 現状

- `src/encoder.rs::H264Encoder::handle_capture` の `runtime.pending_values.pop_front()` が `None` のとき `Error::NoAvailableBuffer` を通知
- `src/decoder.rs::H264Decoder::handle_capture` の同パターンで `Error::NoAvailableBuffer` を通知
- `src/converter.rs::ImageConverter::handle_capture` の同パターンで `Error::NoAvailableBuffer` を通知
- `src/error.rs::Error::NoAvailableBuffer` の Display 文言は「利用可能なバッファがありません」で、実際の意味（pending_values の同期ズレ = プロトコル異常）と乖離する
- 呼び出し側は「バッファが枯渇した」と誤診断しうる

## 設計方針

- `src/error.rs::Error` に `PendingValueMismatch` バリアントを追加する（あるいは `UnexpectedFrame` などプロトコル異常を示す命名）
- 3 モジュールの `handle_capture` で `NoAvailableBuffer` を `PendingValueMismatch` に置き換える
- `Error::NoAvailableBuffer` は「OUTPUT queue から buffer index が取れなかった」経路（既存の `dequeue_available` が `None` を返す場合）専用に戻す
- `pbt/tests/prop_error.rs::arb_error` に新バリアントを追加する
- 公開 API に新エラーバリアントが追加されるため「後方互換のない変更」として `CHANGES.md` の `[CHANGE]` に分類する（`Error` は現状 `#[non_exhaustive]` が無いため）
- 併せて `#[non_exhaustive]` の付与も検討する（別 issue 化）

## 完了条件

- pending_values 同期ズレのエラーが `Error::PendingValueMismatch` として通知される
- `Error::NoAvailableBuffer` は本来の意味（buffer 枯渇）専用になる
- 既存の PBT が新バリアントも網羅する
- `cargo test --workspace` および `cargo clippy --workspace --all-targets -- -D warnings` が成功する

## 変更対象

- `src/error.rs`（新バリアント追加、Display 実装）
- `src/encoder.rs`（`handle_capture` のエラー分岐）
- `src/decoder.rs`（`handle_capture` のエラー分岐）
- `src/converter.rs`（`handle_capture` のエラー分岐）
- `pbt/tests/prop_error.rs`（新バリアントを `arb_error` に追加）
- `CHANGES.md`（`[CHANGE]` として追加）
