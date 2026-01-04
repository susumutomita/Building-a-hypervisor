//! 配列の合計を計算するゲストプログラムの例
//!
//! このプログラムは、VM ゲスト内で配列 [1, 2, 3, 4, 5, 6, 7, 8, 9, 10] の合計を計算し、
//! 結果をレジスタに格納してハイパーバイザーに戻る。
//!
//! # 実行方法
//! ```sh
//! cargo run --example array_sum
//! ```

use hypervisor::Hypervisor;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== 配列の合計計算デモ ===\n");

    // ハイパーバイザーを初期化
    println!("[1] ハイパーバイザーを初期化中...");
    let guest_addr = 0x10000;
    let mut hv = Hypervisor::new(guest_addr, 0x2000)?; // 8KB
    println!("    ✓ ゲストアドレス: 0x{:x}", guest_addr);

    // 配列データをメモリに書き込む
    println!("[2] 配列データをメモリに書き込み中...");
    let array_addr = 0x10200; // ゲストメモリ内の配列アドレス
    let array = [1u64, 2, 3, 4, 5, 6, 7, 8, 9, 10];

    println!("    配列: {:?}", array);
    println!("    アドレス: 0x{:x}", array_addr);

    for (i, &value) in array.iter().enumerate() {
        hv.write_data(0x200 + (i * 8) as u64, value)?;
    }
    println!("    ✓ {} 要素を書き込み完了", array.len());

    // 書き込みを確認
    print!("    検証: ");
    for i in 0..3 {
        let val = hv.read_data(0x200 + (i * 8))?;
        print!("[{}]=0x{:x} ", i, val);
    }
    println!("...");

    // ゲストコードを書き込む
    println!("[3] ゲストコードを書き込み中...");

    // ARM64 アセンブリ:
    //
    // ```assembly
    // movz x0, #0x1, lsl #16    // x0 = 0x10000
    // movk x0, #0x200, lsl #0   // x0 = 0x10200 (絶対アドレス)
    // mov x1, #10               // x1 = 配列の要素数
    // mov x2, #0                // x2 = 合計 (初期値 0)
    // loop:
    //     ldr x3, [x0], #8      // x3 = *x0, x0 += 8
    //     add x2, x2, x3        // x2 += x3
    //     sub x1, x1, #1        // x1--
    //     cbnz x1, loop         // if x1 != 0, continue
    // brk #0
    // ```
    let instructions = [
        0xd2a00020, // movz x0, #0x1, lsl #16 (= 0x10000)
        0xf2804000, // movk x0, #0x200, lsl #0 (= 0x10200)
        0xd2800141, // mov x1, #10
        0xd2800002, // mov x2, #0
        0xf8408403, // ldr x3, [x0], #8 (loop start)
        0x8b030042, // add x2, x2, x3
        0xd1000421, // sub x1, x1, #1
        0xb5ffff81, // cbnz x1, loop (offset = -4 instructions = -16 bytes)
        0xd4200000, // brk #0
    ];

    hv.write_instructions(&instructions)?;
    println!("    ✓ {} 命令を書き込み完了", instructions.len());

    // ゲストプログラムを実行
    println!("[4] ゲストプログラムを実行中...\n");
    println!("---");

    let result = hv.run(None, None)?;

    // 結果を表示
    println!("VM Exit:");
    println!("  - Reason: {:?}", result.exit_reason);
    println!("  - PC: 0x{:x}", result.pc);
    println!("\nレジスタ:");
    println!("  - X0: 0x{:x} (配列の終端アドレス)", result.registers[0]);
    println!("  - X1: {} (ループカウンタ)", result.registers[1]);
    println!("  - X2: {} (合計)", result.registers[2]);

    // 検証
    let expected_sum: u64 = array.iter().sum();
    println!("\n✓ 計算結果: {}", result.registers[2]);
    println!("  (期待値: {})", expected_sum);

    if result.registers[2] == expected_sum {
        println!("\n✅ 正しい結果です！");
    } else {
        println!("\n❌ 結果が期待値と異なります");
    }

    println!("\n=== デモ完了 ===");
    Ok(())
}
