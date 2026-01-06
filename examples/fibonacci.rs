//! フィボナッチ数列を計算するゲストプログラムの例
//!
//! このプログラムは、VM ゲスト内でフィボナッチ数列 F(10) を計算し、
//! 結果をレジスタに格納してハイパーバイザーに戻る。
//!
//! # 実行方法
//! ```sh
//! cargo run --example fibonacci
//! ```

use hypervisor::Hypervisor;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== フィボナッチ数列計算デモ ===\n");

    // ハイパーバイザーを初期化
    println!("[1] ハイパーバイザーを初期化中...");
    let guest_addr = 0x10000;
    let mut hv = Hypervisor::new(guest_addr, 0x1000)?;
    println!("    ✓ ゲストアドレス: 0x{:x}", guest_addr);

    // ゲストコードを書き込む
    println!("[2] ゲストコードを書き込み中...");
    println!("    計算: フィボナッチ数列 F(10)");

    // ARM64 アセンブリ:
    //
    // ```assembly
    // mov x0, #0          // F(0) = 0
    // mov x1, #1          // F(1) = 1
    // mov x2, #10         // n = 10
    // loop:
    //     add x3, x0, x1  // x3 = F(i-1) + F(i-2)
    //     mov x0, x1      // x0 = F(i-1)
    //     mov x1, x3      // x1 = F(i)
    //     sub x2, x2, #1  // n--
    //     cbnz x2, loop   // if n != 0, continue
    // brk #0              // 終了
    // ```
    let instructions = [
        0xd2800000, // mov x0, #0
        0xd2800021, // mov x1, #1
        0xd2800142, // mov x2, #10
        0x8b010003, // add x3, x0, x1
        0xaa0103e0, // mov x0, x1
        0xaa0303e1, // mov x1, x3
        0xd1000442, // sub x2, x2, #1
        0xb5ffff82, // cbnz x2, loop (offset = -4 instructions)
        0xd4200000, // brk #0
    ];

    hv.write_instructions(&instructions)?;
    println!("    ✓ {} 命令を書き込み完了", instructions.len());

    // ゲストプログラムを実行
    println!("[3] ゲストプログラムを実行中...\n");
    println!("---");

    let result = hv.run(None, None, None)?;

    // 結果を表示
    println!("VM Exit:");
    println!("  - Reason: {:?}", result.exit_reason);
    println!("  - PC: 0x{:x}", result.pc);
    println!("\nレジスタ:");
    println!("  - X0: {} (F(10))", result.registers[0]);
    println!("  - X1: {} (F(11))", result.registers[1]);
    println!("  - X2: {} (ループカウンタ)", result.registers[2]);

    // 検証
    println!("\n✓ 計算結果: F(10) = {}", result.registers[0]);
    println!("  (期待値: 55)");

    if result.registers[0] == 55 {
        println!("\n✅ 正しい結果です！");
    } else {
        println!("\n❌ 結果が期待値と異なります");
    }

    println!("\n=== デモ完了 ===");
    Ok(())
}
