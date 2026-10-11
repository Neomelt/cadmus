//! EPUB 解析（DESIGN-ENGINE §4.3）：容器 → OPF → spine 顺序的块序列。

use std::io::{Cursor, Read, Seek};
use std::path::{Path, PathBuf};

use cadmus_core::{
    BitmapRef, Block, BlockKind, Document, Error, Payload, Result, content_hash, stable_block_id,
};
use zip::ZipArchive;

use crate::content::{self, ContentBlock};
use crate::{container, opf};

/// EPUB 解析选项。
#[derive(Debug, Clone, Default)]
pub struct EpubOptions {
    /// 图片导出目录；`None` 时 BitmapRef 指向包内路径而不落盘。
    pub out_dir: Option<PathBuf>,
}

/// 解析报告。
#[derive(Debug, Clone, Default)]
pub struct EpubReport {
    pub title: Option<String>,
    pub language: Option<String>,
    /// 作者（dc:creator）。
    pub creators: Vec<String>,
    /// 导出的封面文件路径（未导出时为 None）。
    pub cover_path: Option<String>,
    pub chapters: usize,
    /// 跳过/降级的警告（CLI 输出到 stderr）。
    pub warnings: Vec<String>,
}

/// 解析 EPUB 文件。
pub fn parse_epub(path: &Path, options: &EpubOptions) -> Result<(Document, EpubReport)> {
    let bytes = std::fs::read(path)?;
    parse_epub_bytes(&bytes, options)
}

/// 解析 EPUB 字节。
pub fn parse_epub_bytes(bytes: &[u8], options: &EpubOptions) -> Result<(Document, EpubReport)> {
    let doc_hash = content_hash(bytes);
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| Error::Parse(format!("EPUB（zip）打开失败：{error}")))?;

    let container_xml = read_string(&mut archive, "META-INF/container.xml")?;
    let opf_path = container::rootfile_path(&container_xml)?;
    let opf_xml = read_string(&mut archive, &opf_path)?;
    let opf = opf::parse(&opf_xml)?;

    let base_dir = opf_path
        .rsplit_once('/')
        .map(|(dir, _)| dir.to_string())
        .unwrap_or_default();

    let mut report = EpubReport {
        title: opf.title.clone(),
        language: opf.language.clone(),
        creators: opf.creators.clone(),
        cover_path: None,
        chapters: 0,
        warnings: Vec::new(),
    };
    if let Some(out_dir) = &options.out_dir {
        std::fs::create_dir_all(out_dir)?;
    }

    // 封面导出（可选，见 DESIGN-ENGINE §4.3）。
    if let Some(id) = opf.cover_id.clone()
        && let Some(item) = opf.manifest.iter().find(|item| item.id == id)
    {
        let href = resolve_path(&base_dir, &item.href);
        match read_bytes(&mut archive, &href) {
            Ok(data) => {
                if let Some(out_dir) = &options.out_dir {
                    let file = out_dir.join(format!("cover{}", extension_of(&href)));
                    match std::fs::write(&file, data) {
                        Ok(()) => report.cover_path = Some(file.display().to_string()),
                        Err(error) => report.warnings.push(format!("封面导出失败：{error}")),
                    }
                }
            }
            Err(_) => report.warnings.push(format!("封面缺失：{href}")),
        }
    }

    let mut blocks: Vec<Block> = Vec::new();
    let mut order = 0u32;

    for (spine_index, idref) in opf.spine.iter().enumerate() {
        let Some(item) = opf.manifest.iter().find(|item| &item.id == idref) else {
            report.warnings.push(format!("spine 引用缺失：{idref}"));
            continue;
        };
        if !item.media_type.contains("xhtml") && !item.href.ends_with(".xhtml") {
            continue;
        }
        let href = resolve_path(&base_dir, &item.href);
        let Ok(xhtml) = read_string(&mut archive, &href) else {
            report.warnings.push(format!("章节读取失败：{href}"));
            continue;
        };
        let Ok(content_blocks) = content::parse(&xhtml) else {
            report.warnings.push(format!("章节解析失败：{href}"));
            continue;
        };

        report.chapters += 1;
        let chapter_index = spine_index as u32;
        let chapter_dir = href
            .rsplit_once('/')
            .map(|(dir, _)| dir.to_string())
            .unwrap_or_else(|| base_dir.clone());
        let mut seq = 0u32;

        for content_block in content_blocks {
            match content_block {
                ContentBlock::Text { kind, text } => {
                    blocks.push(Block::text(
                        stable_block_id(doc_hash, chapter_index, seq),
                        kind,
                        order,
                        text,
                    ));
                    order += 1;
                    seq += 1;
                }
                ContentBlock::Image { src } => {
                    let image_path = resolve_path(&chapter_dir, &src);
                    let mut bitmap = BitmapRef {
                        path: image_path.clone(),
                        width_px: 0,
                        height_px: 0,
                    };
                    if let Some(out_dir) = &options.out_dir {
                        match read_bytes(&mut archive, &image_path) {
                            Ok(data) => {
                                let file = out_dir.join(format!(
                                    "epub-{chapter_index}-{seq}{}",
                                    extension_of(&image_path)
                                ));
                                if let Err(error) = std::fs::write(&file, data) {
                                    report.warnings.push(format!("图片导出失败：{error}"));
                                } else {
                                    bitmap.path = file.display().to_string();
                                }
                            }
                            Err(_) => {
                                report.warnings.push(format!("图片缺失：{image_path}"));
                            }
                        }
                    }
                    blocks.push(Block {
                        id: stable_block_id(doc_hash, chapter_index, seq),
                        kind: BlockKind::Figure,
                        order,
                        page_range: None,
                        payload: Payload::Bitmap(bitmap),
                    });
                    order += 1;
                    seq += 1;
                }
            }
        }
    }

    let document = Document {
        doc_hash,
        title: opf.title,
        blocks,
    };
    Ok((document, report))
}

fn read_string<R: Read + Seek>(archive: &mut ZipArchive<R>, name: &str) -> Result<String> {
    let mut file = archive
        .by_name(name)
        .map_err(|error| Error::Parse(format!("zip 条目缺失：{name}（{error}）")))?;
    let mut out = String::new();
    file.read_to_string(&mut out)?;
    Ok(out)
}

fn read_bytes<R: Read + Seek>(archive: &mut ZipArchive<R>, name: &str) -> Result<Vec<u8>> {
    let mut file = archive
        .by_name(name)
        .map_err(|error| Error::Parse(format!("zip 条目缺失：{name}（{error}）")))?;
    let mut out = Vec::new();
    file.read_to_end(&mut out)?;
    Ok(out)
}

/// 解析相对路径（处理 `.` / `..`；不做百分号解码 [待定]）。
fn resolve_path(base: &str, href: &str) -> String {
    if href.starts_with('/') {
        return href.trim_start_matches('/').to_string();
    }
    let mut parts: Vec<&str> = if base.is_empty() {
        Vec::new()
    } else {
        base.split('/').collect()
    };
    for segment in href.split('/') {
        match segment {
            "." | "" => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

fn extension_of(path: &str) -> String {
    path.rsplit_once('.')
        .map(|(_, extension)| format!(".{extension}"))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    const CONTAINER: &str = r#"<?xml version="1.0"?>
    <container version="1.0">
      <rootfiles>
        <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
      </rootfiles>
    </container>"#;

    const OPF: &str = r#"<?xml version="1.0"?>
    <package xmlns="http://www.idpf.org/2007/opf" version="3.0">
      <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
        <dc:title>测试书</dc:title><dc:language>zh</dc:language>
        <dc:creator>作者甲</dc:creator>
      </metadata>
      <manifest>
        <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
      </manifest>
      <spine><itemref idref="ch1"/></spine>
    </package>"#;

    const CHAPTER: &str = r#"<?xml version="1.0"?>
    <html xmlns="http://www.w3.org/1999/xhtml"><body>
      <h1>第一章</h1><p>你好，世界。</p><img src="pic.png"/><pre>fn main() {}</pre>
    </body></html>"#;

    fn build_epub() -> Vec<u8> {
        let mut buffer = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut buffer);
            let options = zip::write::SimpleFileOptions::default();
            writer.start_file("mimetype", options).unwrap();
            writer.write_all(b"application/epub+zip").unwrap();
            writer
                .start_file("META-INF/container.xml", options)
                .unwrap();
            writer.write_all(CONTAINER.as_bytes()).unwrap();
            writer.start_file("OEBPS/content.opf", options).unwrap();
            writer.write_all(OPF.as_bytes()).unwrap();
            writer.start_file("OEBPS/ch1.xhtml", options).unwrap();
            writer.write_all(CHAPTER.as_bytes()).unwrap();
            writer.finish().unwrap();
        }
        buffer.into_inner()
    }

    #[test]
    fn parses_minimal_epub() {
        let bytes = build_epub();
        let (document, report) = parse_epub_bytes(&bytes, &EpubOptions::default()).unwrap();
        assert_eq!(document.title.as_deref(), Some("测试书"));
        assert_eq!(report.creators, vec!["作者甲"]);
        assert_eq!(report.chapters, 1);
        let kinds: Vec<BlockKind> = document.blocks.iter().map(|block| block.kind).collect();
        assert_eq!(
            kinds,
            vec![
                BlockKind::Heading { level: 1 },
                BlockKind::Paragraph,
                BlockKind::Figure,
                BlockKind::Code,
            ]
        );
        assert_eq!(document.blocks[1].as_text(), Some("你好，世界。"));
        assert!(report.warnings.is_empty()); // 未请求图片导出时不报缺失
    }

    #[test]
    fn resolves_relative_paths() {
        assert_eq!(resolve_path("OEBPS", "ch1.xhtml"), "OEBPS/ch1.xhtml");
        assert_eq!(
            resolve_path("OEBPS/text", "../img/a.png"),
            "OEBPS/img/a.png"
        );
        assert_eq!(resolve_path("", "/abs/x.png"), "abs/x.png");
    }
}
