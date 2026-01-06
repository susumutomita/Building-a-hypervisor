//! macOS Hypervisor.framework を使ったハイパーバイザーの共通ライブラリ

pub mod boot;
pub mod devices;
pub mod mmio;

use applevisor::{Mappable, Mapping, MemPerms, Reg, SysReg, Vcpu, VirtualMachine};
use mmio::MmioManager;

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
    _vm: VirtualMachine,
    vcpu: Vcpu,
    mem: Mapping,
    guest_addr: u64,
    mmio_manager: MmioManager,
}

impl Hypervisor {
    /// 新しいハイパーバイザーを作成する
    ///
    /// # Arguments
    /// * `guest_addr` - ゲストコードを配置するアドレス
    /// * `mem_size` - ゲストメモリのサイズ (bytes)
    pub fn new(guest_addr: u64, mem_size: usize) -> Result<Self, Box<dyn std::error::Error>> {
        let _vm = VirtualMachine::new()?;
        let vcpu = Vcpu::new()?;

        let mut mem = Mapping::new(mem_size)?;
        mem.map(guest_addr, MemPerms::RWX)?;

        Ok(Self {
            _vm,
            vcpu,
            mem,
            guest_addr,
            mmio_manager: MmioManager::new(),
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
            self.vcpu.run()?;
            let exit_info = self.vcpu.get_exit_info();

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
                    0x24 => {
                        // Data Abort from lower EL
                        if !self.handle_data_abort(syndrome)? {
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
    /// # Arguments
    /// * `syndrome` - ESR_EL2 の値
    ///
    /// # Returns
    /// 続行する場合は true、VM Exit する場合は false
    fn handle_data_abort(&mut self, syndrome: u64) -> Result<bool, Box<dyn std::error::Error>> {
        // WnR ビット: 0 = read, 1 = write
        let is_write = (syndrome & (1 << 6)) != 0;

        // SAS (Syndrome Access Size) ビット [23:22]
        // 0b00 = byte, 0b01 = halfword, 0b10 = word, 0b11 = doubleword
        let sas = (syndrome >> 22) & 0x3;
        let size = 1 << sas; // 1, 2, 4, 8 bytes

        // FAR_EL1 から fault address を取得
        // Note: macOS Hypervisor.framework では FAR_EL1 が 0 になることが多い。
        // これは、例外が EL2 にトラップされた際に FAR_EL1 が設定されないためと思われる。
        // その場合は命令をデコードして base register から取得する必要がある。
        let far_el1 = self.vcpu.get_sys_reg(SysReg::FAR_EL1)?;

        // Workaround: FAR_EL1 が 0 の場合、X1 から取得
        // TODO: 命令を完全にデコードして実際の base register を特定する
        let fault_addr = if far_el1 == 0 {
            // とりあえず X1 をフォールバックとして使用
            // これは str w0, [x1] のような単純な命令では機能するが、
            // より複雑なアドレッシングモードでは不正確になる可能性がある
            self.vcpu.get_reg(Reg::X1)?
        } else {
            far_el1
        };

        // デバッグ用: Data Abort の詳細をログ出力
        // eprintln!(
        //     "Data Abort: addr=0x{:x}, is_write={}, size={}, syndrome=0x{:x}",
        //     fault_addr, is_write, size, syndrome
        // );

        // MMIO ハンドリング
        if is_write {
            // 書き込み: X0 から値を取得して MMIO デバイスに書き込む
            // TODO: ISS の SRT フィールドから実際のレジスタを取得
            let value = self.vcpu.get_reg(Reg::X0)?;
            self.mmio_manager.handle_write(fault_addr, value, size)?;
        } else {
            // 読み取り: MMIO デバイスから値を読み取って X0 に設定
            // TODO: ISS の SRT フィールドから実際のレジスタを取得
            let value = self.mmio_manager.handle_read(fault_addr, size)?;
            self.vcpu.set_reg(Reg::X0, value)?;
        }

        // PC を進める
        let pc = self.vcpu.get_reg(Reg::PC)?;
        self.vcpu.set_reg(Reg::PC, pc + 4)?;

        Ok(true) // 続行
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
                cmdline: cmdline.to_string(),
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
