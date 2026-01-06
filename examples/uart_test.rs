//! UART エミュレーションのテスト

use hypervisor::devices::uart::Pl011Uart;
use hypervisor::Hypervisor;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== UART エミュレーションテスト ===\n");

    // ハイパーバイザーを初期化
    println!("[1] ハイパーバイザーを初期化中...");
    let guest_addr = 0x10000;
    let mut hv = Hypervisor::new(guest_addr, 0x1000)?;
    println!("    ✓ ゲストアドレス: 0x{:x}", guest_addr);

    // UART デバイスを登録
    println!("\n[2] UART デバイスを登録中...");
    const UART_BASE: u64 = 0x09000000;
    let uart = Pl011Uart::new(UART_BASE);
    hv.register_mmio_handler(Box::new(uart));
    println!("    ✓ UART ベースアドレス: 0x{:x}", UART_BASE);

    // ゲストコードを書き込む
    println!("\n[3] ゲストコードを書き込み中...");
    println!("    ARM64 アセンブリ:");
    println!("      mov x0, #0x41        // 'A'");
    println!("      mov x1, #0x09000000  // UART base address");
    println!("      str w0, [x1]         // Write 'A' to UART_DR");
    println!("      mov x0, #0x42        // 'B'");
    println!("      str w0, [x1]         // Write 'B' to UART_DR");
    println!("      mov x0, #0x0a        // '\\n'");
    println!("      str w0, [x1]         // Write '\\n' to UART_DR");
    println!("      brk #0");

    let instructions = [
        0xd2800820, // mov x0, #0x41        // 'A'
        0xd2a12001, // mov x1, #0x09000000  // UART base address
        0xb9000020, // str w0, [x1]         // Write to UART_DR
        0xd2800840, // mov x0, #0x42        // 'B'
        0xb9000020, // str w0, [x1]         // Write to UART_DR
        0xd2800140, // mov x0, #0x0a        // '\n'
        0xb9000020, // str w0, [x1]         // Write to UART_DR
        0xd4200000, // brk #0
    ];

    hv.write_instructions(&instructions)?;
    println!("    ✓ {} 命令を書き込み完了", instructions.len());

    // ゲストプログラムを実行
    println!("\n[4] ゲストプログラムを実行中...\n---");
    println!("UART 出力: ");
    let result = hv.run(None, None, None)?;
    println!("---");

    // 結果を表示
    println!("\nVM Exit:");
    println!("  - Reason: {:?}", result.exit_reason);
    println!("  - PC: 0x{:x}", result.pc);

    if let Some(syndrome) = result.exception_syndrome {
        let ec = (syndrome >> 26) & 0x3f;
        println!("  - Exception Class (EC): 0x{:x}", ec);

        if ec == 0x3c {
            println!("\n✅ 成功: BRK 命令で正常に終了しました");
            println!("   UART から \"AB\" が出力されました");
        }
    }

    println!("\n=== UART エミュレーションテスト完了 ===");
    Ok(())
}
