# sys::v4l2_format_union::raw のサイズと V4L2_FORMAT_SIZE を静的アサートで検証する

- Created: 2026-08-21
- Completed:
- Branch: feature/add-v4l2-format-static-size-assert
- Polished:

## 目的

`src/sys.rs` は ioctl 番号計算で `V4L2_FORMAT_SIZE = 208` を手打ちし、`v4l2_format_union::raw: [u8; 200]` で union サイズを保証しているが、Rust 側 `v4l2_pix_format_mplane` のフィールドを追加・変更したときに 200 を超えたり `v4l2_format` 全体のサイズが 208 でなくなったりしても気付けない。静的アサートを追加してカーネル ABI との一致をコンパイル時に保証する。

## 現状

- `src/sys.rs` に `const V4L2_FORMAT_SIZE: u32 = 208;` の手打ち定数
- `src/sys.rs::v4l2_format_union` の `raw: [u8; 200]` はカーネル union サイズを確保するための便宜バリアント
- `V4L2_FORMAT_SIZE` は `VIDIOC_S_FMT` / `VIDIOC_G_FMT` の ioctl 番号計算に使われるため、サイズがずれると kernel が拒否する

## 設計方針

- `src/sys.rs` に以下の静的アサートを追加する:
  ```rust
  const _: () = assert!(std::mem::size_of::<v4l2_pix_format_mplane>() <= 200);
  const _: () = assert!(std::mem::size_of::<v4l2_format>() == V4L2_FORMAT_SIZE as usize);
  ```
- 他のサイズ手打ち定数（`V4L2_REQUESTBUFFERS_SIZE` / `V4L2_BUFFER_SIZE` / `V4L2_CONTROL_SIZE` / `V4L2_EXPORTBUFFER_SIZE` / `V4L2_EVENT_SUBSCRIPTION_SIZE` / `V4L2_EVENT_SIZE`）も同様に静的アサートで検証する
- `issues/0023-change-align-v4l2-format-union-to-8-bytes.md` の align 修正と同時に対応するのが効率的
- `std::mem::size_of` は `const` context で使えるため `assert!` マクロで検証可能（Rust 1.85+）

## 完了条件

- `v4l2_format` / `v4l2_requestbuffers` / `v4l2_buffer` / `v4l2_control` / `v4l2_exportbuffer` / `v4l2_event_subscription` / `v4l2_event` のサイズがそれぞれ対応する手打ち定数と一致することが静的にアサートされる
- 手打ち定数と Rust 側構造体サイズが不一致な場合にコンパイル時にエラーになる
- `cargo test --workspace` および `cargo clippy --workspace --all-targets -- -D warnings` が成功する

## 変更対象

- `src/sys.rs`（静的アサートの追加）
