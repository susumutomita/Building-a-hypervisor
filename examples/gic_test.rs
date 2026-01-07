//! GIC (Generic Interrupt Controller) のテスト
//!
//! 割り込みの発生と処理フローを確認します。
//!
//! 実行方法:
//! ```bash
//! cargo run --example gic_test
//! ```

use hypervisor::devices::gic::{Gic, GIC_CPU_BASE, GIC_DIST_BASE, GIC_DIST_SIZE};
use hypervisor::mmio::MmioHandler;

fn main() {
    println!("=== GICv2 エミュレーションテスト ===\n");

    // GIC を作成
    let mut gic = Gic::new();

    println!("1. GIC 初期状態");
    println!("   GICD ベースアドレス: 0x{:08X}", GIC_DIST_BASE);
    println!("   GICC ベースアドレス: 0x{:08X}", GIC_CPU_BASE);

    // MMIO 経由で状態を読み取り
    let gicd_ctlr = gic.read(0x000, 4).unwrap();
    let gicc_ctlr = gic.read(GIC_DIST_SIZE, 4).unwrap();
    println!("   GICD_CTLR: 0x{:X} (Distributor {})", gicd_ctlr, if gicd_ctlr != 0 { "有効" } else { "無効" });
    println!("   GICC_CTLR: 0x{:X} (CPU Interface {})", gicc_ctlr, if gicc_ctlr != 0 { "有効" } else { "無効" });

    // GIC を有効化
    println!("\n2. GIC を有効化");
    gic.write(0x000, 1, 4).unwrap(); // GICD_CTLR = 1
    gic.write(GIC_DIST_SIZE, 1, 4).unwrap(); // GICC_CTLR = 1

    let gicd_ctlr = gic.read(0x000, 4).unwrap();
    let gicc_ctlr = gic.read(GIC_DIST_SIZE, 4).unwrap();
    println!("   GICD_CTLR: 0x{:X} (Distributor {})", gicd_ctlr, if gicd_ctlr != 0 { "有効" } else { "無効" });
    println!("   GICC_CTLR: 0x{:X} (CPU Interface {})", gicc_ctlr, if gicc_ctlr != 0 { "有効" } else { "無効" });

    // TYPER レジスタを読み取り
    let typer = gic.read(0x004, 4).unwrap();
    let it_lines = (typer & 0x1F) as u32;
    let max_irqs = (it_lines + 1) * 32;
    println!("\n3. GICD_TYPER: 0x{:X}", typer);
    println!("   ITLinesNumber: {} (最大 {} IRQs)", it_lines, max_irqs);

    // IRQ 33 (SPI #1) を設定
    let irq = 33u32;
    println!("\n4. IRQ {} を設定", irq);

    // IRQ を有効化 (ISENABLER1 の bit 1)
    gic.write(0x104, 1 << (irq - 32), 4).unwrap();
    println!("   ISENABLER1 に書き込み -> IRQ {} 有効化", irq);

    // 優先度を設定 (IPRIORITYR)
    gic.write(0x400 + irq as u64, 0x80, 4).unwrap();
    println!("   IPRIORITYR[{}] = 0x80 (中程度の優先度)", irq);

    // 割り込みを発生させる
    println!("\n5. IRQ {} をペンディングにする", irq);
    gic.set_irq_pending(irq);
    println!("   set_irq_pending({}) 呼び出し完了", irq);

    // HPPIR を読んで最高優先度ペンディング割り込みを確認
    let hppir = gic.read(GIC_DIST_SIZE + 0x018, 4).unwrap();
    println!("   GICC_HPPIR: {} (最高優先度ペンディング IRQ)", hppir);

    // IAR を読んで割り込みを acknowledge
    println!("\n6. 割り込みを acknowledge (GICC_IAR 読み取り)");
    let acked_irq = gic.read(GIC_DIST_SIZE + 0x00C, 4).unwrap();
    println!("   GICC_IAR: {} (Acknowledged IRQ)", acked_irq);

    // 実行優先度を確認
    let rpr = gic.read(GIC_DIST_SIZE + 0x014, 4).unwrap();
    println!("   GICC_RPR: 0x{:X} (現在の実行優先度)", rpr);

    // 割り込み処理をシミュレート
    println!("\n7. 割り込みハンドラ実行中...");
    println!("   [シミュレーション] デバイスからのデータを処理");

    // EOIR に書いて割り込み完了
    println!("\n8. 割り込み完了 (GICC_EOIR 書き込み)");
    gic.write(GIC_DIST_SIZE + 0x010, acked_irq, 4).unwrap();
    println!("   GICC_EOIR <- {} (割り込み処理完了)", acked_irq);

    // 状態確認
    let hppir_after = gic.read(GIC_DIST_SIZE + 0x018, 4).unwrap();
    let rpr_after = gic.read(GIC_DIST_SIZE + 0x014, 4).unwrap();
    println!("\n9. 最終状態");
    println!("   GICC_HPPIR: {} (1023 = ペンディングなし)", hppir_after);
    println!("   GICC_RPR: 0x{:X} (0xFF = アイドル)", rpr_after);

    println!("\n=== テスト完了 ===");
    println!("\n割り込みフロー:");
    println!("  1. set_irq_pending() で割り込み発生");
    println!("  2. GICC_IAR 読み取りで acknowledge");
    println!("  3. 割り込みハンドラで処理");
    println!("  4. GICC_EOIR 書き込みで完了通知");
}
