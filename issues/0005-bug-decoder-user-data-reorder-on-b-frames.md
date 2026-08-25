# H264Decoder が B フレームを含むストリームで user_data とフレームの対応を誤る問題を修正する

- Created: 2026-08-21
- Completed:
- Branch: feature/fix-decoder-user-data-reorder-on-b-frames
- Polished: 2026-08-23

## 目的

`H264Decoder` は `runtime.pending_values: VecDeque<T>` に `push_back` した `user_data` を、CAPTURE 完了時に `pop_front` で FIFO 順に取り出す。デコーダの出力順（プレゼンテーション順）はデコード順と一致しないため、B フレームなど reorder のある H.264 ストリームでは `user_data` が誤ったフレームに紐付き、ユーザーが `DecodedFrame::user_data()` から取得するメタデータが壊れる。データ整合性が破壊される致命的な問題のため修正する。

## 現状

- `src/decoder.rs` の `DecoderRuntime<T>` は `pending_values: VecDeque<T>` を持つ
- `H264Decoder::decode` は入力（デコード順）で `runtime.pending_values.push_back(user_data)` する
- `H264Decoder::handle_capture` は CAPTURE 完了時に `runtime.pending_values.pop_front()` で先頭の `user_data` を取り出し、`DecodedFrame::new` に渡す
- OUTPUT 側 QBUF には `V4L2_BUF_FLAG_TIMESTAMP_COPY` が立っており（`src/queue.rs` の `OutputQueue::enqueue_with_plane`）、CAPTURE 完了時にドライバがタイムスタンプを 1:1 で搬送してくる
- `DecodedFrame::new` は搬送されたタイムスタンプを `timestamp_us` として保持しているが、`user_data` の対応付けにはタイムスタンプではなく `pending_values` の先頭を無条件で使っている

H.264 High profile などデコード順とプレゼンテーション順が異なるストリームでは、`user_data` と `DecodedFrame` の対応が入れ違う。Linux カーネルの V4L2 decoder interface 仕様（dev-decoder.rst）は「CAPTURE キューからデキューされるデコード済みフレームの順序は、OUTPUT キューへの符号化フレーム投入順と異なる場合がある（フレーム reordering 等による）」と定義しており、B フレームを含むストリームでは発生する。bcm2835-codec のエンコーダは B フレーム制御が無く既定で B フレームを生成しない想定（ファームウェア依存・未検証）なので encoder-decoder ラウンドトリップ（`tests/test_converter.rs` 相当）では顕在化しにくいが、`H264Decoder` は外部から任意の H.264 ストリームを受け取るため、B フレームを含むストリームでは発生する。

## 設計方針

- `pending_values` を `VecDeque<T>` から `BTreeMap<i64, VecDeque<T>>` に変更し、`timestamp_us` をキーに `user_data` を対応付ける（同一タイムスタンプは投入順に FIFO で対応する）
- `H264Decoder::decode` 側では、`OUTPUT` に QBUF する `timestamp_us` をキーとして `pending_values.entry(timestamp_us).or_default().push_back(user_data)` する
- `handle_capture` 側では、CAPTURE から取得した `timestamp_us` のキューから先頭の `user_data` を `pop_front` で取り出し、空になったらキーごと削除する
- V4L2 仕様は「1 つの OUTPUT バッファから複数の CAPTURE バッファが生成される（同一の OUTPUT タイムスタンプが複数の CAPTURE バッファにコピーされる）」ケースを定義している。bcm2835-codec（1 バッファ = 1 フレーム）では発生しないが、同一 `timestamp_us` を複数回投入するユーザーケース（公式 `skills/shiguredo-v4l2/SKILL.md` が `timestamp_us = 0` を例示）も従来は動作していたため、値に `VecDeque<T>` を持たせて FIFO で対応する
- 見つからないケース（`timestamp_us` に対応する `user_data` が無い）は、現状の `pop_front` が `None` のときと同様に `Error::NoAvailableBuffer` で処理する。`Error::PendingValueMismatch` への置き換えは `issues/0012-change-add-pending-value-mismatch-error.md` の担当とし、本 issue では `src/error.rs` を変更しない（0012 を前提として参照する）
- `encoder.rs` / `converter.rs` にも同型の `pending_values` FIFO パターンがある。converter は入力順 = 出力順で実害がなく、encoder も bcm2835-codec が B フレームを生成しない想定（上記）の範囲では実害がない。本 issue は decoder 単独に絞る。共通化リファクタリングは別 issue（`RequeueToken` / Frame 型の共通化と併せて）とする

## 完了条件

- B フレームを含む H.264 ストリーム（外部で生成した B フレーム入り H.264 ストリームを実機で feed する等）をデコードした際、`DecodedFrame::user_data()` が対応するフレームの `user_data` を返す（入力順と出力順が異なっても一致する）
- 通常の I / P フレームのみのストリームでも既存の挙動が変わらない
- `cargo test --workspace` および `cargo clippy --workspace --all-targets -- -D warnings` が成功する
- 実機（Raspberry Pi + bcm2835-codec）でエンコード → デコードのラウンドトリップ（`tests/test_converter.rs::test_converter_pipeline_all_mmap` 等）が引き続き成功する

## 変更対象

- `src/decoder.rs`（`DecoderRuntime.pending_values` の型変更、`H264Decoder::decode` と `handle_capture` の対応付けロジック）
- `CHANGES.md`（`[FIX]` として追加）
- `skills/shiguredo-v4l2/SKILL.md`（対応付けキーが `timestamp_us` になる旨、既知の制限事項として明記）
