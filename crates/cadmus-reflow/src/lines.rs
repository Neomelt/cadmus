//! 字形 → 文本行（DESIGN-ENGINE §4.2 lines）。
//!
//! 两步：① 按垂直中心聚类成 y 簇（中心均值 + 限幅高度容差，防雪球）；
//! ② 每簇按 x 大间隙切段（分栏留白 / 大间距），每段即一条文本行。

use crate::parse::GlyphBox;

/// 段内间隙超过该倍率（× 中位字号）时切分 [待定]。
const SEGMENT_GAP_EM: f32 = 0.9;

/// 一行文本（左上原点，单位 pt）。
#[derive(Debug, Clone)]
pub struct TextLine {
    pub glyphs: Vec<GlyphBox>,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// 中位字号。
    pub size: f32,
}

impl TextLine {
    /// 拼接行内文本；间距明显处补空格（西文含英文标点，也覆盖中英间距）。
    pub fn text(&self) -> String {
        let mut out = String::new();
        let mut previous: Option<&GlyphBox> = None;
        for glyph in &self.glyphs {
            if let Some(prev) = previous {
                let gap = glyph.x - (prev.x + prev.w);
                let ascii_side = prev.ch.is_ascii() || glyph.ch.is_ascii();
                let needs_space = gap > 0.2 * self.size
                    && ascii_side
                    && !prev.ch.is_whitespace()
                    && !glyph.ch.is_whitespace();
                if needs_space {
                    out.push(' ');
                }
            }
            out.push(glyph.ch);
            previous = Some(glyph);
        }
        out
    }
}

/// 生成文本行。
pub fn build_lines(glyphs: &[GlyphBox]) -> Vec<TextLine> {
    let mut sorted: Vec<&GlyphBox> = glyphs.iter().collect();
    sorted.sort_by(|a, b| {
        let center_a = a.y + a.h / 2.0;
        let center_b = b.y + b.h / 2.0;
        center_a
            .partial_cmp(&center_b)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal))
    });

    // ① 垂直中心聚类。
    let mut rows: Vec<Vec<&GlyphBox>> = Vec::new();
    let mut centers: Vec<f32> = Vec::new();
    let mut heights: Vec<f32> = Vec::new();
    for glyph in sorted {
        let center = glyph.y + glyph.h / 2.0;
        if let Some(index) = rows.len().checked_sub(1)
            && (center - centers[index]).abs() <= 0.45 * heights[index] + 1.5
        {
            let count = rows[index].len() as f32;
            centers[index] = (centers[index] * count + center) / (count + 1.0);
            let clamped = glyph.h.min(1.8 * heights[index]);
            heights[index] = (heights[index] * count + clamped) / (count + 1.0);
            rows[index].push(glyph);
            continue;
        }
        rows.push(vec![glyph]);
        centers.push(center);
        heights.push(glyph.h);
    }

    // ② 每簇按 x 大间隙切段。
    let mut lines = Vec::new();
    for row in rows {
        let mut glyphs: Vec<GlyphBox> = row.iter().map(|glyph| (*glyph).clone()).collect();
        glyphs.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));
        let median = median_size(&glyphs);
        let mut segment_start = 0usize;
        for index in 1..glyphs.len() {
            let gap = glyphs[index].x - (glyphs[index - 1].x + glyphs[index - 1].w);
            if gap > SEGMENT_GAP_EM * median {
                lines.push(make_line(&glyphs[segment_start..index]));
                segment_start = index;
            }
        }
        lines.push(make_line(&glyphs[segment_start..]));
    }
    lines
}

fn median_size(glyphs: &[GlyphBox]) -> f32 {
    let mut sizes: Vec<f32> = glyphs
        .iter()
        .map(|glyph| glyph.size)
        .filter(|size| size.is_finite() && *size > 1.5)
        .collect();
    if sizes.is_empty() {
        return 10.0;
    }
    sizes.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    sizes[sizes.len() / 2]
}

fn make_line(glyphs: &[GlyphBox]) -> TextLine {
    let glyphs = glyphs.to_vec();
    let x = glyphs.iter().map(|g| g.x).fold(f32::INFINITY, f32::min);
    let x2 = glyphs
        .iter()
        .map(|g| g.x + g.w)
        .fold(f32::NEG_INFINITY, f32::max);
    let y = glyphs.iter().map(|g| g.y).fold(f32::INFINITY, f32::min);
    let y2 = glyphs
        .iter()
        .map(|g| g.y + g.h)
        .fold(f32::NEG_INFINITY, f32::max);

    TextLine {
        size: median_size(&glyphs),
        glyphs,
        x,
        y,
        w: x2 - x,
        h: y2 - y,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn glyph(ch: char, x: f32, y: f32) -> GlyphBox {
        GlyphBox {
            ch,
            x,
            y,
            w: 8.0,
            h: 10.0,
            size: 10.0,
            font: "Test".into(),
        }
    }

    #[test]
    fn groups_glyphs_into_lines() {
        let glyphs = vec![
            glyph('a', 72.0, 100.0),
            glyph('b', 80.0, 100.5),
            glyph('c', 72.0, 114.0),
            glyph('d', 80.0, 114.2),
        ];
        let lines = build_lines(&glyphs);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text(), "ab");
        assert_eq!(lines[1].text(), "cd");
    }

    #[test]
    fn sorts_glyphs_within_line() {
        let glyphs = vec![glyph('世', 82.0, 50.0), glyph('你', 72.0, 50.2)];
        let lines = build_lines(&glyphs);
        assert_eq!(lines[0].text(), "你世");
    }

    #[test]
    fn splits_columns_by_horizontal_gap() {
        let glyphs = vec![
            glyph('左', 50.0, 100.0),
            glyph('栏', 58.0, 100.0),
            glyph('右', 340.0, 100.2),
            glyph('栏', 348.0, 100.2),
        ];
        let lines = build_lines(&glyphs);
        assert_eq!(lines.len(), 2);
        assert!(lines[0].x < 100.0);
        assert!(lines[1].x > 300.0);
    }

    #[test]
    fn tall_glyph_does_not_cascade() {
        // 一个高字形（积分号）夹在两行之间，不应把上下行吞并（原 bug：雪球合并）。
        let mut glyphs: Vec<GlyphBox> = (0..10)
            .map(|index| glyph('x', 72.0 + index as f32 * 8.0, 100.0))
            .collect();
        let mut tall = glyph('∫', 200.0, 95.0);
        tall.h = 30.0;
        tall.w = 6.0;
        glyphs.push(tall);
        glyphs.extend((0..10).map(|index| glyph('y', 72.0 + index as f32 * 8.0, 112.0)));
        let lines = build_lines(&glyphs);
        assert!(lines.len() <= 3);
        for line in &lines {
            let has_x = line.glyphs.iter().any(|g| g.ch == 'x');
            let has_y = line.glyphs.iter().any(|g| g.ch == 'y');
            assert!(!(has_x && has_y), "行被雪球合并：{}", line.text());
        }
    }
}
