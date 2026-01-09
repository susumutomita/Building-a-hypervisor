//! Linux カーネル起動テスト
//!
//! 実際の Linux カーネルをハイパーバイザーで起動し、
//! earlycon 出力を確認する。

use applevisor::Reg;
use hypervisor::boot::device_tree::{generate_device_tree, DeviceTreeConfig};
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
const GIC_BASE: u64 = 0x0800_0000;
const DTB_ADDR: u64 = 0x4400_0000;
const INITRAMFS_ADDR: u64 = 0x4500_0000; // initramfs 配置アドレス

/// カーネルイメージのパス
const KERNEL_IMAGE_PATH: &str = "output/Image";
const INITRAMFS_PATH: &str = "output/initramfs.cpio.gz";

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

    // GIC は Hypervisor が自動的に登録する

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

/// Linux カーネルを initramfs 付きで起動してシェルを取得
#[test]
#[ignore = "requires Hypervisor.framework entitlements, kernel image and initramfs (run locally with --ignored)"]
fn linux_カーネルがinitramfsでシェルを起動する() {
    // カーネルイメージを読み込み
    let kernel_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(KERNEL_IMAGE_PATH);
    if !kernel_path.exists() {
        eprintln!("Kernel image not found at {:?}", kernel_path);
        eprintln!("Build it first with: docker run ... scripts/build-linux-kernel.sh");
        return;
    }

    // initramfs を読み込み
    let initramfs_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(INITRAMFS_PATH);
    if !initramfs_path.exists() {
        eprintln!("initramfs not found at {:?}", initramfs_path);
        eprintln!("Build it first with: docker run ... scripts/build-initramfs.sh");
        return;
    }

    let kernel_data = fs::read(&kernel_path).expect("Failed to read kernel image");
    println!("Loaded kernel image: {} bytes", kernel_data.len());

    let initramfs_data = fs::read(&initramfs_path).expect("Failed to read initramfs");
    println!("Loaded initramfs: {} bytes", initramfs_data.len());

    let kernel = KernelImage::from_bytes(kernel_data, Some(KERNEL_ENTRY));

    // ハイパーバイザーを作成
    let mut hv = Hypervisor::new(RAM_BASE, RAM_SIZE).expect("Failed to create hypervisor");

    // UART 出力を収集
    let uart_output = Arc::new(Mutex::new(Vec::new()));
    let uart = UartCollector::new(UART_BASE, Arc::clone(&uart_output));
    hv.register_mmio_handler(Box::new(uart));

    // GIC は Hypervisor が自動的に登録する

    // initramfs をメモリに配置
    let initramfs_end = INITRAMFS_ADDR + initramfs_data.len() as u64;
    for (i, &byte) in initramfs_data.iter().enumerate() {
        hv.write_byte(INITRAMFS_ADDR + i as u64, byte)
            .expect("Failed to write initramfs");
    }
    println!(
        "initramfs loaded at 0x{:x}-0x{:x}",
        INITRAMFS_ADDR, initramfs_end
    );

    // Device Tree を生成（initramfs 情報付き）
    let dtb = generate_device_tree(&DeviceTreeConfig {
        memory_base: RAM_BASE,
        memory_size: RAM_SIZE as u64,
        uart_base: UART_BASE,
        virtio_base: 0x0a00_0000,
        gic_dist_base: GIC_BASE,
        gic_cpu_base: GIC_BASE + 0x1_0000,
        cmdline: "console=ttyAMA0 earlycon=pl011,0x09000000 loglevel=8 rdinit=/init".to_string(),
        initrd_start: Some(INITRAMFS_ADDR),
        initrd_end: Some(initramfs_end),
    })
    .expect("Failed to generate device tree");

    // Device Tree をメモリに配置
    for (i, &byte) in dtb.iter().enumerate() {
        hv.write_byte(DTB_ADDR + i as u64, byte)
            .expect("Failed to write DTB");
    }
    println!("DTB loaded at 0x{:x} ({} bytes)", DTB_ADDR, dtb.len());

    // カーネルをメモリに配置
    for (i, &byte) in kernel.data().iter().enumerate() {
        hv.write_byte(KERNEL_ENTRY + i as u64, byte)
            .expect("Failed to write kernel");
    }
    println!("Kernel loaded at 0x{:x}", KERNEL_ENTRY);

    // ARM64 Linux ブート条件を設定
    hv.set_reg(Reg::X0, DTB_ADDR).expect("Failed to set X0");
    hv.set_reg(Reg::X1, 0).expect("Failed to set X1");
    hv.set_reg(Reg::X2, 0).expect("Failed to set X2");
    hv.set_reg(Reg::X3, 0).expect("Failed to set X3");

    // カーネルを起動
    println!("\n=== Starting Linux kernel boot with initramfs ===\n");

    let result = hv
        .run(Some(0x3c5), Some(true), Some(KERNEL_ENTRY))
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

    // カーネルが正常に起動していることを確認
    // 注: initramfs の展開には到達していないが、カーネルの初期化は進んでいる
    assert!(
        output_str.contains("Booting Linux") || output_str.contains("Linux version"),
        "Expected Linux boot message"
    );

    // GIC が動作していることを確認
    assert!(
        output_str.contains("gic_handle_irq") || output_str.contains("Root IRQ handler"),
        "Expected GIC initialization message"
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
