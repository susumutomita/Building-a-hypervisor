# Linux カーネルビルド手順

macOS で ARM64 Linux カーネルをクロスコンパイルする手順。

## 前提条件

### クロスコンパイラのインストール

```bash
# Homebrew で ARM64 クロスコンパイラをインストール
brew install aarch64-linux-gnu-binutils
brew install aarch64-linux-gnu-gcc

# または LLVM/Clang を使用 (推奨)
brew install llvm
export PATH="/opt/homebrew/opt/llvm/bin:$PATH"
```

## カーネルソースの取得

```bash
# Linux カーネル v6.x をダウンロード
git clone --depth 1 --branch v6.6 https://github.com/torvalds/linux.git linux-6.6
cd linux-6.6
```

## 最小構成の defconfig

```bash
# ARM64 向け最小構成を作成
make ARCH=arm64 CROSS_COMPILE=aarch64-linux-gnu- defconfig

# または LLVM を使用
make ARCH=arm64 LLVM=1 defconfig
```

### 推奨カーネル設定

```bash
# menuconfig で設定を調整
make ARCH=arm64 CROSS_COMPILE=aarch64-linux-gnu- menuconfig
```

必要な設定:
- `CONFIG_SERIAL_AMBA_PL011=y` - PL011 UART ドライバ
- `CONFIG_SERIAL_AMBA_PL011_CONSOLE=y` - PL011 コンソール
- `CONFIG_ARM_GIC=y` - GIC 割り込みコントローラ
- `CONFIG_ARM_ARCH_TIMER=y` - ARM Generic Timer
- `CONFIG_EARLY_PRINTK=y` - early printk
- `CONFIG_CMDLINE="console=ttyAMA0 earlycon"` - デフォルトコマンドライン

無効化推奨:
- `CONFIG_MODULES=n` - モジュールサポート（不要）
- `CONFIG_NETWORK=n` - ネットワーク（不要）
- `CONFIG_BLOCK=n` - ブロックデバイス（初期テストでは不要）

## カーネルビルド

```bash
# GNU ツールチェインでビルド
make ARCH=arm64 CROSS_COMPILE=aarch64-linux-gnu- Image -j$(nproc)

# または LLVM でビルド
make ARCH=arm64 LLVM=1 Image -j$(nproc)
```

生成されるファイル:
- `arch/arm64/boot/Image` - 非圧縮カーネルイメージ

## Docker を使用したビルド (推奨)

macOS でクロスコンパイラの設定が困難な場合、Docker を使用すると簡単です。

```bash
# Dockerfile
cat > Dockerfile.linux-build << 'EOF'
FROM ubuntu:22.04
RUN apt-get update && apt-get install -y \
    build-essential \
    gcc-aarch64-linux-gnu \
    binutils-aarch64-linux-gnu \
    bison \
    flex \
    libncurses-dev \
    libssl-dev \
    libelf-dev \
    bc \
    git
WORKDIR /linux
EOF

# ビルド
docker build -t linux-build -f Dockerfile.linux-build .

# カーネルソースをマウントしてビルド
docker run -v $(pwd)/linux-6.6:/linux linux-build \
    make ARCH=arm64 CROSS_COMPILE=aarch64-linux-gnu- defconfig Image -j$(nproc)
```

## ハイパーバイザーでの起動

```rust
use hypervisor::{Hypervisor, boot::kernel::KernelImage};

// カーネルイメージを読み込み
let kernel_data = std::fs::read("linux-6.6/arch/arm64/boot/Image")?;
let kernel = KernelImage::from_bytes(kernel_data, Some(0x4008_0000));

// ハイパーバイザーを作成
let mut hv = Hypervisor::new(0x4000_0000, 128 * 1024 * 1024)?;

// UART デバイスを登録
let uart = hypervisor::devices::uart::Pl011Uart::new(0x0900_0000);
hv.register_mmio_handler(Box::new(uart));

// カーネルを起動
let result = hv.boot_linux(&kernel, "console=ttyAMA0 earlycon", None)?;
```

## トラブルシューティング

### UART 出力がない場合

1. Device Tree の UART ノードが正しいか確認
2. カーネルコマンドラインに `earlycon` が含まれているか確認
3. UART ベースアドレス (0x0900_0000) が正しいか確認

### カーネルがハングする場合

1. `EC` コード（例外クラス）を確認
2. 未対応のシステムレジスタアクセスがないか確認
3. WFI/WFE が正しくハンドリングされているか確認

### VFS マウント失敗で panic

これは正常な動作です。initramfs がない場合、カーネルは VFS マウントに失敗して panic します。
earlycon 出力まで到達していれば、ハイパーバイザーは正常に動作しています。

## 期待される出力

正常に起動した場合、UART に以下のような出力が表示されます。

```
Booting Linux on physical CPU 0x0
Linux version 6.6.0 ...
earlycon: pl011 at MMIO 0x09000000 ...
Machine: linux,dummy-virt
...
Kernel panic - not syncing: VFS: Unable to mount root fs on unknown-block(0,0)
```

"Kernel panic - VFS" は initramfs がないことによる正常な終了です。
