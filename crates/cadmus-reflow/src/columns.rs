//! 分栏检测（DESIGN-ENGINE §4.2 columns）。
//!
//! x 投影谷 + 置信度；置信度不足由上层走整页位图降级。

use crate::lines::TextLine;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnKind {
    Single,
    Double,
    Multi,
}

#[derive(Debug, Clone)]
pub struct ColumnLayout {
    pub kind: ColumnKind,
    /// 主分栏线（最宽间隙中心）；单列时为 `None`。
    pub cut: Option<f32>,
    pub confidence: f32,
}

const BINS: usize = 240;

/// 检测页面分栏。
pub fn detect(page_width: f32, text_lines: &[TextLine]) -> ColumnLayout {
    if text_lines.is_empty() || page_width <= 0.0 {
        return ColumnLayout {
            kind: ColumnKind::Single,
            cut: None,
            confidence: 0.0,
        };
    }

    // 覆盖高度投影：某 bin 上被行覆盖的累计高度达到阈值才算"有内容"，
    // 通栏标题等少量跨缝的行不会把栏缝填掉。
    let mut covered_height = [0f32; BINS];
    for line in text_lines {
        let a = ((line.x / page_width) * BINS as f32).floor().max(0.0) as usize;
        let b = (((line.x + line.w) / page_width) * BINS as f32)
            .ceil()
            .max(0.0) as usize;
        for slot in covered_height
            .iter_mut()
            .take(b.min(BINS))
            .skip(a.min(BINS))
        {
            *slot += line.h.max(1.0);
        }
    }

    let text_x0 = text_lines.iter().map(|l| l.x).fold(f32::INFINITY, f32::min);
    let text_x1 = text_lines
        .iter()
        .map(|l| l.x + l.w)
        .fold(f32::NEG_INFINITY, f32::max);
    let text_top = text_lines.iter().map(|l| l.y).fold(f32::INFINITY, f32::min);
    let text_bottom = text_lines
        .iter()
        .map(|l| l.y + l.h)
        .fold(f32::NEG_INFINITY, f32::max);
    let coverage_threshold = 0.12 * (text_bottom - text_top).max(1.0); // [待定]
    let is_covered = |slot: f32| slot >= coverage_threshold;
    let min_gap = (0.015 * page_width).max(6.0); // [待定]

    // 文本边界内的空格：(中心, 宽度)
    let mut gaps: Vec<(f32, f32)> = Vec::new();
    let mut run_start: Option<usize> = None;
    for (index, height) in covered_height.iter().enumerate() {
        let covered = is_covered(*height);
        if !covered && run_start.is_none() {
            run_start = Some(index);
        }
        if (covered || index == BINS - 1)
            && let Some(start_bin) = run_start.take()
        {
            let end_bin = if covered { index } else { index + 1 };
            let start_x = start_bin as f32 / BINS as f32 * page_width;
            let end_x = end_bin as f32 / BINS as f32 * page_width;
            let gap_w = end_x - start_x;
            if gap_w >= min_gap && start_x > text_x0 + 1.0 && end_x < text_x1 - 1.0 {
                gaps.push(((start_x + end_x) / 2.0, gap_w));
            }
        }
    }

    if gaps.is_empty() {
        return ColumnLayout {
            kind: ColumnKind::Single,
            cut: None,
            confidence: 1.0,
        };
    }

    let (cut, gap_w) = gaps
        .iter()
        .copied()
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .unwrap();
    let width_score = (gap_w / (0.045 * page_width)).min(1.0);
    let crossing = text_lines
        .iter()
        .filter(|line| line.x < cut && cut < line.x + line.w)
        .count() as f32;
    let crossing_score = 1.0 - crossing / text_lines.len() as f32;
    let kind = if gaps.len() == 1 {
        ColumnKind::Double
    } else {
        ColumnKind::Multi
    };
    ColumnLayout {
        kind,
        cut: Some(cut),
        confidence: width_score.min(crossing_score).clamp(0.05, 1.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(x: f32, y: f32, w: f32) -> TextLine {
        TextLine {
            glyphs: vec![],
            x,
            y,
            w,
            h: 10.0,
            size: 10.0,
        }
    }

    #[test]
    fn detects_two_columns() {
        let mut lines = Vec::new();
        for i in 0..20 {
            lines.push(line(50.0, 100.0 + i as f32 * 12.0, 220.0));
            lines.push(line(340.0, 100.0 + i as f32 * 12.0, 220.0));
        }
        let layout = detect(612.0, &lines);
        assert_eq!(layout.kind, ColumnKind::Double);
        let cut = layout.cut.unwrap();
        assert!((cut - 305.0).abs() < 30.0, "cut={cut}");
        assert!(layout.confidence > 0.5);
    }

    #[test]
    fn ignores_full_width_title_when_detecting_columns() {
        let mut lines = vec![line(120.0, 40.0, 372.0)]; // 通栏标题
        for i in 0..20 {
            lines.push(line(50.0, 100.0 + i as f32 * 12.0, 220.0));
            lines.push(line(340.0, 100.0 + i as f32 * 12.0, 220.0));
        }
        let layout = detect(612.0, &lines);
        assert_eq!(layout.kind, ColumnKind::Double);
    }

    #[test]
    fn detects_single_column() {
        let lines: Vec<_> = (0..20)
            .map(|i| line(50.0, 100.0 + i as f32 * 12.0, 512.0))
            .collect();
        let layout = detect(612.0, &lines);
        assert_eq!(layout.kind, ColumnKind::Single);
        assert!(layout.cut.is_none());
    }
}
