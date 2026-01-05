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
    /// Kernel command line
    pub cmdline: String,
}

impl Default for DeviceTreeConfig {
    fn default() -> Self {
        Self {
            memory_base: 0x4000_0000,
            memory_size: 0x800_0000, // 128MB
            uart_base: 0x0900_0000,
            cmdline: "console=ttyAMA0".to_string(),
        }
    }
}

/// Generate a Device Tree binary for ARM64 Linux boot
///
/// Creates a minimal Device Tree with:
/// - CPU node (single ARM64 CPU)
/// - Memory node
/// - UART (PL011) node
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

    // UART node (PL011)
    let uart_node_name = format!("pl011@{:x}", config.uart_base);
    let uart_node = fdt.begin_node(&uart_node_name)?;
    fdt.property_string("compatible", "arm,pl011")?;
    fdt.property_array_u64("reg", &[config.uart_base, 0x1000])?;
    fdt.property_null("clock-names")?;
    fdt.end_node(uart_node)?; // pl011

    // chosen node (boot parameters)
    let chosen_node = fdt.begin_node("chosen")?;
    fdt.property_string("bootargs", &config.cmdline)?;
    fdt.property_string("stdout-path", &uart_node_name)?;
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
            cmdline: "console=ttyAMA0 earlycon".to_string(),
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
        assert_eq!(config.cmdline, "console=ttyAMA0");
    }
}
