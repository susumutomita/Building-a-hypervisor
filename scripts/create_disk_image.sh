#!/bin/bash
# ディスクイメージ作成スクリプト
#
# 使い方: ./scripts/create_disk_image.sh [サイズMB] [出力ファイル]

set -e

# デフォルト値
SIZE_MB=${1:-64}
OUTPUT=${2:-disk.img}

echo "=== VirtIO Block ディスクイメージ作成 ==="
echo "サイズ: ${SIZE_MB}MB"
echo "出力: ${OUTPUT}"

# 1. 空のディスクイメージを作成
echo ""
echo "[1] 空のディスクイメージを作成中..."
dd if=/dev/zero of="${OUTPUT}" bs=1M count="${SIZE_MB}" status=progress

# 2. ディスクイメージのサイズを確認
echo ""
echo "[2] 作成されたディスクイメージ:"
ls -lh "${OUTPUT}"

# 3. ディスクイメージの情報を表示
echo ""
echo "[3] ディスクイメージ情報:"
FILE_SIZE=$(stat -f%z "${OUTPUT}" 2>/dev/null || stat -c%s "${OUTPUT}")
SECTOR_SIZE=512
SECTORS=$((FILE_SIZE / SECTOR_SIZE))
echo "  - ファイルサイズ: ${FILE_SIZE} bytes"
echo "  - セクタサイズ: ${SECTOR_SIZE} bytes"
echo "  - セクタ数: ${SECTORS}"

echo ""
echo "✅ ディスクイメージが正常に作成されました: ${OUTPUT}"
echo ""
echo "次のステップ:"
echo "  1. Linux カーネルをビルド（ARM64）"
echo "  2. BusyBox ベースの initramfs を作成"
echo "  3. ディスクイメージに rootfs を配置"
echo ""
echo "参考:"
echo "  - Linux カーネルビルド: https://www.kernel.org/doc/html/latest/admin-guide/README.html"
echo "  - BusyBox: https://www.busybox.net/"
