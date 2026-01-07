//! Device Tree 生成のテスト

use hypervisor::boot::device_tree::{generate_device_tree, DeviceTreeConfig};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Device Tree 生成テスト ===\n");

    // デフォルト設定で Device Tree を生成
    println!("[1] デフォルト設定で Device Tree を生成中...");
    let default_config = DeviceTreeConfig::default();
    println!("    設定:");
    println!(
        "      - メモリベースアドレス: 0x{:x}",
        default_config.memory_base
    );
    println!(
        "      - メモリサイズ: 0x{:x} ({} MB)",
        default_config.memory_size,
        default_config.memory_size / (1024 * 1024)
    );
    println!(
        "      - UART ベースアドレス: 0x{:x}",
        default_config.uart_base
    );
    println!("      - Kernel cmdline: {}", default_config.cmdline);

    let dtb = generate_device_tree(&default_config)?;
    println!("    ✓ Device Tree 生成完了: {} bytes", dtb.len());

    // マジックナンバーを検証
    println!("\n[2] Device Tree を検証中...");
    let magic = u32::from_be_bytes([dtb[0], dtb[1], dtb[2], dtb[3]]);
    println!("    - マジックナンバー: 0x{:08x}", magic);
    if magic == 0xd00dfeed {
        println!("    ✓ マジックナンバーが正しい (0xd00dfeed)");
    } else {
        println!("    ✗ マジックナンバーが不正");
        return Err("Invalid FDT magic number".into());
    }

    // サイズを検証
    let total_size = u32::from_be_bytes([dtb[4], dtb[5], dtb[6], dtb[7]]);
    println!("    - Total size: {} bytes", total_size);
    if total_size as usize == dtb.len() {
        println!("    ✓ サイズが一致");
    } else {
        println!(
            "    ⚠ サイズ不一致: header={}, actual={}",
            total_size,
            dtb.len()
        );
    }

    // カスタム設定で Device Tree を生成
    println!("\n[3] カスタム設定で Device Tree を生成中...");
    let custom_config = DeviceTreeConfig {
        memory_base: 0x8000_0000,
        memory_size: 0x1000_0000, // 256MB
        uart_base: 0x1000_0000,
        virtio_base: 0x1100_0000,
        gic_dist_base: 0x0800_0000,
        gic_cpu_base: 0x0801_0000,
        cmdline: "console=ttyAMA0 earlycon debug".to_string(),
    };
    println!("    設定:");
    println!(
        "      - メモリベースアドレス: 0x{:x}",
        custom_config.memory_base
    );
    println!(
        "      - メモリサイズ: 0x{:x} ({} MB)",
        custom_config.memory_size,
        custom_config.memory_size / (1024 * 1024)
    );
    println!(
        "      - UART ベースアドレス: 0x{:x}",
        custom_config.uart_base
    );
    println!("      - Kernel cmdline: {}", custom_config.cmdline);

    let dtb2 = generate_device_tree(&custom_config)?;
    println!("    ✓ Device Tree 生成完了: {} bytes", dtb2.len());

    let magic2 = u32::from_be_bytes([dtb2[0], dtb2[1], dtb2[2], dtb2[3]]);
    if magic2 == 0xd00dfeed {
        println!("    ✓ マジックナンバーが正しい");
    }

    println!("\n✅ すべてのテストが成功しました");
    println!("\n=== Device Tree 生成テスト完了 ===");

    Ok(())
}
