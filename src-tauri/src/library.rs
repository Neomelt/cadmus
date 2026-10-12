//! 书库：文件存储 + JSON 索引。
//!
//! 目录布局：
//! ```text
//! <data>/
//! ├─ library.json        # 索引
//! ├─ books/<id>.<ext>    # 书文件
//! └─ covers/<id>/cover.* # 封面（由解析器导出）
//! ```

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookEntry {
    pub id: String,
    pub title: String,
    pub author: Option<String>,
    /// epub | txt
    pub format: String,
    pub file: String,
    pub cover: Option<String>,
    pub size: u64,
    pub added_at: u64,
    /// 阅读进度 0..1
    pub progress: f32,
    pub builtin: bool,
}

fn lib_path(dir: &Path) -> PathBuf {
    dir.join("library.json")
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

/// 读取索引（不存在或损坏时返回空）。
pub fn load(dir: &Path) -> Vec<BookEntry> {
    fs::read_to_string(lib_path(dir))
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn save(dir: &Path, entries: &[BookEntry]) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    let json = serde_json::to_string_pretty(entries).map_err(|error| error.to_string())?;
    fs::write(lib_path(dir), json).map_err(|error| error.to_string())
}

/// 将书写入书库并登记索引。
pub fn ingest(
    dir: &Path,
    bytes: &[u8],
    ext: &str,
    fallback_title: &str,
    builtin: bool,
    forced_id: Option<String>,
) -> Result<BookEntry, String> {
    let id = forced_id.unwrap_or_else(|| format!("{:016x}", cadmus_core::content_hash(bytes)));
    let books = dir.join("books");
    fs::create_dir_all(&books).map_err(|error| error.to_string())?;
    let file = books.join(format!("{id}.{ext}"));
    fs::write(&file, bytes).map_err(|error| error.to_string())?;

    let (title, author, cover) = match ext {
        "epub" => {
            let covers = dir.join("covers").join(&id);
            let (document, report) = cadmus_epub::parse_epub_bytes(
                bytes,
                &cadmus_epub::EpubOptions {
                    out_dir: Some(covers),
                },
            )
            .map_err(|error| format!("EPUB 解析失败：{error}"))?;
            (
                document.title.unwrap_or_else(|| fallback_title.to_string()),
                report.creators.first().cloned(),
                report.cover_path,
            )
        }
        _ => (fallback_title.to_string(), None, None),
    };

    let entry = BookEntry {
        id,
        title,
        author,
        format: ext.to_string(),
        file: file.display().to_string(),
        cover,
        size: bytes.len() as u64,
        added_at: now(),
        progress: 0.0,
        builtin,
    };
    let mut entries = load(dir);
    entries.retain(|existing| existing.id != entry.id);
    entries.insert(0, entry.clone());
    save(dir, &entries)?;
    Ok(entry)
}

/// 首次运行：把内置示例书写入书库。
pub fn seed_sample(dir: &Path) -> Result<(), String> {
    if load(dir).iter().any(|entry| entry.builtin) {
        return Ok(());
    }
    ingest(
        dir,
        include_bytes!("../sample/sample.epub"),
        "epub",
        "示例书",
        true,
        Some("sample".to_string()),
    )
    .map(|_| ())
}

/// 读取一本书并渲染为阅读 HTML。
pub fn read_entry(dir: &Path, id: &str) -> Result<String, String> {
    let entries = load(dir);
    let entry = entries
        .into_iter()
        .find(|entry| entry.id == id)
        .ok_or_else(|| format!("未找到书籍：{id}"))?;
    let bytes = fs::read(&entry.file).map_err(|error| format!("读取文件失败：{error}"))?;

    match entry.format.as_str() {
        "txt" => {
            let (document, _report) =
                cadmus_epub::parse_txt_bytes(&bytes, &cadmus_epub::TxtOptions::default())
                    .map_err(|error| error.to_string())?;
            render_document(
                &document,
                Some(entry.title.as_str()),
                entry.author.as_deref(),
            )
        }
        _ => {
            let (document, report) =
                cadmus_epub::parse_epub_bytes(&bytes, &cadmus_epub::EpubOptions::default())
                    .map_err(|error| error.to_string())?;
            render_document(
                &document,
                document.title.as_deref().or(Some(entry.title.as_str())),
                report
                    .creators
                    .first()
                    .map(String::as_str)
                    .or(entry.author.as_deref()),
            )
        }
    }
}

fn render_document(
    document: &cadmus_core::Document,
    title: Option<&str>,
    author: Option<&str>,
) -> Result<String, String> {
    let meta = cadmus_preview::PreviewMeta {
        title: title.unwrap_or("未命名"),
        byline: author,
        cover: None,
    };
    cadmus_preview::render_to_string(document, Path::new(""), &meta)
        .map_err(|error| error.to_string())
}

/// 删除一本书（内置书不可删除）。
pub fn delete_entry(dir: &Path, id: &str) -> Result<(), String> {
    let mut entries = load(dir);
    let Some(position) = entries.iter().position(|entry| entry.id == id) else {
        return Err(format!("未找到书籍：{id}"));
    };
    if entries[position].builtin {
        return Err("内置示例书不可删除".into());
    }
    let entry = entries.remove(position);
    let _ = fs::remove_file(&entry.file);
    if let Some(cover) = &entry.cover {
        let _ = fs::remove_file(cover);
        if let Some(parent) = Path::new(cover).parent() {
            let _ = fs::remove_dir(parent);
        }
    }
    save(dir, &entries)
}

/// 更新阅读进度。
pub fn set_progress(dir: &Path, id: &str, progress: f32) -> Result<(), String> {
    let mut entries = load(dir);
    let Some(entry) = entries.iter_mut().find(|entry| entry.id == id) else {
        return Err(format!("未找到书籍：{id}"));
    };
    entry.progress = progress.clamp(0.0, 1.0);
    save(dir, &entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cadmus-lib-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn seed_import_read_progress_delete() {
        let dir = temp_dir("flow");
        seed_sample(&dir).unwrap();
        let entries = load(&dir);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].title, "山径夜行");
        assert!(entries[0].builtin);
        assert!(entries[0].cover.is_some(), "示例书封面应已导出");

        // 以另一本书导入同样的内容（模拟导入外部文件）
        let sample = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("sample/sample.epub");
        let bytes = fs::read(&sample).unwrap();
        let entry = ingest(&dir, &bytes, "epub", "外来书", false, Some("ext1".into())).unwrap();
        assert_eq!(load(&dir).len(), 2);

        let html = read_entry(&dir, &entry.id).unwrap();
        assert!(html.contains("第一章"), "阅读 HTML 缺章节");
        assert!(html.contains("c-theme"), "阅读 HTML 缺主题设置");

        set_progress(&dir, &entry.id, 0.5).unwrap();
        let progress = load(&dir)
            .iter()
            .find(|item| item.id == entry.id)
            .map(|item| item.progress)
            .unwrap_or_default();
        assert!((progress - 0.5).abs() < 1e-6);

        delete_entry(&dir, &entry.id).unwrap();
        assert_eq!(load(&dir).len(), 1);
        assert!(delete_entry(&dir, "sample").is_err(), "内置书不可删");

        let _ = fs::remove_dir_all(&dir);
    }
}
