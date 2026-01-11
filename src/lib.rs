//! macOS Hypervisor.framework を使ったハイパーバイザーの共通ライブラリ

pub mod boot;
pub mod devices;
pub mod mmio;

use applevisor::{InterruptType, Mappable, Mapping, MemPerms, Reg, Vcpu, VirtualMachine};
use devices::gic::{create_shared_gic, SharedGicWrapper, GIC_DIST_BASE};
use devices::interrupt::InterruptController;
use devices::timer::TimerReg;
use mmio::MmioManager;
use std::mem::ManuallyDrop;

/// レジスタインデックスから Reg enum への変換テーブル
const REGISTER_TABLE: [Reg; 31] = [
    Reg::X0,
    Reg::X1,
    Reg::X2,
    Reg::X3,
    Reg::X4,
    Reg::X5,
    Reg::X6,
    Reg::X7,
    Reg::X8,
    Reg::X9,
    Reg::X10,
    Reg::X11,
    Reg::X12,
    Reg::X13,
    Reg::X14,
    Reg::X15,
    Reg::X16,
    Reg::X17,
    Reg::X18,
    Reg::X19,
    Reg::X20,
    Reg::X21,
    Reg::X22,
    Reg::X23,
    Reg::X24,
    Reg::X25,
    Reg::X26,
    Reg::X27,
    Reg::X28,
    Reg::X29,
    Reg::X30,
];

/// デバッグ統計情報
#[derive(Debug, Default)]
struct DebugStats {
    exit_count: u64,
    wfi_count: u64,
    mmio_count: u64,
    vtimer_activated_count: u64,
    other_exception_count: u64,
    timer_pending_count: u64,
    timer_sync_count: u64,
    sw_timer_fire_count: u64,
}

impl DebugStats {
    fn log_timer_pending(&mut self) {
        self.timer_pending_count += 1;
        if self.timer_pending_count <= 10 {
            eprintln!("[TIMER] IRQ pending #{}", self.timer_pending_count);
        }
    }

    fn log_timer_sync(&mut self, guest_ctl: u64, guest_cval: u64, virt_counter: u64) {
        self.timer_sync_count += 1;
        if self.timer_sync_count <= 20 || self.timer_sync_count % 5000 == 0 {
            let enabled = (guest_ctl & 0x1) != 0;
            let imask = (guest_ctl & 0x2) != 0;
            eprintln!(
                "[TIMER_SYNC #{}] guest_ctl=0x{:x} (enabled={}, imask={}), guest_cval=0x{:x}, sw_counter=0x{:x}",
                self.timer_sync_count, guest_ctl, enabled, imask, guest_cval, virt_counter
            );
        }
    }

    fn log_sw_timer_fire(&mut self, hw_counter: u64, cval: u64) {
        self.sw_timer_fire_count += 1;
        if self.sw_timer_fire_count <= 20 || self.sw_timer_fire_count % 1000 == 0 {
            eprintln!(
                "[SW_TIMER_FIRE #{}] counter=0x{:x} >= cval=0x{:x} -> injecting IRQ via GIC",
                self.sw_timer_fire_count, hw_counter, cval
            );
        }
    }

    fn log_vtimer_activated(&mut self) {
        self.vtimer_activated_count += 1;
        if self.vtimer_activated_count <= 10 {
            eprintln!(
                "[VTIMER_ACTIVATED #{}] Timer fired!",
                self.vtimer_activated_count
            );
        }
    }

    fn log_exit_summary(&self, post_run_ctl: u64, post_run_cval: u64, hw_counter: u64, gic_pending: bool) {
        let timer_enabled = (post_run_ctl & 0x1) != 0;
        let timer_imask = (post_run_ctl & 0x2) != 0;
        let istatus = timer_enabled && hw_counter >= post_run_cval;
        eprintln!(
            "[TIMER STATE @{}] CTL=0x{:x} (enable={}, imask={}, istatus={}), CVAL=0x{:x}, counter=0x{:x}",
            self.exit_count, post_run_ctl, timer_enabled, timer_imask, istatus, post_run_cval, hw_counter
        );
        eprintln!(
            "[STATS @{}] WFI={}, MMIO={}, VTIMER_ACTIVATED={}, OTHER_EXC={}, GIC_pending={}",
            self.exit_count, self.wfi_count, self.mmio_count, self.vtimer_activated_count,
            self.other_exception_count, gic_pending
        );
    }
}

/// ハードウェアカウンタを読み取る
fn read_hardware_counter() -> u64 {
    let counter: u64;
    unsafe {
        std::arch::asm!("mrs {}, cntvct_el0", out(reg) counter);
    }
    counter
}

/// ハイパーバイザーの実行結果
pub struct HypervisorResult {
    /// VM Exit が発生したときの PC (Program Counter)
    pub pc: u64,
    /// VM Exit が発生したときの汎用レジスタ X0-X30
    pub registers: [u64; 31],
    /// VM Exit の理由
    pub exit_reason: applevisor::ExitReason,
    /// 例外情報 (EXCEPTION の場合のみ)
    pub exception_syndrome: Option<u64>,
}

/// ゲストプログラムを実行するハイパーバイザー
pub struct Hypervisor {
    _vm: ManuallyDrop<VirtualMachine>,
    vcpu: ManuallyDrop<Vcpu>,
    mem: Mapping,
    guest_addr: u64,
    mmio_manager: MmioManager,
    interrupt_controller: InterruptController,
    debug_stats: DebugStats,
}

impl Hypervisor {
    /// 新しいハイパーバイザーを作成する
    ///
    /// # Arguments
    /// * `guest_addr` - ゲストコードを配置するアドレス
    /// * `mem_size` - ゲストメモリのサイズ (bytes)
    pub fn new(guest_addr: u64, mem_size: usize) -> Result<Self, Box<dyn std::error::Error>> {
        let _vm = ManuallyDrop::new(VirtualMachine::new()?);
        let vcpu = ManuallyDrop::new(Vcpu::new()?);

        // 仮想タイマー割り込みをマスクして FIQ 配信を抑制
        // これにより VTIMER_ACTIVATED イベントで GIC 経由の IRQ として配信できる
        vcpu.set_vtimer_mask(true)?;

        // vtimer_mask が正しく設定されたか確認
        let vtimer_masked = vcpu.get_vtimer_mask()?;
        eprintln!("[DEBUG] vtimer_mask set: {}", vtimer_masked);

        // vtimer_offset を現在のハードウェアカウンタに設定
        // これにより、ゲストの CNTVCT_EL0 は 0 から始まる
        // FIQ 問題の根本原因: vtimer_offset=0 だと、ゲストは生のハードウェアカウンタ
        // (~40兆 ticks、システム起動から19日) を見てしまう
        // Linux がタイマーを設定すると、カウンタ >> CVAL となり即座に FIQ が発生
        let hw_counter: u64;
        unsafe {
            std::arch::asm!("mrs {}, cntvct_el0", out(reg) hw_counter);
        }
        vcpu.set_vtimer_offset(hw_counter)?;
        eprintln!(
            "[DEBUG] vtimer_offset set to hw_counter: 0x{:x} (guest counter starts from 0)",
            hw_counter
        );

        // 仮想タイマーを初期化
        // CVAL=i64::MAX (符号付きで最大の正の値), CTL=0x2 (無効+マスク) を設定
        // u64::MAX は符号付きで -1 となり即座に発火するため避ける
        // i64::MAX は符号付きでも符号なしでも大きな正の値
        // IMASK=1 を設定して FIQ を防止
        vcpu.set_sys_reg(applevisor::SysReg::CNTV_CVAL_EL0, i64::MAX as u64)?;
        vcpu.set_sys_reg(applevisor::SysReg::CNTV_CTL_EL0, 0x2)?; // ENABLE=0, IMASK=1
        eprintln!("[DEBUG] vtimer initialized: CVAL=i64::MAX (0x{:x}), CTL=0x2 (IMASK=1)", i64::MAX as u64);


        // vtimer_offset を確認
        let verified_offset = vcpu.get_vtimer_offset().unwrap_or(0);
        eprintln!("[DEBUG] vtimer_offset verified: 0x{:x}", verified_offset);

        let mut mem = Mapping::new(mem_size)?;
        mem.map(guest_addr, MemPerms::RWX)?;

        // 共有 GIC を作成
        let shared_gic = create_shared_gic(GIC_DIST_BASE);

        // GIC MMIO ハンドラを登録
        let mut mmio_manager = MmioManager::new();
        let gic_wrapper = SharedGicWrapper::new(shared_gic.clone(), GIC_DIST_BASE);
        mmio_manager.register(Box::new(gic_wrapper));

        // InterruptController は同じ GIC を使用
        let interrupt_controller = InterruptController::with_gic(shared_gic);

        Ok(Self {
            _vm,
            vcpu,
            mem,
            guest_addr,
            mmio_manager,
            interrupt_controller,
            debug_stats: DebugStats::default(),
        })
    }

    /// ゲストメモリに ARM64 命令 (32-bit) を書き込む
    ///
    /// # Arguments
    /// * `offset` - guest_addr からのオフセット (bytes)
    /// * `instruction` - ARM64 機械語命令 (32-bit)
    pub fn write_instruction(
        &mut self,
        offset: u64,
        instruction: u32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.mem
            .write_dword(self.guest_addr + offset, instruction)?;
        Ok(())
    }

    /// ゲストメモリに複数の ARM64 命令を書き込む
    ///
    /// # Arguments
    /// * `instructions` - ARM64 機械語命令の配列
    pub fn write_instructions(
        &mut self,
        instructions: &[u32],
    ) -> Result<(), Box<dyn std::error::Error>> {
        for (i, &instruction) in instructions.iter().enumerate() {
            self.write_instruction((i * 4) as u64, instruction)?;
        }
        Ok(())
    }

    /// ゲストメモリにデータを書き込む (64-bit)
    ///
    /// # Arguments
    /// * `offset` - guest_addr からのオフセット (bytes)
    /// * `data` - 書き込むデータ (64-bit)
    pub fn write_data(&mut self, offset: u64, data: u64) -> Result<(), Box<dyn std::error::Error>> {
        self.mem.write_qword(self.guest_addr + offset, data)?;
        Ok(())
    }

    /// ゲストメモリからデータを読み取る (64-bit)
    ///
    /// # Arguments
    /// * `offset` - guest_addr からのオフセット (bytes)
    pub fn read_data(&self, offset: u64) -> Result<u64, Box<dyn std::error::Error>> {
        Ok(self.mem.read_qword(self.guest_addr + offset)?)
    }

    /// ゲストメモリにバイトデータを書き込む
    ///
    /// # Arguments
    /// * `addr` - 書き込むアドレス（絶対アドレス）
    /// * `byte` - 書き込むバイト
    ///
    /// # Note
    /// `Mapping` は 4-byte 単位の read/write のみサポートするため、
    /// 4-byte 単位で読み書きして部分更新を行う
    pub fn write_byte(&mut self, addr: u64, byte: u8) -> Result<(), Box<dyn std::error::Error>> {
        let aligned_addr = addr & !0x3;
        let offset = (addr & 0x3) as usize;
        let mut word = self.mem.read_dword(aligned_addr)?;
        let mut bytes = word.to_le_bytes();
        bytes[offset] = byte;
        word = u32::from_le_bytes(bytes);
        self.mem.write_dword(aligned_addr, word)?;
        Ok(())
    }

    /// ゲストメモリからバイトデータを読み取る
    ///
    /// # Arguments
    /// * `addr` - 読み取るアドレス（絶対アドレス）
    ///
    /// # Note
    /// `Mapping` は 4-byte 単位の read/write のみサポートするため、
    /// 4-byte 単位で読み書きして部分更新を行う
    pub fn read_byte(&self, addr: u64) -> Result<u8, Box<dyn std::error::Error>> {
        let aligned_addr = addr & !0x3;
        let offset = (addr & 0x3) as usize;
        let word = self.mem.read_dword(aligned_addr)?;
        let bytes = word.to_le_bytes();
        Ok(bytes[offset])
    }

    /// vCPU のレジスタを設定する
    ///
    /// # Arguments
    /// * `reg` - 設定するレジスタ
    /// * `value` - 設定する値
    pub fn set_reg(&self, reg: Reg, value: u64) -> Result<(), Box<dyn std::error::Error>> {
        self.vcpu.set_reg(reg, value)?;
        Ok(())
    }

    /// vCPU のレジスタを取得する
    ///
    /// # Arguments
    /// * `reg` - 取得するレジスタ
    pub fn get_reg(&self, reg: Reg) -> Result<u64, Box<dyn std::error::Error>> {
        Ok(self.vcpu.get_reg(reg)?)
    }

    /// MMIO デバイスハンドラを登録する
    ///
    /// # Arguments
    /// * `handler` - 登録する MMIO ハンドラ
    pub fn register_mmio_handler(&mut self, handler: Box<dyn crate::mmio::MmioHandler>) {
        self.mmio_manager.register(handler);
    }

    /// ゲストプログラムを実行する
    ///
    /// # Arguments
    /// * `initial_cpsr` - 初期 CPSR 値 (デフォルト: 0x3c4 = EL1h)
    /// * `trap_debug` - デバッグ例外をトラップするか (デフォルト: true)
    /// * `initial_pc` - 初期 PC 値 (デフォルト: self.guest_addr)
    ///
    /// # Returns
    /// 実行結果 (HypervisorResult)
    pub fn run(
        &mut self,
        initial_cpsr: Option<u64>,
        trap_debug: Option<bool>,
        initial_pc: Option<u64>,
    ) -> Result<HypervisorResult, Box<dyn std::error::Error>> {
        // PC を設定
        let pc = initial_pc.unwrap_or(self.guest_addr);
        self.vcpu.set_reg(Reg::PC, pc)?;

        // CPSR を設定 (デフォルト: EL1h mode)
        let cpsr = initial_cpsr.unwrap_or(0x3c4);
        self.vcpu.set_reg(Reg::CPSR, cpsr)?;

        // デバッグ例外のトラップを設定
        if trap_debug.unwrap_or(true) {
            self.vcpu.set_trap_debug_exceptions(true)?;
        }

        // ゲストプログラムを実行
        loop {
            // タイマー IRQ をポーリング
            let had_pending_before = self.interrupt_controller.has_pending_irq();
            self.interrupt_controller.poll_timer_irqs();
            let has_pending_after = self.interrupt_controller.has_pending_irq();

            if !had_pending_before && has_pending_after {
                self.debug_stats.log_timer_pending();
            }

            // FIQ をクリアし、IRQ 状態を更新
            self.vcpu.set_pending_interrupt(InterruptType::FIQ, false)?;
            self.vcpu.set_pending_interrupt(
                InterruptType::IRQ,
                self.interrupt_controller.has_pending_irq(),
            )?;

            // ゲストのタイマー設定を読み取りソフトウェアタイマーに同期
            let guest_ctl = self
                .vcpu
                .get_sys_reg(applevisor::SysReg::CNTV_CTL_EL0)
                .unwrap_or(0);
            let guest_cval = self
                .vcpu
                .get_sys_reg(applevisor::SysReg::CNTV_CVAL_EL0)
                .unwrap_or(i64::MAX as u64);

            let virt_counter = self.interrupt_controller.timer.get_virt_counter();
            self.interrupt_controller
                .timer
                .virt_timer
                .write_ctl(guest_ctl);
            self.interrupt_controller
                .timer
                .virt_timer
                .write_cval(guest_cval);

            self.debug_stats
                .log_timer_sync(guest_ctl, guest_cval, virt_counter);

            // FIQ 防止: ハードウェアタイマーを無効化して vcpu.run() を実行
            self.vcpu
                .set_sys_reg(applevisor::SysReg::CNTV_CTL_EL0, 0x2)?;
            self.vcpu
                .set_sys_reg(applevisor::SysReg::CNTV_CVAL_EL0, i64::MAX as u64)?;
            self.vcpu.set_pending_interrupt(InterruptType::FIQ, false)?;

            self.vcpu.run()?;

            // ゲストが設定したタイマー値を再読み取り
            let post_run_ctl = self
                .vcpu
                .get_sys_reg(applevisor::SysReg::CNTV_CTL_EL0)
                .unwrap_or(0);
            let post_run_cval = self
                .vcpu
                .get_sys_reg(applevisor::SysReg::CNTV_CVAL_EL0)
                .unwrap_or(i64::MAX as u64);

            let timer_enabled = (post_run_ctl & 0x1) != 0;
            let timer_imask = (post_run_ctl & 0x2) != 0;
            let hw_counter = read_hardware_counter();

            // タイマー発火条件をチェックし GIC 経由で IRQ を注入
            if timer_enabled && !timer_imask && hw_counter >= post_run_cval {
                self.debug_stats.log_sw_timer_fire(hw_counter, post_run_cval);
                let mut gic = self.interrupt_controller.gic.lock().unwrap();
                gic.set_irq_pending(devices::timer::VIRT_TIMER_IRQ);
            }

            let exit_info = self.vcpu.get_exit_info();

            // IRQ 状態を更新
            self.vcpu.set_pending_interrupt(
                InterruptType::IRQ,
                self.interrupt_controller.has_pending_irq(),
            )?;

            // exit reason を記録
            self.debug_stats.exit_count += 1;
            match exit_info.reason {
                applevisor::ExitReason::EXCEPTION => {
                    let ec = (exit_info.exception.syndrome >> 26) & 0x3f;
                    match ec {
                        0x01 => {
                            self.debug_stats.wfi_count += 1;
                            let wfi_count = self.debug_stats.wfi_count;
                            let exit_count = self.debug_stats.exit_count;
                            if wfi_count <= 5 || wfi_count % 10000 == 0 {
                                eprintln!("[WFI #{}] at exit #{}", wfi_count, exit_count);
                            }
                        }
                        0x24 => self.debug_stats.mmio_count += 1,
                        _ => self.debug_stats.other_exception_count += 1,
                    }
                }
                applevisor::ExitReason::VTIMER_ACTIVATED => {
                    eprintln!("[EXIT #{}] VTIMER_ACTIVATED!", self.debug_stats.exit_count);
                }
                _ => {}
            }

            // 定期的にサマリーを出力
            if self.debug_stats.exit_count % 5000 == 0 {
                let gic_pending = self.interrupt_controller.has_pending_irq();
                self.debug_stats
                    .log_exit_summary(post_run_ctl, post_run_cval, hw_counter, gic_pending);
            }

            // 汎用レジスタを取得
            let registers = [
                self.vcpu.get_reg(Reg::X0)?,
                self.vcpu.get_reg(Reg::X1)?,
                self.vcpu.get_reg(Reg::X2)?,
                self.vcpu.get_reg(Reg::X3)?,
                self.vcpu.get_reg(Reg::X4)?,
                self.vcpu.get_reg(Reg::X5)?,
                self.vcpu.get_reg(Reg::X6)?,
                self.vcpu.get_reg(Reg::X7)?,
                self.vcpu.get_reg(Reg::X8)?,
                self.vcpu.get_reg(Reg::X9)?,
                self.vcpu.get_reg(Reg::X10)?,
                self.vcpu.get_reg(Reg::X11)?,
                self.vcpu.get_reg(Reg::X12)?,
                self.vcpu.get_reg(Reg::X13)?,
                self.vcpu.get_reg(Reg::X14)?,
                self.vcpu.get_reg(Reg::X15)?,
                self.vcpu.get_reg(Reg::X16)?,
                self.vcpu.get_reg(Reg::X17)?,
                self.vcpu.get_reg(Reg::X18)?,
                self.vcpu.get_reg(Reg::X19)?,
                self.vcpu.get_reg(Reg::X20)?,
                self.vcpu.get_reg(Reg::X21)?,
                self.vcpu.get_reg(Reg::X22)?,
                self.vcpu.get_reg(Reg::X23)?,
                self.vcpu.get_reg(Reg::X24)?,
                self.vcpu.get_reg(Reg::X25)?,
                self.vcpu.get_reg(Reg::X26)?,
                self.vcpu.get_reg(Reg::X27)?,
                self.vcpu.get_reg(Reg::X28)?,
                self.vcpu.get_reg(Reg::X29)?,
                self.vcpu.get_reg(Reg::X30)?,
            ];

            let pc = self.vcpu.get_reg(Reg::PC)?;

            // 例外処理
            if let applevisor::ExitReason::EXCEPTION = exit_info.reason {
                let syndrome = exit_info.exception.syndrome;
                let ec = (syndrome >> 26) & 0x3f;

                match ec {
                    0x01 => {
                        // WFI/WFE (Wait For Interrupt/Event)
                        if !self.handle_wfi_wfe(syndrome)? {
                            return Ok(HypervisorResult {
                                pc,
                                registers,
                                exit_reason: exit_info.reason,
                                exception_syndrome: Some(syndrome),
                            });
                        }
                    }
                    0x16 => {
                        // HVC (Hypervisor Call) - PSCI
                        if !self.handle_hvc(syndrome)? {
                            return Ok(HypervisorResult {
                                pc,
                                registers,
                                exit_reason: exit_info.reason,
                                exception_syndrome: Some(syndrome),
                            });
                        }
                    }
                    0x18 => {
                        // MSR/MRS (System Register Access)
                        if !self.handle_sysreg_access(syndrome)? {
                            return Ok(HypervisorResult {
                                pc,
                                registers,
                                exit_reason: exit_info.reason,
                                exception_syndrome: Some(syndrome),
                            });
                        }
                    }
                    0x24 => {
                        // Data Abort from lower EL
                        // physical_address は IPA (Intermediate Physical Address)
                        let fault_ipa = exit_info.exception.physical_address;
                        if !self.handle_data_abort(syndrome, fault_ipa)? {
                            return Ok(HypervisorResult {
                                pc,
                                registers,
                                exit_reason: exit_info.reason,
                                exception_syndrome: Some(syndrome),
                            });
                        }
                    }
                    0x3c => {
                        // BRK instruction (AArch64)
                        return Ok(HypervisorResult {
                            pc,
                            registers,
                            exit_reason: exit_info.reason,
                            exception_syndrome: Some(syndrome),
                        });
                    }
                    _ => {
                        // その他の例外は VM Exit
                        // デバッグ用: 予期しない例外をログ出力
                        // eprintln!(
                        //     "Unknown exception: EC=0x{:x}, syndrome=0x{:x}",
                        //     ec, syndrome
                        // );
                        return Ok(HypervisorResult {
                            pc,
                            registers,
                            exit_reason: exit_info.reason,
                            exception_syndrome: Some(syndrome),
                        });
                    }
                }
            } else if let applevisor::ExitReason::VTIMER_ACTIVATED = exit_info.reason {
                // 仮想タイマーがアクティブになった - GIC 経由で IRQ を注入
                self.debug_stats.log_vtimer_activated();
                self.interrupt_controller.poll_timer_irqs();

                {
                    let mut gic = self.interrupt_controller.gic.lock().unwrap();
                    gic.set_irq_pending(devices::timer::VIRT_TIMER_IRQ);
                }

                if self.interrupt_controller.has_pending_irq() {
                    self.vcpu.set_pending_interrupt(InterruptType::IRQ, true)?;
                }
            } else {
                // 予期しない VM Exit
                return Ok(HypervisorResult {
                    pc,
                    registers,
                    exit_reason: exit_info.reason,
                    exception_syndrome: None,
                });
            }
        }
    }

    /// Data Abort 例外を処理する
    ///
    /// ISS (Instruction Specific Syndrome) フィールドの構造:
    /// - [24]: ISV - Instruction Syndrome Valid
    /// - [23:22]: SAS - Syndrome Access Size (0=byte, 1=halfword, 2=word, 3=doubleword)
    /// - [21]: SSE - Syndrome Sign Extend
    /// - [20:16]: SRT - Syndrome Register Transfer (転送元/先レジスタ番号)
    /// - [15]: SF - Sixty-Four bit register
    /// - [9]: FnV - FAR not Valid
    /// - [6]: WnR - Write not Read
    ///
    /// # Arguments
    /// * `syndrome` - ESR_EL2 の値
    /// * `fault_ipa` - フォールトした IPA (Intermediate Physical Address)
    ///
    /// # Returns
    /// 続行する場合は true、VM Exit する場合は false
    fn handle_data_abort(
        &mut self,
        syndrome: u64,
        fault_ipa: u64,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let iss = syndrome & 0x1FF_FFFF; // ISS は下位 25 ビット

        // ISV (Instruction Syndrome Valid) ビット [24]
        let isv = (iss >> 24) & 0x1;

        // WnR ビット [6]: 0 = read, 1 = write
        let is_write = (iss & (1 << 6)) != 0;

        // SAS (Syndrome Access Size) ビット [23:22]
        // 0b00 = byte, 0b01 = halfword, 0b10 = word, 0b11 = doubleword
        let sas = (iss >> 22) & 0x3;
        let size = 1 << sas; // 1, 2, 4, 8 bytes

        // SRT (Syndrome Register Transfer) ビット [20:16]
        // 転送元/先レジスタ番号 (0-30 = X0-X30, 31 = XZR)
        let srt = if isv != 0 {
            ((iss >> 16) & 0x1F) as u8
        } else {
            // ISV が無効の場合、X0 をデフォルトとして使用
            0
        };

        // fault_ipa は Hypervisor.framework が提供する IPA
        let fault_addr = fault_ipa;

        // MMIO ハンドリング
        if is_write {
            // 書き込み: SRT で指定されたレジスタから値を取得
            let value = self.get_register_by_index(srt)?;
            self.mmio_manager.handle_write(fault_addr, value, size)?;
        } else {
            // 読み取り: MMIO デバイスから値を読み取って SRT レジスタに設定
            let value = self.mmio_manager.handle_read(fault_addr, size)?;
            self.set_register_by_index(srt, value)?;
        }

        // PC を進める
        let pc = self.vcpu.get_reg(Reg::PC)?;
        self.vcpu.set_reg(Reg::PC, pc + 4)?;

        Ok(true) // 続行
    }

    /// システムレジスタアクセス (MSR/MRS) 例外を処理する
    ///
    /// # Arguments
    /// * `syndrome` - ESR_EL2 の値
    ///
    /// # Returns
    /// 続行する場合は true、VM Exit する場合は false
    fn handle_sysreg_access(&mut self, syndrome: u64) -> Result<bool, Box<dyn std::error::Error>> {
        // ISS フィールドをデコード
        // ISS encoding for MSR/MRS (EC=0x18):
        // [24:22] = Op0 (2 bits used)
        // [21:19] = Op2 (3 bits)
        // [18:16] = Op1 (3 bits)
        // [15:12] = CRn (4 bits)
        // [11:8] = Rt (5 bits in ISS[9:5], but use lower 4 bits for X0-X30)
        // [7:4] = CRm (4 bits in ISS[4:1])
        // [0] = Direction (0 = read/MRS, 1 = write/MSR)

        // 正しい ISS エンコーディング (ARM ARM D17.2.37)
        let iss = syndrome & 0x1FFFFFF; // ISS は下位25ビット
        let direction = iss & 0x1; // 0 = MRS (read), 1 = MSR (write)
        let crm = ((iss >> 1) & 0xf) as u8;
        let rt = ((iss >> 5) & 0x1f) as u8; // Rt (0-30, 31 = XZR)
        let crn = ((iss >> 10) & 0xf) as u8;
        let op1 = ((iss >> 14) & 0x7) as u8;
        let op2 = ((iss >> 17) & 0x7) as u8;
        let op0 = ((iss >> 20) & 0x3) as u8;

        // Timer レジスタかどうか判定
        if let Some(timer_reg) = TimerReg::from_encoding(op0, op1, crn, crm, op2) {
            if direction == 0 {
                // MRS (read): Timer レジスタの値を Rt に設定
                let value = self.interrupt_controller.timer.read_sysreg(timer_reg)?;
                if rt < 31 {
                    self.set_register_by_index(rt, value)?;
                }
                // rt == 31 は XZR なので何もしない
            } else {
                // MSR (write): Rt の値を Timer レジスタに設定
                let value = if rt < 31 {
                    self.get_register_by_index(rt)?
                } else {
                    0 // XZR
                };
                self.interrupt_controller
                    .timer
                    .write_sysreg(timer_reg, value)?;
            }

            // PC を進める
            let pc = self.vcpu.get_reg(Reg::PC)?;
            self.vcpu.set_reg(Reg::PC, pc + 4)?;

            return Ok(true); // 続行
        }

        // 未対応のシステムレジスタ
        // Linux カーネル起動のためにエミュレート

        // キャッシュ・ID レジスタ (Op0=3, Op1=0-7, CRn=0)
        // Debug レジスタ (Op0=2)
        // これらは読み取り時に 0 を返し、書き込み時は無視する

        if direction == 0 {
            // MRS (read): 0 を返す
            if rt < 31 {
                self.set_register_by_index(rt, 0)?;
            }
        }
        // MSR (write): 無視する

        // PC を進める
        let pc = self.vcpu.get_reg(Reg::PC)?;
        self.vcpu.set_reg(Reg::PC, pc + 4)?;

        Ok(true) // 続行
    }

    /// WFI/WFE (Wait For Interrupt/Event) 例外を処理する
    ///
    /// # Arguments
    /// * `_syndrome` - ESR_EL2 の値（現在は未使用）
    ///
    /// # Returns
    /// 続行する場合は true、VM Exit する場合は false
    fn handle_wfi_wfe(&mut self, _syndrome: u64) -> Result<bool, Box<dyn std::error::Error>> {
        // タイマー IRQ をポーリング
        self.interrupt_controller.poll_timer_irqs();

        // ペンディング IRQ があれば即座に続行
        if self.interrupt_controller.has_pending_irq() {
            // PC を進める（WFI/WFE 命令の次へ）
            let pc = self.vcpu.get_reg(Reg::PC)?;
            self.vcpu.set_reg(Reg::PC, pc + 4)?;
            return Ok(true);
        }

        // WFI をスキップせずに再実行して、VTIMER_ACTIVATED を待つ
        // ハードウェア vtimer の発火を検出するために、短いスリープ後に再実行
        // (ソフトウェアタイマーは使用されていないため time_until_next_event() は None)

        // CPU を過度に使用しないよう短いスリープを入れる
        std::thread::sleep(std::time::Duration::from_micros(100));

        // PC を進めて次の命令へ
        // 注: Linux はタイマー割り込みがなければすぐに WFI を再実行する
        let pc = self.vcpu.get_reg(Reg::PC)?;
        self.vcpu.set_reg(Reg::PC, pc + 4)?;

        Ok(true) // 続行
    }

    /// HVC (Hypervisor Call) 例外を処理する - PSCI 実装
    ///
    /// # Arguments
    /// * `_syndrome` - ESR_EL2 の値（現在は未使用）
    ///
    /// # Returns
    /// 続行する場合は true、VM Exit する場合は false
    fn handle_hvc(&mut self, _syndrome: u64) -> Result<bool, Box<dyn std::error::Error>> {
        // PSCI Function ID は X0 に格納される
        let function_id = self.vcpu.get_reg(Reg::X0)?;

        // PSCI 戻り値（デフォルト: SUCCESS）
        let result = match function_id {
            // PSCI_VERSION (0x84000000)
            // Returns: 32-bit version (major << 16 | minor)
            // PSCI 1.0 を返す
            0x8400_0000 => {
                0x0001_0000_u64 // Version 1.0
            }

            // PSCI_CPU_SUSPEND (0xC4000001) - 64-bit
            // Args: X1=power_state, X2=entry_point, X3=context_id
            // CPU をスリープ状態にする（簡易実装: 短いスリープ）
            0xC400_0001 => {
                std::thread::sleep(std::time::Duration::from_micros(100));
                0 // PSCI_SUCCESS
            }

            // PSCI_CPU_OFF (0x84000002)
            // CPU をオフにする（シングル vCPU なので VM Exit）
            // HVC は preferred return なので PC は既に HVC+4 を指している
            0x8400_0002 => {
                return Ok(false); // VM Exit
            }

            // PSCI_CPU_ON (0xC4000003) - 64-bit
            // Args: X1=target_cpu, X2=entry_point, X3=context_id
            // シングル vCPU なので ALREADY_ON を返す
            0xC400_0003 => {
                0xFFFF_FFFF_FFFF_FFFC_u64 // PSCI_E_ALREADY_ON (-4)
            }

            // PSCI_AFFINITY_INFO (0xC4000004) - 64-bit
            // Args: X1=target_affinity, X2=lowest_affinity_level
            // シングル vCPU なので ON を返す
            0xC400_0004 => {
                0 // ON
            }

            // PSCI_SYSTEM_OFF (0x84000008)
            // システムをシャットダウン（VM Exit）
            // HVC は preferred return なので PC は既に HVC+4 を指している
            0x8400_0008 => {
                return Ok(false); // VM Exit
            }

            // PSCI_SYSTEM_RESET (0x84000009)
            // システムをリセット（VM Exit）
            // HVC は preferred return なので PC は既に HVC+4 を指している
            0x8400_0009 => {
                return Ok(false); // VM Exit
            }

            // PSCI_FEATURES (0x8400000A)
            // Args: X1=psci_func_id
            // 対応している機能を返す
            0x8400_000A => {
                let queried_func = self.vcpu.get_reg(Reg::X1)?;
                match queried_func {
                    0x8400_0000 | // VERSION
                    0xC400_0001 | // CPU_SUSPEND
                    0x8400_0002 | // CPU_OFF
                    0xC400_0003 | // CPU_ON
                    0xC400_0004 | // AFFINITY_INFO
                    0x8400_0008 | // SYSTEM_OFF
                    0x8400_0009   // SYSTEM_RESET
                        => 0, // PSCI_SUCCESS (supported)
                    _ => 0xFFFF_FFFF_FFFF_FFFF_u64, // PSCI_E_NOT_SUPPORTED (-1)
                }
            }

            // 未知の PSCI 関数
            _ => {
                eprintln!("Unknown PSCI function: 0x{:x}", function_id);
                0xFFFF_FFFF_FFFF_FFFF_u64 // PSCI_E_NOT_SUPPORTED (-1)
            }
        };

        // 結果を X0 に設定
        self.vcpu.set_reg(Reg::X0, result)?;

        // HVC は preferred return exception なので、PC は既に HVC+4 を指している
        // PC を進める必要はない

        Ok(true) // 続行
    }

    /// レジスタインデックスから値を取得
    fn get_register_by_index(&self, index: u8) -> Result<u64, Box<dyn std::error::Error>> {
        if index < 31 {
            self.get_reg(REGISTER_TABLE[index as usize])
        } else {
            Ok(0) // XZR
        }
    }

    /// レジスタインデックスに値を設定
    fn set_register_by_index(
        &self,
        index: u8,
        value: u64,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if index < 31 {
            self.set_reg(REGISTER_TABLE[index as usize], value)
        } else {
            Ok(()) // XZR - 何もしない
        }
    }

    /// Timer への参照を取得
    pub fn timer(&self) -> &devices::timer::Timer {
        &self.interrupt_controller.timer
    }

    /// Timer への可変参照を取得
    pub fn timer_mut(&mut self) -> &mut devices::timer::Timer {
        &mut self.interrupt_controller.timer
    }

    /// InterruptController への参照を取得
    pub fn interrupt_controller(&self) -> &InterruptController {
        &self.interrupt_controller
    }

    /// InterruptController への可変参照を取得
    pub fn interrupt_controller_mut(&mut self) -> &mut InterruptController {
        &mut self.interrupt_controller
    }

    /// Linux カーネルをブートする
    ///
    /// # Arguments
    /// * `kernel` - カーネルイメージ
    /// * `cmdline` - カーネルコマンドライン
    /// * `dtb_addr` - Device Tree を配置するアドレス（省略時: 0x44000000）
    ///
    /// # Returns
    /// 実行結果 (HypervisorResult)
    ///
    /// # Example
    /// ```no_run
    /// use hypervisor::{Hypervisor, boot::kernel::KernelImage};
    ///
    /// let mut hv = Hypervisor::new(0x40000000, 128 * 1024 * 1024).unwrap();
    /// let kernel = KernelImage::from_bytes(vec![0x00, 0x00, 0x00, 0x14], None);
    /// hv.boot_linux(&kernel, "console=ttyAMA0", None).unwrap();
    /// ```
    pub fn boot_linux(
        &mut self,
        kernel: &crate::boot::kernel::KernelImage,
        cmdline: &str,
        dtb_addr: Option<u64>,
    ) -> Result<HypervisorResult, Box<dyn std::error::Error>> {
        // 1. Device Tree 生成
        let dtb = crate::boot::device_tree::generate_device_tree(
            &crate::boot::device_tree::DeviceTreeConfig {
                memory_base: self.guest_addr,
                memory_size: self.mem.get_size() as u64,
                uart_base: 0x0900_0000,
                virtio_base: 0x0a00_0000,
                gic_dist_base: 0x0800_0000,
                gic_cpu_base: 0x0801_0000,
                cmdline: cmdline.to_string(),
                initrd_start: None,
                initrd_end: None,
            },
        )?;

        // 2. Device Tree をメモリに配置
        let dtb_addr = dtb_addr.unwrap_or(0x4400_0000);
        for (i, &byte) in dtb.iter().enumerate() {
            self.write_byte(dtb_addr + i as u64, byte)?;
        }

        // 3. カーネルをメモリに配置
        let kernel_addr = kernel.entry_point();
        for (i, &byte) in kernel.data().iter().enumerate() {
            self.write_byte(kernel_addr + i as u64, byte)?;
        }

        // 4. ARM64 Linux ブート条件を設定
        // 参考: https://docs.kernel.org/arch/arm64/booting.html
        self.set_reg(Reg::X0, dtb_addr)?; // Device Tree アドレス
        self.set_reg(Reg::X1, 0)?; // Reserved
        self.set_reg(Reg::X2, 0)?; // Reserved
        self.set_reg(Reg::X3, 0)?; // Reserved

        // CPSR: EL1h, MMU off, 割り込みマスク（DAIF）
        // 0x3c5 = 0b001111000101
        //   M[4:0] = 0b00101 = EL1h
        //   DAIF = 0b1111 = すべての割り込みをマスク

        // デバッグ例外のトラップを有効化
        self.vcpu.set_trap_debug_exceptions(true)?;

        // 5. VM Exit ループ (PC をカーネルエントリーポイントに設定)
        self.run(Some(0x3c5), Some(true), Some(kernel_addr))
    }
}

impl Drop for Hypervisor {
    fn drop(&mut self) {
        use std::panic::{catch_unwind, AssertUnwindSafe};

        // Vcpu を先に破棄（panic をキャッチして無視）
        let _ = catch_unwind(AssertUnwindSafe(|| unsafe {
            ManuallyDrop::drop(&mut self.vcpu);
        }));

        // VirtualMachine を破棄（panic をキャッチして無視）
        let _ = catch_unwind(AssertUnwindSafe(|| unsafe {
            ManuallyDrop::drop(&mut self._vm);
        }));
    }
}
