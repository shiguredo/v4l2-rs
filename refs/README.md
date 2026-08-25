# refs

このリポジトリに関する一次資料（規格・RFC・IETF draft・カーネル仕様書など）を、出典の原文のまま保持する場所。

## 取得方法

一次資料の種類ごとに取得手順が異なる。

### Linux カーネルの V4L2 関連（`v4l2/`）

カーネル v6.12 をスパース・パーシャルクローンし、カーネル内のパスを保ったまま `refs/v4l2/` にコピーする。

```sh
TAG=v6.12
git clone --filter=blob:none --no-checkout --branch "$TAG" \
  https://github.com/torvalds/linux.git "$TMP/linux"
cd "$TMP/linux"
git sparse-checkout init --cone
git sparse-checkout set Documentation/userspace-api/media/v4l drivers/media/v4l2-core
git checkout
mkdir -p refs/v4l2
cp -a Documentation/userspace-api/media/v4l "refs/v4l2/Documentation/userspace-api/media/v4l"
cp -a drivers/media/v4l2-core "refs/v4l2/drivers/media/v4l2-core"
git show Documentation/userspace-api/media/gen-errors.rst \
  > refs/v4l2/Documentation/userspace-api/media/gen-errors.rst
```

`$TAG` で版を固定する。v6.12 は commit `adc218676eef25575469234709c2d87185ca223a` に相当する。
