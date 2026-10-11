//! 源文件内容哈希（DESIGN-ENGINE §4.1）。

/// FNV-1a 64 位内容哈希，用于文档身份识别（不做安全用途）。
pub fn content_hash(bytes: &[u8]) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

    let mut h = FNV_OFFSET;
    for &byte in bytes {
        h ^= u64::from(byte);
        h = h.wrapping_mul(FNV_PRIME);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_and_sensitive() {
        let data = b"cadmus content hash";
        assert_eq!(content_hash(data), content_hash(data));
        assert_ne!(content_hash(data), content_hash(b"cadmus content hash!"));
    }

    #[test]
    fn empty_input_is_offset_basis() {
        assert_eq!(content_hash(b""), 0xcbf2_9ce4_8422_2325);
    }
}
