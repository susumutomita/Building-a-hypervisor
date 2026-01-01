//! macOS Hypervisor.framework を使ったシンプルなハイパーバイザー
//!
//! 元記事 (KVM ベース): https://iovec.net/2024-01-29
//! このコードは Apple Silicon 向けに Hypervisor.framework を使用して移植したもの。

use applevisor::{Mappable, Mapping, MemPerms, Reg, Vcpu, VirtualMachine};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== macOS Hypervisor Demo (Apple Silicon) ===\n");

    // Step 1: VirtualMachine を作成
    // プロセスごとに1つの VM のみ許可される
    println!("[1] VirtualMachine を作成中...");
    let _vm = VirtualMachine::new()?;
    println!("    ✓ VM 作成完了");

    // Step 2: vCPU を作成
    println!("[2] vCPU を作成中...");
    let vcpu = Vcpu::new()?;
    println!("    ✓ vCPU 作成完了");

    // Step 3: ゲストメモリをマッピング
    // 4KB のメモリ領域を確保し、ゲストアドレス 0x10000 にマップ
    println!("[3] ゲストメモリをマッピング中...");
    let guest_addr: u64 = 0x10000;
    let mem_size: usize = 0x1000; // 4KB

    let mut mem = Mapping::new(mem_size)?;
    mem.map(guest_addr, MemPerms::RWX)?;
    println!(
        "    ✓ メモリマッピング完了 (ゲストアドレス: 0x{:x}, サイズ: {} bytes)",
        guest_addr, mem_size
    );

    // Step 4: ゲストコードを書き込む
    // シンプルな ARM64 アセンブリ:
    //   mov x0, #42      ; x0 に 42 を設定
    //   brk #0           ; ブレークポイントで VM Exit
    println!("[4] ゲストコードを書き込み中...");

    // ARM64 命令を書き込む
    // mov x0, #42 (0xD2800540)
    mem.write_dword(guest_addr, 0xD2800540)?;
    // brk #0 (0xD4200000) - デバッグ例外を発生
    mem.write_dword(guest_addr + 4, 0xD4200000)?;

    println!("    ✓ ゲストコード書き込み完了 (8 bytes)");

    // Step 5: vCPU レジスタを設定
    println!("[5] vCPU レジスタを設定中...");
    vcpu.set_reg(Reg::PC, guest_addr)?;
    vcpu.set_reg(Reg::CPSR, 0x3c4)?; // EL1h mode
    println!("    ✓ PC = 0x{:x}", guest_addr);
    println!("    ✓ CPSR = 0x3c4 (EL1h)");

    // デバッグ例外のトラップを有効化
    vcpu.set_trap_debug_exceptions(true)?;
    println!("    ✓ デバッグ例外トラップ有効化");

    // Step 6: vCPU を実行
    println!("\n[6] vCPU を実行中...");
    println!("---");

    loop {
        vcpu.run()?;
        let exit_info = vcpu.get_exit_info();

        let pc = vcpu.get_reg(Reg::PC)?;
        let x0 = vcpu.get_reg(Reg::X0)?;

        println!("VM Exit:");
        println!("  - Reason: {:?}", exit_info.reason);
        println!("  - PC: 0x{:x}", pc);
        println!("  - X0: {} (0x{:x})", x0, x0);

        // BRK 命令による例外の場合
        if let applevisor::ExitReason::EXCEPTION = exit_info.reason {
            let syndrome = exit_info.exception.syndrome;
            let ec = (syndrome >> 26) & 0x3f;

            println!("  - Exception Syndrome: 0x{:x}", syndrome);
            println!("  - Exception Class (EC): 0x{:x}", ec);

            // EC=0x3C: BRK instruction (AArch64)
            if ec == 0x3C {
                println!("\n✓ BRK 命令を検出!");
                println!("  ゲストが x0 = {} を設定して BRK を呼び出しました。", x0);
                break;
            }

            // 他の例外の場合は PC を進めて続行
            vcpu.set_reg(Reg::PC, pc + 4)?;
        } else {
            println!("予期しない VM Exit: {:?}", exit_info.reason);
            break;
        }
    }

    println!("\n=== ハイパーバイザーデモ完了 ===");
    Ok(())
}
