//! 锚点与稳定块 id（DESIGN-ENGINE §2.2）。

/// 稳定块标识。
pub type BlockId = u64;

/// 语义锚点：块 + 块内字符偏移。
///
/// `char_offset` 为块文本中的 Unicode 标量序号（0 起）。字号、旋转、重启
/// 不影响锚点。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Anchor {
    pub block_id: BlockId,
    pub char_offset: u32,
}

/// 由 `(doc_hash, page_or_chapter, seq)` 派生稳定块 id。
///
/// 纯函数：同输入多次运行（含并行）结果一致。字段按固定小端字节序参与
/// FNV-1a 混合，保证跨平台一致。
pub fn stable_block_id(doc_hash: u64, page_or_chapter: u32, seq: u32) -> BlockId {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

    let mut h = FNV_OFFSET;
    for value in [doc_hash, u64::from(page_or_chapter), u64::from(seq)] {
        for byte in value.to_le_bytes() {
            h ^= u64::from(byte);
            h = h.wrapping_mul(FNV_PRIME);
        }
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_input_same_id_across_threads() {
        let expected = stable_block_id(0xdead_beef, 3, 7);
        let handles: Vec<_> = (0..8)
            .map(|_| std::thread::spawn(|| stable_block_id(0xdead_beef, 3, 7)))
            .collect();
        for handle in handles {
            assert_eq!(handle.join().unwrap(), expected);
        }
    }

    #[test]
    fn each_field_changes_id() {
        let base = stable_block_id(1, 2, 3);
        assert_ne!(base, stable_block_id(2, 2, 3));
        assert_ne!(base, stable_block_id(1, 3, 3));
        assert_ne!(base, stable_block_id(1, 2, 4));
    }
}
