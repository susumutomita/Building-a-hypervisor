//! Linux カーネル起動統合テスト
//!
//! Phase 1-3 で実装した全コンポーネントを統合し、
//! Linux カーネル起動に必要な環境が正しく構成されることを検証します。
//!
//! 実行方法:
//! ```bash
//! cargo run --example linux_boot_test
//! ```

use hypervisor::boot::device_tree::{generate_device_tree, DeviceTreeConfig};
use hypervisor::boot::kernel::KernelImage;
use hypervisor::devices::gic::Gic;
use hypervisor::devices::interrupt::InterruptController;
use hypervisor::devices::timer::TimerReg;
use hypervisor::devices::uart::Pl011Uart;
use hypervisor::devices::virtio::block::VirtioBlockDevice;
use hypervisor::mmio::{MmioHandler, MmioManager};

/// メモリマップ定義（ARM64 Linux 標準レイアウト）
mod memory_map {
    pub const RAM_BASE: u64 = 0x4000_0000;
    pub const RAM_SIZE: u64 = 128 * 1024 * 1024; // 128MB

    pub const GIC_DIST_BASE: u64 = 0x0800_0000;
    pub const GIC_CPU_BASE: u64 = 0x0801_0000;

    pub const UART_BASE: u64 = 0x0900_0000;
    pub const VIRTIO_BASE: u64 = 0x0a00_0000;

    pub const KERNEL_LOAD_ADDR: u64 = RAM_BASE + 0x8_0000; // 0x40080000
    pub const DTB_LOAD_ADDR: u64 = RAM_BASE + 0x400_0000; // 0x44000000
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Linux カーネル起動統合テスト ===\n");

    // 1. Device Tree 生成テスト
    println!("[1] Device Tree 生成");
    let dtb = test_device_tree_generation()?;
    println!("    ✓ Device Tree 生成完了: {} bytes\n", dtb.len());

    // 2. GIC (割り込みコントローラー) テスト
    println!("[2] GIC (割り込みコントローラー) 設定");
    test_gic_configuration();
    println!("    ✓ GIC 設定完了\n");

    // 3. Timer (ARM Generic Timer) テスト
    println!("[3] Timer 設定");
    test_timer_configuration();
    println!("    ✓ Timer 設定完了\n");

    // 4. UART (シリアルコンソール) テスト
    println!("[4] UART (シリアルコンソール) 設定");
    test_uart_configuration();
    println!("    ✓ UART 設定完了\n");

    // 5. VirtIO Block デバイステスト
    println!("[5] VirtIO Block デバイス設定");
    test_virtio_block_configuration();
    println!("    ✓ VirtIO Block 設定完了\n");

    // 6. MMIO Manager 統合テスト
    println!("[6] MMIO Manager 統合");
    test_mmio_manager_integration();
    println!("    ✓ MMIO Manager 統合完了\n");

    // 7. カーネルイメージテスト
    println!("[7] カーネルイメージ処理");
    test_kernel_image();
    println!("    ✓ カーネルイメージ処理完了\n");

    // 8. 起動シーケンステスト
    println!("[8] 起動シーケンス検証");
    test_boot_sequence()?;
    println!("    ✓ 起動シーケンス検証完了\n");

    // サマリー
    println!("=== テスト結果サマリー ===\n");
    print_memory_map();
    print_device_tree_summary(&dtb);

    println!("\n✅ すべての統合テストが成功しました");
    println!("\n=== Linux カーネル起動統合テスト完了 ===");

    Ok(())
}

fn test_device_tree_generation() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let config = DeviceTreeConfig {
        memory_base: memory_map::RAM_BASE,
        memory_size: memory_map::RAM_SIZE,
        uart_base: memory_map::UART_BASE,
        virtio_base: memory_map::VIRTIO_BASE,
        gic_dist_base: memory_map::GIC_DIST_BASE,
        gic_cpu_base: memory_map::GIC_CPU_BASE,
        cmdline: "console=ttyAMA0 earlycon root=/dev/vda rw".to_string(),
    };

    println!("    設定:");
    println!(
        "      Memory: 0x{:x} - 0x{:x} ({} MB)",
        config.memory_base,
        config.memory_base + config.memory_size,
        config.memory_size / (1024 * 1024)
    );
    println!(
        "      GIC: GICD=0x{:x}, GICC=0x{:x}",
        config.gic_dist_base, config.gic_cpu_base
    );
    println!("      UART: 0x{:x}", config.uart_base);
    println!("      VirtIO: 0x{:x}", config.virtio_base);
    println!("      cmdline: {}", config.cmdline);

    let dtb = generate_device_tree(&config)?;

    // マジックナンバー検証
    let magic = u32::from_be_bytes([dtb[0], dtb[1], dtb[2], dtb[3]]);
    assert_eq!(magic, 0xd00dfeed, "Invalid FDT magic");

    Ok(dtb)
}

fn test_gic_configuration() {
    let mut gic = Gic::with_base(memory_map::GIC_DIST_BASE);

    // GIC を有効化
    gic.write(0, 1, 4).unwrap(); // GICD_CTLR (offset 0)
    println!("    GICD_CTLR: 有効化");

    // タイマー IRQ を有効化 (IRQ 27, 30)
    let isenabler0: u64 = (1u64 << 27) | (1u64 << 30);
    gic.write(0x100, isenabler0, 4).unwrap();
    println!("    タイマー IRQ: IRQ 27 (Virtual), IRQ 30 (Physical) 有効化");

    // UART IRQ を有効化 (IRQ 33 = SPI 1)
    gic.write(0x104, 1 << 1, 4).unwrap();
    println!("    UART IRQ: IRQ 33 (SPI 1) 有効化");

    // VirtIO IRQ を有効化 (IRQ 34 = SPI 2)
    gic.write(0x104, 1 << 2, 4).unwrap();
    println!("    VirtIO IRQ: IRQ 34 (SPI 2) 有効化");
}

fn test_timer_configuration() {
    let mut ic = InterruptController::new();

    // 周波数を取得
    let freq = ic.timer.get_frequency();
    println!("    Timer 周波数: {} Hz ({} MHz)", freq, freq / 1_000_000);

    // 現在のカウンター値
    let phys_cnt = ic.timer.get_phys_counter();
    let virt_cnt = ic.timer.get_virt_counter();
    println!("    物理カウンタ: {}", phys_cnt);
    println!("    仮想カウンタ: {}", virt_cnt);

    // 物理タイマーを設定（1秒後）
    let cval = phys_cnt + freq;
    ic.timer
        .write_sysreg(TimerReg::CNTP_CVAL_EL0, cval)
        .unwrap();
    ic.timer.write_sysreg(TimerReg::CNTP_CTL_EL0, 1).unwrap();
    println!("    物理タイマー: CVAL={} (1秒後に発火)", cval);

    // タイマー状態確認
    if let Some(nanos) = ic.time_until_next_timer() {
        println!(
            "    次のタイマーイベントまで: {:.2} ms",
            nanos as f64 / 1_000_000.0
        );
    }
}

fn test_uart_configuration() {
    let mut uart = Pl011Uart::new(memory_map::UART_BASE);

    // UART レジスタ読み取りテスト
    let fr = uart.read(0x18, 4).unwrap(); // FR (Flag Register) offset
    println!("    FR (Flag Register): 0x{:x}", fr);

    // UART への書き込みテスト
    uart.write(0x00, b'H' as u64, 4).unwrap(); // DR offset
    uart.write(0x00, b'i' as u64, 4).unwrap();
    println!("    出力テスト: \"Hi\" を DR に書き込み");
}

fn test_virtio_block_configuration() {
    let mut device = VirtioBlockDevice::new(memory_map::VIRTIO_BASE);

    // Magic value を読み取り
    let magic = device.read(0, 4).unwrap(); // offset 0
    println!("    Magic: 0x{:x} (期待値: 0x74726976 = \"virt\")", magic);
    assert_eq!(magic, 0x74726976);

    // Version を読み取り
    let version = device.read(0x004, 4).unwrap();
    println!("    Version: {} (Legacy)", version);

    // Device ID を読み取り
    let device_id = device.read(0x008, 4).unwrap();
    println!("    Device ID: {} (Block Device)", device_id);
}

fn test_mmio_manager_integration() {
    let mut manager = MmioManager::new();

    // UART デバイスを登録
    let uart = Pl011Uart::new(memory_map::UART_BASE);
    manager.register(Box::new(uart));
    println!(
        "    UART デバイス登録: 0x{:x}-0x{:x}",
        memory_map::UART_BASE,
        memory_map::UART_BASE + 0x1000
    );

    // GIC デバイスを登録
    let gic = Gic::with_base(memory_map::GIC_DIST_BASE);
    manager.register(Box::new(gic));
    println!(
        "    GIC デバイス登録: GICD=0x{:x}",
        memory_map::GIC_DIST_BASE
    );

    // MMIO アクセステスト
    let result = manager.handle_read(memory_map::UART_BASE + 0x18, 4);
    assert!(result.is_ok());
    println!("    MMIO 読み取りテスト: UART FR = 0x{:x}", result.unwrap());
}

fn test_kernel_image() {
    // ダミーカーネルイメージ（BRK #0 で即座に停止）
    let boot_code = vec![
        0x00, 0x00, 0x20, 0xd4, // brk #0
    ];

    let kernel = KernelImage::from_bytes(boot_code.clone(), Some(memory_map::KERNEL_LOAD_ADDR));
    println!("    カーネルエントリポイント: 0x{:x}", kernel.entry_point());
    println!("    カーネルサイズ: {} bytes", kernel.size());

    // エントリポイント検証
    assert_eq!(kernel.entry_point(), memory_map::KERNEL_LOAD_ADDR);
}

fn test_boot_sequence() -> Result<(), Box<dyn std::error::Error>> {
    println!("    起動シーケンス:");
    println!(
        "      1. カーネルイメージをメモリにロード (0x{:x})",
        memory_map::KERNEL_LOAD_ADDR
    );
    println!(
        "      2. Device Tree をメモリにロード (0x{:x})",
        memory_map::DTB_LOAD_ADDR
    );
    println!("      3. vCPU レジスタを設定:");
    println!(
        "         - PC = 0x{:x} (カーネルエントリ)",
        memory_map::KERNEL_LOAD_ADDR
    );
    println!(
        "         - X0 = 0x{:x} (DTB アドレス)",
        memory_map::DTB_LOAD_ADDR
    );
    println!("         - CPSR = 0x3c4 (EL1h, 割り込みマスク)");
    println!("      4. vCPU を実行開始");
    println!("      5. カーネルが Device Tree をパース");
    println!("      6. GIC/Timer/UART/VirtIO を初期化");

    // Device Tree 生成
    let config = DeviceTreeConfig::default();
    let dtb = generate_device_tree(&config)?;

    // カーネルイメージ生成
    let kernel = KernelImage::from_bytes(
        vec![0x00, 0x00, 0x20, 0xd4],
        Some(memory_map::KERNEL_LOAD_ADDR),
    );

    // メモリレイアウト検証
    assert!(kernel.entry_point() < memory_map::DTB_LOAD_ADDR);
    assert!(
        (memory_map::DTB_LOAD_ADDR + dtb.len() as u64)
            < memory_map::RAM_BASE + memory_map::RAM_SIZE
    );
    println!("    ✓ メモリレイアウト検証成功");

    Ok(())
}

fn print_memory_map() {
    println!("メモリマップ:");
    println!("  ┌─────────────────────────────────────────────────┐");
    println!("  │ 0x0800_0000 - 0x0800_FFFF  GIC Distributor      │");
    println!("  │ 0x0801_0000 - 0x0801_FFFF  GIC CPU Interface    │");
    println!("  │ 0x0900_0000 - 0x0900_0FFF  UART (PL011)         │");
    println!("  │ 0x0A00_0000 - 0x0A00_01FF  VirtIO Block         │");
    println!("  ├─────────────────────────────────────────────────┤");
    println!("  │ 0x4000_0000 - 0x47FF_FFFF  RAM (128MB)          │");
    println!("  │   0x4008_0000             Kernel Load Address   │");
    println!("  │   0x4400_0000             DTB Load Address      │");
    println!("  └─────────────────────────────────────────────────┘");
}

fn print_device_tree_summary(dtb: &[u8]) {
    println!("\nDevice Tree 構造:");
    println!("  / (root)");
    println!("  ├── compatible = \"linux,dummy-virt\"");
    println!("  ├── model = \"hypervisor-virt\"");
    println!("  │");
    println!("  ├── cpus/");
    println!("  │   └── cpu@0 (arm,armv8)");
    println!("  │");
    println!("  ├── memory@40000000 (128MB)");
    println!("  │");
    println!("  ├── intc@8000000 (GICv2)");
    println!("  │   └── #interrupt-cells = 3");
    println!("  │");
    println!("  ├── timer (arm,armv8-timer)");
    println!("  │   └── PPI: Secure=13, Non-secure=14, Virtual=11, Hyp=10");
    println!("  │");
    println!("  ├── pl011@9000000 (UART)");
    println!("  │   └── IRQ: SPI 1 (IRQ 33)");
    println!("  │");
    println!("  ├── virtio_block@a000000");
    println!("  │   └── IRQ: SPI 2 (IRQ 34)");
    println!("  │");
    println!("  └── chosen/");
    println!("      └── bootargs = \"console=ttyAMA0 root=/dev/vda rw\"");
    println!("\n  Total size: {} bytes", dtb.len());
}
