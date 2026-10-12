//! EPUB 解析（DESIGN-ENGINE §4.3）：容器 → OPF → spine 顺序的块序列。
//!
//! 脚注配对：`noteref` 引用的脚注（章节末注或独立注释文件）移动到引用块之后；
//! 独立注释文件中未被引用的脚注不进入正文。

use std::collections::{HashMap, HashSet};
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

    /// 脚注所在：正文块（配对时向前移动）或独立注释池（配对时插入）。
    #[derive(Clone, Copy, PartialEq, Eq, Hash)]
    enum NoteSource {
        Inline(usize),
        Pool(usize),
    }
    let mut blocks: Vec<Block> = Vec::new();
    let mut order = 0u32;
    // noteref 目标（块下标、章节、目标 id）与脚注候选（id → （章节, 所在）），用于配对。
    let mut note_refs: Vec<(usize, u32, Vec<String>)> = Vec::new();
    let mut notes: HashMap<String, Vec<(u32, NoteSource)>> = HashMap::new();
    // 独立注释文件（不在 spine）里的脚注池：仅保留被引用的。
    let mut note_pool: Vec<Block> = Vec::new();

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
                ContentBlock::Text {
                    kind,
                    text,
                    note_refs: refs,
                } => {
                    let block_index = blocks.len();
                    blocks.push(Block::text(
                        stable_block_id(doc_hash, chapter_index, seq),
                        kind,
                        order,
                        text,
                    ));
                    order += 1;
                    seq += 1;
                    if !refs.is_empty() {
                        note_refs.push((block_index, chapter_index, refs));
                    }
                }
                ContentBlock::Footnote { id, text } => {
                    let block_index = blocks.len();
                    blocks.push(Block::text(
                        stable_block_id(doc_hash, chapter_index, seq),
                        BlockKind::Footnote,
                        order,
                        text,
                    ));
                    order += 1;
                    seq += 1;
                    if let Some(id) = id {
                        notes
                            .entry(id)
                            .or_default()
                            .push((chapter_index, NoteSource::Inline(block_index)));
                    }
                }
                ContentBlock::Image { src } => {
                    let image_path = resolve_path(&chapter_dir, &src);
                    let mut bitmap = BitmapRef {
                        path: image_path.clone(),
                        width_px: 0,
                        height_px: 0,
                    };
                    if let Some(out_dir) = &options.out_dir {
                        let file = out_dir.join(format!(
                            "epub-{chapter_index}-{seq}{}",
                            extension_of(&image_path)
                        ));
                        if file.exists() {
                            // 已导出过（内容寻址：同书同章同位同文件），复用而不重写。
                            bitmap.path = file.display().to_string();
                        } else {
                            match read_bytes(&mut archive, &image_path) {
                                Ok(data) => {
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

    // 独立注释文件（manifest 中不在 spine 的 xhtml）：只提取脚注，供 noteref 配对。
    let spine_ids: HashSet<&str> = opf.spine.iter().map(String::as_str).collect();
    for (file_index, item) in opf.manifest.iter().enumerate() {
        if spine_ids.contains(item.id.as_str()) {
            continue;
        }
        if !item.media_type.contains("xhtml") && !item.href.ends_with(".xhtml") {
            continue;
        }
        let href = resolve_path(&base_dir, &item.href);
        let Ok(xhtml) = read_string(&mut archive, &href) else {
            continue;
        };
        let Ok(content_blocks) = content::parse(&xhtml) else {
            continue;
        };
        let chapter_index = opf.spine.len() as u32 + file_index as u32;
        for (seq, content_block) in content_blocks.into_iter().enumerate() {
            let ContentBlock::Footnote { id: Some(id), text } = content_block else {
                continue;
            };
            let pool_index = note_pool.len();
            note_pool.push(Block::text(
                stable_block_id(doc_hash, chapter_index, seq as u32),
                BlockKind::Footnote,
                0,
                text,
            ));
            notes
                .entry(id)
                .or_default()
                .push((chapter_index, NoteSource::Pool(pool_index)));
        }
    }

    // 配对（两轮）：优先同章节匹配（各章 id 常各自从 1 编号），再按 id 全局兜底；
    // 被引用的脚注移动到引用块之后（不区分源位置），未引用的独立注释丢弃。
    let mut after: HashMap<usize, Vec<NoteSource>> = HashMap::new();
    let mut moved: HashSet<NoteSource> = HashSet::new();
    let mut pending: Vec<(usize, String)> = Vec::new();
    for (anchor, chapter, refs) in &note_refs {
        for key in refs {
            let local = notes.get(key).and_then(|candidates| {
                candidates
                    .iter()
                    .find(|(note_chapter, source)| {
                        note_chapter == chapter && !moved.contains(source)
                    })
                    .map(|(_, source)| *source)
            });
            match local {
                Some(source) => {
                    moved.insert(source);
                    after.entry(*anchor).or_default().push(source);
                }
                None => pending.push((*anchor, key.clone())),
            }
        }
    }
    for (anchor, key) in pending {
        // 全局兜底只连注释池（跨文件注释）：避免把别章的 in-spine 脚注错配过来。
        let Some(source) = notes.get(&key).and_then(|candidates| {
            candidates
                .iter()
                .find(|(_, source)| {
                    matches!(source, NoteSource::Pool(_)) && !moved.contains(source)
                })
                .map(|(_, source)| *source)
        }) else {
            continue;
        };
        moved.insert(source);
        after.entry(anchor).or_default().push(source);
    }

    let mut slots: Vec<Option<Block>> = blocks.into_iter().map(Some).collect();
    let mut ordered: Vec<Block> = Vec::with_capacity(slots.len() + note_pool.len());
    for index in 0..slots.len() {
        if moved.contains(&NoteSource::Inline(index)) {
            continue;
        }
        if let Some(block) = slots[index].take() {
            ordered.push(block);
        }
        if let Some(homes) = after.get(&index) {
            for home in homes {
                match home {
                    NoteSource::Inline(note_index) => {
                        if let Some(block) = slots[*note_index].take() {
                            ordered.push(block);
                        }
                    }
                    NoteSource::Pool(pool_index) => ordered.push(note_pool[*pool_index].clone()),
                }
            }
        }
    }
    for (index, block) in ordered.iter_mut().enumerate() {
        block.order = index as u32;
    }

    let document = Document {
        doc_hash,
        title: opf.title,
        blocks: ordered,
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

    /// 同一文件内的章节末注：脚注应移到引用段落之后；独立注释文件仅提取被引用的脚注。
    fn build_footnote_epub() -> Vec<u8> {
        let opf = r#"<?xml version="1.0"?>
        <package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="id">
          <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
            <dc:title>带注书</dc:title>
          </metadata>
          <manifest>
            <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
            <item id="ch2" href="ch2.xhtml" media-type="application/xhtml+xml"/>
            <item id="notes" href="notes.xhtml" media-type="application/xhtml+xml"/>
          </manifest>
          <spine><itemref idref="ch1"/><itemref idref="ch2"/></spine>
        </package>"#;
        let ch1 = r##"<?xml version="1.0"?>
        <html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><body>
          <p>第一段<a epub:type="noteref" href="#n1">1</a>结束。</p>
          <p>第二段。</p>
          <aside epub:type="footnote" id="n1">注一。</aside>
          <aside epub:type="footnote" id="n4">前注。</aside>
          <p>第四段<a epub:type="noteref" href="#n4">4</a>结束。</p>
        </body></html>"##;
        let ch2 = r##"<?xml version="1.0"?>
        <html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><body>
          <p>第三段<a epub:type="noteref" href="#n2">2</a>结束。</p>
        </body></html>"##;
        // 独立注释文件：不在 spine，只有脚注；n3 无人引用。
        let notes = r##"<?xml version="1.0"?>
        <html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><body>
          <aside epub:type="footnote" id="n2">注二。</aside>
          <aside epub:type="footnote" id="n3">注三（应被丢弃）。</aside>
        </body></html>"##;
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
            writer.write_all(opf.as_bytes()).unwrap();
            writer.start_file("OEBPS/ch1.xhtml", options).unwrap();
            writer.write_all(ch1.as_bytes()).unwrap();
            writer.start_file("OEBPS/ch2.xhtml", options).unwrap();
            writer.write_all(ch2.as_bytes()).unwrap();
            writer.start_file("OEBPS/notes.xhtml", options).unwrap();
            writer.write_all(notes.as_bytes()).unwrap();
            writer.finish().unwrap();
        }
        buffer.into_inner()
    }

    #[test]
    fn footnotes_follow_referencing_paragraph() {
        let bytes = build_footnote_epub();
        let (document, _report) = parse_epub_bytes(&bytes, &EpubOptions::default()).unwrap();
        let kinds: Vec<BlockKind> = document.blocks.iter().map(|block| block.kind).collect();
        assert_eq!(
            kinds,
            vec![
                BlockKind::Paragraph,
                BlockKind::Footnote,
                BlockKind::Paragraph,
                BlockKind::Paragraph,
                BlockKind::Footnote,
                BlockKind::Paragraph,
                BlockKind::Footnote,
            ],
            "脚注应移到引用段落之后（含前注与独立注释文件），未引用的脚注丢弃"
        );
        let texts: Vec<Option<&str>> = document
            .blocks
            .iter()
            .map(|block| block.as_text())
            .collect();
        assert_eq!(
            texts,
            vec![
                Some("第一段[1]结束。"),
                Some("注一。"),
                Some("第二段。"),
                Some("第四段[4]结束。"),
                Some("前注。"),
                Some("第三段[2]结束。"),
                Some("注二。"),
            ]
        );
    }

    /// 两章各自拥有同 id（#fn1）的引用：只能各配各的，不跨章错配。
    #[test]
    fn pairs_notes_within_same_chapter_first() {
        let opf = r#"<?xml version="1.0"?>
        <package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="id">
          <metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>双章注</dc:title></metadata>
          <manifest>
            <item id="a" href="a.xhtml" media-type="application/xhtml+xml"/>
            <item id="b" href="b.xhtml" media-type="application/xhtml+xml"/>
          </manifest>
          <spine><itemref idref="a"/><itemref idref="b"/></spine>
        </package>"#;
        let a = r##"<?xml version="1.0"?>
        <html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><body>
          <p>甲<a epub:type="noteref" href="#fn1">1</a>。</p>
        </body></html>"##;
        let b = r##"<?xml version="1.0"?>
        <html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><body>
          <p>乙<a epub:type="noteref" href="#fn1">1</a>。</p>
          <aside epub:type="footnote" id="fn1">乙注。</aside>
        </body></html>"##;
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
            writer.write_all(opf.as_bytes()).unwrap();
            writer.start_file("OEBPS/a.xhtml", options).unwrap();
            writer.write_all(a.as_bytes()).unwrap();
            writer.start_file("OEBPS/b.xhtml", options).unwrap();
            writer.write_all(b.as_bytes()).unwrap();
            writer.finish().unwrap();
        }
        let bytes = buffer.into_inner();
        let (document, _report) = parse_epub_bytes(&bytes, &EpubOptions::default()).unwrap();
        let texts: Vec<Option<&str>> = document
            .blocks
            .iter()
            .map(|block| block.as_text())
            .collect();
        assert_eq!(
            texts,
            vec![Some("甲[1]。"), Some("乙[1]。"), Some("乙注。")],
            "甲的引用不应偷走乙章的脚注"
        );
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
