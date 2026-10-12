//! 批注存储：每书一份 JSON（`annotations/<book_id>.json`）。
//!
//! 说明：ARCHITECTURE §7 曾规划 SQLite；一期落地为 JSON（与书库索引一致，
//! 单进程写入足够），待需要跨表查询/并发时再迁移。

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// 一条高亮批注。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Annotation {
    pub id: String,
    /// 目标块 id（十进制字符串，避免 JS 大整数精度问题）。
    pub block_id: String,
    /// 块内字符区间（半开）。
    pub start: u32,
    pub end: u32,
    /// 高亮原文摘录（列表/导出用）。
    pub text: String,
    pub color: String,
    pub note: Option<String>,
    pub created_at: u64,
}

/// 一本书的批注集合。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AnnotationStore {
    #[serde(default)]
    pub annotations: Vec<Annotation>,
    #[serde(default)]
    pub bookmarks: Vec<Bookmark>,
}

/// 一条书签。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bookmark {
    pub id: String,
    /// 附近块 id（十进制字符串；旧数据可能为 None）。
    pub block_id: Option<String>,
    /// 摘录（列表展示用）。
    pub label: String,
    /// 阅读进度 0..1。
    pub progress: f32,
    pub created_at: u64,
}

const COLORS: [&str; 4] = ["yellow", "green", "blue", "pink"];
const EXCERPT_MAX: usize = 500;

fn store_path(dir: &Path, book_id: &str) -> PathBuf {
    dir.join("annotations").join(format!("{book_id}.json"))
}

/// 读取一本书的批注（不存在或损坏时返回空）。
pub fn load(dir: &Path, book_id: &str) -> AnnotationStore {
    fs::read_to_string(store_path(dir, book_id))
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn save(dir: &Path, book_id: &str, store: &AnnotationStore) -> Result<(), String> {
    let path = store_path(dir, book_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let json = serde_json::to_string_pretty(store).map_err(|error| error.to_string())?;
    fs::write(path, json).map_err(|error| error.to_string())
}

/// 新增高亮；同书同块同区间重复添加时返回既有条目（幂等）。
pub fn add(
    dir: &Path,
    book_id: &str,
    block_id: &str,
    start: u32,
    end: u32,
    text: &str,
    color: &str,
) -> Result<Annotation, String> {
    if end <= start {
        return Err("高亮区间无效".into());
    }
    let color = if COLORS.contains(&color) {
        color
    } else {
        "yellow"
    };
    let id = format!(
        "{:016x}",
        cadmus_core::content_hash(format!("{book_id}:{block_id}:{start}:{end}").as_bytes())
    );
    let mut store = load(dir, book_id);
    if let Some(existing) = store.annotations.iter().find(|item| item.id == id) {
        return Ok(existing.clone());
    }
    let annotation = Annotation {
        id,
        block_id: block_id.to_string(),
        start,
        end,
        text: text.chars().take(EXCERPT_MAX).collect(),
        color: color.to_string(),
        note: None,
        created_at: crate::library::now(),
    };
    store.annotations.push(annotation.clone());
    save(dir, book_id, &store)?;
    Ok(annotation)
}

/// 修改颜色与/或笔记（`None` 表示不改；笔记为空白时清除）。
pub fn update(
    dir: &Path,
    book_id: &str,
    ann_id: &str,
    color: Option<&str>,
    note: Option<&str>,
) -> Result<Annotation, String> {
    let mut store = load(dir, book_id);
    let Some(annotation) = store.annotations.iter_mut().find(|item| item.id == ann_id) else {
        return Err(format!("未找到批注：{ann_id}"));
    };
    if let Some(color) = color
        && COLORS.contains(&color)
    {
        annotation.color = color.to_string();
    }
    if let Some(note) = note {
        let trimmed = note.trim();
        annotation.note = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        };
    }
    let updated = annotation.clone();
    save(dir, book_id, &store)?;
    Ok(updated)
}

/// 删除一条批注。
pub fn delete(dir: &Path, book_id: &str, ann_id: &str) -> Result<(), String> {
    let mut store = load(dir, book_id);
    let before = store.annotations.len();
    store.annotations.retain(|item| item.id != ann_id);
    if store.annotations.len() == before {
        return Err(format!("未找到批注：{ann_id}"));
    }
    save(dir, book_id, &store)
}

/// 新增书签（同一块重复添加时返回既有条目）。
pub fn add_bookmark(
    dir: &Path,
    book_id: &str,
    block_id: Option<&str>,
    label: &str,
    progress: f32,
) -> Result<Bookmark, String> {
    let id = format!(
        "{:016x}",
        cadmus_core::content_hash(format!("{book_id}:bm:{}", block_id.unwrap_or("")).as_bytes())
    );
    let mut store = load(dir, book_id);
    if let Some(existing) = store.bookmarks.iter().find(|item| item.id == id) {
        return Ok(existing.clone());
    }
    let bookmark = Bookmark {
        id,
        block_id: block_id.map(str::to_string),
        label: label.chars().take(120).collect(),
        progress: progress.clamp(0.0, 1.0),
        created_at: crate::library::now(),
    };
    store.bookmarks.push(bookmark.clone());
    save(dir, book_id, &store)?;
    Ok(bookmark)
}

/// 删除书签。
pub fn delete_bookmark(dir: &Path, book_id: &str, bookmark_id: &str) -> Result<(), String> {
    let mut store = load(dir, book_id);
    let before = store.bookmarks.len();
    store.bookmarks.retain(|item| item.id != bookmark_id);
    if store.bookmarks.len() == before {
        return Err(format!("未找到书签：{bookmark_id}"));
    }
    save(dir, book_id, &store)
}

/// 渲染用标记（供阅读页注入预览）。
pub fn marks(dir: &Path, book_id: &str) -> Vec<cadmus_preview::Mark> {
    load(dir, book_id)
        .annotations
        .into_iter()
        .map(|annotation| cadmus_preview::Mark {
            block_id: annotation.block_id,
            start: annotation.start,
            end: annotation.end,
            id: annotation.id,
            color: annotation.color,
            note: annotation.note,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cadmus-ann-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn add_list_update_delete() {
        let dir = temp_dir("flow");
        let first = add(&dir, "book1", "42", 2, 5, "中间片", "yellow").unwrap();
        assert_eq!(first.color, "yellow");
        assert!(first.note.is_none());

        // 重复添加同一区间：幂等返回既有条目，不覆盖原颜色。
        let again = add(&dir, "book1", "42", 2, 5, "中间片", "blue").unwrap();
        assert_eq!(again.id, first.id);
        assert_eq!(again.color, "yellow");

        // 不同书互不影响。
        add(&dir, "book2", "7", 0, 2, "ab", "green").unwrap();
        assert_eq!(load(&dir, "book1").annotations.len(), 1);
        assert_eq!(load(&dir, "book2").annotations.len(), 1);

        // 更新颜色与笔记（笔记去空白）。
        let updated = update(&dir, "book1", &first.id, Some("pink"), Some("  我的理解  ")).unwrap();
        assert_eq!(updated.color, "pink");
        assert_eq!(updated.note.as_deref(), Some("我的理解"));

        // 持久化：重新读取仍在。
        let reloaded = load(&dir, "book1");
        assert_eq!(reloaded.annotations[0].color, "pink");

        // 渲染标记。
        let marks = marks(&dir, "book1");
        assert_eq!(marks.len(), 1);
        assert_eq!(marks[0].block_id, "42");
        assert_eq!(marks[0].start, 2);

        delete(&dir, "book1", &first.id).unwrap();
        assert!(load(&dir, "book1").annotations.is_empty());
        assert!(delete(&dir, "book1", "missing").is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_invalid_ranges_and_falls_back_color() {
        let dir = temp_dir("valid");
        assert!(add(&dir, "b", "1", 5, 5, "x", "yellow").is_err());
        assert!(add(&dir, "b", "1", 9, 3, "x", "yellow").is_err());
        let annotation = add(&dir, "b", "1", 0, 3, "abc", "rainbow").unwrap();
        assert_eq!(annotation.color, "yellow", "未知颜色回退黄");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn bookmarks_add_and_delete() {
        let dir = temp_dir("bm");
        let first = add_bookmark(&dir, "book1", Some("42"), "第一章 开头", 0.12).unwrap();
        assert_eq!(first.block_id.as_deref(), Some("42"));
        // 同块重复：幂等
        let again = add_bookmark(&dir, "book1", Some("42"), "又点了一次", 0.5).unwrap();
        assert_eq!(again.id, first.id);
        assert_eq!(again.label, "第一章 开头");
        // 持久化 + 删除
        assert_eq!(load(&dir, "book1").bookmarks.len(), 1);
        delete_bookmark(&dir, "book1", &first.id).unwrap();
        assert!(load(&dir, "book1").bookmarks.is_empty());
        assert!(delete_bookmark(&dir, "book1", "missing").is_err());
        let _ = fs::remove_dir_all(&dir);
    }
}
