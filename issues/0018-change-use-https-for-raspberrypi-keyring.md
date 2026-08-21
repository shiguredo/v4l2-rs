# raspberrypi-archive-keyring の取得を HTTP から HTTPS へ変更し署名検証を追加する

- Created: 2026-08-21
- Completed:
- Branch: feature/change-use-https-for-raspberrypi-keyring
- Polished:

## 目的

`Dockerfile.check` / `.devcontainer/Dockerfile` / `.github/workflows/ci.yml` の 3 箇所で `raspberrypi-archive-keyring_2021.1.1+rpt1_all.deb` を **HTTP** で取得して `dpkg -i` している。ローカル .deb の署名を検証しない経路のため、MITM されると以降の APT 署名検証全体の信頼起点が破綻する。HTTPS への変更と SHA256 固定によりサプライチェーン攻撃面を削減する。

## 現状

- `Dockerfile.check` に `curl -fsSL http://archive.raspberrypi.com/debian/pool/main/r/raspberrypi-archive-keyring/raspberrypi-archive-keyring_2021.1.1+rpt1_all.deb ... dpkg -i /tmp/rpi-keyring.deb` パターンがある
- `.devcontainer/Dockerfile` にも同一パターンがある
- `.github/workflows/ci.yml::build::Install dependencies` ステップにも同一パターンがある
- 3 箇所とも `http://archive.raspberrypi.com/` を使用しており、SHA256 検証も無い
- この keyring は以降の APT 署名検証の信頼起点になるため、ここが改竄されると Raspberry Pi 側の全パッケージが改竄可能

## 設計方針

- URL を `https://archive.raspberrypi.com/` に変更する
- 加えて keyring .deb の SHA256 を Dockerfile / workflow 内に固定して `sha256sum -c` で verify する
- あるいは Debian apt サーバー経由の署名済みパッケージインストール（`apt-get install raspberrypi-archive-keyring`）を検討する
- 3 箇所同時に修正し、内容が食い違わないようにする

## 完了条件

- 3 箇所とも HTTPS で keyring を取得する
- SHA256 検証が入る
- CI / devcontainer / Dockerfile.check ビルドが引き続き成功する

## 変更対象

- `Dockerfile.check`
- `.devcontainer/Dockerfile`
- `.github/workflows/ci.yml`
