//! 书库：文件存储 + JSON 索引。
//!
//! 目录布局：
//! ```text
//! <data>/
//! ├─ library.json          # 索引
//! ├─ books/<id>.<ext>      # 书文件
//! └─ assets/<id>/cover.*   # 封面与正文图片（由解析器导出）
//! ```

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine as _;
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

/// 一本书的导出资产目录（封面 + 正文图片）。
fn assets_dir(dir: &Path, id: &str) -> PathBuf {
    dir.join("assets").join(id)
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
            // 解析时导出封面与正文图片到 assets/<id>/。
            let assets = assets_dir(dir, &id);
            let (document, report) = cadmus_epub::parse_epub_bytes(
                bytes,
                &cadmus_epub::EpubOptions {
                    out_dir: Some(assets),
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
                entry.cover.as_deref(),
            )
        }
        "pdf" => read_pdf(dir, &entry),
        _ => {
            // 解析时把正文图片导出到 assets/<id>/，位图块路径即可直接内联。
            let assets = assets_dir(dir, &entry.id);
            let (document, report) = cadmus_epub::parse_epub_bytes(
                &bytes,
                &cadmus_epub::EpubOptions {
                    out_dir: Some(assets),
                },
            )
            .map_err(|error| error.to_string())?;
            render_document(
                &document,
                document.title.as_deref().or(Some(entry.title.as_str())),
                report
                    .creators
                    .first()
                    .map(String::as_str)
                    .or(entry.author.as_deref()),
                // 旧数据封面可能存在别处，索引里的路径优先。
                entry.cover.as_deref().or(report.cover_path.as_deref()),
            )
        }
    }
}

/// PDF 重排缓存文件名：重排算法升级时递增，旧缓存自动失效。
const REFLOW_CACHE_FILE: &str = "reflow-v1.html";

/// PDF 阅读：重排一次，把渲染结果缓存到 `assets/<id>/`。
fn read_pdf(dir: &Path, entry: &BookEntry) -> Result<String, String> {
    let assets = assets_dir(dir, &entry.id);
    let cached = assets.join(REFLOW_CACHE_FILE);
    if let Ok(html) = fs::read_to_string(&cached) {
        return Ok(html);
    }
    let options = cadmus_reflow::ReflowOptions::new(&assets);
    let cancel = cadmus_core::Cancel::new();
    let document = cadmus_reflow::reflow(Path::new(&entry.file), &options, &cancel)
        .map_err(|error| format!("PDF 重排失败：{error}"))?;
    let html = render_document(&document, Some(entry.title.as_str()), None, None)?;
    if let Err(error) = fs::write(&cached, &html) {
        eprintln!("重排缓存写入失败：{error}");
    }
    Ok(html)
}

fn render_document(
    document: &cadmus_core::Document,
    title: Option<&str>,
    author: Option<&str>,
    cover: Option<&str>,
) -> Result<String, String> {
    let meta = cadmus_preview::PreviewMeta {
        title: title.unwrap_or("未命名"),
        byline: author,
        cover,
        asset_src: Some(&asset_data_url),
    };
    cadmus_preview::render_to_string(document, Path::new(""), &meta)
        .map_err(|error| error.to_string())
}

/// 内联图片体积上限：超过则不内联（避免阅读 HTML 失控）。
const MAX_INLINE_ASSET: u64 = 2_500_000;

/// 位图路径 → data URL；不可用（缺失/过大）时返回空串（渲染端会整体省略）。
fn asset_data_url(path: &str) -> String {
    data_url(path, MAX_INLINE_ASSET).unwrap_or_default()
}

/// 文件 → data URL；读取失败或超过 `max_bytes` 返回 `None`（书库封面与正文图片共用）。
pub(crate) fn data_url(path: &str, max_bytes: u64) -> Option<String> {
    let meta = fs::metadata(path).ok()?;
    if meta.len() > max_bytes {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    Some(format!(
        "data:{};base64,{}",
        mime_for(path),
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

/// 扩展名 → MIME（未知格式按 JPEG 兜底）。
pub(crate) fn mime_for(path: &str) -> &'static str {
    let path = path.to_ascii_lowercase();
    if path.ends_with(".png") {
        "image/png"
    } else if path.ends_with(".gif") {
        "image/gif"
    } else if path.ends_with(".svg") {
        "image/svg+xml"
    } else if path.ends_with(".webp") {
        "image/webp"
    } else if path.ends_with(".avif") {
        "image/avif"
    } else if path.ends_with(".bmp") {
        "image/bmp"
    } else {
        "image/jpeg"
    }
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
    // 清掉整份导出资产（封面 + 正文图片）。
    let _ = fs::remove_dir_all(assets_dir(dir, &entry.id));
    if let Some(cover) = &entry.cover {
        // 兼容旧数据：封面可能存放在 covers/<id>/。
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

    /// 章节 XHTML：段落内嵌图（覆盖 `<p>文字<img/></p>` 混排路径）+ 一个缺失图引用。
    const IMAGE_CHAPTER: &str = r#"<?xml version="1.0"?>
<html xmlns="http://www.w3.org/1999/xhtml"><body>
  <h1>第一章 有图</h1>
  <p>前面是图：<img src="pic.png" alt="插"/>后面是字。</p>
  <img src="missing.png" alt="缺失"/>
</body></html>"#;

    /// 最小 EPUB 骨架：给定章节 XHTML 与附加文件（zip 内路径 → 内容）。
    fn build_epub(chapter: &str, files: &[(&str, &[u8])]) -> Vec<u8> {
        use std::io::Write as _;
        let container = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;
        let opf = r#"<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>测试书</dc:title>
  </metadata>
  <manifest>
    <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
    <item id="pic" href="pic.png" media-type="image/png"/>
  </manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#;
        let mut buffer = std::io::Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut buffer);
            let options = zip::write::SimpleFileOptions::default();
            writer.start_file("mimetype", options).unwrap();
            writer.write_all(b"application/epub+zip").unwrap();
            writer
                .start_file("META-INF/container.xml", options)
                .unwrap();
            writer.write_all(container.as_bytes()).unwrap();
            writer.start_file("OEBPS/content.opf", options).unwrap();
            writer.write_all(opf.as_bytes()).unwrap();
            writer.start_file("OEBPS/ch1.xhtml", options).unwrap();
            writer.write_all(chapter.as_bytes()).unwrap();
            for (path, data) in files {
                writer.start_file(*path, options).unwrap();
                writer.write_all(data).unwrap();
            }
            writer.finish().unwrap();
        }
        buffer.into_inner()
    }

    fn tiny_png() -> Vec<u8> {
        use base64::Engine as _;
        base64::engine::general_purpose::STANDARD
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==")
            .unwrap()
    }

    #[test]
    fn imported_images_render_inline() {
        let dir = temp_dir("img");
        let bytes = build_epub(IMAGE_CHAPTER, &[("OEBPS/pic.png", &tiny_png())]);
        let entry = ingest(&dir, &bytes, "epub", "图册", false, Some("img1".into())).unwrap();
        let html = read_entry(&dir, &entry.id).unwrap();
        assert!(
            html.contains("data:image/png;base64,"),
            "段落内图片应内联为 data URL，实际片段：{}",
            &html[..html.len().min(400)]
        );
        assert!(
            html.contains("前面是图") && html.contains("后面是字"),
            "图片前后正文都应保留"
        );
        assert!(
            !html.contains("missing.png"),
            "缺失图片应整体省略，不出现裂图"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn oversize_images_are_omitted_not_broken() {
        let dir = temp_dir("bigimg");
        // 2.6 MB 大图：超过内联上限（2.5 MB）时整块省略。
        let big: Vec<u8> = (0..2_600_000u32).map(|i| (i % 251) as u8).collect();
        let bytes = build_epub(IMAGE_CHAPTER, &[("OEBPS/pic.png", &big)]);
        let entry = ingest(&dir, &bytes, "epub", "大图册", false, Some("big1".into())).unwrap();
        let html = read_entry(&dir, &entry.id).unwrap();
        assert!(!html.contains("<figure"), "超限图片应省略而非裂图");
        assert!(html.contains("后面是字"), "正文不受大图影响");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn footnotes_render_as_asides() {
        let dir = temp_dir("note");
        let chapter = r##"<?xml version="1.0"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><body>
  <h1>第一章 有注</h1>
  <p>这是一句话<a epub:type="noteref" href="#fn1">1</a>末尾。</p>
  <aside epub:type="footnote" id="fn1">脚注正文：解释这句话。<a href="#ref1" epub:type="backlink">↩</a></aside>
</body></html>"##;
        let bytes = build_epub(chapter, &[]);
        let entry = ingest(&dir, &bytes, "epub", "注册", false, Some("note1".into())).unwrap();
        let html = read_entry(&dir, &entry.id).unwrap();
        assert!(html.contains("[1]"), "noteref 标记应可见");
        assert!(
            html.contains("class=\"footnote\""),
            "脚注正文应渲染为 footnote 块"
        );
        assert!(html.contains("脚注正文：解释这句话。"), "脚注内容应完整");
        assert!(!html.contains("↩"), "回链箭头应被剔除");
        let _ = fs::remove_dir_all(&dir);
    }

    /// PDF 重排较慢且依赖本机 pdfium：`cargo test -- --ignored` 手动触发。
    #[test]
    #[ignore = "需要 pdfium 动态库且耗时较长"]
    fn pdf_import_reflow_and_cache() {
        let dir = temp_dir("pdf");
        let fixture =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/1812.01537.pdf");
        let Ok(bytes) = fs::read(&fixture) else {
            eprintln!("跳过：fixture 不存在");
            return;
        };
        let entry = ingest(&dir, &bytes, "pdf", "论文", false, Some("pdf1".into())).unwrap();
        assert_eq!(entry.title, "论文");

        let html = match read_entry(&dir, &entry.id) {
            Ok(html) => html,
            Err(error) if error.contains("pdfium") => {
                eprintln!("跳过：本机无 pdfium（{error}）");
                return;
            }
            Err(error) => panic!("PDF 阅读失败：{error}"),
        };
        assert!(html.contains("<main"), "重排结果应可渲染");
        assert!(
            html.contains("robotics") && html.contains("INTRODUCTION"),
            "重排应提取到正文文本"
        );
        assert!(
            assets_dir(&dir, &entry.id).join(REFLOW_CACHE_FILE).exists(),
            "重排结果应缓存到 assets/<id>/"
        );

        // 缓存命中：二次读取直接返回缓存内容。
        let cached = read_entry(&dir, &entry.id).unwrap();
        assert_eq!(html, cached, "二次读取应命中缓存");
        let _ = fs::remove_dir_all(&dir);
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
