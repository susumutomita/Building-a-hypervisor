//! VirtQueue (Split Virtqueues) の実装
//!
//! VirtIO 1.2 仕様に基づいた Split Virtqueues の実装。
//!
//! # 構造
//!
//! Split Virtqueues は以下の 3 つの部分から構成される：
//! - Descriptor Table: バッファを記述する記述子のテーブル
//! - Available Ring: ドライバー（ゲスト）が利用可能にした記述子のインデックス
//! - Used Ring: デバイス（ホスト）が処理完了した記述子のインデックス

use std::error::Error;

/// Descriptor フラグ: 次の記述子へチェーン
const VIRTQ_DESC_F_NEXT: u16 = 1;

/// Descriptor フラグ: 書き込み専用バッファ
const VIRTQ_DESC_F_WRITE: u16 = 2;

/// Descriptor フラグ: 間接記述子
const VIRTQ_DESC_F_INDIRECT: u16 = 4;

/// VirtQueue Descriptor (16 bytes)
///
/// バッファの記述子。複数の記述子を next でチェーンできる。
#[derive(Debug, Clone, Copy, Default)]
pub struct Descriptor {
    /// ゲスト物理アドレス
    pub addr: u64,
    /// バッファ長
    pub len: u32,
    /// フラグ（NEXT, WRITE, INDIRECT）
    pub flags: u16,
    /// 次の記述子のインデックス（NEXT フラグが立っている場合）
    pub next: u16,
}

impl Descriptor {
    /// 新しい Descriptor を作成
    pub fn new(addr: u64, len: u32, flags: u16, next: u16) -> Self {
        Self {
            addr,
            len,
            flags,
            next,
        }
    }

    /// NEXT フラグが立っているか
    pub fn has_next(&self) -> bool {
        (self.flags & VIRTQ_DESC_F_NEXT) != 0
    }

    /// WRITE フラグが立っているか（書き込み専用）
    pub fn is_write(&self) -> bool {
        (self.flags & VIRTQ_DESC_F_WRITE) != 0
    }

    /// INDIRECT フラグが立っているか
    pub fn is_indirect(&self) -> bool {
        (self.flags & VIRTQ_DESC_F_INDIRECT) != 0
    }
}

/// Available Ring
///
/// ドライバー（ゲスト）が利用可能にした記述子のインデックスを保持。
#[derive(Debug)]
struct AvailRing {
    /// フラグ（将来の実装で使用予定）
    #[allow(dead_code)]
    flags: u16,
    /// 次に書き込むインデックス
    idx: u16,
    /// 記述子インデックスのリング
    ring: Vec<u16>,
}

impl AvailRing {
    fn new(queue_size: u16) -> Self {
        Self {
            flags: 0,
            idx: 0,
            ring: vec![0; queue_size as usize],
        }
    }

    /// 次の利用可能な記述子インデックスを取得（将来の実装で使用予定）
    #[allow(dead_code)]
    fn pop(&mut self) -> Option<u16> {
        // TODO: 実際の実装では last_avail_idx と比較
        None
    }

    /// 記述子インデックスを追加（将来の実装で使用予定）
    #[allow(dead_code)]
    fn push(&mut self, desc_idx: u16) {
        let idx = self.idx as usize % self.ring.len();
        self.ring[idx] = desc_idx;
        self.idx = self.idx.wrapping_add(1);
    }
}

/// Used Ring Element
///
/// 処理完了した記述子チェーンの情報。
#[derive(Debug, Clone, Copy)]
struct UsedElem {
    /// 記述子チェーンの開始インデックス（将来の実装で使用予定）
    #[allow(dead_code)]
    id: u32,
    /// 書き込まれた合計バイト数（将来の実装で使用予定）
    #[allow(dead_code)]
    len: u32,
}

impl UsedElem {
    fn new(id: u32, len: u32) -> Self {
        Self { id, len }
    }
}

/// Used Ring
///
/// デバイス（ホスト）が処理完了した記述子の情報を保持。
#[derive(Debug)]
struct UsedRing {
    /// フラグ（将来の実装で使用予定）
    #[allow(dead_code)]
    flags: u16,
    /// 次に書き込むインデックス
    idx: u16,
    /// Used Element のリング
    ring: Vec<UsedElem>,
}

impl UsedRing {
    fn new(queue_size: u16) -> Self {
        Self {
            flags: 0,
            idx: 0,
            ring: vec![UsedElem::new(0, 0); queue_size as usize],
        }
    }

    /// Used Element を追加
    fn push(&mut self, id: u32, len: u32) {
        let idx = self.idx as usize % self.ring.len();
        self.ring[idx] = UsedElem::new(id, len);
        self.idx = self.idx.wrapping_add(1);
    }
}

/// VirtQueue (Split Virtqueues)
///
/// ドライバーとデバイス間のデータ転送用リングバッファ。
#[derive(Debug)]
pub struct VirtQueue {
    /// キューサイズ（2 の累乗）
    num: u16,
    /// Descriptor Table
    desc_table: Vec<Descriptor>,
    /// Available Ring
    avail_ring: AvailRing,
    /// Used Ring
    used_ring: UsedRing,
    /// 次に処理する Available Ring のインデックス
    last_avail_idx: u16,
}

impl VirtQueue {
    /// 新しい VirtQueue を作成
    ///
    /// # Arguments
    ///
    /// * `num` - キューサイズ（2 の累乗である必要がある）
    ///
    /// # Panics
    ///
    /// `num` が 2 の累乗でない場合、または 0 の場合にパニックする。
    pub fn new(num: u16) -> Self {
        assert!(
            num > 0 && num.is_power_of_two(),
            "Queue size must be a power of 2"
        );

        Self {
            num,
            desc_table: vec![Descriptor::default(); num as usize],
            avail_ring: AvailRing::new(num),
            used_ring: UsedRing::new(num),
            last_avail_idx: 0,
        }
    }

    /// キューサイズを取得
    pub fn size(&self) -> u16 {
        self.num
    }

    /// Available Ring から次の記述子インデックスを取得
    ///
    /// ドライバーが利用可能にした記述子があれば、そのインデックスを返す。
    pub fn pop_avail(&mut self) -> Option<u16> {
        if self.last_avail_idx == self.avail_ring.idx {
            // 新しい記述子がない
            return None;
        }

        let idx = self.last_avail_idx as usize % self.num as usize;
        let desc_idx = self.avail_ring.ring[idx];
        self.last_avail_idx = self.last_avail_idx.wrapping_add(1);

        Some(desc_idx)
    }

    /// Used Ring に処理完了した記述子を追加
    ///
    /// # Arguments
    ///
    /// * `idx` - 記述子インデックス
    /// * `len` - 書き込まれたバイト数
    pub fn push_used(&mut self, idx: u16, len: u32) {
        self.used_ring.push(idx as u32, len);
    }

    /// Descriptor Table から記述子を取得
    pub fn get_desc(&self, idx: u16) -> Result<&Descriptor, Box<dyn Error>> {
        self.desc_table
            .get(idx as usize)
            .ok_or_else(|| format!("Invalid descriptor index: {}", idx).into())
    }

    /// Descriptor Table に記述子を設定
    pub fn set_desc(&mut self, idx: u16, desc: Descriptor) -> Result<(), Box<dyn Error>> {
        if idx >= self.num {
            return Err(format!("Invalid descriptor index: {}", idx).into());
        }
        self.desc_table[idx as usize] = desc;
        Ok(())
    }

    /// Available Ring に記述子を追加（テスト用）
    #[cfg(test)]
    pub fn push_avail(&mut self, desc_idx: u16) {
        self.avail_ring.push(desc_idx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_virtqueue_new() {
        let queue = VirtQueue::new(16);
        assert_eq!(queue.size(), 16);
        assert_eq!(queue.desc_table.len(), 16);
    }

    #[test]
    #[should_panic(expected = "Queue size must be a power of 2")]
    fn test_virtqueue_new_invalid_size() {
        VirtQueue::new(15); // 2 の累乗でない
    }

    #[test]
    fn test_descriptor_flags() {
        let desc = Descriptor::new(0x1000, 512, VIRTQ_DESC_F_NEXT | VIRTQ_DESC_F_WRITE, 1);
        assert!(desc.has_next());
        assert!(desc.is_write());
        assert!(!desc.is_indirect());
    }

    #[test]
    fn test_pop_avail_empty() {
        let mut queue = VirtQueue::new(16);
        assert_eq!(queue.pop_avail(), None);
    }

    #[test]
    fn test_push_and_pop_avail() {
        let mut queue = VirtQueue::new(16);

        // Available Ring に記述子を追加
        queue.push_avail(0);
        queue.push_avail(1);
        queue.push_avail(2);

        // pop_avail で取得
        assert_eq!(queue.pop_avail(), Some(0));
        assert_eq!(queue.pop_avail(), Some(1));
        assert_eq!(queue.pop_avail(), Some(2));
        assert_eq!(queue.pop_avail(), None);
    }

    #[test]
    fn test_push_used() {
        let mut queue = VirtQueue::new(16);
        queue.push_used(0, 512);
        queue.push_used(1, 1024);

        assert_eq!(queue.used_ring.idx, 2);
        assert_eq!(queue.used_ring.ring[0].id, 0);
        assert_eq!(queue.used_ring.ring[0].len, 512);
        assert_eq!(queue.used_ring.ring[1].id, 1);
        assert_eq!(queue.used_ring.ring[1].len, 1024);
    }

    #[test]
    fn test_get_set_desc() {
        let mut queue = VirtQueue::new(16);
        let desc = Descriptor::new(0x1000, 512, 0, 0);

        queue.set_desc(0, desc).unwrap();
        let retrieved = queue.get_desc(0).unwrap();

        assert_eq!(retrieved.addr, 0x1000);
        assert_eq!(retrieved.len, 512);
    }

    #[test]
    fn test_set_desc_invalid_index() {
        let mut queue = VirtQueue::new(16);
        let desc = Descriptor::new(0x1000, 512, 0, 0);

        let result = queue.set_desc(16, desc);
        assert!(result.is_err());
    }

    #[test]
    fn test_avail_ring_wrapping() {
        let mut queue = VirtQueue::new(4); // 小さいサイズでテスト

        // リングサイズと同じ数を追加
        for i in 0..4 {
            queue.push_avail(i);
        }

        // すべて順番に取得できる
        for i in 0..4 {
            assert_eq!(queue.pop_avail(), Some(i));
        }
        assert_eq!(queue.pop_avail(), None);

        // さらに追加してラップアラウンドをテスト
        for i in 4..8 {
            queue.push_avail(i);
        }

        // ラップアラウンド後も順番に取得できる
        for i in 4..8 {
            assert_eq!(queue.pop_avail(), Some(i));
        }
        assert_eq!(queue.pop_avail(), None);
    }
}
