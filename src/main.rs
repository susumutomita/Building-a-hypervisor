//! macOS Hypervisor.framework を使ったシンプルなハイパーバイザー
//!
//! 元記事 (KVM ベース): https://iovec.net/2024-01-29
//! このコードは Apple Silicon 向けに Hypervisor.framework を使用して移植したもの。
//!
//! # より実用的な例
//!
//! より実用的なゲストプログラムの例は、`examples/` ディレクトリにあります。
//!
//! - `fibonacci.rs`: フィボナッチ数列の計算
//! - `array_sum.rs`: 配列の合計計算
//!
//! 実行方法:
//! ```sh
//! cargo run --example fibonacci
//! cargo run --example array_sum
//! ```

use hypervisor::Hypervisor;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== macOS Hypervisor Demo (Apple Silicon) ===\n");

    // ハイパーバイザーを初期化
    println!("[1] ハイパーバイザーを初期化中...");
    let guest_addr = 0x10000;
    let mut hv = Hypervisor::new(guest_addr, 0x1000)?;
    println!("    ✓ ゲストアドレス: 0x{:x}", guest_addr);

    // ゲストコードを書き込む
    println!("[2] ゲストコードを書き込み中...");
    println!("    シンプルな ARM64 アセンブリ:");
    println!("      mov x0, #42");
    println!("      brk #0");

    let instructions = [
        0xD2800540, // mov x0, #42
        0xD4200000, // brk #0
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
    println!(
        "  - X0: {} (0x{:x})",
        result.registers[0], result.registers[0]
    );

    if let Some(syndrome) = result.exception_syndrome {
        let ec = (syndrome >> 26) & 0x3f;
        println!("  - Exception Syndrome: 0x{:x}", syndrome);
        println!("  - Exception Class (EC): 0x{:x}", ec);
    }

    println!("\n✓ BRK 命令を検出!");
    println!(
        "  ゲストが x0 = {} を設定して BRK を呼び出しました。",
        result.registers[0]
    );

    println!("\n=== ハイパーバイザーデモ完了 ===");
    println!("\nヒント: より実用的な例は以下のコマンドで実行できます:");
    println!("  cargo run --example fibonacci");
    println!("  cargo run --example array_sum");

    Ok(())
}
