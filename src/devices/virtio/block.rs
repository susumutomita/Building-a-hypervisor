//! VirtIO Block デバイス実装
//!
//! VirtIO 1.2 仕様に基づいた Block デバイスのエミュレーション。

use crate::devices::virtio::VirtQueue;
use crate::mmio::MmioHandler;
use std::error::Error;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};

/// VirtIO MMIO マジック値 ("virt")
const VIRT_MAGIC: u32 = 0x74726976;

/// VirtIO MMIO バージョン (2 for modern)
const VIRT_VERSION: u32 = 0x2;

/// VirtIO Block デバイス ID
const VIRTIO_ID_BLOCK: u32 = 0x2;

/// VirtIO Vendor ID ("QEMU")
const VIRT_VENDOR: u32 = 0x554D4551;

/// セクタサイズ（512 bytes）
const SECTOR_SIZE: usize = 512;

/// VirtIO Block リクエストタイプ
#[allow(dead_code)]
const VIRTIO_BLK_T_IN: u32 = 0; // Read
#[allow(dead_code)]
const VIRTIO_BLK_T_OUT: u32 = 1; // Write
#[allow(dead_code)]
const VIRTIO_BLK_T_FLUSH: u32 = 4; // Flush

/// VirtIO Block ステータス
#[allow(dead_code)]
const VIRTIO_BLK_S_OK: u8 = 0; // Success
#[allow(dead_code)]
const VIRTIO_BLK_S_IOERR: u8 = 1; // I/O Error
#[allow(dead_code)]
const VIRTIO_BLK_S_UNSUPP: u8 = 2; // Unsupported

/// VirtIO Block リクエスト
#[allow(dead_code)]
#[derive(Debug)]
struct VirtioBlkReq {
    /// リクエストタイプ（IN, OUT, FLUSH）
    type_: u32,
    /// セクタ番号
    sector: u64,
    /// データバッファ
    data: Vec<u8>,
    /// ステータス（OK, IOERR, UNSUPP）
    status: u8,
}

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
    /// ディスクイメージファイル
    #[allow(dead_code)]
    disk_image: Option<File>,
    /// ディスク容量（セクタ数）
    #[allow(dead_code)]
    capacity: u64,
}

impl VirtioBlockDevice {
    /// 新しい VirtIO Block デバイスを作成（ディスクなし）
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
            disk_image: None,
            capacity: 0,
        }
    }

    /// ディスクイメージ付きの VirtIO Block デバイスを作成
    ///
    /// # Arguments
    ///
    /// * `base_addr` - MMIO ベースアドレス
    /// * `disk_image` - ディスクイメージファイル
    /// * `capacity` - ディスク容量（セクタ数）
    #[allow(dead_code)]
    pub fn with_disk_image(base_addr: u64, disk_image: File, capacity: u64) -> Self {
        Self {
            base_addr,
            queue: VirtQueue::new(16),
            status: 0,
            queue_sel: 0,
            device_features_sel: 0,
            driver_features_sel: 0,
            disk_image: Some(disk_image),
            capacity,
        }
    }

    /// セクタを読み取る
    ///
    /// # Arguments
    ///
    /// * `sector` - 開始セクタ番号
    /// * `data` - 読み取ったデータを格納するバッファ
    pub fn read_sectors(&mut self, sector: u64, data: &mut [u8]) -> Result<(), Box<dyn Error>> {
        let disk = self.disk_image.as_mut().ok_or("No disk image attached")?;

        let offset = sector * SECTOR_SIZE as u64;
        disk.seek(SeekFrom::Start(offset))?;
        disk.read_exact(data)?;

        Ok(())
    }

    /// セクタに書き込む
    ///
    /// # Arguments
    ///
    /// * `sector` - 開始セクタ番号
    /// * `data` - 書き込むデータ
    pub fn write_sectors(&mut self, sector: u64, data: &[u8]) -> Result<(), Box<dyn Error>> {
        let disk = self.disk_image.as_mut().ok_or("No disk image attached")?;

        let offset = sector * SECTOR_SIZE as u64;
        disk.seek(SeekFrom::Start(offset))?;
        disk.write_all(data)?;
        disk.flush()?;

        Ok(())
    }

    /// VirtQueue を処理する
    ///
    /// Available Ring から記述子を取得し、リクエストを処理する。
    /// 現時点ではスタブ実装。
    #[allow(dead_code)]
    fn process_queue(&mut self) -> Result<(), Box<dyn Error>> {
        // TODO: ゲストメモリアクセス機能を実装後に完全実装
        // 現時点では Available Ring をチェックするのみ
        while let Some(_idx) = self.queue.pop_avail() {
            // TODO: 記述子チェーンを辿る
            // TODO: リクエストヘッダを読み取る
            // TODO: read/write 操作を実行
            // TODO: ステータスを書き込む
            // TODO: Used Ring に追加
        }

        Ok(())
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
                // キュー通知 - VirtQueue を処理
                if let Err(e) = self.process_queue() {
                    eprintln!("Failed to process queue: {}", e);
                }
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
    use std::fs::OpenOptions;

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

    #[test]
    fn test_write_and_read_sectors() {
        // テスト用ディスクイメージを作成
        let path = "/tmp/test_virtio_disk.img";
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
            .unwrap();

        // ディスクサイズを 1MB に設定
        file.set_len(1024 * 1024).unwrap();

        // VirtioBlockDevice を作成
        let capacity = 1024 * 1024 / SECTOR_SIZE as u64;
        let mut device = VirtioBlockDevice::with_disk_image(0x0a00_0000, file, capacity);

        // テストデータを作成（512 bytes）
        let mut write_data = vec![0u8; SECTOR_SIZE];
        for i in 0..SECTOR_SIZE {
            write_data[i] = (i % 256) as u8;
        }

        // セクタ 0 に書き込む
        device.write_sectors(0, &write_data).unwrap();

        // セクタ 0 から読み取る
        let mut read_data = vec![0u8; SECTOR_SIZE];
        device.read_sectors(0, &mut read_data).unwrap();

        // 読み取ったデータを検証
        assert_eq!(write_data, read_data);

        // クリーンアップ
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn test_read_write_multiple_sectors() {
        // テスト用ディスクイメージを作成
        let path = "/tmp/test_virtio_disk_multi.img";
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
            .unwrap();

        // ディスクサイズを 1MB に設定
        file.set_len(1024 * 1024).unwrap();

        // VirtioBlockDevice を作成
        let capacity = 1024 * 1024 / SECTOR_SIZE as u64;
        let mut device = VirtioBlockDevice::with_disk_image(0x0a00_0000, file, capacity);

        // テストデータを作成（1024 bytes = 2 セクタ）
        let mut write_data = vec![0u8; SECTOR_SIZE * 2];
        for i in 0..SECTOR_SIZE * 2 {
            write_data[i] = ((i / 512 + 1) * 10 + (i % 512)) as u8;
        }

        // セクタ 1-2 に書き込む
        device
            .write_sectors(1, &write_data[0..SECTOR_SIZE])
            .unwrap();
        device
            .write_sectors(2, &write_data[SECTOR_SIZE..SECTOR_SIZE * 2])
            .unwrap();

        // セクタ 1-2 から読み取る
        let mut read_data = vec![0u8; SECTOR_SIZE * 2];
        device
            .read_sectors(1, &mut read_data[0..SECTOR_SIZE])
            .unwrap();
        device
            .read_sectors(2, &mut read_data[SECTOR_SIZE..SECTOR_SIZE * 2])
            .unwrap();

        // 読み取ったデータを検証
        assert_eq!(write_data, read_data);

        // クリーンアップ
        std::fs::remove_file(path).unwrap();
    }
}
