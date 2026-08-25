# .devcontainer/setup.sh の cat >> による .cargo/config.toml 追記を冪等にする

- Created: 2026-08-21
- Completed:
- Branch: feature/fix-devcontainer-setup-idempotency
- Polished: 2026-08-23

## 目的

`.devcontainer/setup.sh` は `cat >> .cargo/config.toml <<'EOF' [build] target = ... EOF` の形で `.cargo/config.toml` に `[build]` セクションを追記するが、`postCreateCommand` が再実行されたり既存の `.cargo/config.toml` に `[build]` セクションが既にある場合、重複追記で TOML パースエラーになる。devcontainer の再ビルド（再作成）でセットアップが壊れる可能性があるため、冪等性を確保する。

## 現状

- `.devcontainer/setup.sh` は以下の 2 段構成:
  1. `cargo shiguredo-sysroot --config sysroot/raspberry-pi-os_armv8.json` で sysroot を構築し `.cargo/config.toml` を自動生成
  2. `cat >> .cargo/config.toml <<'EOF' [build] target = "aarch64-unknown-linux-gnu" EOF` で `[build]` セクションを追記
- 2 の `>>` は冪等でなく、再実行で `[build]` セクションが重複追加される
- TOML パーサーは重複セクションでエラーになる
- devcontainer の `postCreateCommand` はコンテナの再ビルド（再作成）で再実行される

## 設計方針

- `[build]` セクションの存在チェックを入れる:
  ```bash
  if ! grep -q '^\[build\]' .cargo/config.toml 2>/dev/null; then
      cat >> .cargo/config.toml <<'EOF'

[build]
target = "aarch64-unknown-linux-gnu"
EOF
  fi
  ```
- あるいは `cargo shiguredo-sysroot` に `.cargo/config.toml` 生成時に `[build]` セクションも含める機能を追加できれば、そもそも追記不要になる（別 issue で `cargo shiguredo-sysroot` 側の対応を検討）

## 完了条件

- `.devcontainer/setup.sh` を複数回連続実行しても `.cargo/config.toml` が TOML パースエラーにならない（`cargo build` が設定読み込みで失敗しない）
- devcontainer の初回セットアップは従来通り成功する（`cargo build` が成功する）
- 再ビルド（再作成）でも `[build]` セクションが 1 つだけ残る

## 変更対象

- `.devcontainer/setup.sh`
