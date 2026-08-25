# sys::v4l2_format_union のアライメントを C 側 (8) に合わせる

- Created: 2026-08-21
- Completed:
- Branch: feature/change-align-v4l2-format-union-to-8-bytes
- Polished:

## 目的

`src/sys.rs::v4l2_format` は手動 `_pad: u32` で `fmt` フィールドのオフセットを C 側の 8 バイト境界に合わせているが、Rust 側 `v4l2_format_union` のアライメントは 4（`v4l2_pix_format_mplane` の align 4、`[u8; 200]` の align 1 で決まる）で、C 側の align 8（union に `v4l2_window` の `*mut v4l2_rect` を含むため）と異なる。現状のフィールドアクセスは正しく動くが、将来の union 拡張や配列化で潜在バグの余地があるため、明示的に align 8 を指定する。

## 現状

- `src/sys.rs::v4l2_format` は `#[repr(C)]` で `type: u32` / `_pad: u32` / `fmt: v4l2_format_union` の構成
- `src/sys.rs::v4l2_format_union` は `#[repr(C)] union { pix_mp: v4l2_pix_format_mplane, raw: [u8; 200] }` で `align = max(4, 1) = 4`
- C 側 `union v4l2_format::fmt` は `v4l2_window` を含むため align 8（`v4l2_window` に `struct v4l2_rect *r` があるため 64-bit target で 8 バイトアライン）
- 現状は `_pad: u32` の手動追加で `fmt` の開始オフセットが 8 バイト境界になっているため個別フィールドアクセスは正しく動く
- ただし `[v4l2_format; N]` のような配列化や、将来 union に 8 バイトアラインなフィールドを追加した場合にレイアウトがずれる

## 設計方針

- `v4l2_format_union` に `#[repr(C, align(8))]` を明示する
- または `v4l2_format` 自体に `#[repr(C, align(8))]` を付け、`_pad: u32` を削除して自然なパディングに任せる
- どちらの方針も layout 変更を伴うため、実装後に静的アサート（`const _: () = assert!(offset_of!(v4l2_format, fmt) == 8);`）を追加する
- 併せて `issues/0024-add-v4l2-format-static-size-assert.md` の静的サイズ検証と同時に対応するのが効率的

## 完了条件

- Rust 側 `v4l2_format` のレイアウトが C 側 `struct v4l2_format` と一致する
- 静的アサートで検証される
- `cargo test --workspace` および `cargo clippy --workspace --all-targets -- -D warnings` が成功する
- 実機で S_FMT / G_FMT 系 ioctl が引き続き正常動作する

## 変更対象

- `src/sys.rs`（`v4l2_format` / `v4l2_format_union` のアライメント指定）
- `CHANGES.md`（`[CHANGE]` として追加。ABI 変更のためユーザーへの影響を明記）
