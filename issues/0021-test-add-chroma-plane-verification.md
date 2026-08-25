# tests/test_converter.rs で chroma plane (U/V) の検証を追加し回帰検出能力を強化する

- Created: 2026-08-21
- Completed:
- Branch: feature/add-chroma-plane-verification
- Polished:

## 目的

`tests/test_converter.rs` の統合パイプラインテスト（I420 → NV12 → H.264 → I420 → I420）は `y_plane_mae` で Y 面のみの平均絶対誤差を検証しており、chroma plane（U/V）を完全に無視している。NV12 の interleave（UVUV）と I420 の planar（UU..VV..）の変換ミス、U と V の入れ替わり、chroma stride の取り扱いミスが silent pass するため、回帰検出能力が本来の 1/3 程度に留まる。chroma 検証を追加する。

## 現状

- `tests/test_converter.rs::y_plane_mae` は Y 面のみ MAE を計算する関数
- テストの assert は `y_plane_mae(...) <= threshold` の 1 種類のみ
- NV12（semi-planar）と I420（planar）の 4 段変換で chroma 配置が変わるが、Y だけを見るテストではその配置ミスを検出できない
- SKILL.md にも「CAPTURE プレーン数 = 1 前提」と書かれており、chroma 配置は Y と同じバッファ内の後半に置かれる想定

## 設計方針

- U plane / V plane それぞれの MAE を計算する関数 `u_plane_mae` / `v_plane_mae` を追加する
- NV12 / I420 それぞれのプレーン切り出しロジックを共通化してテストヘルパー化
- assert を Y / U / V の 3 threshold で行う
- U と V を入れ替えるバグを検出するため、`assert!(u_mae < threshold)` と `assert!(v_mae < threshold)` を分けて書く
- 実機テストのみで走る点は変わらない（`#[ignore]` は別 issue で追加）

## 完了条件

- Y / U / V の 3 プレーンそれぞれの MAE が assert される
- U/V 入れ替わりバグが混入した場合、テストが失敗する
- 通常のラウンドトリップでは全ての assert が通る
- `cargo test --workspace -- --ignored`（Raspberry Pi 上）が引き続き pass する

## 変更対象

- `tests/test_converter.rs`（`y_plane_mae` の拡張、chroma プレーン検証の追加）
