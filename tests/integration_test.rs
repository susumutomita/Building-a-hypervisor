//! 統合テスト
//!
//! Week 4 実装の機能テスト

use hypervisor::boot::device_tree::{generate_device_tree, DeviceTreeConfig};
use hypervisor::boot::kernel::KernelImage;

#[test]
fn test_kernel_image_creation() {
    // 簡単なブートコード（BRK #0）
    let boot_code = vec![0x00, 0x00, 0x20, 0xd4];
    let kernel = KernelImage::from_bytes(boot_code.clone(), Some(0x4008_0000));

    assert_eq!(kernel.entry_point(), 0x4008_0000);
    assert_eq!(kernel.size(), 4);
    assert_eq!(kernel.data(), &boot_code);
}

#[test]
fn test_device_tree_with_kernel() {
    // Device Tree を生成
    let config = DeviceTreeConfig {
        memory_base: 0x4000_0000,
        memory_size: 128 * 1024 * 1024, // 128MB
        uart_base: 0x0900_0000,
        virtio_base: 0x0a00_0000,
        cmdline: "console=ttyAMA0 earlycon".to_string(),
    };

    let dtb = generate_device_tree(&config).unwrap();

    // DTB が正しく生成されていることを確認
    assert_eq!(dtb[0..4], [0xd0, 0x0d, 0xfe, 0xed]); // Magic number
    assert!(dtb.len() > 100);
}

#[test]
fn test_kernel_image_and_device_tree_integration() {
    // 1. カーネルイメージを作成
    let boot_code = vec![
        0x00, 0x00, 0xa1, 0xd2, // movz x1, #0x9000, lsl #16
        0x40, 0x08, 0x80, 0xd2, // movz x0, #0x42
        0x20, 0x00, 0x00, 0xf9, // str x0, [x1]
        0x00, 0x00, 0x20, 0xd4, // brk #0
    ];
    let kernel = KernelImage::from_bytes(boot_code, Some(0x4008_0000));

    // 2. Device Tree を生成
    let config = DeviceTreeConfig {
        memory_base: 0x4000_0000,
        memory_size: 128 * 1024 * 1024,
        uart_base: 0x0900_0000,
        virtio_base: 0x0a00_0000,
        cmdline: "console=ttyAMA0".to_string(),
    };
    let dtb = generate_device_tree(&config).unwrap();

    // 3. カーネルと Device Tree が正しく生成されていることを確認
    assert_eq!(kernel.entry_point(), 0x4008_0000);
    assert_eq!(kernel.size(), 16);
    assert_eq!(dtb[0..4], [0xd0, 0x0d, 0xfe, 0xed]);

    // 4. メモリレイアウトを確認
    // カーネル: 0x40080000
    // DTB: 0x44000000
    let kernel_addr = kernel.entry_point();
    let dtb_addr = 0x4400_0000u64;

    // カーネルと DTB が重ならないことを確認
    assert!(kernel_addr + kernel.size() as u64 <= dtb_addr);
}
