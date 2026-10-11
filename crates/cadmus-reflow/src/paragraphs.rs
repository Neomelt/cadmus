//! 行 → 段落（DESIGN-ENGINE §4.2 paragraphs）。

use crate::lines::TextLine;

/// 一个段落（左上原点，单位 pt）。
#[derive(Debug, Clone)]
pub struct Paragraph {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// 中位字号。
    pub size: f32,
    pub text: String,
    pub lines: usize,
    /// 数学字体字符占比。
    pub math_ratio: f32,
    /// 等宽字体字符占比。
    pub mono_ratio: f32,
}

/// 在给定栏范围（x0, x1）内，把行合并为段落。
pub fn build(column: (f32, f32), lines: &[TextLine]) -> Vec<Paragraph> {
    let (x0, x1) = column;
    let mut in_column: Vec<&TextLine> = lines
        .iter()
        .filter(|line| {
            let center = line.x + line.w / 2.0;
            center >= x0 && center < x1
        })
        .collect();
    in_column.sort_by(|a, b| {
        a.y.partial_cmp(&b.y)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal))
    });

    let mut pitches: Vec<f32> = in_column
        .windows(2)
        .map(|pair| pair[1].y - pair[0].y)
        .filter(|pitch| *pitch > 0.0)
        .collect();
    pitches.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median_pitch = if pitches.is_empty() {
        0.0
    } else {
        pitches[(pitches.len() - 1) / 2]
    };

    let mut paragraphs = Vec::new();
    let mut current: Vec<&TextLine> = Vec::new();
    let mut column_x0 = f32::INFINITY;

    for line in in_column {
        // 跨栏行（全宽标题等）与公式行自成一段，不与栏内文本合并。
        let crossing = line.x < x0 - 4.0 || line.x + line.w > x1 + 4.0;
        let line_formula = is_formula_line(line);
        let starts_new = if current.is_empty() {
            true
        } else {
            let prev = current.last().unwrap();
            let prev_formula = is_formula_line(prev);
            let pitch = line.y - prev.y;
            let indent = line.x - column_x0;
            crossing
                || line_formula
                || prev_formula
                || pitch > (1.25 * median_pitch).max(0.4 * prev.size)
                || indent > 0.8 * prev.size
        };
        if starts_new {
            if !current.is_empty() {
                paragraphs.push(finish(&current));
                current = Vec::new();
            }
            column_x0 = line.x;
            current.push(line);
            if crossing || line_formula {
                paragraphs.push(finish(&current));
                current = Vec::new();
            }
        } else {
            current.push(line);
        }
    }
    if !current.is_empty() {
        paragraphs.push(finish(&current));
    }
    paragraphs
}

fn finish(lines: &[&TextLine]) -> Paragraph {
    let x = lines.iter().map(|l| l.x).fold(f32::INFINITY, f32::min);
    let x2 = lines
        .iter()
        .map(|l| l.x + l.w)
        .fold(f32::NEG_INFINITY, f32::max);
    let y = lines.iter().map(|l| l.y).fold(f32::INFINITY, f32::min);
    let y2 = lines
        .iter()
        .map(|l| l.y + l.h)
        .fold(f32::NEG_INFINITY, f32::max);

    let mut sizes: Vec<f32> = lines.iter().map(|l| l.size).collect();
    sizes.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let size = sizes[sizes.len() / 2];

    let mut text = String::new();
    let mut total = 0usize;
    let mut math = 0usize;
    let mut mono = 0usize;
    for line in lines {
        append_line(&mut text, line);
        for glyph in &line.glyphs {
            total += 1;
            if is_math_font(&glyph.font) {
                math += 1;
            }
            if is_mono_font(&glyph.font) {
                mono += 1;
            }
        }
    }
    let denominator = total.max(1) as f32;

    Paragraph {
        x,
        y,
        w: x2 - x,
        h: y2 - y,
        size,
        text,
        lines: lines.len(),
        math_ratio: math as f32 / denominator,
        mono_ratio: mono as f32 / denominator,
    }
}

/// 公式行：过半字形来自数学字体。
fn is_formula_line(line: &TextLine) -> bool {
    if line.glyphs.is_empty() {
        return false;
    }
    let math = line
        .glyphs
        .iter()
        .filter(|glyph| is_math_font(&glyph.font))
        .count();
    math as f32 / line.glyphs.len() as f32 > 0.5
}

fn append_line(text: &mut String, line: &TextLine) {
    let addition = line.text();
    if addition.is_empty() {
        return;
    }
    let previous_ascii = text
        .chars()
        .last()
        .is_some_and(|c| c.is_ascii() && !c.is_ascii_whitespace());
    let next_ascii = addition
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii() && !c.is_ascii_whitespace());
    if previous_ascii && next_ascii {
        text.push(' ');
    }
    text.push_str(&addition);
}

/// 数学字体名单（[待定] 可扩展）。
pub fn is_math_font(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    [
        "cmmi", "cmsy", "cmex", "msam", "msbm", "stix", "math", "euclid", "symbol",
    ]
    .iter()
    .any(|needle| name.contains(needle))
}

/// 等宽字体名单（[待定] 可扩展）。
pub fn is_mono_font(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    ["mono", "courier", "cmtt", "consol", "menlo"]
        .iter()
        .any(|needle| name.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::GlyphBox;

    fn line(text: &str, x: f32, y: f32) -> TextLine {
        let glyphs = text
            .chars()
            .enumerate()
            .map(|(index, ch)| GlyphBox {
                ch,
                x: x + index as f32 * 8.0,
                y,
                w: 8.0,
                h: 10.0,
                size: 10.0,
                font: "Test".into(),
            })
            .collect();
        TextLine {
            glyphs,
            x,
            y,
            w: text.chars().count() as f32 * 8.0,
            h: 10.0,
            size: 10.0,
        }
    }

    #[test]
    fn merges_adjacent_lines() {
        let lines = vec![
            line("第一行内容", 50.0, 100.0),
            line("第二行内容", 50.0, 112.0),
            line("新段落", 50.0, 150.0),
        ];
        let paragraphs = build((0.0, 300.0), &lines);
        assert_eq!(paragraphs.len(), 2);
        assert_eq!(paragraphs[0].text, "第一行内容第二行内容");
        assert_eq!(paragraphs[1].text, "新段落");
    }

    #[test]
    fn detects_math_fonts() {
        assert!(is_math_font("CMMI10"));
        assert!(is_math_font("STIXTwoMath"));
        assert!(!is_math_font("Helvetica"));
    }
}
