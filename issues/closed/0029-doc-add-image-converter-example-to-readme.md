# README.md に ImageConverter のサンプルと /dev/video12 の説明を追加する

- Created: 2026-08-21
- Completed: 2026-08-25
- Branch: feature/update-readme-image-converter-example
- Polished:

## 目的

`README.md` は H.264 エンコード / デコード / DMABUF 入力 / エンコーダー設定のサンプルを掲載しているが、`ImageConverter`（`/dev/video12`）のサンプルと言及が無い。`SKILL.md` には節が立っているのに README で認知できないため、crates.io を見るユーザーに ImageConverter の存在が伝わらない。README に追加する。

## 現状

- `README.md::特徴` に `/dev/video12` の記載が無い
- `README.md::使い方` に `H.264 エンコード` / `H.264 デコード` / `DMABUF 入力` / `エンコーダー設定` の 4 サンプルはあるが `ImageConverter` は無い
- `skills/shiguredo-v4l2/SKILL.md` には `画像変換器 (ImageConverter<T>)` の節が立っている
- crates.io は README をそのまま表示するため、README に無い機能はユーザーが発見できない

## 設計方針

- `README.md::特徴` に `/dev/video12` の画像変換（スケーリング / I420 ⇔ NV12）の記載を追加する
- `README.md::使い方` に `ImageConverter` の最小サンプルを追加する（`ConverterConfig::new(...)` → `ImageConverter::new(...)` → `convert(...)` の流れ）
- 他のサンプルと同じ粒度・スタイルで書く
- サンプルは docstring 化して doctest で検証するかは別途検討（本 issue のスコープ外）

## 完了条件

- README に ImageConverter の節と `/dev/video12` の言及が入る
- crates.io の crate ページから ImageConverter の存在が確認できる

## 変更対象

- `README.md`

## 解決方法

- `README.md::特徴` に画像変換 (`/dev/video12`) - スケーリングと I420 ⇔ NV12 相互変換 を追加し、`/dev/video12` の存在を認知できるようにした
- `README.md::使い方` に `### 画像変換` の節を追加し、`ConverterConfig::new(...)` → `ImageConverter::new(...)` → `convert(...)` の最小サンプルを掲載した
- crates.io の crate ページから ImageConverter の存在を確認できる状態になった
