//! VirtIO Block ディスクイメージテスト
//!
//! ディスクイメージの読み書き機能をテストする。

use hypervisor::devices::virtio::VirtioBlockDevice;
use std::fs::OpenOptions;

const SECTOR_SIZE: usize = 512;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== VirtIO Block ディスクイメージテスト ===\n");

    // ディスクイメージファイルのパス
    let disk_path = "disk.img";

    // 1. ディスクイメージの存在確認
    println!("[1] ディスクイメージを確認中...");
    if !std::path::Path::new(disk_path).exists() {
        eprintln!("エラー: ディスクイメージが見つかりません: {}", disk_path);
        eprintln!("次のコマンドで作成してください:");
        eprintln!("  ./scripts/create_disk_image.sh 64 disk.img");
        return Err("ディスクイメージが見つかりません".into());
    }

    // ディスクサイズを取得
    let metadata = std::fs::metadata(disk_path)?;
    let file_size = metadata.len();
    let capacity = file_size / SECTOR_SIZE as u64;
    println!("    ✓ ディスクイメージ: {}", disk_path);
    println!(
        "      - ファイルサイズ: {} bytes ({} MB)",
        file_size,
        file_size / 1024 / 1024
    );
    println!("      - セクタ数: {}", capacity);

    // 2. ディスクイメージを開く
    println!("\n[2] ディスクイメージを開いています...");
    let file = OpenOptions::new().read(true).write(true).open(disk_path)?;

    // 3. VirtIO Block デバイスを作成
    println!("    ✓ VirtIO Block デバイスを作成");
    let mut device = VirtioBlockDevice::with_disk_image(0x0a00_0000, file, capacity);

    // 4. テストデータを作成（セクタ 0 に書き込む）
    println!("\n[3] セクタ 0 にテストデータを書き込んでいます...");
    let mut write_data = vec![0u8; SECTOR_SIZE];

    // テストパターン: "VIRTIO BLOCK TEST\n" + 連番
    let test_message = b"VIRTIO BLOCK TEST\n";
    write_data[0..test_message.len()].copy_from_slice(test_message);
    for i in test_message.len()..SECTOR_SIZE {
        write_data[i] = (i % 256) as u8;
    }

    device.write_sectors(0, &write_data)?;
    println!("    ✓ {} bytes 書き込み完了", write_data.len());

    // 5. セクタ 0 から読み取る
    println!("\n[4] セクタ 0 からデータを読み取っています...");
    let mut read_data = vec![0u8; SECTOR_SIZE];
    device.read_sectors(0, &mut read_data)?;
    println!("    ✓ {} bytes 読み取り完了", read_data.len());

    // 6. データを検証
    println!("\n[5] データを検証中...");
    if read_data == write_data {
        println!("    ✓ データが一致しました");

        // 最初の 32 bytes を表示
        println!("\n    最初の 32 bytes:");
        print!("      ");
        for (i, &byte) in read_data[0..32].iter().enumerate() {
            if byte >= 32 && byte < 127 {
                print!("{}", byte as char);
            } else {
                print!(".");
            }
            if (i + 1) % 16 == 0 {
                print!("\n      ");
            }
        }
        println!();
    } else {
        println!("    ✗ データが一致しません");
        return Err("データ検証エラー".into());
    }

    // 7. 複数セクタのテスト
    println!("\n[6] 複数セクタ（セクタ 10-12）のテストを実行中...");
    let mut multi_write = vec![0u8; SECTOR_SIZE * 3];
    for i in 0..SECTOR_SIZE * 3 {
        multi_write[i] = ((i / SECTOR_SIZE) as u8) + 65; // 'A', 'B', 'C'
    }

    device.write_sectors(10, &multi_write[0..SECTOR_SIZE])?;
    device.write_sectors(11, &multi_write[SECTOR_SIZE..SECTOR_SIZE * 2])?;
    device.write_sectors(12, &multi_write[SECTOR_SIZE * 2..SECTOR_SIZE * 3])?;
    println!("    ✓ {} bytes 書き込み完了", multi_write.len());

    let mut multi_read = vec![0u8; SECTOR_SIZE * 3];
    device.read_sectors(10, &mut multi_read[0..SECTOR_SIZE])?;
    device.read_sectors(11, &mut multi_read[SECTOR_SIZE..SECTOR_SIZE * 2])?;
    device.read_sectors(12, &mut multi_read[SECTOR_SIZE * 2..SECTOR_SIZE * 3])?;
    println!("    ✓ {} bytes 読み取り完了", multi_read.len());

    if multi_read == multi_write {
        println!("    ✓ 複数セクタのデータが一致しました");
    } else {
        println!("    ✗ 複数セクタのデータが一致しません");
        return Err("複数セクタデータ検証エラー".into());
    }

    println!("\n✅ すべてのテストが成功しました");
    println!("\n=== VirtIO Block ディスクイメージテスト完了 ===");

    Ok(())
}
