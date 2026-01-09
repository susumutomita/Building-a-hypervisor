//! PL011 UART device emulation
//!
//! ARM PL011 UART コントローラーのエミュレーション。
//! Linux カーネルの earlycon および標準 UART ドライバに対応。

use crate::mmio::MmioHandler;
use std::error::Error;
use std::io::{self, Write};

/// PL011 UART register offsets
mod regs {
    /// Data Register (R/W)
    pub const DR: u64 = 0x00;
    /// Receive Status / Error Clear (R/W)
    pub const RSR_ECR: u64 = 0x04;
    /// Flag Register (RO)
    pub const FR: u64 = 0x18;
    /// IrDA Low-Power Counter (R/W) - not implemented
    pub const ILPR: u64 = 0x20;
    /// Integer Baud Rate (R/W)
    pub const IBRD: u64 = 0x24;
    /// Fractional Baud Rate (R/W)
    pub const FBRD: u64 = 0x28;
    /// Line Control Register (R/W)
    pub const LCR_H: u64 = 0x2C;
    /// Control Register (R/W)
    pub const CR: u64 = 0x30;
    /// Interrupt FIFO Level Select (R/W)
    pub const IFLS: u64 = 0x34;
    /// Interrupt Mask Set/Clear (R/W)
    pub const IMSC: u64 = 0x38;
    /// Raw Interrupt Status (RO)
    pub const RIS: u64 = 0x3C;
    /// Masked Interrupt Status (RO)
    pub const MIS: u64 = 0x40;
    /// Interrupt Clear Register (WO)
    pub const ICR: u64 = 0x44;
    /// DMA Control Register (R/W)
    pub const DMACR: u64 = 0x48;

    /// Peripheral ID registers (RO)
    pub const PERIPHID0: u64 = 0xFE0;
    pub const PERIPHID1: u64 = 0xFE4;
    pub const PERIPHID2: u64 = 0xFE8;
    pub const PERIPHID3: u64 = 0xFEC;

    /// Cell ID registers (RO)
    pub const CELLID0: u64 = 0xFF0;
    pub const CELLID1: u64 = 0xFF4;
    pub const CELLID2: u64 = 0xFF8;
    pub const CELLID3: u64 = 0xFFC;
}

/// Flag Register bits
#[allow(dead_code)]
mod fr_bits {
    /// Clear To Send
    pub const CTS: u64 = 1 << 0;
    /// Data Set Ready
    pub const DSR: u64 = 1 << 1;
    /// Data Carrier Detect
    pub const DCD: u64 = 1 << 2;
    /// UART Busy
    pub const BUSY: u64 = 1 << 3;
    /// Receive FIFO Empty
    pub const RXFE: u64 = 1 << 4;
    /// Transmit FIFO Full
    pub const TXFF: u64 = 1 << 5;
    /// Receive FIFO Full
    pub const RXFF: u64 = 1 << 6;
    /// Transmit FIFO Empty
    pub const TXFE: u64 = 1 << 7;
    /// Ring Indicator
    pub const RI: u64 = 1 << 8;
}

/// Control Register bits
#[allow(dead_code)]
mod cr_bits {
    /// UART Enable
    pub const UARTEN: u64 = 1 << 0;
    /// SIR Enable (IrDA)
    pub const SIREN: u64 = 1 << 1;
    /// SIR Low-Power
    pub const SIRLP: u64 = 1 << 2;
    /// Loopback Enable
    pub const LBE: u64 = 1 << 7;
    /// Transmit Enable
    pub const TXE: u64 = 1 << 8;
    /// Receive Enable
    pub const RXE: u64 = 1 << 9;
    /// Data Transmit Ready
    pub const DTR: u64 = 1 << 10;
    /// Request To Send
    pub const RTS: u64 = 1 << 11;
    /// CTS Hardware Flow Control
    pub const CTSEN: u64 = 1 << 14;
    /// RTS Hardware Flow Control
    pub const RTSEN: u64 = 1 << 15;
}

/// Line Control Register bits
#[allow(dead_code)]
mod lcr_h_bits {
    /// Send Break
    pub const BRK: u64 = 1 << 0;
    /// Parity Enable
    pub const PEN: u64 = 1 << 1;
    /// Even Parity Select
    pub const EPS: u64 = 1 << 2;
    /// Two Stop Bits Select
    pub const STP2: u64 = 1 << 3;
    /// Enable FIFOs
    pub const FEN: u64 = 1 << 4;
    /// Word Length (bits 5-6)
    pub const WLEN_MASK: u64 = 0x3 << 5;
    /// Stick Parity Select
    pub const SPS: u64 = 1 << 7;
}

/// Interrupt bits (for IMSC, RIS, MIS, ICR)
#[allow(dead_code)]
mod int_bits {
    /// Ring Indicator Modem
    pub const RIMIM: u64 = 1 << 0;
    /// Clear To Send Modem
    pub const CTSMIM: u64 = 1 << 1;
    /// Data Carrier Detect Modem
    pub const DCDMIM: u64 = 1 << 2;
    /// Data Set Ready Modem
    pub const DSRMIM: u64 = 1 << 3;
    /// Receive
    pub const RXIM: u64 = 1 << 4;
    /// Transmit
    pub const TXIM: u64 = 1 << 5;
    /// Receive Timeout
    pub const RTIM: u64 = 1 << 6;
    /// Framing Error
    pub const FEIM: u64 = 1 << 7;
    /// Parity Error
    pub const PEIM: u64 = 1 << 8;
    /// Break Error
    pub const BEIM: u64 = 1 << 9;
    /// Overrun Error
    pub const OEIM: u64 = 1 << 10;
}

/// PL011 UART device emulator
///
/// ARM PL011 UART コントローラーをエミュレート。
/// - UART_DR (0x00) への書き込みは stdout に出力
/// - UART_FR (0x18) の読み取りは TXFE (TX FIFO empty) を返す
/// - 各種制御レジスタをサポート
pub struct Pl011Uart {
    base_addr: u64,
    /// Integer Baud Rate Divisor
    ibrd: u64,
    /// Fractional Baud Rate Divisor
    fbrd: u64,
    /// Line Control Register
    lcr_h: u64,
    /// Control Register
    cr: u64,
    /// Interrupt FIFO Level Select
    ifls: u64,
    /// Interrupt Mask Set/Clear
    imsc: u64,
    /// Raw Interrupt Status
    ris: u64,
    /// DMA Control Register
    dmacr: u64,
    /// Receive Status / Error Clear
    rsr: u64,
}

impl Pl011Uart {
    /// Create a new PL011 UART device
    ///
    /// # Arguments
    /// * `base_addr` - Base address of the UART device (typically 0x09000000)
    pub fn new(base_addr: u64) -> Self {
        Self {
            base_addr,
            ibrd: 0,
            fbrd: 0,
            lcr_h: 0,
            // Default: UART disabled, TX/RX enabled
            cr: cr_bits::TXE | cr_bits::RXE,
            // Default FIFO levels: 1/2 full
            ifls: 0b010_010,
            imsc: 0,
            ris: int_bits::TXIM, // TX interrupt always asserted (FIFO empty)
            dmacr: 0,
            rsr: 0,
        }
    }

    /// Check if UART is enabled
    #[allow(dead_code)]
    fn is_enabled(&self) -> bool {
        (self.cr & cr_bits::UARTEN) != 0
    }

    /// Check if transmit is enabled
    #[allow(dead_code)]
    fn is_tx_enabled(&self) -> bool {
        (self.cr & cr_bits::TXE) != 0
    }

    /// Get Flag Register value
    fn get_flags(&self) -> u64 {
        let mut flags = 0u64;

        // TX FIFO is always empty (we flush immediately)
        flags |= fr_bits::TXFE;

        // RX FIFO is always empty (no input support yet)
        flags |= fr_bits::RXFE;

        // CTS is always asserted (ready to send)
        flags |= fr_bits::CTS;

        // DSR is always asserted
        flags |= fr_bits::DSR;

        // DCD is always asserted
        flags |= fr_bits::DCD;

        flags
    }

    /// Get Masked Interrupt Status
    fn get_mis(&self) -> u64 {
        self.ris & self.imsc
    }
}

impl MmioHandler for Pl011Uart {
    fn base(&self) -> u64 {
        self.base_addr
    }

    fn size(&self) -> u64 {
        0x1000 // 4KB memory-mapped region
    }

    fn read(&mut self, offset: u64, _size: usize) -> Result<u64, Box<dyn Error>> {
        let value = match offset {
            regs::DR => {
                // No receive data available, return 0
                0
            }
            regs::RSR_ECR => self.rsr,
            regs::FR => self.get_flags(),
            regs::ILPR => 0, // Not implemented
            regs::IBRD => self.ibrd,
            regs::FBRD => self.fbrd,
            regs::LCR_H => self.lcr_h,
            regs::CR => self.cr,
            regs::IFLS => self.ifls,
            regs::IMSC => self.imsc,
            regs::RIS => self.ris,
            regs::MIS => self.get_mis(),
            regs::ICR => 0, // Write-only register
            regs::DMACR => self.dmacr,

            // Peripheral ID (PL011 identification)
            regs::PERIPHID0 => 0x11, // Part number [7:0]
            regs::PERIPHID1 => 0x10, // Part number [11:8], Designer [3:0]
            regs::PERIPHID2 => 0x14, // Revision, Designer [7:4]
            regs::PERIPHID3 => 0x00, // Configuration

            // Cell ID (PrimeCell identification)
            regs::CELLID0 => 0x0D,
            regs::CELLID1 => 0xF0,
            regs::CELLID2 => 0x05,
            regs::CELLID3 => 0xB1,

            _ => 0,
        };

        Ok(value)
    }

    fn write(&mut self, offset: u64, value: u64, _size: usize) -> Result<(), Box<dyn Error>> {
        match offset {
            regs::DR => {
                // Only output if UART and TX are enabled
                // (但し earlycon 対応のため、無効でも出力する)
                let ch = (value & 0xFF) as u8;
                print!("{}", ch as char);
                io::stdout().flush()?;
            }
            regs::RSR_ECR => {
                // Writing any value clears the error flags
                self.rsr = 0;
            }
            regs::FR => {
                // Flag register is read-only, ignore
            }
            regs::ILPR => {
                // IrDA not implemented, ignore
            }
            regs::IBRD => {
                self.ibrd = value & 0xFFFF;
            }
            regs::FBRD => {
                self.fbrd = value & 0x3F;
            }
            regs::LCR_H => {
                self.lcr_h = value & 0xFF;
            }
            regs::CR => {
                self.cr = value & 0xFFFF;
            }
            regs::IFLS => {
                self.ifls = value & 0x3F;
            }
            regs::IMSC => {
                self.imsc = value & 0x7FF;
            }
            regs::RIS => {
                // Read-only, ignore
            }
            regs::MIS => {
                // Read-only, ignore
            }
            regs::ICR => {
                // Clear the specified interrupt bits
                self.ris &= !value;
                // TX interrupt is always re-asserted (FIFO always empty)
                self.ris |= int_bits::TXIM;
            }
            regs::DMACR => {
                self.dmacr = value & 0x7;
            }
            _ => {
                // Ignore writes to unknown registers
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uart_base_and_size() {
        let uart = Pl011Uart::new(0x09000000);
        assert_eq!(uart.base(), 0x09000000);
        assert_eq!(uart.size(), 0x1000);
    }

    #[test]
    fn test_uart_fr_read() {
        let mut uart = Pl011Uart::new(0x09000000);
        let value = uart.read(regs::FR, 4).unwrap();
        // TX FIFO empty, RX FIFO empty, CTS/DSR/DCD asserted
        assert_ne!(value & fr_bits::TXFE, 0);
        assert_ne!(value & fr_bits::RXFE, 0);
    }

    #[test]
    fn test_uart_unknown_register_read() {
        let mut uart = Pl011Uart::new(0x09000000);
        let value = uart.read(0x100, 4).unwrap();
        assert_eq!(value, 0);
    }

    #[test]
    fn test_uart_dr_write() {
        let mut uart = Pl011Uart::new(0x09000000);
        // Writing 'A' (0x41) should succeed
        uart.write(regs::DR, 0x41, 4).unwrap();
    }

    #[test]
    fn test_uart_unknown_register_write() {
        let mut uart = Pl011Uart::new(0x09000000);
        // Writes to unknown registers should be ignored (no error)
        uart.write(0x100, 0x42, 4).unwrap();
    }

    #[test]
    fn test_uart_cr_read_write() {
        let mut uart = Pl011Uart::new(0x09000000);

        // Default: TX and RX enabled
        let cr = uart.read(regs::CR, 4).unwrap();
        assert_ne!(cr & cr_bits::TXE, 0);
        assert_ne!(cr & cr_bits::RXE, 0);

        // Enable UART
        uart.write(regs::CR, cr_bits::UARTEN | cr_bits::TXE | cr_bits::RXE, 4)
            .unwrap();
        let cr = uart.read(regs::CR, 4).unwrap();
        assert_ne!(cr & cr_bits::UARTEN, 0);
    }

    #[test]
    fn test_uart_lcr_h_read_write() {
        let mut uart = Pl011Uart::new(0x09000000);

        // Write 8-bit, no parity, 1 stop bit, FIFO enabled
        let lcr = lcr_h_bits::FEN | (0b11 << 5); // 8-bit word length
        uart.write(regs::LCR_H, lcr, 4).unwrap();

        let value = uart.read(regs::LCR_H, 4).unwrap();
        assert_eq!(value, lcr);
    }

    #[test]
    fn test_uart_baud_rate_registers() {
        let mut uart = Pl011Uart::new(0x09000000);

        // Set baud rate divisors
        uart.write(regs::IBRD, 0x0027, 4).unwrap(); // Integer part
        uart.write(regs::FBRD, 0x04, 4).unwrap(); // Fractional part

        assert_eq!(uart.read(regs::IBRD, 4).unwrap(), 0x0027);
        assert_eq!(uart.read(regs::FBRD, 4).unwrap(), 0x04);
    }

    #[test]
    fn test_uart_interrupt_registers() {
        let mut uart = Pl011Uart::new(0x09000000);

        // Set interrupt mask
        uart.write(regs::IMSC, int_bits::TXIM | int_bits::RXIM, 4)
            .unwrap();
        assert_eq!(
            uart.read(regs::IMSC, 4).unwrap(),
            int_bits::TXIM | int_bits::RXIM
        );

        // RIS should have TXIM set (TX FIFO empty)
        let ris = uart.read(regs::RIS, 4).unwrap();
        assert_ne!(ris & int_bits::TXIM, 0);

        // MIS should reflect masked interrupts
        let mis = uart.read(regs::MIS, 4).unwrap();
        assert_ne!(mis & int_bits::TXIM, 0);
    }

    #[test]
    fn test_uart_icr_clears_interrupts() {
        let mut uart = Pl011Uart::new(0x09000000);

        // Clear TX interrupt
        uart.write(regs::ICR, int_bits::TXIM, 4).unwrap();

        // But TXIM should be re-asserted (FIFO always empty)
        let ris = uart.read(regs::RIS, 4).unwrap();
        assert_ne!(ris & int_bits::TXIM, 0);
    }

    #[test]
    fn test_uart_peripheral_id() {
        let mut uart = Pl011Uart::new(0x09000000);

        // Check PL011 identification
        assert_eq!(uart.read(regs::PERIPHID0, 4).unwrap(), 0x11);
        assert_eq!(uart.read(regs::PERIPHID1, 4).unwrap(), 0x10);
        assert_eq!(uart.read(regs::PERIPHID2, 4).unwrap(), 0x14);
    }

    #[test]
    fn test_uart_cell_id() {
        let mut uart = Pl011Uart::new(0x09000000);

        // Check PrimeCell identification
        assert_eq!(uart.read(regs::CELLID0, 4).unwrap(), 0x0D);
        assert_eq!(uart.read(regs::CELLID1, 4).unwrap(), 0xF0);
        assert_eq!(uart.read(regs::CELLID2, 4).unwrap(), 0x05);
        assert_eq!(uart.read(regs::CELLID3, 4).unwrap(), 0xB1);
    }

    #[test]
    fn test_uart_rsr_ecr() {
        let mut uart = Pl011Uart::new(0x09000000);

        // RSR should be 0 initially
        assert_eq!(uart.read(regs::RSR_ECR, 4).unwrap(), 0);

        // Writing clears error flags (no-op since RSR is 0)
        uart.write(regs::RSR_ECR, 0xFF, 4).unwrap();
        assert_eq!(uart.read(regs::RSR_ECR, 4).unwrap(), 0);
    }
}
