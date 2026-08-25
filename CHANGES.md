# 変更履歴

- CHANGE
  - 後方互換のない変更
- ADD
  - 後方互換がある追加
- UPDATE
  - 後方互換がある変更
- FIX
  - バグ修正

## develop

- [CHANGE] MSRV (rust-version) を 1.93 に上げる
  - @voluntas
- [FIX] 極端に大きな解像度で `yuv420_size()` が算術オーバーフローでパニックする問題を修正する
  - @voluntas
- [FIX] `yuv420_size()` が奇数 stride / height で平面分割と必要なバイト数が一致しない問題を修正する
  - @melpon
- [FIX] Poller がアイドル状態の `POLLERR` で busy loop に陥る問題を修正し、`POLLHUP` / `POLLNVAL` はデバイス切断として終了するようにする
  - @melpon

### misc

- [ADD] フォーマット変換 API のパニック耐性を検証する fuzz ターゲットを追加する
  - @voluntas
- [UPDATE] prek の cargo test を pre-push のみで実行するように変更し、cargo clippy に `--all-targets` を追加する
  - @voluntas

## 2026.1.0

**リリース日**: 2026-06-23
