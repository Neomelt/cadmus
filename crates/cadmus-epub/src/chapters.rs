//! 章节切分（DESIGN-ENGINE §4.4）。
//!
//! 内置默认规则集（卷/章/节/回/篇/番外等）+ 兜底分段。标题判定是启发式，
//! 阈值以常量给出（[待定] 可调）。

/// 标题行长度上限（字符）。
const MAX_TITLE_LEN: usize = 40;

/// 中文数字字符集。
const NUMERALS: &str = "零一二三四五六七八九十百千万两〇";

/// 一个章节。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chapter {
    /// 章节标题；无标题（前言/兜底分段）时为 `None`。
    pub title: Option<String>,
    /// 层级：1 = 卷/部，2 = 章/节/回/篇/番外。
    pub level: u8,
    /// 正文（不含标题行本身）。
    pub body: String,
}

/// 按内置规则集切分章节。
///
/// 未命中任何标题规则时返回空列表，由调用方走兜底分段。
pub fn split_chapters(text: &str) -> Vec<Chapter> {
    let mut chapters = Vec::new();
    let mut current: Option<(Option<String>, u8, String)> = None;

    for line in text.lines() {
        if let Some(level) = heading_level(line) {
            push_chapter(&mut chapters, current.take());
            current = Some((Some(line.trim().to_string()), level, String::new()));
        } else if let Some((_, _, body)) = current.as_mut() {
            body.push_str(line);
            body.push('\n');
        } else if !line.trim().is_empty() {
            // 首个标题之前的卷首内容。
            current = Some((None, 1, format!("{line}\n")));
        }
    }
    push_chapter(&mut chapters, current.take());
    chapters
}

fn push_chapter(chapters: &mut Vec<Chapter>, chapter: Option<(Option<String>, u8, String)>) {
    if let Some((title, level, body)) = chapter {
        chapters.push(Chapter { title, level, body });
    }
}

/// 判定标题行；返回层级。
fn heading_level(line: &str) -> Option<u8> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.chars().count() > MAX_TITLE_LEN || trimmed.contains('。') {
        return None;
    }
    for (prefix, level) in [("番外", 2u8), ("序章", 2), ("楔子", 2)] {
        if trimmed.starts_with(prefix) {
            return Some(level);
        }
    }
    let rest = trimmed.strip_prefix('第')?;
    let mut digits_end = 0;
    for (index, ch) in rest.char_indices() {
        if ch.is_ascii_digit() || NUMERALS.contains(ch) {
            digits_end = index + ch.len_utf8();
        } else {
            break;
        }
    }
    if digits_end == 0 {
        return None;
    }
    let marker = rest[digits_end..].chars().next()?;
    let level = match marker {
        '卷' | '部' => 1,
        '章' | '节' | '回' | '篇' => 2,
        _ => return None,
    };
    // 排除“第一章的故事……”这类正文行。
    let tail = rest[digits_end + marker.len_utf8()..].trim_start();
    if let Some(first) = tail.chars().next()
        && "的地得是在中里".contains(first)
    {
        return None;
    }
    Some(level)
}

/// 兜底分段：按行聚合到 `max_bytes` 以内。
pub fn chunk_by_size(text: &str, max_bytes: usize) -> Vec<String> {
    let max_bytes = max_bytes.max(1);
    let mut chunks = Vec::new();
    let mut current = String::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if !current.is_empty() && current.len() + line.len() + 1 > max_bytes {
            chunks.push(std::mem::take(&mut current));
        }
        current.push_str(line);
        current.push('\n');
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_volumes_and_chapters() {
        let text = "第一卷 风起\n\n第一章 出发\n他上路了。\n\n第二章 抵达\n到了。";
        let chapters = split_chapters(text);
        assert_eq!(chapters.len(), 3);
        assert_eq!(chapters[0].title.as_deref(), Some("第一卷 风起"));
        assert_eq!(chapters[0].level, 1);
        assert_eq!(chapters[1].title.as_deref(), Some("第一章 出发"));
        assert_eq!(chapters[1].level, 2);
        assert!(chapters[1].body.contains("他上路了。"));
    }

    #[test]
    fn prose_with_connector_is_not_heading() {
        let text = "第一章的故事从这里开始\n我们继续往下读。";
        let chapters = split_chapters(text);
        assert_eq!(chapters.len(), 1);
        assert_eq!(chapters[0].title, None);
        assert!(chapters[0].body.contains("第一章的故事"));
    }

    #[test]
    fn extra_patterns() {
        let text = "番外 后日谈\n内容。\n第三回 再会\n内容。";
        let chapters = split_chapters(text);
        assert_eq!(chapters.len(), 2);
        assert_eq!(chapters[0].title.as_deref(), Some("番外 后日谈"));
        assert_eq!(chapters[0].level, 2);
        assert_eq!(chapters[1].title.as_deref(), Some("第三回 再会"));
    }

    #[test]
    fn chunk_by_size_splits() {
        let text = "0123456789\n".repeat(50);
        let chunks = chunk_by_size(&text, 100);
        assert!(chunks.len() >= 5);
        assert!(chunks.iter().all(|chunk| chunk.len() <= 100));
    }
}
