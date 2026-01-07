//! 割り込みコントローラー統合モジュール
//!
//! GIC と Timer を統合して、タイマー割り込みを自動的に GIC に配信します。

use super::gic::{Gic, GIC_DIST_SIZE};
use super::timer::{Timer, PHYS_TIMER_IRQ, VIRT_TIMER_IRQ};
use crate::mmio::MmioHandler;

// GICD レジスタオフセット
const GICD_CTLR: u64 = 0x000;
const GICD_ISENABLER: u64 = 0x100;
const GICD_IPRIORITYR: u64 = 0x400;

// GICC レジスタオフセット
const GICC_CTLR: u64 = 0x000;

/// 割り込みコントローラー
///
/// GIC と Timer を統合管理し、タイマー割り込みを自動的に GIC にルーティングします。
#[derive(Debug)]
pub struct InterruptController {
    /// GIC (Generic Interrupt Controller)
    pub gic: Gic,
    /// ARM Generic Timer
    pub timer: Timer,
}

impl Default for InterruptController {
    fn default() -> Self {
        Self::new()
    }
}

impl InterruptController {
    /// 新しい割り込みコントローラーを作成
    pub fn new() -> Self {
        Self {
            gic: Gic::new(),
            timer: Timer::new(),
        }
    }

    /// タイマー IRQ をポーリングして GIC に反映
    ///
    /// タイマーがペンディング状態の場合、対応する IRQ を GIC にセットします。
    /// VM のメインループで定期的に呼び出す必要があります。
    pub fn poll_timer_irqs(&mut self) {
        // 物理タイマー
        if self.timer.phys_timer_pending() {
            self.gic.set_irq_pending(PHYS_TIMER_IRQ);
        }

        // 仮想タイマー
        if self.timer.virt_timer_pending() {
            self.gic.set_irq_pending(VIRT_TIMER_IRQ);
        }
    }

    /// ペンディング中の IRQ があるかチェック
    pub fn has_pending_irq(&self) -> bool {
        self.gic.get_highest_pending_irq().is_some()
    }

    /// 最高優先度のペンディング IRQ を取得
    pub fn get_pending_irq(&self) -> Option<u32> {
        self.gic.get_highest_pending_irq()
    }

    /// GIC を有効化
    pub fn enable(&mut self) {
        // GICD_CTLR = 1
        self.gic.write(GICD_CTLR, 1, 4).unwrap();
        // GICC_CTLR = 1
        self.gic.write(GIC_DIST_SIZE + GICC_CTLR, 1, 4).unwrap();
    }

    /// タイマー IRQ を有効化
    pub fn enable_timer_irqs(&mut self) {
        // PPI は IRQ 16-31 で、ISENABLER[0] のビット 16-31 に対応
        // 物理タイマー IRQ 30 を有効化
        // 仮想タイマー IRQ 27 を有効化
        let mask = (1u32 << PHYS_TIMER_IRQ) | (1u32 << VIRT_TIMER_IRQ);
        self.gic.write(GICD_ISENABLER, mask as u64, 4).unwrap();

        // 優先度を設定 (中程度: 0x80)
        // IPRIORITYR はバイト単位でアクセス
        // IRQ 27 の優先度
        self.gic
            .write(GICD_IPRIORITYR + VIRT_TIMER_IRQ as u64, 0x80, 4)
            .unwrap();
        // IRQ 30 の優先度
        self.gic
            .write(GICD_IPRIORITYR + PHYS_TIMER_IRQ as u64, 0x80, 4)
            .unwrap();
    }

    /// 次のタイマーイベントまでの時間（ナノ秒）
    pub fn time_until_next_timer(&self) -> Option<u64> {
        self.timer.time_until_next_event()
    }

    /// 割り込みを acknowledge して IRQ 番号を返す
    pub fn acknowledge(&mut self) -> u32 {
        self.gic.acknowledge_irq()
    }

    /// 割り込み処理完了を通知
    pub fn end_of_interrupt(&mut self, irq: u32) {
        self.gic.end_of_interrupt(irq);
    }

    /// GIC が有効かどうか
    pub fn is_enabled(&mut self) -> bool {
        let gicd = self.gic.read(GICD_CTLR, 4).unwrap();
        let gicc = self.gic.read(GIC_DIST_SIZE + GICC_CTLR, 4).unwrap();
        gicd != 0 && gicc != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::devices::timer::TimerReg;

    #[test]
    fn interrupt_controller_new_の初期状態を確認() {
        let mut ic = InterruptController::new();
        assert!(!ic.is_enabled());
    }

    #[test]
    fn enable_でgicが有効になる() {
        let mut ic = InterruptController::new();
        ic.enable();
        assert!(ic.is_enabled());
    }

    #[test]
    fn enable_timer_irqs_でタイマーirqが有効になる() {
        let mut ic = InterruptController::new();
        ic.enable_timer_irqs();

        // ISENABLER[0] を読み取って確認
        let enabled = ic.gic.read(GICD_ISENABLER, 4).unwrap() as u32;
        // 物理タイマー IRQ 30 が有効
        assert_ne!(enabled & (1 << 30), 0);
        // 仮想タイマー IRQ 27 が有効
        assert_ne!(enabled & (1 << 27), 0);
    }

    #[test]
    fn poll_timer_irqs_で物理タイマーirqがgicにセットされる() {
        let mut ic = InterruptController::new();
        ic.enable();
        ic.enable_timer_irqs();

        // タイマーを過去に設定してペンディング状態にする
        let counter = ic.timer.get_phys_counter();
        ic.timer.write_sysreg(TimerReg::CNTP_CTL_EL0, 1).unwrap(); // 有効化
        ic.timer
            .write_sysreg(TimerReg::CNTP_CVAL_EL0, counter.saturating_sub(100))
            .unwrap();

        // ポーリング
        ic.poll_timer_irqs();

        // GIC に IRQ がセットされている
        assert!(ic.has_pending_irq());
        assert_eq!(ic.get_pending_irq(), Some(PHYS_TIMER_IRQ));
    }

    #[test]
    fn poll_timer_irqs_で仮想タイマーirqがgicにセットされる() {
        let mut ic = InterruptController::new();
        ic.enable();
        ic.enable_timer_irqs();

        // 仮想タイマーを過去に設定
        let counter = ic.timer.get_virt_counter();
        ic.timer.write_sysreg(TimerReg::CNTV_CTL_EL0, 1).unwrap(); // 有効化
        ic.timer
            .write_sysreg(TimerReg::CNTV_CVAL_EL0, counter.saturating_sub(100))
            .unwrap();

        // ポーリング
        ic.poll_timer_irqs();

        // GIC に IRQ がセットされている
        assert!(ic.has_pending_irq());
        assert_eq!(ic.get_pending_irq(), Some(VIRT_TIMER_IRQ));
    }

    #[test]
    fn acknowledge_と_end_of_interrupt_のフローが動作する() {
        let mut ic = InterruptController::new();
        ic.enable();
        ic.enable_timer_irqs();

        // タイマーをペンディング状態にする
        let counter = ic.timer.get_phys_counter();
        ic.timer.write_sysreg(TimerReg::CNTP_CTL_EL0, 1).unwrap();
        ic.timer
            .write_sysreg(TimerReg::CNTP_CVAL_EL0, counter.saturating_sub(100))
            .unwrap();

        ic.poll_timer_irqs();

        // Acknowledge
        let irq = ic.acknowledge();
        assert_eq!(irq, PHYS_TIMER_IRQ);

        // End of Interrupt
        ic.end_of_interrupt(irq);

        // 次の acknowledge はスプリアスを返す
        let next_irq = ic.acknowledge();
        assert_eq!(next_irq, 1023);
    }

    #[test]
    fn has_pending_irq_はペンディングがない場合falseを返す() {
        let mut ic = InterruptController::new();
        ic.enable();
        assert!(!ic.has_pending_irq());
    }

    #[test]
    fn time_until_next_timer_はタイマーが無効の場合noneを返す() {
        let ic = InterruptController::new();
        assert!(ic.time_until_next_timer().is_none());
    }

    #[test]
    fn time_until_next_timer_は次のイベントまでの時間を返す() {
        let mut ic = InterruptController::new();

        // 1秒後にタイマーを設定
        let counter = ic.timer.get_phys_counter();
        let freq = ic.timer.get_frequency();
        ic.timer.write_sysreg(TimerReg::CNTP_CTL_EL0, 1).unwrap();
        ic.timer
            .write_sysreg(TimerReg::CNTP_CVAL_EL0, counter + freq)
            .unwrap();

        let time = ic.time_until_next_timer();
        assert!(time.is_some());
        // おおよそ 1 秒 (1_000_000_000 ナノ秒)
        let nanos = time.unwrap();
        assert!(nanos > 900_000_000 && nanos < 1_100_000_000);
    }
}
