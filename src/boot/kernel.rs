//! Linux カーネルローダー

use std::error::Error;
use std::fs;
use std::path::Path;

/// Linux カーネルイメージ
#[derive(Debug)]
pub struct KernelImage {
    /// カーネルバイナリデータ
    data: Vec<u8>,
    /// エントリーポイントアドレス（ARM64 標準: 0x40080000）
    entry_point: u64,
}

impl KernelImage {
    /// カーネルイメージをファイルから読み込む
    ///
    /// # Arguments
    /// * `path` - カーネルイメージファイルのパス
    ///
    /// # Returns
    /// カーネルイメージ
    ///
    /// # Example
    /// ```no_run
    /// use hypervisor::boot::kernel::KernelImage;
    ///
    /// let kernel = KernelImage::load("vmlinux").unwrap();
    /// ```
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn Error>> {
        let data = fs::read(path)?;

        // ARM64 カーネルの標準エントリーポイント
        // 参考: https://docs.kernel.org/arch/arm64/booting.html
        let entry_point = 0x4008_0000;

        Ok(Self { data, entry_point })
    }

    /// カーネルイメージをバイトデータから作成する
    ///
    /// # Arguments
    /// * `data` - カーネルバイナリデータ
    /// * `entry_point` - エントリーポイントアドレス（省略時: 0x40080000）
    ///
    /// # Example
    /// ```
    /// use hypervisor::boot::kernel::KernelImage;
    ///
    /// let data = vec![0x00, 0x00, 0x00, 0x14]; // b #0 (無限ループ)
    /// let kernel = KernelImage::from_bytes(data, None);
    /// ```
    pub fn from_bytes(data: Vec<u8>, entry_point: Option<u64>) -> Self {
        Self {
            data,
            entry_point: entry_point.unwrap_or(0x4008_0000),
        }
    }

    /// エントリーポイントアドレスを取得する
    pub fn entry_point(&self) -> u64 {
        self.entry_point
    }

    /// カーネルイメージのサイズを取得する
    pub fn size(&self) -> usize {
        self.data.len()
    }

    /// カーネルイメージのデータへの参照を取得する
    pub fn data(&self) -> &[u8] {
        &self.data
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kernel_image_from_bytes() {
        let data = vec![0x00, 0x00, 0x00, 0x14]; // b #0
        let kernel = KernelImage::from_bytes(data.clone(), None);

        assert_eq!(kernel.entry_point(), 0x4008_0000);
        assert_eq!(kernel.size(), 4);
        assert_eq!(kernel.data(), &data);
    }

    #[test]
    fn test_kernel_image_from_bytes_with_custom_entry_point() {
        let data = vec![0x00, 0x00, 0x00, 0x14];
        let custom_entry = 0x8000_0000;
        let kernel = KernelImage::from_bytes(data, Some(custom_entry));

        assert_eq!(kernel.entry_point(), custom_entry);
    }

    #[test]
    fn test_kernel_image_empty_data() {
        let kernel = KernelImage::from_bytes(vec![], None);
        assert_eq!(kernel.size(), 0);
        assert_eq!(kernel.data(), &[]);
    }

    #[test]
    fn test_kernel_image_large_data() {
        let data = vec![0x42; 1024 * 1024]; // 1MB
        let kernel = KernelImage::from_bytes(data.clone(), None);

        assert_eq!(kernel.size(), 1024 * 1024);
        assert_eq!(kernel.data(), &data);
    }
}
