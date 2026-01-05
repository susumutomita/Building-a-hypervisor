//! カーネルブート機能のテスト
//!
//! 実際の Linux カーネルの代わりに、簡単なブートコードをテストする

use hypervisor::boot::kernel::KernelImage;
use hypervisor::devices::uart::Pl011Uart;
use hypervisor::Hypervisor;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== カーネルブート機能のテスト ===\n");

    // 1. ハイパーバイザーを作成
    println!("[1] ハイパーバイザーを作成中...");
    let mut hv = Hypervisor::new(0x4000_0000, 128 * 1024 * 1024)?;
    println!("    ✓ ハイパーバイザー作成完了");

    // 2. UART デバイスを登録
    println!("\n[2] UART デバイスを登録中...");
    let uart = Box::new(Pl011Uart::new(0x0900_0000));
    hv.register_mmio_handler(uart);
    println!("    ✓ UART デバイス登録完了");

    // 3. 簡単なブートコードを作成
    // UART に "Boot!" と出力して BRK で終了
    println!("\n[3] ブートコードを作成中...");
    let boot_code = vec![
        // UART base address (0x09000000) を X1 に設定
        0x01, 0x00, 0xa1, 0xd2, // movz x1, #0x9000, lsl #16
        // 'B' (0x42) を X0 に設定して UART に書き込み
        0x40, 0x08, 0x80, 0xd2, // movz x0, #0x42
        0x20, 0x00, 0x00, 0xf9, // str x0, [x1]
        // 'o' (0x6f) を X0 に設定して UART に書き込み
        0xe0, 0x0d, 0x80, 0xd2, // movz x0, #0x6f
        0x20, 0x00, 0x00, 0xf9, // str x0, [x1]
        // 'o' (0x6f) を X0 に設定して UART に書き込み
        0xe0, 0x0d, 0x80, 0xd2, // movz x0, #0x6f
        0x20, 0x00, 0x00, 0xf9, // str x0, [x1]
        // 't' (0x74) を X0 に設定して UART に書き込み
        0x80, 0x0e, 0x80, 0xd2, // movz x0, #0x74
        0x20, 0x00, 0x00, 0xf9, // str x0, [x1]
        // '!' (0x21) を X0 に設定して UART に書き込み
        0x20, 0x04, 0x80, 0xd2, // movz x0, #0x21
        0x20, 0x00, 0x00, 0xf9, // str x0, [x1]
        // '\n' (0x0a) を X0 に設定して UART に書き込み
        0x40, 0x01, 0x80, 0xd2, // movz x0, #0x0a
        0x20, 0x00, 0x00, 0xf9, // str x0, [x1]
        // BRK #0 で終了
        0x00, 0x00, 0x20, 0xd4, // brk #0
    ];

    let kernel = KernelImage::from_bytes(boot_code, Some(0x4008_0000));
    println!(
        "    ✓ ブートコード作成完了: {} bytes, entry_point=0x{:x}",
        kernel.size(),
        kernel.entry_point()
    );

    // 4. boot_linux() でカーネルをブート
    println!("\n[4] カーネルをブート中...");
    println!("    設定:");
    println!("      - エントリーポイント: 0x{:x}", kernel.entry_point());
    println!("      - Device Tree アドレス: 0x44000000");
    println!("      - コマンドライン: console=ttyAMA0");
    println!("\n    === カーネル出力 ===");

    let result = hv.boot_linux(&kernel, "console=ttyAMA0", None)?;

    println!("\n    === カーネル終了 ===");

    // 5. 結果を検証
    println!("\n[5] 結果を検証中...");
    println!("    - Exit reason: {:?}", result.exit_reason);
    println!("    - PC: 0x{:x}", result.pc);

    if let Some(syndrome) = result.exception_syndrome {
        let ec = (syndrome >> 26) & 0x3f;
        println!("    - Exception Class (EC): 0x{:x}", ec);

        if ec == 0x3c {
            println!("    ✓ BRK 命令で正常終了");
        } else {
            println!("    ✗ 予期しない例外");
        }
    }

    println!("\n✅ テスト完了");
    println!("\n=== カーネルブート機能のテスト完了 ===");

    Ok(())
}
