//! 阅读顺序重建（DESIGN-ENGINE §4.2 order）。
//!
//! 全宽元素切带；带内先左栏后右栏（各按 y）。

use crate::paragraphs::Paragraph;
use crate::parse::Rect;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Item {
    Paragraph(usize),
    Image(usize),
}

pub struct OrderInput<'a> {
    pub paragraphs: &'a [Paragraph],
    pub images: &'a [Rect],
    /// 主分栏线；`None` 表示单列。
    pub cut: Option<f32>,
}

struct Entry {
    y: f32,
    is_full: bool,
    column: usize,
    item: Item,
}

/// 返回按阅读顺序排列的条目。
pub fn arrange(input: &OrderInput<'_>) -> Vec<Item> {
    let paragraphs = input.paragraphs;
    let images = input.images;
    let cut = input.cut;

    let x0 = paragraphs.iter().map(|p| p.x).fold(f32::INFINITY, f32::min);
    let x1 = paragraphs
        .iter()
        .map(|p| p.x + p.w)
        .fold(f32::NEG_INFINITY, f32::max);
    let is_full = |x: f32, w: f32| x <= x0 + 2.0 && x + w >= x1 - 2.0;
    let column = |x: f32, w: f32| match cut {
        Some(cut) if x + w / 2.0 >= cut => 1,
        _ => 0,
    };

    let mut entries: Vec<Entry> = Vec::new();
    for (index, paragraph) in paragraphs.iter().enumerate() {
        entries.push(Entry {
            y: paragraph.y,
            is_full: is_full(paragraph.x, paragraph.w),
            column: column(paragraph.x, paragraph.w),
            item: Item::Paragraph(index),
        });
    }
    for (index, rect) in images.iter().enumerate() {
        entries.push(Entry {
            y: rect.y,
            is_full: is_full(rect.x, rect.w),
            column: column(rect.x, rect.w),
            item: Item::Image(index),
        });
    }
    entries.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal));

    let mut ordered = Vec::new();
    let mut segment: Vec<Entry> = Vec::new();
    for entry in entries {
        if entry.is_full {
            flush(&mut segment, &mut ordered);
            ordered.push(entry.item);
        } else {
            segment.push(entry);
        }
    }
    flush(&mut segment, &mut ordered);
    ordered
}

fn flush(segment: &mut Vec<Entry>, ordered: &mut Vec<Item>) {
    segment.sort_by(|a, b| {
        a.column
            .cmp(&b.column)
            .then(a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal))
    });
    ordered.extend(segment.drain(..).map(|entry| entry.item));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paragraph(text: &str, x: f32, y: f32, w: f32) -> Paragraph {
        Paragraph {
            x,
            y,
            w,
            h: 10.0,
            size: 10.0,
            text: text.into(),
            lines: 1,
            math_ratio: 0.0,
            mono_ratio: 0.0,
        }
    }

    #[test]
    fn full_width_heading_then_columns() {
        let paragraphs = vec![
            paragraph("标题", 50.0, 50.0, 500.0),
            paragraph("左一", 50.0, 80.0, 220.0),
            paragraph("左二", 50.0, 95.0, 220.0),
            paragraph("右一", 330.0, 80.0, 220.0),
            paragraph("右二", 330.0, 95.0, 220.0),
        ];
        let order = arrange(&OrderInput {
            paragraphs: &paragraphs,
            images: &[],
            cut: Some(300.0),
        });
        let texts: Vec<&str> = order
            .iter()
            .map(|item| match item {
                Item::Paragraph(i) => paragraphs[*i].text.as_str(),
                Item::Image(_) => "[img]",
            })
            .collect();
        assert_eq!(texts, vec!["标题", "左一", "左二", "右一", "右二"]);
    }
}
