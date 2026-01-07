//! ARM Generic Timer のテスト
//!
//! タイマーの動作とシステムレジスタアクセスを確認します。
//!
//! 実行方法:
//! ```bash
//! cargo run --example timer_test
//! ```

use hypervisor::devices::timer::{Timer, TimerReg, PHYS_TIMER_IRQ, TIMER_FREQ, VIRT_TIMER_IRQ};
use std::thread;
use std::time::Duration;

fn main() {
    println!("=== ARM Generic Timer テスト ===\n");

    // タイマーを作成
    let mut timer = Timer::new();

    println!("1. タイマー初期状態");
    println!(
        "   周波数: {} Hz ({:.2} MHz)",
        TIMER_FREQ,
        TIMER_FREQ as f64 / 1_000_000.0
    );
    println!("   物理タイマー IRQ: {}", PHYS_TIMER_IRQ);
    println!("   仮想タイマー IRQ: {}", VIRT_TIMER_IRQ);

    // カウンタを読み取り
    let phys_cnt = timer.read_sysreg(TimerReg::CNTPCT_EL0).unwrap();
    let virt_cnt = timer.read_sysreg(TimerReg::CNTVCT_EL0).unwrap();
    println!("\n2. カウンタ値");
    println!("   CNTPCT_EL0 (物理): {}", phys_cnt);
    println!("   CNTVCT_EL0 (仮想): {}", virt_cnt);

    // 少し待ってカウンタが増加することを確認
    println!("\n3. 100ms 待機してカウンタ増加を確認");
    thread::sleep(Duration::from_millis(100));
    let phys_cnt_after = timer.read_sysreg(TimerReg::CNTPCT_EL0).unwrap();
    let expected_increase = TIMER_FREQ / 10; // 100ms = 1/10秒
    println!("   CNTPCT_EL0: {} -> {}", phys_cnt, phys_cnt_after);
    println!(
        "   増加量: {} (期待値: 約 {})",
        phys_cnt_after - phys_cnt,
        expected_increase
    );

    // 物理タイマーを設定
    println!("\n4. 物理タイマーを設定");

    // 現在のカウンタ値を取得
    let current = timer.get_phys_counter();

    // 50ms 後にタイマーを発火させる
    let fire_after_ticks = TIMER_FREQ / 20; // 50ms
    let cval = current + fire_after_ticks;

    // CVAL を設定
    timer.write_sysreg(TimerReg::CNTP_CVAL_EL0, cval).unwrap();
    println!(
        "   CNTP_CVAL_EL0 <- {} (現在 {} + {})",
        cval, current, fire_after_ticks
    );

    // タイマーを有効化 (ENABLE=1, IMASK=0)
    timer.write_sysreg(TimerReg::CNTP_CTL_EL0, 1).unwrap();
    println!("   CNTP_CTL_EL0 <- 1 (タイマー有効化)");

    // タイマー状態を確認
    let ctl = timer.read_sysreg(TimerReg::CNTP_CTL_EL0).unwrap();
    println!(
        "   CNTP_CTL_EL0 = 0x{:X} (ENABLE={}, IMASK={}, ISTATUS={})",
        ctl,
        (ctl >> 0) & 1,
        (ctl >> 1) & 1,
        (ctl >> 2) & 1
    );

    // 次のイベントまでの時間を確認
    if let Some(nanos) = timer.time_until_next_event() {
        println!(
            "   次のタイマーイベントまで: {:.2} ms",
            nanos as f64 / 1_000_000.0
        );
    }

    // 割り込みがペンディングしていないことを確認
    println!("\n5. 割り込み状態を確認（発火前）");
    let pending = timer.get_pending_irqs();
    println!("   ペンディング IRQ: {:?}", pending);
    println!(
        "   物理タイマーペンディング: {}",
        timer.phys_timer_pending()
    );

    // 60ms 待機してタイマーを発火させる
    println!("\n6. 60ms 待機してタイマー発火を確認");
    thread::sleep(Duration::from_millis(60));

    let ctl_after = timer.read_sysreg(TimerReg::CNTP_CTL_EL0).unwrap();
    println!(
        "   CNTP_CTL_EL0 = 0x{:X} (ENABLE={}, IMASK={}, ISTATUS={})",
        ctl_after,
        (ctl_after >> 0) & 1,
        (ctl_after >> 1) & 1,
        (ctl_after >> 2) & 1
    );

    let pending_after = timer.get_pending_irqs();
    println!("   ペンディング IRQ: {:?}", pending_after);
    println!(
        "   物理タイマーペンディング: {}",
        timer.phys_timer_pending()
    );

    // 仮想オフセットのデモ
    println!("\n7. 仮想オフセット (CNTVOFF_EL2) のデモ");
    let offset = TIMER_FREQ; // 1秒分のオフセット
    timer.set_virt_offset(offset);
    println!("   CNTVOFF_EL2 <- {} (1秒分のオフセット)", offset);

    let phys = timer.get_phys_counter();
    let virt = timer.get_virt_counter();
    println!("   物理カウンタ: {}", phys);
    println!("   仮想カウンタ: {} (= 物理 - オフセット)", virt);
    println!("   差分: {} (= オフセット値)", phys.saturating_sub(virt));

    // TVAL レジスタのデモ
    println!("\n8. TVAL レジスタのデモ");
    let tval_set = TIMER_FREQ / 2; // 500ms
    timer
        .write_sysreg(TimerReg::CNTP_TVAL_EL0, tval_set)
        .unwrap();
    println!("   CNTP_TVAL_EL0 <- {} (500ms 分)", tval_set);

    let tval_read = timer.read_sysreg(TimerReg::CNTP_TVAL_EL0).unwrap();
    let cval_read = timer.read_sysreg(TimerReg::CNTP_CVAL_EL0).unwrap();
    println!("   CNTP_TVAL_EL0 = {} (カウンタダウン値)", tval_read);
    println!("   CNTP_CVAL_EL0 = {} (絶対比較値)", cval_read);

    // タイマー無効化
    println!("\n9. タイマー無効化");
    timer.write_sysreg(TimerReg::CNTP_CTL_EL0, 0).unwrap();
    println!("   CNTP_CTL_EL0 <- 0 (タイマー無効化)");

    let pending_final = timer.get_pending_irqs();
    println!("   ペンディング IRQ: {:?}", pending_final);

    println!("\n=== テスト完了 ===");
    println!("\nARM Generic Timer の動作:");
    println!("  1. CNTPCT_EL0/CNTVCT_EL0 で現在のカウンタ値を取得");
    println!("  2. CNTP_CVAL_EL0 で絶対比較値を設定");
    println!("  3. CNTP_TVAL_EL0 で相対タイマー値を設定");
    println!("  4. CNTP_CTL_EL0 でタイマーを有効化/無効化");
    println!("  5. カウンタ >= CVAL になると ISTATUS=1 で割り込み発生");
}
