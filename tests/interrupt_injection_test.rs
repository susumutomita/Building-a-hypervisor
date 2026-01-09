//! 割り込みインジェクションの統合テスト
//!
//! Phase 4 Week 2: タイマー割り込みの vCPU インジェクションをテスト

use hypervisor::devices::timer::TimerReg;
use hypervisor::Hypervisor;

/// BRK 命令をエンコード
fn encode_brk(imm: u16) -> u32 {
    0xd4200000 | ((imm as u32) << 5)
}

#[test]
fn interrupt_controller_が正しく初期化される() {
    let hv = Hypervisor::new(0x10000, 4096).expect("Failed to create hypervisor");

    // InterruptController が初期化されている
    let ic = hv.interrupt_controller();
    // GIC が無効な状態で開始
    assert!(!ic.has_pending_irq());
}

#[test]
fn タイマーを設定すると割り込みがペンディングになる() {
    let mut hv = Hypervisor::new(0x10000, 4096).expect("Failed to create hypervisor");

    // GIC を有効化
    hv.interrupt_controller_mut().enable();
    hv.interrupt_controller_mut().enable_timer_irqs();

    // 仮想タイマーを過去に設定してペンディング状態にする
    let counter = hv.timer().get_virt_counter();
    hv.timer_mut()
        .write_sysreg(TimerReg::CNTV_CTL_EL0, 1)
        .unwrap(); // 有効化
    hv.timer_mut()
        .write_sysreg(TimerReg::CNTV_CVAL_EL0, counter.saturating_sub(100))
        .unwrap();

    // タイマー IRQ をポーリング
    hv.interrupt_controller_mut().poll_timer_irqs();

    // 割り込みがペンディングになっている
    assert!(hv.interrupt_controller().has_pending_irq());
}

#[test]
fn acknowledge_と_eoi_のフローが動作する() {
    let mut hv = Hypervisor::new(0x10000, 4096).expect("Failed to create hypervisor");

    // GIC を有効化
    hv.interrupt_controller_mut().enable();
    hv.interrupt_controller_mut().enable_timer_irqs();

    // 仮想タイマーをペンディング状態にする
    let counter = hv.timer().get_virt_counter();
    hv.timer_mut()
        .write_sysreg(TimerReg::CNTV_CTL_EL0, 1)
        .unwrap();
    hv.timer_mut()
        .write_sysreg(TimerReg::CNTV_CVAL_EL0, counter.saturating_sub(100))
        .unwrap();

    hv.interrupt_controller_mut().poll_timer_irqs();

    // Acknowledge
    let irq = hv.interrupt_controller_mut().acknowledge();
    assert_eq!(irq, 27); // 仮想タイマー IRQ

    // End of Interrupt
    hv.interrupt_controller_mut().end_of_interrupt(irq);

    // 次の acknowledge はスプリアスを返す
    let next_irq = hv.interrupt_controller_mut().acknowledge();
    assert_eq!(next_irq, 1023);
}

#[test]
fn タイマーなしでゲストを実行できる() {
    let mut hv = Hypervisor::new(0x10000, 4096).expect("Failed to create hypervisor");

    // 単純な BRK 命令
    let instructions = vec![encode_brk(0)];
    hv.write_instructions(&instructions)
        .expect("Failed to write instructions");

    let result = hv.run(None, None, None).expect("Failed to run");

    // 正常に終了
    assert!(matches!(
        result.exit_reason,
        applevisor::ExitReason::EXCEPTION
    ));
}

#[test]
fn gic_有効時でもゲストを実行できる() {
    let mut hv = Hypervisor::new(0x10000, 4096).expect("Failed to create hypervisor");

    // GIC を有効化
    hv.interrupt_controller_mut().enable();
    hv.interrupt_controller_mut().enable_timer_irqs();

    // 単純な BRK 命令
    let instructions = vec![encode_brk(0)];
    hv.write_instructions(&instructions)
        .expect("Failed to write instructions");

    let result = hv.run(None, None, None).expect("Failed to run");

    // 正常に終了
    assert!(matches!(
        result.exit_reason,
        applevisor::ExitReason::EXCEPTION
    ));
}

#[test]
fn ペンディングirqがない場合はインジェクトしない() {
    let mut hv = Hypervisor::new(0x10000, 4096).expect("Failed to create hypervisor");

    // GIC を有効化するがタイマーは設定しない
    hv.interrupt_controller_mut().enable();
    hv.interrupt_controller_mut().enable_timer_irqs();

    // ペンディング IRQ がない
    assert!(!hv.interrupt_controller().has_pending_irq());

    // 単純な BRK 命令
    let instructions = vec![encode_brk(0)];
    hv.write_instructions(&instructions)
        .expect("Failed to write instructions");

    let result = hv.run(None, None, None).expect("Failed to run");

    // 正常に終了（割り込みなし）
    assert!(matches!(
        result.exit_reason,
        applevisor::ExitReason::EXCEPTION
    ));
}
