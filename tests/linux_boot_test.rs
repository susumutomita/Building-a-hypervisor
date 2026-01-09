//! Linux カーネル起動テスト
//!
//! 実際の Linux カーネルをハイパーバイザーで起動し、
//! earlycon 出力を確認する。

use hypervisor::boot::kernel::KernelImage;
use hypervisor::devices::uart::Pl011Uart;
use hypervisor::mmio::MmioHandler;
use hypervisor::Hypervisor;
use std::error::Error;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// UART 出力を収集する構造体
struct UartCollector {
    inner: Pl011Uart,
    output: Arc<Mutex<Vec<u8>>>,
}

impl UartCollector {
    fn new(base_addr: u64, output: Arc<Mutex<Vec<u8>>>) -> Self {
        Self {
            inner: Pl011Uart::new(base_addr),
            output,
        }
    }
}

impl MmioHandler for UartCollector {
    fn base(&self) -> u64 {
        self.inner.base()
    }

    fn size(&self) -> u64 {
        self.inner.size()
    }

    fn read(&mut self, offset: u64, size: usize) -> Result<u64, Box<dyn Error>> {
        self.inner.read(offset, size)
    }

    fn write(&mut self, offset: u64, value: u64, size: usize) -> Result<(), Box<dyn Error>> {
        // DR レジスタ (offset 0x00) への書き込みを収集
        if offset == 0x00 && size >= 1 {
            let byte = (value & 0xFF) as u8;
            if let Ok(mut output) = self.output.lock() {
                output.push(byte);
            }
            // 標準出力にも出力
            print!("{}", byte as char);
        }
        self.inner.write(offset, value, size)
    }
}

// Send + Sync は inner の Pl011Uart が既に実装済み
unsafe impl Send for UartCollector {}
unsafe impl Sync for UartCollector {}

/// メモリ定数
const RAM_BASE: u64 = 0x4000_0000;
const RAM_SIZE: usize = 256 * 1024 * 1024; // 256MB
const KERNEL_ENTRY: u64 = 0x4008_0000;
const UART_BASE: u64 = 0x0900_0000;
const DTB_ADDR: u64 = 0x4400_0000;

/// カーネルイメージのパス
const KERNEL_IMAGE_PATH: &str = "output/Image";

/// Linux カーネルを起動して earlycon 出力を確認
#[test]
#[ignore = "requires Hypervisor.framework entitlements and kernel image (run locally with --ignored)"]
fn linux_カーネルが起動してuart出力する() {
    // カーネルイメージを読み込み
    let kernel_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(KERNEL_IMAGE_PATH);
    if !kernel_path.exists() {
        eprintln!("Kernel image not found at {:?}", kernel_path);
        eprintln!("Build it first with: docker run ... scripts/build-linux-kernel.sh");
        return;
    }

    let kernel_data = fs::read(&kernel_path).expect("Failed to read kernel image");
    println!("Loaded kernel image: {} bytes", kernel_data.len());

    let kernel = KernelImage::from_bytes(kernel_data, Some(KERNEL_ENTRY));

    // ハイパーバイザーを作成
    let mut hv = Hypervisor::new(RAM_BASE, RAM_SIZE).expect("Failed to create hypervisor");

    // UART 出力を収集
    let uart_output = Arc::new(Mutex::new(Vec::new()));
    let uart = UartCollector::new(UART_BASE, Arc::clone(&uart_output));
    hv.register_mmio_handler(Box::new(uart));

    // カーネルを起動
    println!("\n=== Starting Linux kernel boot ===\n");

    let result = hv
        .boot_linux(
            &kernel,
            "console=ttyAMA0 earlycon=pl011,0x09000000 loglevel=8",
            Some(DTB_ADDR),
        )
        .expect("Failed to boot kernel");

    // 終了理由を表示
    println!("\n\n=== Kernel execution ended ===");
    println!("Exit reason: {:?}", result.exit_reason);
    if let Some(esr) = result.exception_syndrome {
        let ec = (esr >> 26) & 0x3f;
        println!("Exception Class (EC): 0x{:x}", ec);
    }
    println!("PC at exit: 0x{:x}", result.pc);

    // UART 出力を表示
    let output = uart_output.lock().unwrap();
    let output_str = String::from_utf8_lossy(&output);
    println!("\n=== UART Output ({} bytes) ===", output.len());
    println!("{}", output_str);

    // 出力があることを確認
    assert!(
        !output.is_empty(),
        "Expected some UART output from the kernel"
    );
}

/// カーネルイメージが存在するかチェック
#[test]
fn カーネルイメージが存在する() {
    let kernel_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(KERNEL_IMAGE_PATH);
    if kernel_path.exists() {
        let metadata = fs::metadata(&kernel_path).expect("Failed to get metadata");
        println!(
            "Kernel image found: {:?} ({} MB)",
            kernel_path,
            metadata.len() / 1024 / 1024
        );
        assert!(metadata.len() > 1024 * 1024, "Kernel image seems too small");
    } else {
        println!("Kernel image not found at {:?}", kernel_path);
        println!("This is expected if you haven't built the kernel yet.");
        println!("Build it with: docker run ... scripts/build-linux-kernel.sh");
    }
}
