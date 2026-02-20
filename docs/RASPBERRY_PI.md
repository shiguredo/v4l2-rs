# Raspberry Pi

## Raspberry Pi 5 について

Raspberry Pi 5 (BCM2712) にはハードウェアビデオエンコーダー/デコーダーが搭載されていません。
そのため、このライブラリが利用する bcm2835-codec (`/dev/video10`, `/dev/video11`) は Raspberry Pi 5 では使用できません。

## 対応機種

bcm2835-codec が利用可能な以下の機種に対応しています。

- Raspberry Pi 4 Model B (BCM2711)
- Raspberry Pi 3 Model B / B+ (BCM2837)
- Raspberry Pi 2 Model B (BCM2836)
- Raspberry Pi Zero 2 W (BCM2710)
