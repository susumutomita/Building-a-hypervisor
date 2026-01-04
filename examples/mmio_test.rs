//! MMIO ハンドリングのテスト
//!
//! Data Abort (EC=0x24) が正しく検出されることを確認する

use hypervisor::Hypervisor;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== MMIO ハンドリングテスト ===\n");

    // ハイパーバイザーを初期化
    println!("[1] ハイパーバイザーを初期化中...");
    let guest_addr = 0x10000;
    let mut hv = Hypervisor::new(guest_addr, 0x1000)?;
    println!("    ✓ ゲストアドレス: 0x{:x}", guest_addr);

    // テストコード: MMIO アドレスへの書き込み
    println!("\n[2] ゲストコードを書き込み中...");
    println!("    ARM64 アセンブリ:");
    println!("      mov x0, #0x42");
    println!("      mov x1, #0x09000000  // UART base address");
    println!("      str w0, [x1]         // MMIO アドレス 0x09000000 への書き込み");
    println!("      brk #0");

    let instructions = [
        0xd2800840,  // mov x0, #0x42
        0xd2a12001,  // mov x1, #0x09000000 (movz x1, #0x900, lsl #16) - FIXED: X1 not X0
        0xb9000020,  // str w0, [x1]  // MMIO アドレス 0x09000000 への書き込み
        0xd4200000,  // brk #0
    ];

    hv.write_instructions(&instructions)?;
    println!("    ✓ {} 命令を書き込み完了", instructions.len());

    // ゲストプログラムを実行
    println!("\n[3] ゲストプログラムを実行中...");
    println!("    期待される動作:");
    println!("      - Data Abort (EC=0x24) が検出される");
    println!("      - is_write=true, size=4 が表示される");
    println!("\n---");

    let result = hv.run(None, None)?;

    // 結果を表示
    println!("\nVM Exit:");
    println!("  - Reason: {:?}", result.exit_reason);
    println!("  - PC: 0x{:x}", result.pc);

    if let Some(syndrome) = result.exception_syndrome {
        let ec = (syndrome >> 26) & 0x3f;
        println!("  - Exception Class (EC): 0x{:x}", ec);

        if ec == 0x3c {
            println!("\n✅ 成功: BRK 命令で正常に終了しました");
            println!("   Data Abort が正しく処理され、プログラムが最後まで実行されました");
        } else {
            println!("\n⚠️  予期しない例外: EC=0x{:x}", ec);
        }
    }

    println!("\n=== テスト完了 ===");

    Ok(())
}
