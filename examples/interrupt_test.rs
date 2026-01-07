//! 割り込みコントローラー (GIC + Timer) のテスト
//!
//! GIC と Timer の統合動作を確認します。
//!
//! 実行方法:
//! ```bash
//! cargo run --example interrupt_test
//! ```

use hypervisor::devices::interrupt::InterruptController;
use hypervisor::devices::timer::TimerReg;
use std::thread;
use std::time::Duration;

fn main() {
    println!("=== 割り込みコントローラーテスト ===\n");

    // 割り込みコントローラーを作成
    let mut ic = InterruptController::new();

    println!("1. 初期状態");
    println!("   GIC 有効: {}", ic.is_enabled());
    println!("   ペンディング IRQ: {:?}", ic.get_pending_irq());

    // GIC を有効化
    println!("\n2. GIC を有効化");
    ic.enable();
    println!("   GIC 有効: {}", ic.is_enabled());

    // タイマー IRQ を有効化
    println!("\n3. タイマー IRQ を有効化");
    ic.enable_timer_irqs();
    println!("   物理タイマー IRQ (30) と仮想タイマー IRQ (27) を有効化");

    // 現在のタイマー状態
    println!("\n4. タイマー状態");
    let phys_cnt = ic.timer.get_phys_counter();
    let virt_cnt = ic.timer.get_virt_counter();
    println!("   物理カウンタ: {}", phys_cnt);
    println!("   仮想カウンタ: {}", virt_cnt);
    println!(
        "   次のタイマーイベントまで: {:?}",
        ic.time_until_next_timer()
    );

    // 物理タイマーを 50ms 後に設定
    println!("\n5. 物理タイマーを 50ms 後に設定");
    let freq = ic.timer.get_frequency();
    let fire_after = freq / 20; // 50ms
    let cval = phys_cnt + fire_after;

    ic.timer.write_sysreg(TimerReg::CNTP_CVAL_EL0, cval).unwrap();
    ic.timer.write_sysreg(TimerReg::CNTP_CTL_EL0, 1).unwrap(); // 有効化

    println!("   CNTP_CVAL_EL0 <- {}", cval);
    println!("   CNTP_CTL_EL0 <- 1 (タイマー有効化)");

    if let Some(nanos) = ic.time_until_next_timer() {
        println!(
            "   次のタイマーイベントまで: {:.2} ms",
            nanos as f64 / 1_000_000.0
        );
    }

    // タイマー発火前の状態
    println!("\n6. タイマー発火前");
    ic.poll_timer_irqs();
    println!("   poll_timer_irqs() 実行");
    println!("   ペンディング IRQ あり: {}", ic.has_pending_irq());
    println!("   最高優先度 IRQ: {:?}", ic.get_pending_irq());

    // 60ms 待機してタイマーを発火
    println!("\n7. 60ms 待機...");
    thread::sleep(Duration::from_millis(60));

    // タイマー IRQ をポーリング
    println!("\n8. タイマー IRQ をポーリング");
    ic.poll_timer_irqs();
    println!("   poll_timer_irqs() 実行");
    println!("   ペンディング IRQ あり: {}", ic.has_pending_irq());
    println!("   最高優先度 IRQ: {:?}", ic.get_pending_irq());

    // 割り込みを acknowledge
    if ic.has_pending_irq() {
        println!("\n9. 割り込み処理フロー");
        let irq = ic.acknowledge();
        println!("   acknowledge() -> IRQ {}", irq);
        println!("   [シミュレーション] タイマー割り込みハンドラ実行中...");

        // 割り込み完了
        ic.end_of_interrupt(irq);
        println!("   end_of_interrupt({}) 完了", irq);

        // 次の acknowledge はスプリアス (1023)
        let next_irq = ic.acknowledge();
        println!(
            "   次の acknowledge() -> {} (1023 = スプリアス)",
            next_irq
        );
    }

    // 最終状態
    println!("\n10. 最終状態");
    println!("    ペンディング IRQ あり: {}", ic.has_pending_irq());

    println!("\n=== テスト完了 ===");
    println!("\n割り込み統合フロー:");
    println!("  1. InterruptController::new() で GIC と Timer を統合作成");
    println!("  2. enable() で GIC を有効化");
    println!("  3. enable_timer_irqs() でタイマー IRQ を有効化");
    println!("  4. タイマーを設定 (CNTP_CVAL_EL0, CNTP_CTL_EL0)");
    println!("  5. poll_timer_irqs() でタイマー状態を GIC にルーティング");
    println!("  6. has_pending_irq() で割り込み発生を検出");
    println!("  7. acknowledge() で割り込みを受け付け");
    println!("  8. end_of_interrupt() で処理完了を通知");
}
