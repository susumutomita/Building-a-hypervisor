//! VirtIO Block デバイス実装
//!
//! VirtIO 1.2 仕様に基づいた Block デバイスのエミュレーション。

use crate::devices::virtio::VirtQueue;
use crate::mmio::MmioHandler;
use std::error::Error;

/// VirtIO MMIO マジック値 ("virt")
const VIRT_MAGIC: u32 = 0x74726976;

/// VirtIO MMIO バージョン (2 for modern)
const VIRT_VERSION: u32 = 0x2;

/// VirtIO Block デバイス ID
const VIRTIO_ID_BLOCK: u32 = 0x2;

/// VirtIO Vendor ID ("QEMU")
const VIRT_VENDOR: u32 = 0x554D4551;

/// VirtIO MMIO レジスタオフセット
#[allow(dead_code)]
mod regs {
    pub const MAGIC_VALUE: u64 = 0x00;
    pub const VERSION: u64 = 0x04;
    pub const DEVICE_ID: u64 = 0x08;
    pub const VENDOR_ID: u64 = 0x0c;
    pub const DEVICE_FEATURES: u64 = 0x10;
    pub const DEVICE_FEATURES_SEL: u64 = 0x14;
    pub const DRIVER_FEATURES: u64 = 0x20;
    pub const DRIVER_FEATURES_SEL: u64 = 0x24;
    pub const QUEUE_SEL: u64 = 0x30;
    pub const QUEUE_NUM_MAX: u64 = 0x34;
    pub const QUEUE_NUM: u64 = 0x38;
    pub const QUEUE_READY: u64 = 0x44;
    pub const QUEUE_NOTIFY: u64 = 0x50;
    pub const INTERRUPT_STATUS: u64 = 0x60;
    pub const INTERRUPT_ACK: u64 = 0x64;
    pub const STATUS: u64 = 0x70;
    pub const QUEUE_DESC_LOW: u64 = 0x80;
    pub const QUEUE_DESC_HIGH: u64 = 0x84;
    pub const QUEUE_DRIVER_LOW: u64 = 0x90;
    pub const QUEUE_DRIVER_HIGH: u64 = 0x94;
    pub const QUEUE_DEVICE_LOW: u64 = 0xa0;
    pub const QUEUE_DEVICE_HIGH: u64 = 0xa4;
    pub const CONFIG_GENERATION: u64 = 0xfc;
}

/// VirtIO Block デバイス
pub struct VirtioBlockDevice {
    /// ベースアドレス
    base_addr: u64,
    /// VirtQueue（キューサイズ 16）
    queue: VirtQueue,
    /// デバイスステータス
    status: u32,
    /// 選択中のキューインデックス
    queue_sel: u32,
    /// デバイス Features セレクタ
    #[allow(dead_code)]
    device_features_sel: u32,
    /// ドライバー Features セレクタ
    #[allow(dead_code)]
    driver_features_sel: u32,
}

impl VirtioBlockDevice {
    /// 新しい VirtIO Block デバイスを作成
    ///
    /// # Arguments
    ///
    /// * `base_addr` - MMIO ベースアドレス
    pub fn new(base_addr: u64) -> Self {
        Self {
            base_addr,
            queue: VirtQueue::new(16),
            status: 0,
            queue_sel: 0,
            device_features_sel: 0,
            driver_features_sel: 0,
        }
    }
}

impl MmioHandler for VirtioBlockDevice {
    fn base(&self) -> u64 {
        self.base_addr
    }

    fn size(&self) -> u64 {
        0x200 // VirtIO MMIO レジスタ領域のサイズ
    }

    fn read(&mut self, offset: u64, _size: usize) -> Result<u64, Box<dyn Error>> {
        let value = match offset {
            regs::MAGIC_VALUE => VIRT_MAGIC as u64,
            regs::VERSION => VIRT_VERSION as u64,
            regs::DEVICE_ID => VIRTIO_ID_BLOCK as u64,
            regs::VENDOR_ID => VIRT_VENDOR as u64,
            regs::QUEUE_NUM_MAX => self.queue.size() as u64,
            regs::STATUS => self.status as u64,
            regs::DEVICE_FEATURES => {
                // 最小限の実装: Features なし
                0
            }
            regs::INTERRUPT_STATUS => {
                // 割り込みは未実装
                0
            }
            _ => {
                // 未実装のレジスタは 0 を返す
                0
            }
        };

        Ok(value)
    }

    fn write(&mut self, offset: u64, value: u64, _size: usize) -> Result<(), Box<dyn Error>> {
        match offset {
            regs::STATUS => {
                self.status = value as u32;
            }
            regs::QUEUE_SEL => {
                self.queue_sel = value as u32;
            }
            regs::QUEUE_NOTIFY => {
                // キュー通知（将来実装）
            }
            regs::DEVICE_FEATURES_SEL => {
                self.device_features_sel = value as u32;
            }
            regs::DRIVER_FEATURES_SEL => {
                self.driver_features_sel = value as u32;
            }
            regs::INTERRUPT_ACK => {
                // 割り込み ACK（将来実装）
            }
            _ => {
                // 未実装のレジスタへの書き込みは無視
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_virtio_block_new() {
        let device = VirtioBlockDevice::new(0x0a00_0000);
        assert_eq!(device.base(), 0x0a00_0000);
        assert_eq!(device.size(), 0x200);
    }

    #[test]
    fn test_read_magic_value() {
        let mut device = VirtioBlockDevice::new(0x0a00_0000);
        let magic = device.read(regs::MAGIC_VALUE, 4).unwrap();
        assert_eq!(magic, VIRT_MAGIC as u64);
    }

    #[test]
    fn test_read_version() {
        let mut device = VirtioBlockDevice::new(0x0a00_0000);
        let version = device.read(regs::VERSION, 4).unwrap();
        assert_eq!(version, VIRT_VERSION as u64);
    }

    #[test]
    fn test_read_device_id() {
        let mut device = VirtioBlockDevice::new(0x0a00_0000);
        let device_id = device.read(regs::DEVICE_ID, 4).unwrap();
        assert_eq!(device_id, VIRTIO_ID_BLOCK as u64);
    }

    #[test]
    fn test_read_vendor_id() {
        let mut device = VirtioBlockDevice::new(0x0a00_0000);
        let vendor_id = device.read(regs::VENDOR_ID, 4).unwrap();
        assert_eq!(vendor_id, VIRT_VENDOR as u64);
    }

    #[test]
    fn test_read_queue_num_max() {
        let mut device = VirtioBlockDevice::new(0x0a00_0000);
        let queue_num_max = device.read(regs::QUEUE_NUM_MAX, 4).unwrap();
        assert_eq!(queue_num_max, 16);
    }

    #[test]
    fn test_write_status() {
        let mut device = VirtioBlockDevice::new(0x0a00_0000);
        device.write(regs::STATUS, 0x0f, 4).unwrap();
        assert_eq!(device.status, 0x0f);

        let status = device.read(regs::STATUS, 4).unwrap();
        assert_eq!(status, 0x0f);
    }

    #[test]
    fn test_write_queue_sel() {
        let mut device = VirtioBlockDevice::new(0x0a00_0000);
        device.write(regs::QUEUE_SEL, 0, 4).unwrap();
        assert_eq!(device.queue_sel, 0);
    }
}
