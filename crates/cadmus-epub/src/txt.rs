//! TXT 解析（DESIGN-ENGINE §4.4）：编码探测 → 章节切分 → 块序列。
//!
//! 段落粒度暂定“每个非空行一个段落”（中文小说 TXT 的常见形态，[待定]）。

use std::path::Path;

use cadmus_core::{Block, BlockKind, Document, Result, content_hash, stable_block_id};

use crate::chapters::{Chapter, chunk_by_size, split_chapters};
use crate::encoding::{DecodeReport, decode};

/// TXT 解析选项。
#[derive(Debug, Clone)]
pub struct TxtOptions {
    /// 手动指定编码（覆盖探测），如 `utf-8`、`gb18030`、`big5`。
    pub encoding: Option<String>,
    /// 无章节结构时的兜底分段上限（字节，默认 10 KiB）。
    pub fallback_chunk_bytes: usize,
}

impl Default for TxtOptions {
    fn default() -> Self {
        Self {
            encoding: None,
            fallback_chunk_bytes: 10 * 1024,
        }
    }
}

/// 解析 TXT 文件；`title` 取文件名。
pub fn parse_txt(path: &Path, options: &TxtOptions) -> Result<Document> {
    let bytes = std::fs::read(path)?;
    let (mut document, _report) = parse_txt_bytes(&bytes, options)?;
    if document.title.is_none() {
        document.title = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned());
    }
    Ok(document)
}

/// 解析 TXT 字节；同时返回解码报告（供 CLI 输出 stderr 警告）。
pub fn parse_txt_bytes(bytes: &[u8], options: &TxtOptions) -> Result<(Document, DecodeReport)> {
    let (text, report) = decode(bytes, options.encoding.as_deref())?;
    let found = split_chapters(&text);
    let chapters = if found.is_empty() {
        chunk_by_size(&text, options.fallback_chunk_bytes)
            .into_iter()
            .map(|body| Chapter {
                title: None,
                level: 1,
                body,
            })
            .collect()
    } else {
        found
    };

    let doc_hash = content_hash(bytes);
    let mut blocks = Vec::new();
    let mut order = 0u32;
    for (index, chapter) in chapters.iter().enumerate() {
        let chapter_index = index as u32;
        let mut seq = 0u32;
        if let Some(title) = &chapter.title {
            blocks.push(Block::text(
                stable_block_id(doc_hash, chapter_index, seq),
                BlockKind::Heading {
                    level: chapter.level,
                },
                order,
                title.clone(),
            ));
            order += 1;
            seq += 1;
        }
        for line in chapter.body.lines() {
            if line.trim().is_empty() {
                continue;
            }
            blocks.push(Block::text(
                stable_block_id(doc_hash, chapter_index, seq),
                BlockKind::Paragraph,
                order,
                line.trim(),
            ));
            order += 1;
            seq += 1;
        }
    }

    let document = Document {
        doc_hash,
        title: None,
        blocks,
    };
    Ok((document, report))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_heading_and_paragraph_blocks() {
        let text = "第一章 出发\n他上路了。\n风很大。\n\n第二章 抵达\n到了。\n";
        let bytes = text.as_bytes();
        let (document, report) = parse_txt_bytes(bytes, &TxtOptions::default()).unwrap();

        assert_eq!(report.encoding, "UTF-8");
        assert_eq!(document.doc_hash, content_hash(bytes));
        let kinds: Vec<_> = document.blocks.iter().map(|block| block.kind).collect();
        assert_eq!(
            kinds,
            vec![
                BlockKind::Heading { level: 2 },
                BlockKind::Paragraph,
                BlockKind::Paragraph,
                BlockKind::Heading { level: 2 },
                BlockKind::Paragraph,
            ]
        );
        assert!(
            document
                .blocks
                .windows(2)
                .all(|pair| pair[1].order == pair[0].order + 1)
        );
    }

    #[test]
    fn ids_stable_across_parses() {
        let bytes = "第一章 出发\n内容".as_bytes();
        let (first, _) = parse_txt_bytes(bytes, &TxtOptions::default()).unwrap();
        let (second, _) = parse_txt_bytes(bytes, &TxtOptions::default()).unwrap();
        assert_eq!(first.blocks, second.blocks);
    }

    #[test]
    fn manual_encoding_override() {
        let source = "第一章 测试\n内容文本。";
        let (bytes, _, _) = encoding_rs::GB18030.encode(source);
        let options = TxtOptions {
            encoding: Some("gb18030".into()),
            ..TxtOptions::default()
        };
        let (document, report) = parse_txt_bytes(&bytes, &options).unwrap();
        assert_eq!(report.encoding, "gb18030");
        assert_eq!(document.blocks[0].as_text(), Some("第一章 测试"));
    }

    #[test]
    fn falls_back_to_chunks_without_chapters() {
        let text = "没有章节标记的段落文字。\n".repeat(200);
        let options = TxtOptions {
            fallback_chunk_bytes: 1024,
            ..TxtOptions::default()
        };
        let (document, _) = parse_txt_bytes(text.as_bytes(), &options).unwrap();
        assert!(document.blocks.len() > 1);
        assert!(
            document
                .blocks
                .iter()
                .all(|block| block.kind == BlockKind::Paragraph)
        );
    }
}
