//! Device Tree (FDT) generation for ARM64 Linux boot

use std::error::Error;
use vm_fdt::FdtWriter;

/// Device Tree configuration
#[derive(Debug, Clone)]
pub struct DeviceTreeConfig {
    /// Memory base address (typically 0x40000000)
    pub memory_base: u64,
    /// Memory size in bytes (e.g., 0x8000000 = 128MB)
    pub memory_size: u64,
    /// UART base address (typically 0x09000000)
    pub uart_base: u64,
    /// VirtIO Block device base address (typically 0x0a000000)
    pub virtio_base: u64,
    /// GIC Distributor base address (typically 0x08000000)
    pub gic_dist_base: u64,
    /// GIC CPU Interface base address (typically 0x08010000)
    pub gic_cpu_base: u64,
    /// Kernel command line
    pub cmdline: String,
    /// initramfs start address (optional)
    pub initrd_start: Option<u64>,
    /// initramfs end address (optional)
    pub initrd_end: Option<u64>,
}

impl Default for DeviceTreeConfig {
    fn default() -> Self {
        Self {
            memory_base: 0x4000_0000,
            memory_size: 0x800_0000, // 128MB
            uart_base: 0x0900_0000,
            virtio_base: 0x0a00_0000,
            gic_dist_base: 0x0800_0000,
            gic_cpu_base: 0x0801_0000,
            cmdline: "console=ttyAMA0 root=/dev/vda rw".to_string(),
            initrd_start: None,
            initrd_end: None,
        }
    }
}

/// Generate a Device Tree binary for ARM64 Linux boot
///
/// Creates a minimal Device Tree with:
/// - CPU node (single ARM64 CPU)
/// - Memory node
/// - GICv2 interrupt controller node
/// - Timer node (ARM Generic Timer)
/// - UART (PL011) node
/// - VirtIO Block device node
/// - chosen node with bootargs
///
/// # Arguments
/// * `config` - Device Tree configuration
///
/// # Returns
/// Device Tree binary (FDT blob)
pub fn generate_device_tree(config: &DeviceTreeConfig) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut fdt = FdtWriter::new()?;

    // Root node
    let root_node = fdt.begin_node("")?;
    fdt.property_string("compatible", "linux,dummy-virt")?;
    fdt.property_u32("#address-cells", 2)?;
    fdt.property_u32("#size-cells", 2)?;
    fdt.property_string("model", "hypervisor-virt")?;
    // Interrupt cells for GICv2
    fdt.property_u32("interrupt-parent", 1)?; // phandle of GIC

    // CPUs node
    let cpus_node = fdt.begin_node("cpus")?;
    fdt.property_u32("#address-cells", 1)?;
    fdt.property_u32("#size-cells", 0)?;

    // CPU0
    let cpu0_node = fdt.begin_node("cpu@0")?;
    fdt.property_string("device_type", "cpu")?;
    fdt.property_string("compatible", "arm,armv8")?;
    fdt.property_string("enable-method", "psci")?;
    fdt.property_u32("reg", 0)?;
    fdt.end_node(cpu0_node)?; // cpu@0

    fdt.end_node(cpus_node)?; // cpus

    // Memory node
    let memory_node_name = format!("memory@{:x}", config.memory_base);
    let memory_node = fdt.begin_node(&memory_node_name)?;
    fdt.property_string("device_type", "memory")?;
    // reg = <address-high address-low size-high size-low>
    // For simplicity, we assume 64-bit addresses fit in the low 32-bits
    fdt.property_array_u64("reg", &[config.memory_base, config.memory_size])?;
    fdt.end_node(memory_node)?; // memory

    // GICv2 interrupt controller node
    let gic_node_name = format!("intc@{:x}", config.gic_dist_base);
    let gic_node = fdt.begin_node(&gic_node_name)?;
    fdt.property_string("compatible", "arm,cortex-a15-gic")?;
    fdt.property_null("interrupt-controller")?;
    fdt.property_u32("#interrupt-cells", 3)?; // GIC requires 3 cells
                                              // reg = <GICD_base GICD_size GICC_base GICC_size>
    fdt.property_array_u64(
        "reg",
        &[
            config.gic_dist_base,
            0x1_0000, // GICD size
            config.gic_cpu_base,
            0x1_0000, // GICC size
        ],
    )?;
    fdt.property_u32("phandle", 1)?; // phandle for interrupt-parent reference
    fdt.end_node(gic_node)?; // intc

    // Timer node (ARM Generic Timer)
    // PPI IRQs: Secure Phys=13, Non-secure Phys=14, Virt=11, Hyp=10
    let timer_node = fdt.begin_node("timer")?;
    fdt.property_string("compatible", "arm,armv8-timer")?;
    // interrupts: <type irq flags> for each timer
    // type: 1=PPI, irq: actual IRQ number (PPI base is 16, so subtract 16)
    // flags: 0x304 = edge-triggered, active-low (common for timer)
    fdt.property_array_u32(
        "interrupts",
        &[
            1, 13, 0x304, // Secure Physical Timer (IRQ 29)
            1, 14, 0x304, // Non-secure Physical Timer (IRQ 30)
            1, 11, 0x304, // Virtual Timer (IRQ 27)
            1, 10, 0x304, // Hypervisor Timer (IRQ 26)
        ],
    )?;
    fdt.property_null("always-on")?;
    fdt.end_node(timer_node)?; // timer

    // UART node (PL011)
    let uart_node_name = format!("pl011@{:x}", config.uart_base);
    let uart_node = fdt.begin_node(&uart_node_name)?;
    fdt.property_string("compatible", "arm,pl011")?;
    fdt.property_array_u64("reg", &[config.uart_base, 0x1000])?;
    // UART uses SPI IRQ 1 (IRQ 33)
    fdt.property_array_u32("interrupts", &[0, 1, 0x4])?; // SPI, IRQ 1, level-high
    fdt.property_null("clock-names")?;
    fdt.end_node(uart_node)?; // pl011

    // VirtIO Block device node
    let virtio_node_name = format!("virtio_block@{:x}", config.virtio_base);
    let virtio_node = fdt.begin_node(&virtio_node_name)?;
    fdt.property_string("compatible", "virtio,mmio")?;
    fdt.property_array_u64("reg", &[config.virtio_base, 0x200])?;
    // VirtIO uses SPI IRQ 2 (IRQ 34)
    fdt.property_array_u32("interrupts", &[0, 2, 0x1])?; // SPI, IRQ 2, edge-rising
    fdt.end_node(virtio_node)?; // virtio_block

    // chosen node (boot parameters)
    let chosen_node = fdt.begin_node("chosen")?;
    fdt.property_string("bootargs", &config.cmdline)?;
    fdt.property_string("stdout-path", &uart_node_name)?;
    // initramfs (initrd) addresses
    if let (Some(start), Some(end)) = (config.initrd_start, config.initrd_end) {
        fdt.property_u64("linux,initrd-start", start)?;
        fdt.property_u64("linux,initrd-end", end)?;
    }
    fdt.end_node(chosen_node)?; // chosen

    fdt.end_node(root_node)?; // root

    // Finalize and return FDT blob
    let dtb = fdt.finish()?;
    Ok(dtb.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_device_tree_with_default_config() {
        let config = DeviceTreeConfig::default();
        let dtb = generate_device_tree(&config).unwrap();

        // DTB should start with FDT magic number (0xd00dfeed)
        assert_eq!(dtb[0..4], [0xd0, 0x0d, 0xfe, 0xed]);
        // DTB should be non-empty
        assert!(dtb.len() > 100);
    }

    #[test]
    fn test_generate_device_tree_with_custom_config() {
        let config = DeviceTreeConfig {
            memory_base: 0x8000_0000,
            memory_size: 0x1000_0000, // 256MB
            uart_base: 0x1000_0000,
            virtio_base: 0x1100_0000,
            gic_dist_base: 0x0800_0000,
            gic_cpu_base: 0x0801_0000,
            cmdline: "console=ttyAMA0 earlycon root=/dev/vda rw".to_string(),
            initrd_start: None,
            initrd_end: None,
        };

        let dtb = generate_device_tree(&config).unwrap();

        // DTB should start with FDT magic number
        assert_eq!(dtb[0..4], [0xd0, 0x0d, 0xfe, 0xed]);
        assert!(dtb.len() > 100);
    }

    #[test]
    fn test_generate_device_tree_with_initrd() {
        let config = DeviceTreeConfig {
            memory_base: 0x4000_0000,
            memory_size: 0x1000_0000, // 256MB
            uart_base: 0x0900_0000,
            virtio_base: 0x0a00_0000,
            gic_dist_base: 0x0800_0000,
            gic_cpu_base: 0x0801_0000,
            cmdline: "console=ttyAMA0 rdinit=/init".to_string(),
            initrd_start: Some(0x4500_0000),
            initrd_end: Some(0x4600_0000),
        };

        let dtb = generate_device_tree(&config).unwrap();

        // DTB should start with FDT magic number
        assert_eq!(dtb[0..4], [0xd0, 0x0d, 0xfe, 0xed]);
        assert!(dtb.len() > 100);
    }

    #[test]
    fn test_device_tree_config_default() {
        let config = DeviceTreeConfig::default();
        assert_eq!(config.memory_base, 0x4000_0000);
        assert_eq!(config.memory_size, 0x800_0000);
        assert_eq!(config.uart_base, 0x0900_0000);
        assert_eq!(config.virtio_base, 0x0a00_0000);
        assert_eq!(config.gic_dist_base, 0x0800_0000);
        assert_eq!(config.gic_cpu_base, 0x0801_0000);
        assert_eq!(config.cmdline, "console=ttyAMA0 root=/dev/vda rw");
    }
}
