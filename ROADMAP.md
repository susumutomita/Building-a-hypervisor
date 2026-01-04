# Firecracker-like MicroVM 実装ロードマップ

現在の基本的なハイパーバイザーから、Firecracker のようなマイクロ VM を実装するための段階的なロードマップ。

## 参考資料

- [Firecracker Design](https://github.com/firecracker-microvm/firecracker/blob/main/docs/design.md)
- [ARM64 Linux Booting](https://docs.kernel.org/arch/arm64/booting.html)
- [Firecracker Architecture](https://firecracker-microvm.github.io/)

## Phase 1: 最小限の Linux ブート（2-3ヶ月）

**目標**: 小さな Linux カーネルを起動してシリアルコンソールに "Hello from Linux!" を出力する。

### 1.1 シリアルコンソール（UART）エミュレーション

**実装内容**:
- PL011 UART デバイスのエミュレーション
- MMIO（Memory-Mapped I/O）ハンドリング
- 出力バッファの実装

**技術的詳細**:
```rust
// UART レジスタ
const UART_BASE: u64 = 0x0900_0000;
const UART_DR: u64 = UART_BASE + 0x00;   // Data Register
const UART_FR: u64 = UART_BASE + 0x18;   // Flag Register

// VM Exit 時の MMIO アクセス処理
if fault_address == UART_DR {
    // ゲストからの文字を stdout に出力
    print!("{}", data as u8 as char);
}
```

**参考資料**:
- [PL011 UART Technical Reference Manual](https://developer.arm.com/documentation/ddi0183/latest/)

### 1.2 Device Tree の生成

**実装内容**:
- FDT（Flattened Device Tree）の生成
- メモリ、CPU、UART、割り込みコントローラの記述
- カーネルに渡すための配置

**Device Tree の例**:
```dts
/dts-v1/;

/ {
    compatible = "linux,dummy-virt";
    #address-cells = <2>;
    #size-cells = <2>;

    chosen {
        bootargs = "console=ttyAMA0 earlycon";
        stdout-path = "/pl011@9000000";
    };

    cpus {
        #address-cells = <1>;
        #size-cells = <0>;

        cpu@0 {
            device_type = "cpu";
            compatible = "arm,armv8";
            reg = <0>;
        };
    };

    memory@40000000 {
        device_type = "memory";
        reg = <0x0 0x40000000 0x0 0x8000000>; // 128MB
    };

    pl011@9000000 {
        compatible = "arm,pl011", "arm,primecell";
        reg = <0x0 0x09000000 0x0 0x1000>;
        interrupts = <0 1 4>;
    };
};
```

**Rust での実装**:
```rust
use devicetree::{Devicetree, Node, Property};

fn create_device_tree() -> Vec<u8> {
    let mut dt = Devicetree::new();

    // CPU ノード
    dt.add_node("/cpus/cpu@0", vec![
        Property::new("device_type", "cpu"),
        Property::new("compatible", "arm,armv8"),
    ]);

    // メモリノード
    dt.add_node("/memory@40000000", vec![
        Property::new("device_type", "memory"),
        Property::new_u64("reg", vec![0x40000000, 0x8000000]),
    ]);

    // UART ノード
    dt.add_node("/pl011@9000000", vec![
        Property::new("compatible", "arm,pl011\0arm,primecell"),
        Property::new_u64("reg", vec![0x09000000, 0x1000]),
    ]);

    dt.to_blob()
}
```

### 1.3 Linux カーネルのロード

**実装内容**:
- ARM64 Linux カーネルイメージの読み込み
- カーネルを適切なメモリ位置に配置
- Entry point（0x40080000 など）への PC 設定

**カーネルビルド**:
```bash
# 最小限の Linux カーネルビルド
git clone --depth 1 https://github.com/torvalds/linux.git
cd linux
make ARCH=arm64 CROSS_COMPILE=aarch64-linux-gnu- defconfig
make ARCH=arm64 CROSS_COMPILE=aarch64-linux-gnu- -j$(nproc)
# arch/arm64/boot/Image が生成される
```

**ロード実装**:
```rust
impl Hypervisor {
    pub fn load_kernel(&mut self, kernel_path: &str) -> Result<u64, Box<dyn Error>> {
        let kernel_data = std::fs::read(kernel_path)?;
        let kernel_base = 0x40080000;

        // カーネルをメモリに書き込み
        for (i, &byte) in kernel_data.iter().enumerate() {
            self.mem.write_byte(kernel_base + i as u64, byte)?;
        }

        Ok(kernel_base)
    }
}
```

### 1.4 ブート条件の設定

**必要な設定** ([ARM64 Booting Requirements](https://docs.kernel.org/arch/arm64/booting.html)):

1. **Exception Level**: EL1（現在実装済み）
2. **MMU**: オフ
3. **割り込み**: PSTATE.DAIF でマスク
4. **レジスタ設定**:
   - X0: Device Tree の物理アドレス
   - X1-X3: 0（予約）

```rust
impl Hypervisor {
    pub fn boot_linux(&self, dtb_addr: u64, kernel_entry: u64) -> Result<(), Box<dyn Error>> {
        // X0 に Device Tree アドレスを設定
        self.vcpu.set_reg(Reg::X0, dtb_addr)?;
        self.vcpu.set_reg(Reg::X1, 0)?;
        self.vcpu.set_reg(Reg::X2, 0)?;
        self.vcpu.set_reg(Reg::X3, 0)?;

        // PC をカーネル entry point に設定
        self.vcpu.set_reg(Reg::PC, kernel_entry)?;

        // CPSR: EL1h, MMU off, 割り込みマスク
        // DAIF ビット (bits 9-6) をすべて 1 に設定
        let cpsr = 0x3c5; // EL1h + DAIF masked
        self.vcpu.set_reg(Reg::CPSR, cpsr)?;

        // 実行
        loop {
            let exit_info = self.vcpu.run()?;

            match exit_info.exit_reason {
                ExitReason::Exception => {
                    // MMIO アクセス（UART など）を処理
                    self.handle_mmio(&exit_info)?;
                }
                _ => break,
            }
        }

        Ok(())
    }
}
```

### 1.5 MMIO ハンドリング

**実装内容**:
- Data Abort（メモリアクセス例外）のキャッチ
- ESR_EL2 から MMIO アドレスを取得
- デバイス（UART など）へのディスパッチ

```rust
impl Hypervisor {
    fn handle_mmio(&self, exit_info: &VmExitInfo) -> Result<(), Box<dyn Error>> {
        if let Some(syndrome) = exit_info.exception_syndrome {
            let ec = (syndrome >> 26) & 0x3f;

            // Data Abort from lower EL (0x24)
            if ec == 0x24 {
                let fault_addr = self.vcpu.get_sys_reg(SysReg::FAR_EL1)?;
                let is_write = (syndrome & (1 << 6)) != 0;

                match fault_addr {
                    UART_DR => {
                        if is_write {
                            let data = self.vcpu.get_reg(Reg::X0)?;
                            print!("{}", (data & 0xff) as u8 as char);
                            std::io::stdout().flush()?;
                        }
                    }
                    _ => {
                        eprintln!("Unknown MMIO access: 0x{:x}", fault_addr);
                    }
                }
            }
        }

        Ok(())
    }
}
```

### 1.6 検証とデバッグ

**テスト手順**:

1. 最小限のカーネルビルド
2. Device Tree の生成
3. ハイパーバイザーでの起動
4. シリアル出力の確認

**期待される出力**:
```
[    0.000000] Booting Linux on physical CPU 0x0
[    0.000000] Linux version 6.x.x ...
[    0.000000] Machine model: linux,dummy-virt
...
[    0.100000] Hello from Linux!
```

## Phase 2: VirtIO Block デバイス（1-2ヶ月）

**目標**: VirtIO Block デバイスでルートファイルシステムをマウント。

### 2.1 VirtIO の基礎

**実装内容**:
- VirtIO PCI デバイスのエミュレーション
- Virtqueue の実装
- Descriptor、Available Ring、Used Ring

### 2.2 Block デバイス

**実装内容**:
- VirtIO Block デバイスの実装
- Read/Write リクエストの処理
- ホストファイルをバッキングストアとして使用

### 2.3 initramfs のマウント

**実装内容**:
- 最小限の initramfs 作成
- カーネルに initramfs を渡す
- /init スクリプトの実行

## Phase 3: VirtIO Net デバイス（1-2ヶ月）

**目標**: ネットワーク通信を可能にする。

### 3.1 VirtIO Net デバイス

**実装内容**:
- VirtIO Net デバイスのエミュレーション
- TAP デバイスとのブリッジ
- パケット送受信

### 3.2 ネットワークスタック

**実装内容**:
- TAP デバイスの作成（macOS では utun）
- ルーティングの設定
- DHCP クライアント（ゲスト側）

## Phase 4: API サーバーとセキュリティ（1ヶ月）

**目標**: REST API で VM を管理し、セキュリティを強化。

### 4.1 API サーバー

**実装内容**:
- HTTP API サーバー（Actix-web など）
- VM 作成/起動/停止のエンドポイント
- 設定管理（JSON）

### 4.2 セキュリティ

**実装内容**:
- Seccomp フィルタの実装（macOS では Sandbox）
- リソース制限
- 特権分離

## Phase 5: 最適化とパフォーマンス（継続的）

**目標**: 起動時間とメモリ使用量を最小化。

### 5.1 高速起動

**実装内容**:
- カーネル/initramfs の事前ロード
- デバイス初期化の並列化
- Lazy な初期化

### 5.2 メモリ最適化

**実装内容**:
- メモリバルーニング
- KSM（Kernel Same-page Merging）相当の機能
- オンデマンドページング

## マイルストーン

| Phase | 期間 | 達成目標 |
|-------|------|----------|
| Phase 1 | 2-3ヶ月 | Linux カーネルブート成功 |
| Phase 2 | 1-2ヶ月 | ルートファイルシステムマウント |
| Phase 3 | 1-2ヶ月 | ネットワーク通信可能 |
| Phase 4 | 1ヶ月 | API 経由の VM 管理 |
| Phase 5 | 継続的 | パフォーマンス向上 |

**合計**: 約 6-9ヶ月で Firecracker レベルの機能を実現

## 次のステップ

1. Phase 1.1（UART エミュレーション）から開始
2. 各フェーズごとに別 PR を作成
3. テストとドキュメントを並行して作成
4. ブログ記事で進捗を共有

## 参考実装

- [rust-vmm](https://github.com/rust-vmm) - Rust で書かれた仮想化コンポーネント
- [crosvm](https://chromium.googlesource.com/chromiumos/platform/crosvm/) - Chrome OS の VMM
- [cloud-hypervisor](https://github.com/cloud-hypervisor/cloud-hypervisor) - Rust 製 VMM
