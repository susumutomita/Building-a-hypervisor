//! VirtIO デバイス実装
//!
//! VirtIO 1.2 仕様に基づいた仮想 I/O デバイスの実装。

pub mod queue;

pub use queue::{Descriptor, VirtQueue};
