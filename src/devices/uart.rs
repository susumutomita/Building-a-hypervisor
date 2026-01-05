//! PL011 UART device emulation

use crate::mmio::MmioHandler;
use std::error::Error;
use std::io::{self, Write};

/// PL011 UART register offsets
const UART_DR: u64 = 0x00; // Data Register
const UART_FR: u64 = 0x18; // Flag Register

/// PL011 UART Flag Register bits
const UART_FR_TXFE: u64 = 1 << 7; // Transmit FIFO Empty

/// PL011 UART device emulator
///
/// This emulates the ARM PL011 UART controller.
/// - Writes to UART_DR (0x00) output characters to stdout
/// - Reads from UART_FR (0x18) return TXFE (TX FIFO empty)
pub struct Pl011Uart {
    base_addr: u64,
}

impl Pl011Uart {
    /// Create a new PL011 UART device
    ///
    /// # Arguments
    /// * `base_addr` - Base address of the UART device (typically 0x09000000)
    pub fn new(base_addr: u64) -> Self {
        Self { base_addr }
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
        match offset {
            UART_FR => {
                // Always report TX FIFO as empty (ready to transmit)
                Ok(UART_FR_TXFE)
            }
            _ => {
                // Other registers return 0
                Ok(0)
            }
        }
    }

    fn write(&mut self, offset: u64, value: u64, _size: usize) -> Result<(), Box<dyn Error>> {
        match offset {
            UART_DR => {
                // Extract the character (lower 8 bits)
                let ch = (value & 0xFF) as u8;
                // Output to stdout
                print!("{}", ch as char);
                io::stdout().flush()?;
                Ok(())
            }
            _ => {
                // Ignore writes to other registers
                Ok(())
            }
        }
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
        let value = uart.read(UART_FR, 4).unwrap();
        assert_eq!(value, UART_FR_TXFE);
    }

    #[test]
    fn test_uart_unknown_register_read() {
        let mut uart = Pl011Uart::new(0x09000000);
        let value = uart.read(0xFF, 4).unwrap();
        assert_eq!(value, 0);
    }

    #[test]
    fn test_uart_dr_write() {
        let mut uart = Pl011Uart::new(0x09000000);
        // Writing 'A' (0x41) should succeed
        uart.write(UART_DR, 0x41, 4).unwrap();
    }

    #[test]
    fn test_uart_unknown_register_write() {
        let mut uart = Pl011Uart::new(0x09000000);
        // Writes to unknown registers should be ignored (no error)
        uart.write(0xFF, 0x42, 4).unwrap();
    }
}
