//! 编码探测与解码（DESIGN-ENGINE §4.4）。
//!
//! 顺序：`--encoding` 手动覆盖 > BOM > chardetng 探测。非法字节以 lossy
//! 替换继续，通过 [`DecodeReport`] 上报。

use cadmus_core::{Error, Result};

/// 解码结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodeReport {
    /// 实际使用的编码名（encoding_rs 规范名）。
    pub encoding: String,
    /// 是否发生替换（非法字节被替换为 U+FFFD）。
    pub had_errors: bool,
}

/// 解码文本为 UTF-8 字符串。
pub fn decode(bytes: &[u8], override_label: Option<&str>) -> Result<(String, DecodeReport)> {
    let (encoding, payload) = resolve_encoding(bytes, override_label)?;
    let (text, _, had_errors) = encoding.decode(payload);
    Ok((
        text.into_owned(),
        DecodeReport {
            encoding: encoding.name().to_string(),
            had_errors,
        },
    ))
}

fn resolve_encoding<'a>(
    bytes: &'a [u8],
    override_label: Option<&str>,
) -> Result<(&'static encoding_rs::Encoding, &'a [u8])> {
    // 1. 手动覆盖优先（显式指定的编码即使与 BOM 不一致也照用）。
    if let Some(label) = override_label {
        let encoding = encoding_rs::Encoding::for_label(label.as_bytes())
            .ok_or_else(|| Error::Unsupported(format!("未知编码：{label}")))?;
        return Ok((encoding, strip_matching_bom(encoding, bytes)));
    }
    // 2. BOM。
    if let Some((encoding, rest)) = sniff_bom(bytes) {
        return Ok((encoding, rest));
    }
    // 3. chardetng 探测。
    let mut detector = chardetng::EncodingDetector::new();
    detector.feed(bytes, true);
    Ok((detector.guess(None, true), bytes))
}

fn sniff_bom(bytes: &[u8]) -> Option<(&'static encoding_rs::Encoding, &[u8])> {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        Some((encoding_rs::UTF_8, &bytes[3..]))
    } else if bytes.starts_with(&[0xFF, 0xFE]) {
        Some((encoding_rs::UTF_16LE, &bytes[2..]))
    } else if bytes.starts_with(&[0xFE, 0xFF]) {
        Some((encoding_rs::UTF_16BE, &bytes[2..]))
    } else {
        None
    }
}

fn strip_matching_bom<'a>(encoding: &'static encoding_rs::Encoding, bytes: &'a [u8]) -> &'a [u8] {
    match sniff_bom(bytes) {
        Some((bom_encoding, rest)) if bom_encoding == encoding => rest,
        _ => bytes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_bom_is_stripped() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice("中文".as_bytes());
        let (text, report) = decode(&bytes, None).unwrap();
        assert_eq!(text, "中文");
        assert_eq!(report.encoding, "UTF-8");
    }

    #[test]
    fn utf16le_bom() {
        // UTF-16LE 是只解不编的编码（WHATWG），这里手工构造字节。
        let mut bytes = vec![0xFF, 0xFE];
        for unit in "中文".encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        let (text, report) = decode(&bytes, None).unwrap();
        assert_eq!(text, "中文");
        assert_eq!(report.encoding, "UTF-16LE");
    }

    #[test]
    fn detects_gb18030_text() {
        let source = "这是一段足够长的中文文本，用来让编码探测器作出可靠的判断。".repeat(20);
        let (gb, _, _) = encoding_rs::GB18030.encode(&source);
        let (text, report) = decode(&gb, None).unwrap();
        assert_eq!(text, source);
        assert!(
            report.encoding.to_ascii_uppercase().starts_with("GB"),
            "{}",
            report.encoding
        );
    }

    #[test]
    fn detects_big5_text() {
        let source = "這是一段足夠長的中文文本，用來讓編碼探測器作出可靠的判斷。".repeat(20);
        let (big5, _, _) = encoding_rs::BIG5.encode(&source);
        let (text, report) = decode(&big5, None).unwrap();
        assert_eq!(text, source);
        assert_eq!(report.encoding, "Big5");
    }

    #[test]
    fn lossy_replacement_is_reported() {
        let (text, report) = decode(&[0x41, 0xFF, 0x42], Some("utf-8")).unwrap();
        assert_eq!(text, "A\u{FFFD}B");
        assert!(report.had_errors);
    }

    #[test]
    fn unknown_label_is_rejected() {
        assert!(matches!(
            decode(b"x", Some("no-such-encoding")),
            Err(Error::Unsupported(_))
        ));
    }
}
