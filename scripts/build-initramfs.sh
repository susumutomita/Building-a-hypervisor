#!/bin/bash
# initramfs ビルドスクリプト
# BusyBox を含むミニマルなルートファイルシステムを作成する

set -e

BUSYBOX_VERSION="1.36.1"
BUILD_DIR="/build/initramfs"
OUTPUT_DIR="/output"

echo "=== Building initramfs with BusyBox ==="

# ビルドディレクトリを作成
mkdir -p "$BUILD_DIR"
cd "$BUILD_DIR"

# BusyBox をダウンロード
if [ ! -f "busybox-${BUSYBOX_VERSION}.tar.bz2" ]; then
    echo "Downloading BusyBox ${BUSYBOX_VERSION}..."
    wget -q "https://busybox.net/downloads/busybox-${BUSYBOX_VERSION}.tar.bz2"
fi

# 展開
if [ ! -d "busybox-${BUSYBOX_VERSION}" ]; then
    echo "Extracting BusyBox..."
    tar xjf "busybox-${BUSYBOX_VERSION}.tar.bz2"
fi

cd "busybox-${BUSYBOX_VERSION}"

# ARM64 向け静的リンクビルドの設定
echo "Configuring BusyBox..."
make ARCH=arm64 CROSS_COMPILE=aarch64-linux-gnu- defconfig

# 静的リンクを有効化
sed -i 's/# CONFIG_STATIC is not set/CONFIG_STATIC=y/' .config

# ビルド
echo "Building BusyBox..."
make ARCH=arm64 CROSS_COMPILE=aarch64-linux-gnu- -j$(nproc)

# initramfs のルートディレクトリを作成
INITRAMFS_ROOT="$BUILD_DIR/rootfs"
rm -rf "$INITRAMFS_ROOT"
mkdir -p "$INITRAMFS_ROOT"

# BusyBox をインストール
echo "Installing BusyBox to rootfs..."
make ARCH=arm64 CROSS_COMPILE=aarch64-linux-gnu- CONFIG_PREFIX="$INITRAMFS_ROOT" install

# 必要なディレクトリを作成
cd "$INITRAMFS_ROOT"
mkdir -p proc sys dev etc tmp run var/log

# /dev の基本デバイスノードを作成
echo "Creating device nodes..."
mknod -m 622 dev/console c 5 1
mknod -m 666 dev/null c 1 3
mknod -m 666 dev/zero c 1 5
mknod -m 666 dev/tty c 5 0
mknod -m 666 dev/ttyAMA0 c 204 64

# init スクリプトを作成
echo "Creating init script..."
cat > init << 'INIT_EOF'
#!/bin/sh

echo "=== initramfs init starting ==="

# 基本的なファイルシステムをマウント
mount -t proc none /proc
mount -t sysfs none /sys
mount -t devtmpfs none /dev 2>/dev/null || true

# ホスト名を設定
hostname hypervisor-vm

echo ""
echo "  _    _                             _                "
echo " | |  | |                           (_)               "
echo " | |__| |_   _ _ __   ___ _ ____   ___ ___  ___  _ __ "
echo " |  __  | | | | '_ \ / _ \ '__\ \ / / / __|/ _ \| '__|"
echo " | |  | | |_| | |_) |  __/ |   \ V /| \__ \ (_) | |   "
echo " |_|  |_|\__, | .__/ \___|_|    \_/ |_|___/\___/|_|   "
echo "          __/ | |                                     "
echo "         |___/|_|     Linux on Custom Hypervisor      "
echo ""
echo "Welcome to the hypervisor VM!"
echo ""

# シェルを起動
exec /bin/sh
INIT_EOF

chmod +x init

# initramfs を cpio アーカイブとして作成
echo "Creating initramfs.cpio.gz..."
find . -print0 | cpio --null -ov --format=newc 2>/dev/null | gzip -9 > "$OUTPUT_DIR/initramfs.cpio.gz"

echo ""
echo "=== initramfs build complete ==="
ls -lh "$OUTPUT_DIR/initramfs.cpio.gz"
