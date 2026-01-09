#!/bin/bash
# Linux カーネルビルドスクリプト
# ハイパーバイザー用の最小構成 ARM64 カーネルをビルドする

set -e

KERNEL_VERSION="6.6"
KERNEL_DIR="/build/linux-${KERNEL_VERSION}"

echo "=== Linux kernel build for hypervisor ==="

# カーネルソースがなければ取得
if [ ! -d "$KERNEL_DIR" ]; then
    echo "Downloading Linux kernel v${KERNEL_VERSION}..."
    git clone --depth 1 --branch "v${KERNEL_VERSION}" \
        https://github.com/torvalds/linux.git "$KERNEL_DIR"
fi

cd "$KERNEL_DIR"

# defconfig をベースに最小構成を作成
echo "Creating minimal config..."
make ARCH=arm64 CROSS_COMPILE=aarch64-linux-gnu- defconfig

# 追加設定を適用
cat >> .config << 'EOF'
# UART console
CONFIG_SERIAL_AMBA_PL011=y
CONFIG_SERIAL_AMBA_PL011_CONSOLE=y

# Early printk
CONFIG_EARLY_PRINTK=y
CONFIG_EARLY_PRINTK_DIRECT=y

# GIC
CONFIG_ARM_GIC=y
CONFIG_ARM_GIC_V3=y

# Timer
CONFIG_ARM_ARCH_TIMER=y

# Disable unnecessary features for minimal boot
CONFIG_MODULES=n
CONFIG_NET=n
CONFIG_WLAN=n
CONFIG_WIRELESS=n
CONFIG_BT=n
CONFIG_SOUND=n
CONFIG_USB=n
CONFIG_INPUT=n
CONFIG_HID=n
CONFIG_DRM=n
CONFIG_FB=n
CONFIG_VGA_CONSOLE=n

# Enable debug
CONFIG_DEBUG_INFO=y
CONFIG_PRINTK=y
CONFIG_PRINTK_TIME=y

# Command line
CONFIG_CMDLINE="console=ttyAMA0 earlycon=pl011,0x09000000 loglevel=8"
CONFIG_CMDLINE_FORCE=y
EOF

# 設定を更新
make ARCH=arm64 CROSS_COMPILE=aarch64-linux-gnu- olddefconfig

# ビルド
echo "Building kernel..."
make ARCH=arm64 CROSS_COMPILE=aarch64-linux-gnu- Image -j$(nproc)

echo "=== Build complete ==="
echo "Kernel image: ${KERNEL_DIR}/arch/arm64/boot/Image"
ls -lh "${KERNEL_DIR}/arch/arm64/boot/Image"

# 出力ディレクトリにコピー
cp "${KERNEL_DIR}/arch/arm64/boot/Image" /output/Image
echo "Copied to /output/Image"
