//! 区域分类（DESIGN-ENGINE §4.2 regions）。

use crate::paragraphs::Paragraph;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionKind {
    Heading { level: u8 },
    Formula,
    Code,
    Body,
}

/// 按启发式分类段落。阈值 [待定] 可调。
pub fn classify(paragraph: &Paragraph, body_size: f32) -> RegionKind {
    if paragraph.math_ratio > 0.3 && paragraph.lines <= 6 {
        return RegionKind::Formula;
    }
    if paragraph.mono_ratio > 0.5 {
        return RegionKind::Code;
    }
    let big = paragraph.lines <= 3 && paragraph.size >= body_size * 1.15;
    if big && paragraph.text.chars().count() <= 80 {
        let level = if paragraph.size >= body_size * 1.5 {
            1
        } else {
            2
        };
        return RegionKind::Heading { level };
    }
    RegionKind::Body
}

/// 正文基准字号（按字符量加权的众数档）。
pub fn body_size(paragraphs: &[Paragraph]) -> f32 {
    let mut buckets: std::collections::HashMap<i32, usize> = std::collections::HashMap::new();
    for paragraph in paragraphs {
        let key = (paragraph.size * 2.0).round() as i32;
        *buckets.entry(key).or_default() += paragraph.text.chars().count().max(1);
    }
    buckets
        .into_iter()
        .max_by_key(|(_, weight)| *weight)
        .map(|(key, _)| key as f32 / 2.0)
        .unwrap_or(10.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paragraph(text: &str, size: f32, math_ratio: f32, mono_ratio: f32) -> Paragraph {
        Paragraph {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 10.0,
            size,
            text: text.into(),
            lines: 1,
            math_ratio,
            mono_ratio,
        }
    }

    #[test]
    fn classifies() {
        assert_eq!(
            classify(&paragraph("x = ∫ f dx", 10.0, 0.9, 0.0), 10.0),
            RegionKind::Formula
        );
        assert_eq!(
            classify(&paragraph("fn main() {}", 10.0, 0.0, 0.9), 10.0),
            RegionKind::Code
        );
        assert_eq!(
            classify(&paragraph("1 引言", 16.0, 0.0, 0.0), 10.0),
            RegionKind::Heading { level: 1 }
        );
        assert_eq!(
            classify(&paragraph("正文文字", 10.0, 0.0, 0.0), 10.0),
            RegionKind::Body
        );
    }
}
