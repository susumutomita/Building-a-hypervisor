//! GICv2 (Generic Interrupt Controller) エミュレーション
//!
//! ARM GICv2 の基本的なエミュレーションを提供します。
//! - GICD (Distributor): 割り込みのルーティングと優先度管理
//! - GICC (CPU Interface): CPU への割り込み配信

use crate::mmio::MmioHandler;
use std::error::Error;
use std::sync::{Arc, Mutex};

/// 共有 GIC タイプ
pub type SharedGic = Arc<Mutex<Gic>>;

/// GICv2 のデフォルトベースアドレス
pub const GIC_DIST_BASE: u64 = 0x0800_0000;
pub const GIC_CPU_BASE: u64 = 0x0801_0000;
pub const GIC_DIST_SIZE: u64 = 0x1_0000;
pub const GIC_CPU_SIZE: u64 = 0x1_0000;

/// サポートする最大割り込み数 (SPIs + PPIs + SGIs)
const MAX_IRQS: usize = 256;
/// SPI (Shared Peripheral Interrupts) の開始番号
const SPI_START: usize = 32;

// GICD レジスタオフセット
#[allow(dead_code)]
mod gicd_regs {
    pub const CTLR: u64 = 0x000; // Distributor Control Register
    pub const TYPER: u64 = 0x004; // Interrupt Controller Type Register
    pub const IIDR: u64 = 0x008; // Implementer Identification Register
    pub const IGROUPR: u64 = 0x080; // Interrupt Group Registers (0x080-0x0FC)
    pub const ISENABLER: u64 = 0x100; // Interrupt Set-Enable Registers (0x100-0x17C)
    pub const ICENABLER: u64 = 0x180; // Interrupt Clear-Enable Registers (0x180-0x1FC)
    pub const ISPENDR: u64 = 0x200; // Interrupt Set-Pending Registers (0x200-0x27C)
    pub const ICPENDR: u64 = 0x280; // Interrupt Clear-Pending Registers (0x280-0x2FC)
    pub const ISACTIVER: u64 = 0x300; // Interrupt Set-Active Registers
    pub const ICACTIVER: u64 = 0x380; // Interrupt Clear-Active Registers
    pub const IPRIORITYR: u64 = 0x400; // Interrupt Priority Registers (0x400-0x7FC)
    pub const ITARGETSR: u64 = 0x800; // Interrupt Processor Targets Registers (0x800-0xBFC)
    pub const ICFGR: u64 = 0xC00; // Interrupt Configuration Registers (0xC00-0xCFC)
    pub const SGIR: u64 = 0xF00; // Software Generated Interrupt Register
}

// GICC レジスタオフセット
mod gicc_regs {
    pub const CTLR: u64 = 0x000; // CPU Interface Control Register
    pub const PMR: u64 = 0x004; // Interrupt Priority Mask Register
    pub const BPR: u64 = 0x008; // Binary Point Register
    pub const IAR: u64 = 0x00C; // Interrupt Acknowledge Register
    pub const EOIR: u64 = 0x010; // End of Interrupt Register
    pub const RPR: u64 = 0x014; // Running Priority Register
    pub const HPPIR: u64 = 0x018; // Highest Priority Pending Interrupt Register
    pub const IIDR: u64 = 0x00FC; // CPU Interface Identification Register
}

/// GICv2 Distributor の状態
#[derive(Debug)]
pub struct GicDistributor {
    /// Distributor が有効かどうか
    enabled: bool,
    /// 各割り込みの有効状態 (ビットマップ)
    irq_enabled: [u32; MAX_IRQS / 32],
    /// 各割り込みのペンディング状態 (ビットマップ)
    irq_pending: [u32; MAX_IRQS / 32],
    /// 各割り込みのアクティブ状態 (ビットマップ)
    irq_active: [u32; MAX_IRQS / 32],
    /// 各割り込みの優先度 (0-255, 低い値が高優先度)
    irq_priority: [u8; MAX_IRQS],
    /// 各割り込みのターゲット CPU マスク
    irq_targets: [u8; MAX_IRQS],
    /// 各割り込みの設定 (エッジ/レベルトリガー)
    /// 将来の拡張用に保持
    #[allow(dead_code)]
    irq_config: [u32; MAX_IRQS / 16],
}

impl Default for GicDistributor {
    fn default() -> Self {
        Self::new()
    }
}

impl GicDistributor {
    /// 新しい Distributor を作成
    pub fn new() -> Self {
        let mut dist = Self {
            enabled: false,
            irq_enabled: [0; MAX_IRQS / 32],
            irq_pending: [0; MAX_IRQS / 32],
            irq_active: [0; MAX_IRQS / 32],
            irq_priority: [0xA0; MAX_IRQS], // 中程度の優先度で初期化
            irq_targets: [0x01; MAX_IRQS],  // CPU 0 をターゲット
            irq_config: [0; MAX_IRQS / 16],
        };
        // SGI (0-15) はデフォルトで有効
        dist.irq_enabled[0] = 0xFFFF;
        // PPI (16-31) もデフォルトで有効 (タイマー IRQ を含む)
        dist.irq_enabled[0] |= 0xFFFF_0000;
        dist
    }

    /// TYPER レジスタの値を取得
    fn get_typer(&self) -> u32 {
        // ITLinesNumber: (MAX_IRQS / 32) - 1
        // CPUNumber: 0 (1 CPU)
        // SecurityExtn: 0 (セキュリティ拡張なし)
        let it_lines = ((MAX_IRQS / 32) - 1) as u32;
        it_lines & 0x1F
    }
}

/// GICv2 CPU Interface の状態
#[derive(Debug)]
pub struct GicCpuInterface {
    /// CPU Interface が有効かどうか
    enabled: bool,
    /// 優先度マスク (この値以下の優先度の割り込みのみ配信)
    priority_mask: u8,
    /// Binary Point Register
    binary_point: u8,
    /// 現在処理中の割り込み番号
    running_irq: Option<u32>,
    /// 現在の実行優先度
    running_priority: u8,
}

impl Default for GicCpuInterface {
    fn default() -> Self {
        Self::new()
    }
}

impl GicCpuInterface {
    /// 新しい CPU Interface を作成
    pub fn new() -> Self {
        Self {
            enabled: false,
            priority_mask: 0xFF, // すべての割り込みを許可
            binary_point: 0,
            running_irq: None,
            running_priority: 0xFF, // アイドル状態
        }
    }
}

/// GICv2 全体の状態
#[derive(Debug)]
pub struct Gic {
    /// Distributor
    pub distributor: GicDistributor,
    /// CPU Interface (単一 CPU をサポート)
    pub cpu_interface: GicCpuInterface,
    /// ベースアドレス (Distributor)
    base_addr: u64,
}

impl Default for Gic {
    fn default() -> Self {
        Self::new()
    }
}

impl Gic {
    /// 新しい GIC を作成
    pub fn new() -> Self {
        Self {
            distributor: GicDistributor::new(),
            cpu_interface: GicCpuInterface::new(),
            base_addr: GIC_DIST_BASE,
        }
    }

    /// カスタムベースアドレスで GIC を作成
    pub fn with_base(base_addr: u64) -> Self {
        Self {
            distributor: GicDistributor::new(),
            cpu_interface: GicCpuInterface::new(),
            base_addr,
        }
    }

    /// 割り込みを発生させる (ペンディング状態にする)
    pub fn set_irq_pending(&mut self, irq: u32) {
        if (irq as usize) < MAX_IRQS {
            let idx = irq as usize / 32;
            let bit = irq as usize % 32;
            self.distributor.irq_pending[idx] |= 1 << bit;
        }
    }

    /// 割り込みのペンディング状態をクリア
    pub fn clear_irq_pending(&mut self, irq: u32) {
        if (irq as usize) < MAX_IRQS {
            let idx = irq as usize / 32;
            let bit = irq as usize % 32;
            self.distributor.irq_pending[idx] &= !(1 << bit);
        }
    }

    /// 最高優先度のペンディング割り込みを取得
    pub fn get_highest_pending_irq(&self) -> Option<u32> {
        if !self.distributor.enabled || !self.cpu_interface.enabled {
            return None;
        }

        let mut highest_irq: Option<u32> = None;
        let mut highest_priority: u8 = 0xFF;

        for irq in 0..MAX_IRQS {
            let idx = irq / 32;
            let bit = irq % 32;

            // 有効かつペンディングかつアクティブでない割り込みをチェック
            let is_enabled = (self.distributor.irq_enabled[idx] >> bit) & 1 != 0;
            let is_pending = (self.distributor.irq_pending[idx] >> bit) & 1 != 0;
            let is_active = (self.distributor.irq_active[idx] >> bit) & 1 != 0;

            if is_enabled && is_pending && !is_active {
                let priority = self.distributor.irq_priority[irq];
                // 優先度マスクと現在の実行優先度をチェック
                if priority < self.cpu_interface.priority_mask
                    && priority < self.cpu_interface.running_priority
                    && priority < highest_priority
                {
                    highest_priority = priority;
                    highest_irq = Some(irq as u32);
                }
            }
        }

        highest_irq
    }

    /// 割り込みを acknowledge (IAR 読み取り時に呼ばれる)
    pub fn acknowledge_irq(&mut self) -> u32 {
        if let Some(irq) = self.get_highest_pending_irq() {
            let idx = irq as usize / 32;
            let bit = irq as usize % 32;

            // アクティブ状態にする
            self.distributor.irq_active[idx] |= 1 << bit;
            // ペンディングをクリア (エッジトリガーの場合)
            self.distributor.irq_pending[idx] &= !(1 << bit);

            // 実行優先度を更新
            self.cpu_interface.running_irq = Some(irq);
            self.cpu_interface.running_priority = self.distributor.irq_priority[irq as usize];

            irq
        } else {
            // スプリアス割り込み
            1023
        }
    }

    /// 割り込み処理完了 (EOIR 書き込み時に呼ばれる)
    pub fn end_of_interrupt(&mut self, irq: u32) {
        if (irq as usize) < MAX_IRQS {
            let idx = irq as usize / 32;
            let bit = irq as usize % 32;

            // アクティブ状態をクリア
            self.distributor.irq_active[idx] &= !(1 << bit);

            // 実行状態をリセット
            if self.cpu_interface.running_irq == Some(irq) {
                self.cpu_interface.running_irq = None;
                self.cpu_interface.running_priority = 0xFF;
            }
        }
    }

    /// ペンディング中の割り込みがあるかチェック
    /// GIC が有効でペンディング中の割り込みがあれば true を返す
    pub fn has_pending_interrupt(&self) -> bool {
        self.get_highest_pending_irq().is_some()
    }

    /// GICD (Distributor) の読み取り処理
    fn read_distributor(&mut self, offset: u64) -> u64 {
        match offset {
            gicd_regs::CTLR => self.distributor.enabled as u64,
            gicd_regs::TYPER => self.distributor.get_typer() as u64,
            gicd_regs::IIDR => 0x0102_043B, // ARM GIC-400 互換
            o if (gicd_regs::ISENABLER..gicd_regs::ISENABLER + 0x80).contains(&o) => {
                let idx = ((o - gicd_regs::ISENABLER) / 4) as usize;
                if idx < self.distributor.irq_enabled.len() {
                    self.distributor.irq_enabled[idx] as u64
                } else {
                    0
                }
            }
            o if (gicd_regs::ISPENDR..gicd_regs::ISPENDR + 0x80).contains(&o) => {
                let idx = ((o - gicd_regs::ISPENDR) / 4) as usize;
                if idx < self.distributor.irq_pending.len() {
                    self.distributor.irq_pending[idx] as u64
                } else {
                    0
                }
            }
            o if (gicd_regs::IPRIORITYR..gicd_regs::IPRIORITYR + 0x400).contains(&o) => {
                let base_idx = (o - gicd_regs::IPRIORITYR) as usize;
                let mut value: u32 = 0;
                for i in 0..4 {
                    if base_idx + i < MAX_IRQS {
                        value |= (self.distributor.irq_priority[base_idx + i] as u32) << (i * 8);
                    }
                }
                value as u64
            }
            o if (gicd_regs::ITARGETSR..gicd_regs::ITARGETSR + 0x400).contains(&o) => {
                let base_idx = (o - gicd_regs::ITARGETSR) as usize;
                let mut value: u32 = 0;
                for i in 0..4 {
                    if base_idx + i < MAX_IRQS {
                        value |= (self.distributor.irq_targets[base_idx + i] as u32) << (i * 8);
                    }
                }
                value as u64
            }
            _ => 0,
        }
    }

    /// GICD (Distributor) の書き込み処理
    fn write_distributor(&mut self, offset: u64, value: u64) {
        let value = value as u32;
        match offset {
            gicd_regs::CTLR => {
                self.distributor.enabled = (value & 1) != 0;
            }
            o if (gicd_regs::ISENABLER..gicd_regs::ISENABLER + 0x80).contains(&o) => {
                let idx = ((o - gicd_regs::ISENABLER) / 4) as usize;
                if idx < self.distributor.irq_enabled.len() {
                    self.distributor.irq_enabled[idx] |= value;
                }
            }
            o if (gicd_regs::ICENABLER..gicd_regs::ICENABLER + 0x80).contains(&o) => {
                let idx = ((o - gicd_regs::ICENABLER) / 4) as usize;
                if idx < self.distributor.irq_enabled.len() {
                    self.distributor.irq_enabled[idx] &= !value;
                }
            }
            o if (gicd_regs::ISPENDR..gicd_regs::ISPENDR + 0x80).contains(&o) => {
                let idx = ((o - gicd_regs::ISPENDR) / 4) as usize;
                if idx < self.distributor.irq_pending.len() {
                    self.distributor.irq_pending[idx] |= value;
                }
            }
            o if (gicd_regs::ICPENDR..gicd_regs::ICPENDR + 0x80).contains(&o) => {
                let idx = ((o - gicd_regs::ICPENDR) / 4) as usize;
                if idx < self.distributor.irq_pending.len() {
                    self.distributor.irq_pending[idx] &= !value;
                }
            }
            o if (gicd_regs::IPRIORITYR..gicd_regs::IPRIORITYR + 0x400).contains(&o) => {
                let base_idx = (o - gicd_regs::IPRIORITYR) as usize;
                for i in 0..4 {
                    if base_idx + i < MAX_IRQS {
                        self.distributor.irq_priority[base_idx + i] =
                            ((value >> (i * 8)) & 0xFF) as u8;
                    }
                }
            }
            o if (gicd_regs::ITARGETSR..gicd_regs::ITARGETSR + 0x400).contains(&o) => {
                // SGI (0-15) と PPI (16-31) のターゲットは読み取り専用
                let base_idx = (o - gicd_regs::ITARGETSR) as usize;
                for i in 0..4 {
                    let irq_idx = base_idx + i;
                    if (SPI_START..MAX_IRQS).contains(&irq_idx) {
                        self.distributor.irq_targets[irq_idx] = ((value >> (i * 8)) & 0xFF) as u8;
                    }
                }
            }
            gicd_regs::SGIR => {
                // Software Generated Interrupt
                let target_list = ((value >> 16) & 0xFF) as u8;
                let sgi_id = value & 0xF;
                if target_list != 0 {
                    self.set_irq_pending(sgi_id);
                }
            }
            _ => {}
        }
    }

    /// GICC (CPU Interface) の読み取り処理
    fn read_cpu_interface(&mut self, offset: u64) -> u64 {
        match offset {
            gicc_regs::CTLR => self.cpu_interface.enabled as u64,
            gicc_regs::PMR => self.cpu_interface.priority_mask as u64,
            gicc_regs::BPR => self.cpu_interface.binary_point as u64,
            gicc_regs::IAR => self.acknowledge_irq() as u64,
            gicc_regs::RPR => self.cpu_interface.running_priority as u64,
            gicc_regs::HPPIR => self.get_highest_pending_irq().unwrap_or(1023) as u64,
            gicc_regs::IIDR => 0x0102_043B, // ARM GIC-400 互換
            _ => 0,
        }
    }

    /// GICC (CPU Interface) の書き込み処理
    fn write_cpu_interface(&mut self, offset: u64, value: u64) {
        match offset {
            gicc_regs::CTLR => {
                self.cpu_interface.enabled = (value & 1) != 0;
            }
            gicc_regs::PMR => {
                self.cpu_interface.priority_mask = (value & 0xFF) as u8;
            }
            gicc_regs::BPR => {
                self.cpu_interface.binary_point = (value & 0x7) as u8;
            }
            gicc_regs::EOIR => {
                self.end_of_interrupt((value & 0x3FF) as u32);
            }
            _ => {}
        }
    }
}

/// MmioHandler の実装
impl MmioHandler for Gic {
    fn base(&self) -> u64 {
        self.base_addr
    }

    fn size(&self) -> u64 {
        // Distributor + CPU Interface
        GIC_DIST_SIZE + GIC_CPU_SIZE
    }

    fn read(&mut self, offset: u64, _size: usize) -> Result<u64, Box<dyn Error>> {
        if offset < GIC_DIST_SIZE {
            // GICD 領域
            Ok(self.read_distributor(offset))
        } else if offset < GIC_DIST_SIZE + GIC_CPU_SIZE {
            // GICC 領域
            let gicc_offset = offset - GIC_DIST_SIZE;
            Ok(self.read_cpu_interface(gicc_offset))
        } else {
            Ok(0)
        }
    }

    fn write(&mut self, offset: u64, value: u64, _size: usize) -> Result<(), Box<dyn Error>> {
        if offset < GIC_DIST_SIZE {
            // GICD 領域
            self.write_distributor(offset, value);
        } else if offset < GIC_DIST_SIZE + GIC_CPU_SIZE {
            // GICC 領域
            let gicc_offset = offset - GIC_DIST_SIZE;
            self.write_cpu_interface(gicc_offset, value);
        }
        Ok(())
    }
}

/// 共有 GIC を MMIO ハンドラとして使うためのラッパー
///
/// `Arc<Mutex<Gic>>` を使って GIC を共有しながら、MMIO ハンドラとして登録できます。
#[derive(Debug)]
pub struct SharedGicWrapper {
    gic: SharedGic,
    base_addr: u64,
}

impl SharedGicWrapper {
    /// 新しい共有 GIC ラッパーを作成
    pub fn new(gic: SharedGic, base_addr: u64) -> Self {
        Self { gic, base_addr }
    }

    /// 共有 GIC への参照を取得
    pub fn gic(&self) -> &SharedGic {
        &self.gic
    }
}

impl MmioHandler for SharedGicWrapper {
    fn base(&self) -> u64 {
        self.base_addr
    }

    fn size(&self) -> u64 {
        GIC_DIST_SIZE + GIC_CPU_SIZE
    }

    fn read(&mut self, offset: u64, size: usize) -> Result<u64, Box<dyn Error>> {
        let mut gic = self
            .gic
            .lock()
            .map_err(|e| format!("GIC lock error: {}", e))?;
        gic.read(offset, size)
    }

    fn write(&mut self, offset: u64, value: u64, size: usize) -> Result<(), Box<dyn Error>> {
        let mut gic = self
            .gic
            .lock()
            .map_err(|e| format!("GIC lock error: {}", e))?;
        gic.write(offset, value, size)
    }
}

/// 共有 GIC を作成するヘルパー関数
pub fn create_shared_gic(base_addr: u64) -> SharedGic {
    Arc::new(Mutex::new(Gic::with_base(base_addr)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gic_new_の初期状態を確認() {
        let gic = Gic::new();
        assert!(!gic.distributor.enabled);
        assert!(!gic.cpu_interface.enabled);
        // SGI (0-15) と PPI (16-31) はデフォルトで有効
        assert_eq!(gic.distributor.irq_enabled[0], 0xFFFF_FFFF);
        // デフォルト優先度は 0xA0
        assert_eq!(gic.distributor.irq_priority[0], 0xA0);
    }

    #[test]
    fn set_irq_pending_で割り込みをペンディングにできる() {
        let mut gic = Gic::new();
        gic.set_irq_pending(32);
        assert_eq!(gic.distributor.irq_pending[1], 1);
    }

    #[test]
    fn clear_irq_pending_でペンディングをクリアできる() {
        let mut gic = Gic::new();
        gic.set_irq_pending(32);
        gic.clear_irq_pending(32);
        assert_eq!(gic.distributor.irq_pending[1], 0);
    }

    #[test]
    fn get_highest_pending_irq_は無効時にnoneを返す() {
        let gic = Gic::new();
        // GIC が無効の場合は None
        assert!(gic.get_highest_pending_irq().is_none());
    }

    #[test]
    fn get_highest_pending_irq_は有効な割り込みを返す() {
        let mut gic = Gic::new();
        gic.distributor.enabled = true;
        gic.cpu_interface.enabled = true;

        // IRQ 32 を有効化してペンディングにする
        gic.distributor.irq_enabled[1] = 1;
        gic.distributor.irq_pending[1] = 1;
        gic.distributor.irq_priority[32] = 0x80;

        let highest = gic.get_highest_pending_irq();
        assert_eq!(highest, Some(32));
    }

    #[test]
    fn 優先度の高い割り込みが先に返される() {
        let mut gic = Gic::new();
        gic.distributor.enabled = true;
        gic.cpu_interface.enabled = true;

        // IRQ 32 と 33 を有効化
        gic.distributor.irq_enabled[1] = 0b11;
        gic.distributor.irq_pending[1] = 0b11;
        // IRQ 33 が高優先度 (低い値)
        gic.distributor.irq_priority[32] = 0x80;
        gic.distributor.irq_priority[33] = 0x40;

        let highest = gic.get_highest_pending_irq();
        assert_eq!(highest, Some(33));
    }

    #[test]
    fn acknowledge_irq_で割り込みがアクティブになる() {
        let mut gic = Gic::new();
        gic.distributor.enabled = true;
        gic.cpu_interface.enabled = true;

        gic.distributor.irq_enabled[1] = 1;
        gic.distributor.irq_pending[1] = 1;
        gic.distributor.irq_priority[32] = 0x80;

        let irq = gic.acknowledge_irq();
        assert_eq!(irq, 32);
        // アクティブ状態になっている
        assert_eq!(gic.distributor.irq_active[1], 1);
        // ペンディングがクリアされている
        assert_eq!(gic.distributor.irq_pending[1], 0);
    }

    #[test]
    fn acknowledge_irq_はペンディングなしでスプリアスを返す() {
        let mut gic = Gic::new();
        gic.distributor.enabled = true;
        gic.cpu_interface.enabled = true;
        // ペンディングな割り込みがない
        let irq = gic.acknowledge_irq();
        assert_eq!(irq, 1023); // スプリアス割り込み
    }

    #[test]
    fn end_of_interrupt_でアクティブ状態がクリアされる() {
        let mut gic = Gic::new();
        gic.distributor.enabled = true;
        gic.cpu_interface.enabled = true;

        gic.distributor.irq_enabled[1] = 1;
        gic.distributor.irq_pending[1] = 1;
        gic.distributor.irq_priority[32] = 0x80;

        gic.acknowledge_irq();
        gic.end_of_interrupt(32);

        // アクティブ状態がクリアされている
        assert_eq!(gic.distributor.irq_active[1], 0);
        assert!(gic.cpu_interface.running_irq.is_none());
    }

    #[test]
    fn mmio_read_でgicd_ctlrを読める() {
        let mut gic = Gic::new();
        gic.distributor.enabled = true;
        assert_eq!(gic.read(gicd_regs::CTLR, 4).unwrap(), 1);
    }

    #[test]
    fn mmio_write_でgicd_ctlrを書ける() {
        let mut gic = Gic::new();
        gic.write(gicd_regs::CTLR, 1, 4).unwrap();
        assert!(gic.distributor.enabled);
    }

    #[test]
    fn mmio_read_でgicd_typerを読める() {
        let mut gic = Gic::new();
        let typer = gic.read(gicd_regs::TYPER, 4).unwrap();
        // ITLinesNumber = (256 / 32) - 1 = 7
        assert_eq!(typer & 0x1F, 7);
    }

    #[test]
    fn mmio_write_でisenablerを書ける() {
        let mut gic = Gic::new();
        // IRQ 32-63 を有効化
        gic.write(gicd_regs::ISENABLER + 4, 0xFFFF_FFFF, 4).unwrap();
        assert_eq!(gic.distributor.irq_enabled[1], 0xFFFF_FFFF);
    }

    #[test]
    fn mmio_write_でicenablerを書ける() {
        let mut gic = Gic::new();
        gic.distributor.irq_enabled[1] = 0xFFFF_FFFF;
        // IRQ 32-63 を無効化
        gic.write(gicd_regs::ICENABLER + 4, 0xFFFF_FFFF, 4).unwrap();
        assert_eq!(gic.distributor.irq_enabled[1], 0);
    }

    #[test]
    fn mmio_read_でgicc_ctlrを読める() {
        let mut gic = Gic::new();
        gic.cpu_interface.enabled = true;
        // GICC のオフセットは GIC_DIST_SIZE からの相対
        let value = gic.read(GIC_DIST_SIZE + gicc_regs::CTLR, 4).unwrap();
        assert_eq!(value, 1);
    }

    #[test]
    fn mmio_write_でgicc_pmrを書ける() {
        let mut gic = Gic::new();
        gic.write(GIC_DIST_SIZE + gicc_regs::PMR, 0x80, 4).unwrap();
        assert_eq!(gic.cpu_interface.priority_mask, 0x80);
    }

    #[test]
    fn mmio_iarとeoirのフローが正しく動作する() {
        let mut gic = Gic::new();
        gic.distributor.enabled = true;
        gic.cpu_interface.enabled = true;
        gic.distributor.irq_enabled[1] = 1;
        gic.distributor.irq_pending[1] = 1;
        gic.distributor.irq_priority[32] = 0x80;

        // IAR を読んで割り込みを acknowledge
        let irq = gic.read(GIC_DIST_SIZE + gicc_regs::IAR, 4).unwrap();
        assert_eq!(irq, 32);

        // EOIR に書いて割り込み完了
        gic.write(GIC_DIST_SIZE + gicc_regs::EOIR, 32, 4).unwrap();
        assert_eq!(gic.distributor.irq_active[1], 0);
    }

    #[test]
    fn base_とsize_が正しい値を返す() {
        let gic = Gic::new();
        assert_eq!(gic.base(), GIC_DIST_BASE);
        assert_eq!(gic.size(), GIC_DIST_SIZE + GIC_CPU_SIZE);
    }

    #[test]
    fn with_base_でカスタムベースアドレスを設定できる() {
        let gic = Gic::with_base(0x1000_0000);
        assert_eq!(gic.base(), 0x1000_0000);
    }
}
