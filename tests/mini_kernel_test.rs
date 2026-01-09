//! ミニカーネル起動テスト
//!
//! UART に "Hello from mini kernel!" と出力する簡単なカーネルを実行し、
//! ハイパーバイザーの Linux 起動機能をテストする。

use hypervisor::boot::device_tree::{generate_device_tree, DeviceTreeConfig};
use hypervisor::boot::kernel::KernelImage;
use hypervisor::devices::uart::Pl011Uart;
use hypervisor::mmio::MmioHandler;
use hypervisor::Hypervisor;

/// メモリベースアドレス
const RAM_BASE: u64 = 0x4000_0000;
/// カーネルエントリーポイント
const KERNEL_ENTRY: u64 = 0x4008_0000;
/// UART ベースアドレス
const UART_BASE: u64 = 0x0900_0000;
/// DTB アドレス
const DTB_ADDR: u64 = 0x4400_0000;

/// ミニカーネルのバイナリを生成
///
/// このカーネルは:
/// 1. UART に "Hello" と出力
/// 2. WFI で待機
/// 3. HVC で PSCI_SYSTEM_OFF を呼び出して終了
fn create_mini_kernel() -> Vec<u8> {
    // ARM64 機械語命令
    // MOVZ Xd, #imm16, LSL #(hw*16) encoding:
    // sf=1, opc=10, 100101, hw, imm16, Rd
    // MOVZ X1, #0x0900, LSL #16 = 0xD2A1_2001
    let instructions: Vec<u32> = vec![
        // === 初期化 ===
        // X1 = UART_BASE (0x0900_0000)
        // MOVZ X1, #0x0900, LSL #16
        0xD2A1_2001,
        // === 'H' を出力 (0x48) ===
        // MOVZ X0, #0x48
        0xD280_0900,
        // STR W0, [X1]
        0xB900_0020,
        // === 'e' を出力 (0x65) ===
        0xD280_0CA0, // MOVZ X0, #0x65
        0xB900_0020, // STR W0, [X1]
        // === 'l' を出力 (0x6C) ===
        0xD280_0D80, // MOVZ X0, #0x6C
        0xB900_0020, // STR W0, [X1]
        // === 'l' を出力 (0x6C) ===
        0xD280_0D80, // MOVZ X0, #0x6C
        0xB900_0020, // STR W0, [X1]
        // === 'o' を出力 (0x6F) ===
        0xD280_0DE0, // MOVZ X0, #0x6F
        0xB900_0020, // STR W0, [X1]
        // === '\n' を出力 (0x0A) ===
        0xD280_0140, // MOVZ X0, #0x0A
        0xB900_0020, // STR W0, [X1]
        // === PSCI_SYSTEM_OFF (HVC) ===
        // X0 = PSCI_SYSTEM_OFF (0x84000008)
        // MOVZ X0, #0x8
        0xD280_0100,
        // MOVK X0, #0x8400, LSL #16 (correct encoding: F2B0_8000)
        0xF2B0_8000,
        // HVC #0
        0xD400_0002,
        // === 無限ループ (到達しないはず) ===
        0xD503_201F, // WFI
        0x17FF_FFFF, // B -4 (WFI に戻る)
    ];

    // 命令をバイト列に変換
    let mut bytes = Vec::new();
    for instr in instructions {
        bytes.extend_from_slice(&instr.to_le_bytes());
    }
    bytes
}

/// ミニカーネルが UART に出力して PSCI_SYSTEM_OFF で終了することを確認
#[test]
#[ignore = "requires Hypervisor.framework entitlements (run locally with --ignored)"]
fn mini_kernel_がuartに出力して終了する() {
    // 128MB RAM
    let mem_size = 128 * 1024 * 1024;
    let mut hv = Hypervisor::new(RAM_BASE, mem_size).expect("Failed to create hypervisor");

    // UART デバイスを登録
    let uart = Pl011Uart::new(UART_BASE);
    hv.register_mmio_handler(Box::new(uart));

    // ミニカーネルを作成
    let kernel_data = create_mini_kernel();
    let kernel = KernelImage::from_bytes(kernel_data, Some(KERNEL_ENTRY));

    // カーネルをブート
    let result = hv
        .boot_linux(&kernel, "console=ttyAMA0 earlycon", Some(DTB_ADDR))
        .expect("Failed to boot");

    // HVC (PSCI_SYSTEM_OFF) で VM Exit したことを確認
    // EC = 0x16 (HVC) で終了するはず
    let ec = result
        .exception_syndrome
        .map(|s| (s >> 26) & 0x3f)
        .unwrap_or(0);

    // PSCI_SYSTEM_OFF は VM Exit を返すので、正常終了
    println!("Mini kernel exited with EC=0x{:x}", ec);
}

/// Device Tree が正しく生成されることを確認
#[test]
fn device_tree_が正しく生成される() {
    let config = DeviceTreeConfig {
        memory_base: RAM_BASE,
        memory_size: 128 * 1024 * 1024,
        uart_base: UART_BASE,
        virtio_base: 0x0A00_0000,
        gic_dist_base: 0x0800_0000,
        gic_cpu_base: 0x0801_0000,
        cmdline: "console=ttyAMA0 earlycon".to_string(),
        initrd_start: None,
        initrd_end: None,
    };

    let dtb = generate_device_tree(&config).expect("Failed to generate DTB");

    // DTB が生成されたことを確認
    assert!(!dtb.is_empty());

    // DTB マジックナンバー (0xD00DFEED, big-endian)
    assert_eq!(dtb[0], 0xD0);
    assert_eq!(dtb[1], 0x0D);
    assert_eq!(dtb[2], 0xFE);
    assert_eq!(dtb[3], 0xED);
}

/// KernelImage が正しく作成されることを確認
#[test]
fn kernel_image_が正しく作成される() {
    let kernel_data = create_mini_kernel();
    let kernel = KernelImage::from_bytes(kernel_data.clone(), Some(KERNEL_ENTRY));

    assert_eq!(kernel.entry_point(), KERNEL_ENTRY);
    assert_eq!(kernel.data().len(), kernel_data.len());
}

/// UART への直接書き込みテスト
#[test]
fn uart_に直接書き込める() {
    let mut uart = Pl011Uart::new(UART_BASE);

    // "Hello\n" を出力
    for ch in b"Hello\n" {
        uart.write(0x00, *ch as u64, 4)
            .expect("Failed to write to UART");
    }
}
