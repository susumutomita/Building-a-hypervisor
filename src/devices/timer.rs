//! ARM Generic Timer エミュレーション
//!
//! ARM アーキテクチャの Generic Timer を提供します。
//! - 物理タイマー (EL1 Physical Timer)
//! - 仮想タイマー (EL1 Virtual Timer)
//!
//! Linux カーネルは起動時にタイマーを使用してスケジューリングを行います。

use std::error::Error;
use std::time::Instant;

/// タイマー周波数 (Hz)
/// Apple Silicon のホスト CNTFRQ_EL0 の値と一致させる
pub const TIMER_FREQ: u64 = 24_000_000; // 24 MHz (Apple Silicon)

/// 物理タイマー IRQ (PPI)
pub const PHYS_TIMER_IRQ: u32 = 30;
/// 仮想タイマー IRQ (PPI)
pub const VIRT_TIMER_IRQ: u32 = 27;
/// ハイパーバイザータイマー IRQ (PPI)
pub const HYP_TIMER_IRQ: u32 = 26;
/// セキュア物理タイマー IRQ (PPI)
pub const SEC_TIMER_IRQ: u32 = 29;

/// タイマー制御レジスタのビット
mod ctl_bits {
    /// タイマー有効
    pub const ENABLE: u64 = 1 << 0;
    /// 割り込みマスク
    pub const IMASK: u64 = 1 << 1;
    /// 割り込み状態 (読み取り専用)
    pub const ISTATUS: u64 = 1 << 2;
}

/// 個別タイマーの状態
#[derive(Debug, Clone)]
pub struct TimerState {
    /// 制御レジスタ (CTL)
    ctl: u64,
    /// 比較値レジスタ (CVAL)
    cval: u64,
}

impl Default for TimerState {
    fn default() -> Self {
        Self::new()
    }
}

impl TimerState {
    /// 新しいタイマー状態を作成
    pub fn new() -> Self {
        Self { ctl: 0, cval: 0 }
    }

    /// タイマーが有効かどうか
    pub fn is_enabled(&self) -> bool {
        (self.ctl & ctl_bits::ENABLE) != 0
    }

    /// 割り込みがマスクされているか
    pub fn is_masked(&self) -> bool {
        (self.ctl & ctl_bits::IMASK) != 0
    }

    /// 割り込みがアサートされているか (カウンタ >= CVAL)
    pub fn is_asserted(&self, counter: u64) -> bool {
        self.is_enabled() && counter >= self.cval
    }

    /// 割り込みをトリガーすべきか
    pub fn should_interrupt(&self, counter: u64) -> bool {
        self.is_asserted(counter) && !self.is_masked()
    }

    /// CTL レジスタを読み取り
    pub fn read_ctl(&self, counter: u64) -> u64 {
        let mut value = self.ctl & (ctl_bits::ENABLE | ctl_bits::IMASK);
        if self.is_asserted(counter) {
            value |= ctl_bits::ISTATUS;
        }
        value
    }

    /// CTL レジスタに書き込み
    pub fn write_ctl(&mut self, value: u64) {
        // ISTATUS は読み取り専用なので無視
        self.ctl = value & (ctl_bits::ENABLE | ctl_bits::IMASK);
    }

    /// TVAL レジスタを読み取り (カウンタからの相対値)
    pub fn read_tval(&self, counter: u64) -> u64 {
        // TVAL = CVAL - Counter (符号付き)
        self.cval.wrapping_sub(counter)
    }

    /// TVAL レジスタに書き込み (カウンタからの相対値)
    pub fn write_tval(&mut self, value: u64, counter: u64) {
        // CVAL = Counter + TVAL
        self.cval = counter.wrapping_add(value);
    }

    /// CVAL レジスタを読み取り
    pub fn read_cval(&self) -> u64 {
        self.cval
    }

    /// CVAL レジスタに書き込み
    pub fn write_cval(&mut self, value: u64) {
        self.cval = value;
    }
}

/// ARM Generic Timer
#[derive(Debug)]
pub struct Timer {
    /// 開始時刻 (カウンタ計算用)
    start_time: Instant,
    /// 物理タイマー
    pub phys_timer: TimerState,
    /// 仮想タイマー
    pub virt_timer: TimerState,
    /// 仮想オフセット (CNTVOFF_EL2)
    virt_offset: u64,
}

impl Default for Timer {
    fn default() -> Self {
        Self::new()
    }
}

impl Timer {
    /// 新しいタイマーを作成
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            phys_timer: TimerState::new(),
            virt_timer: TimerState::new(),
            virt_offset: 0,
        }
    }

    /// 物理カウンタ値を取得 (CNTPCT_EL0)
    pub fn get_phys_counter(&self) -> u64 {
        let elapsed = self.start_time.elapsed();
        let nanos = elapsed.as_nanos() as u64;
        // カウンタ = 経過時間 * 周波数 / 10^9
        nanos * TIMER_FREQ / 1_000_000_000
    }

    /// 仮想カウンタ値を取得 (CNTVCT_EL0)
    pub fn get_virt_counter(&self) -> u64 {
        self.get_phys_counter().wrapping_sub(self.virt_offset)
    }

    /// タイマー周波数を取得 (CNTFRQ_EL0)
    pub fn get_frequency(&self) -> u64 {
        TIMER_FREQ
    }

    /// 仮想オフセットを設定 (CNTVOFF_EL2)
    pub fn set_virt_offset(&mut self, offset: u64) {
        self.virt_offset = offset;
    }

    /// 仮想オフセットを取得
    pub fn get_virt_offset(&self) -> u64 {
        self.virt_offset
    }

    /// 物理タイマーが割り込みをトリガーすべきか
    pub fn phys_timer_pending(&self) -> bool {
        self.phys_timer.should_interrupt(self.get_phys_counter())
    }

    /// 仮想タイマーが割り込みをトリガーすべきか
    pub fn virt_timer_pending(&self) -> bool {
        self.virt_timer.should_interrupt(self.get_virt_counter())
    }

    /// 仮想タイマーがアサートされているか（IMASK を無視）
    ///
    /// GIC 経由で IRQ を注入する場合、ハードウェア FIQ 防止のために IMASK=1 を強制している。
    /// そのため IMASK を無視してタイマー発火を検出する必要がある。
    pub fn virt_timer_asserted(&self) -> bool {
        self.virt_timer.is_asserted(self.get_virt_counter())
    }

    /// ペンディング中のタイマー IRQ を取得
    pub fn get_pending_irqs(&self) -> Vec<u32> {
        let mut irqs = Vec::new();
        if self.phys_timer_pending() {
            irqs.push(PHYS_TIMER_IRQ);
        }
        if self.virt_timer_pending() {
            irqs.push(VIRT_TIMER_IRQ);
        }
        irqs
    }

    /// 次のタイマーイベントまでの時間 (ナノ秒)
    pub fn time_until_next_event(&self) -> Option<u64> {
        let phys_counter = self.get_phys_counter();
        let virt_counter = self.get_virt_counter();

        let mut min_ticks: Option<u64> = None;

        // 物理タイマー
        if self.phys_timer.is_enabled()
            && !self.phys_timer.is_masked()
            && self.phys_timer.cval > phys_counter
        {
            let ticks = self.phys_timer.cval - phys_counter;
            min_ticks = Some(min_ticks.map_or(ticks, |m| m.min(ticks)));
        }

        // 仮想タイマー
        if self.virt_timer.is_enabled()
            && !self.virt_timer.is_masked()
            && self.virt_timer.cval > virt_counter
        {
            let ticks = self.virt_timer.cval - virt_counter;
            min_ticks = Some(min_ticks.map_or(ticks, |m| m.min(ticks)));
        }

        // ティックをナノ秒に変換
        min_ticks.map(|ticks| ticks * 1_000_000_000 / TIMER_FREQ)
    }

    /// システムレジスタを読み取り
    pub fn read_sysreg(&self, reg: TimerReg) -> Result<u64, Box<dyn Error>> {
        let phys_counter = self.get_phys_counter();
        let virt_counter = self.get_virt_counter();

        let value = match reg {
            TimerReg::CNTFRQ_EL0 => self.get_frequency(),
            TimerReg::CNTPCT_EL0 => phys_counter,
            TimerReg::CNTVCT_EL0 => virt_counter,
            TimerReg::CNTP_CTL_EL0 => self.phys_timer.read_ctl(phys_counter),
            TimerReg::CNTP_CVAL_EL0 => self.phys_timer.read_cval(),
            TimerReg::CNTP_TVAL_EL0 => self.phys_timer.read_tval(phys_counter),
            TimerReg::CNTV_CTL_EL0 => self.virt_timer.read_ctl(virt_counter),
            TimerReg::CNTV_CVAL_EL0 => self.virt_timer.read_cval(),
            TimerReg::CNTV_TVAL_EL0 => self.virt_timer.read_tval(virt_counter),
            TimerReg::CNTVOFF_EL2 => self.virt_offset,
        };
        Ok(value)
    }

    /// システムレジスタに書き込み
    pub fn write_sysreg(&mut self, reg: TimerReg, value: u64) -> Result<(), Box<dyn Error>> {
        let phys_counter = self.get_phys_counter();
        let virt_counter = self.get_virt_counter();
        let _ = virt_counter; // 将来のデバッグ用に保持

        match reg {
            TimerReg::CNTFRQ_EL0 => {
                // 周波数は読み取り専用として扱う
            }
            TimerReg::CNTPCT_EL0 | TimerReg::CNTVCT_EL0 => {
                // カウンタは読み取り専用
            }
            TimerReg::CNTP_CTL_EL0 => self.phys_timer.write_ctl(value),
            TimerReg::CNTP_CVAL_EL0 => self.phys_timer.write_cval(value),
            TimerReg::CNTP_TVAL_EL0 => self.phys_timer.write_tval(value, phys_counter),
            TimerReg::CNTV_CTL_EL0 => self.virt_timer.write_ctl(value),
            TimerReg::CNTV_CVAL_EL0 => self.virt_timer.write_cval(value),
            TimerReg::CNTV_TVAL_EL0 => self.virt_timer.write_tval(value, virt_counter),
            TimerReg::CNTVOFF_EL2 => self.virt_offset = value,
        }
        Ok(())
    }
}

/// タイマーシステムレジスタ
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(non_camel_case_types)]
pub enum TimerReg {
    /// カウンタ周波数
    CNTFRQ_EL0,
    /// 物理カウンタ
    CNTPCT_EL0,
    /// 仮想カウンタ
    CNTVCT_EL0,
    /// 物理タイマー制御
    CNTP_CTL_EL0,
    /// 物理タイマー比較値
    CNTP_CVAL_EL0,
    /// 物理タイマータイマー値
    CNTP_TVAL_EL0,
    /// 仮想タイマー制御
    CNTV_CTL_EL0,
    /// 仮想タイマー比較値
    CNTV_CVAL_EL0,
    /// 仮想タイマータイマー値
    CNTV_TVAL_EL0,
    /// 仮想オフセット
    CNTVOFF_EL2,
}

impl TimerReg {
    /// システムレジスタエンコーディングから TimerReg を取得
    ///
    /// # Arguments
    /// * `op0` - Op0 フィールド (2 bits)
    /// * `op1` - Op1 フィールド (3 bits)
    /// * `crn` - CRn フィールド (4 bits)
    /// * `crm` - CRm フィールド (4 bits)
    /// * `op2` - Op2 フィールド (3 bits)
    ///
    /// # Returns
    /// 対応する TimerReg があれば Some、なければ None
    pub fn from_encoding(op0: u8, op1: u8, crn: u8, crm: u8, op2: u8) -> Option<Self> {
        // Timer レジスタは CRn=14 が共通
        if crn != 14 {
            return None;
        }

        match (op0, op1, crm, op2) {
            // EL0 タイマーレジスタ (Op0=3, Op1=3)
            (3, 3, 0, 0) => Some(TimerReg::CNTFRQ_EL0),
            (3, 3, 0, 1) => Some(TimerReg::CNTPCT_EL0),
            (3, 3, 0, 2) => Some(TimerReg::CNTVCT_EL0),
            (3, 3, 2, 0) => Some(TimerReg::CNTP_TVAL_EL0),
            (3, 3, 2, 1) => Some(TimerReg::CNTP_CTL_EL0),
            (3, 3, 2, 2) => Some(TimerReg::CNTP_CVAL_EL0),
            (3, 3, 3, 0) => Some(TimerReg::CNTV_TVAL_EL0),
            (3, 3, 3, 1) => Some(TimerReg::CNTV_CTL_EL0),
            (3, 3, 3, 2) => Some(TimerReg::CNTV_CVAL_EL0),
            // EL2 タイマーレジスタ (Op0=3, Op1=4)
            (3, 4, 0, 3) => Some(TimerReg::CNTVOFF_EL2),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn timer_new_の初期状態を確認() {
        let timer = Timer::new();
        assert!(!timer.phys_timer.is_enabled());
        assert!(!timer.virt_timer.is_enabled());
        assert_eq!(timer.virt_offset, 0);
    }

    #[test]
    fn get_frequency_は正しい周波数を返す() {
        let timer = Timer::new();
        assert_eq!(timer.get_frequency(), TIMER_FREQ);
    }

    #[test]
    fn get_phys_counter_は増加する() {
        let timer = Timer::new();
        let c1 = timer.get_phys_counter();
        thread::sleep(Duration::from_millis(10));
        let c2 = timer.get_phys_counter();
        assert!(c2 > c1);
    }

    #[test]
    fn get_virt_counter_はオフセットを反映する() {
        let mut timer = Timer::new();
        let phys = timer.get_phys_counter();
        timer.set_virt_offset(1000);
        let virt = timer.get_virt_counter();
        // virt = phys - offset なので virt < phys
        assert!(virt < phys || phys < 1000);
    }

    #[test]
    fn timer_state_ctl_の読み書き() {
        let mut state = TimerState::new();
        state.write_ctl(ctl_bits::ENABLE | ctl_bits::IMASK);
        assert!(state.is_enabled());
        assert!(state.is_masked());
    }

    #[test]
    fn timer_state_cval_の読み書き() {
        let mut state = TimerState::new();
        state.write_cval(12345);
        assert_eq!(state.read_cval(), 12345);
    }

    #[test]
    fn timer_state_tval_の読み書き() {
        let mut state = TimerState::new();
        let counter = 1000u64;
        state.write_tval(500, counter);
        // CVAL = counter + tval = 1000 + 500 = 1500
        assert_eq!(state.read_cval(), 1500);
        // TVAL = CVAL - counter = 1500 - 1000 = 500
        assert_eq!(state.read_tval(counter), 500);
    }

    #[test]
    fn timer_state_is_asserted_はカウンタがcvalを超えるとtrueを返す() {
        let mut state = TimerState::new();
        state.write_ctl(ctl_bits::ENABLE);
        state.write_cval(100);

        assert!(!state.is_asserted(50));
        assert!(state.is_asserted(100));
        assert!(state.is_asserted(150));
    }

    #[test]
    fn timer_state_should_interrupt_はマスク時にfalseを返す() {
        let mut state = TimerState::new();
        state.write_ctl(ctl_bits::ENABLE | ctl_bits::IMASK);
        state.write_cval(100);

        // アサートされているがマスクされている
        assert!(state.is_asserted(150));
        assert!(!state.should_interrupt(150));
    }

    #[test]
    fn timer_state_read_ctl_はistatusを含む() {
        let mut state = TimerState::new();
        state.write_ctl(ctl_bits::ENABLE);
        state.write_cval(100);

        // カウンタ < CVAL: ISTATUS = 0
        let ctl = state.read_ctl(50);
        assert_eq!(ctl & ctl_bits::ISTATUS, 0);

        // カウンタ >= CVAL: ISTATUS = 1
        let ctl = state.read_ctl(150);
        assert_ne!(ctl & ctl_bits::ISTATUS, 0);
    }

    #[test]
    fn phys_timer_pending_は正しく判定する() {
        let mut timer = Timer::new();
        let counter = timer.get_phys_counter();

        // タイマーを有効化して過去の値を設定
        timer.phys_timer.write_ctl(ctl_bits::ENABLE);
        timer.phys_timer.write_cval(counter.saturating_sub(100));

        assert!(timer.phys_timer_pending());
    }

    #[test]
    fn read_sysreg_でcntfrq_el0を読める() {
        let timer = Timer::new();
        let freq = timer.read_sysreg(TimerReg::CNTFRQ_EL0).unwrap();
        assert_eq!(freq, TIMER_FREQ);
    }

    #[test]
    fn write_sysreg_でcntp_ctl_el0を書ける() {
        let mut timer = Timer::new();
        timer
            .write_sysreg(TimerReg::CNTP_CTL_EL0, ctl_bits::ENABLE)
            .unwrap();
        assert!(timer.phys_timer.is_enabled());
    }

    #[test]
    fn time_until_next_event_は有効なタイマーがない場合noneを返す() {
        let timer = Timer::new();
        assert!(timer.time_until_next_event().is_none());
    }

    #[test]
    fn time_until_next_event_は次のイベントまでの時間を返す() {
        let mut timer = Timer::new();
        let counter = timer.get_phys_counter();

        timer.phys_timer.write_ctl(ctl_bits::ENABLE);
        // 1秒後に設定
        timer.phys_timer.write_cval(counter + TIMER_FREQ);

        let time = timer.time_until_next_event();
        assert!(time.is_some());
        // おおよそ1秒 (1_000_000_000 ナノ秒) 前後
        let nanos = time.unwrap();
        assert!(nanos > 900_000_000 && nanos < 1_100_000_000);
    }

    #[test]
    fn get_pending_irqs_はペンディング中のirqを返す() {
        let mut timer = Timer::new();
        let counter = timer.get_phys_counter();

        // 物理タイマーを過去に設定
        timer.phys_timer.write_ctl(ctl_bits::ENABLE);
        timer.phys_timer.write_cval(counter.saturating_sub(100));

        let irqs = timer.get_pending_irqs();
        assert!(irqs.contains(&PHYS_TIMER_IRQ));
    }

    #[test]
    fn from_encoding_でcntfrq_el0を正しく識別する() {
        // CNTFRQ_EL0: Op0=3, Op1=3, CRn=14, CRm=0, Op2=0
        let reg = TimerReg::from_encoding(3, 3, 14, 0, 0);
        assert_eq!(reg, Some(TimerReg::CNTFRQ_EL0));
    }

    #[test]
    fn from_encoding_でcntpct_el0を正しく識別する() {
        // CNTPCT_EL0: Op0=3, Op1=3, CRn=14, CRm=0, Op2=1
        let reg = TimerReg::from_encoding(3, 3, 14, 0, 1);
        assert_eq!(reg, Some(TimerReg::CNTPCT_EL0));
    }

    #[test]
    fn from_encoding_でcntvct_el0を正しく識別する() {
        // CNTVCT_EL0: Op0=3, Op1=3, CRn=14, CRm=0, Op2=2
        let reg = TimerReg::from_encoding(3, 3, 14, 0, 2);
        assert_eq!(reg, Some(TimerReg::CNTVCT_EL0));
    }

    #[test]
    fn from_encoding_でcntp_ctl_el0を正しく識別する() {
        // CNTP_CTL_EL0: Op0=3, Op1=3, CRn=14, CRm=2, Op2=1
        let reg = TimerReg::from_encoding(3, 3, 14, 2, 1);
        assert_eq!(reg, Some(TimerReg::CNTP_CTL_EL0));
    }

    #[test]
    fn from_encoding_でcntp_cval_el0を正しく識別する() {
        // CNTP_CVAL_EL0: Op0=3, Op1=3, CRn=14, CRm=2, Op2=2
        let reg = TimerReg::from_encoding(3, 3, 14, 2, 2);
        assert_eq!(reg, Some(TimerReg::CNTP_CVAL_EL0));
    }

    #[test]
    fn from_encoding_でcntp_tval_el0を正しく識別する() {
        // CNTP_TVAL_EL0: Op0=3, Op1=3, CRn=14, CRm=2, Op2=0
        let reg = TimerReg::from_encoding(3, 3, 14, 2, 0);
        assert_eq!(reg, Some(TimerReg::CNTP_TVAL_EL0));
    }

    #[test]
    fn from_encoding_でcntv_ctl_el0を正しく識別する() {
        // CNTV_CTL_EL0: Op0=3, Op1=3, CRn=14, CRm=3, Op2=1
        let reg = TimerReg::from_encoding(3, 3, 14, 3, 1);
        assert_eq!(reg, Some(TimerReg::CNTV_CTL_EL0));
    }

    #[test]
    fn from_encoding_でcntv_cval_el0を正しく識別する() {
        // CNTV_CVAL_EL0: Op0=3, Op1=3, CRn=14, CRm=3, Op2=2
        let reg = TimerReg::from_encoding(3, 3, 14, 3, 2);
        assert_eq!(reg, Some(TimerReg::CNTV_CVAL_EL0));
    }

    #[test]
    fn from_encoding_でcntv_tval_el0を正しく識別する() {
        // CNTV_TVAL_EL0: Op0=3, Op1=3, CRn=14, CRm=3, Op2=0
        let reg = TimerReg::from_encoding(3, 3, 14, 3, 0);
        assert_eq!(reg, Some(TimerReg::CNTV_TVAL_EL0));
    }

    #[test]
    fn from_encoding_でcntvoff_el2を正しく識別する() {
        // CNTVOFF_EL2: Op0=3, Op1=4, CRn=14, CRm=0, Op2=3
        let reg = TimerReg::from_encoding(3, 4, 14, 0, 3);
        assert_eq!(reg, Some(TimerReg::CNTVOFF_EL2));
    }

    #[test]
    fn from_encoding_で未対応のレジスタはnoneを返す() {
        // CRn != 14
        let reg = TimerReg::from_encoding(3, 3, 0, 0, 0);
        assert_eq!(reg, None);

        // 存在しない組み合わせ
        let reg = TimerReg::from_encoding(3, 3, 14, 5, 0);
        assert_eq!(reg, None);
    }
}
