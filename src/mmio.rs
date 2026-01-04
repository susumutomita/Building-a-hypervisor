//! MMIO (Memory-Mapped I/O) handling infrastructure

use std::error::Error;

/// MMIO デバイスハンドラの trait
pub trait MmioHandler: Send + Sync {
    /// デバイスのベースアドレスを返す
    fn base(&self) -> u64;

    /// デバイスのメモリマップサイズを返す
    fn size(&self) -> u64;

    /// デバイスからデータを読み取る
    ///
    /// # Arguments
    /// * `offset` - ベースアドレスからのオフセット
    /// * `size` - 読み取るサイズ (1, 2, 4, 8 bytes)
    fn read(&mut self, offset: u64, size: usize) -> Result<u64, Box<dyn Error>>;

    /// デバイスにデータを書き込む
    ///
    /// # Arguments
    /// * `offset` - ベースアドレスからのオフセット
    /// * `value` - 書き込む値
    /// * `size` - 書き込むサイズ (1, 2, 4, 8 bytes)
    fn write(&mut self, offset: u64, value: u64, size: usize) -> Result<(), Box<dyn Error>>;
}

/// MMIO デバイスマネージャ
pub struct MmioManager {
    handlers: Vec<Box<dyn MmioHandler>>,
}

impl MmioManager {
    /// 新しい MMIO マネージャを作成する
    pub fn new() -> Self {
        Self {
            handlers: Vec::new(),
        }
    }

    /// MMIO デバイスハンドラを登録する
    ///
    /// # Arguments
    /// * `handler` - 登録する MMIO ハンドラ
    pub fn register(&mut self, handler: Box<dyn MmioHandler>) {
        self.handlers.push(handler);
    }

    /// 指定されたアドレスからデータを読み取る
    ///
    /// # Arguments
    /// * `addr` - 読み取るアドレス
    /// * `size` - 読み取るサイズ (bytes)
    ///
    /// # Returns
    /// 読み取った値
    pub fn handle_read(&mut self, addr: u64, size: usize) -> Result<u64, Box<dyn Error>> {
        // 該当するハンドラを検索
        for handler in &mut self.handlers {
            let base = handler.base();
            let handler_size = handler.size();

            if addr >= base && addr < base + handler_size {
                let offset = addr - base;
                return handler.read(offset, size);
            }
        }

        // ハンドラが見つからない場合は 0 を返す
        eprintln!(
            "MMIO read from unhandled address: 0x{:x} (size: {})",
            addr, size
        );
        Ok(0)
    }

    /// 指定されたアドレスにデータを書き込む
    ///
    /// # Arguments
    /// * `addr` - 書き込むアドレス
    /// * `value` - 書き込む値
    /// * `size` - 書き込むサイズ (bytes)
    pub fn handle_write(
        &mut self,
        addr: u64,
        value: u64,
        size: usize,
    ) -> Result<(), Box<dyn Error>> {
        // 該当するハンドラを検索
        for handler in &mut self.handlers {
            let base = handler.base();
            let handler_size = handler.size();

            if addr >= base && addr < base + handler_size {
                let offset = addr - base;
                return handler.write(offset, value, size);
            }
        }

        // ハンドラが見つからない場合は警告を出す
        eprintln!(
            "MMIO write to unhandled address: 0x{:x} = 0x{:x} (size: {})",
            addr, value, size
        );
        Ok(())
    }
}

impl Default for MmioManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DummyDevice {
        base: u64,
        size: u64,
        data: u64,
    }

    impl MmioHandler for DummyDevice {
        fn base(&self) -> u64 {
            self.base
        }

        fn size(&self) -> u64 {
            self.size
        }

        fn read(&mut self, _offset: u64, _size: usize) -> Result<u64, Box<dyn Error>> {
            Ok(self.data)
        }

        fn write(&mut self, _offset: u64, value: u64, _size: usize) -> Result<(), Box<dyn Error>> {
            self.data = value;
            Ok(())
        }
    }

    #[test]
    fn test_mmio_manager_register() {
        let mut manager = MmioManager::new();
        let device = Box::new(DummyDevice {
            base: 0x1000,
            size: 0x100,
            data: 0,
        });

        manager.register(device);
        assert_eq!(manager.handlers.len(), 1);
    }

    #[test]
    fn test_mmio_manager_write_read() {
        let mut manager = MmioManager::new();
        let device = Box::new(DummyDevice {
            base: 0x1000,
            size: 0x100,
            data: 0,
        });

        manager.register(device);

        // Write
        manager.handle_write(0x1000, 0x42, 4).unwrap();

        // Read
        let value = manager.handle_read(0x1000, 4).unwrap();
        assert_eq!(value, 0x42);
    }

    #[test]
    fn test_mmio_manager_unhandled_address() {
        let mut manager = MmioManager::new();

        // 未登録のアドレスへの読み取り
        let value = manager.handle_read(0x9999, 4).unwrap();
        assert_eq!(value, 0);

        // 未登録のアドレスへの書き込み（エラーにならない）
        manager.handle_write(0x9999, 0x42, 4).unwrap();
    }
}
