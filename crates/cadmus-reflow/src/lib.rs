//! cadmus PDF 重排引擎（DESIGN-ENGINE §4.2）。
//!
//! 管线：parse → lines → columns → paragraphs → regions → order → atomic/raster。
//! 页级降级：无文本层或版面把握不足时输出整页位图。

pub mod columns;
pub mod lines;
pub mod order;
pub mod paragraphs;
pub mod parse;
pub mod raster;
pub mod regions;

use std::path::{Path, PathBuf};

use cadmus_core::BlockKind;
use cadmus_core::{
    BitmapRef, Block, Cancel, Document, Error, Payload, Result, content_hash, stable_block_id,
};

use columns::ColumnKind;
use order::Item;
use regions::RegionKind;

/// 原子块/降级页的栅格化倍率（相对 PDF 点）。
const RASTER_SCALE: f32 = 2.0;

/// 重排选项。
#[derive(Debug, Clone)]
pub struct ReflowOptions {
    /// 页范围（1 起，含端点）；`None` 表示全部。
    pub page_range: Option<(u32, u32)>,
    /// 位图输出目录（原子块与降级页）。
    pub out_dir: PathBuf,
}

impl ReflowOptions {
    pub fn new(out_dir: impl Into<PathBuf>) -> Self {
        Self {
            page_range: None,
            out_dir: out_dir.into(),
        }
    }
}

/// 渲染 PDF 单页为 PNG 文件（原文页视图用）。返回（宽, 高）像素。
pub fn render_page_to_png(
    path: &Path,
    page_number: u32,
    scale: f32,
    out: &Path,
) -> Result<(u32, u32)> {
    let pdfium = parse::bind_pdfium().map_err(Error::Unsupported)?;
    let document = pdfium
        .load_pdf_from_file(path, None)
        .map_err(|error| Error::Parse(format!("PDF 打开失败：{error}")))?;
    let page = document
        .pages()
        .get(page_number as i32 - 1)
        .map_err(|error| Error::Parse(format!("页读取失败：{error}")))?;
    let raster = raster::render_page(&page, scale)?;
    raster.save_all(out)?;
    Ok((raster.width_px(), raster.height_px()))
}

/// 读取 PDF 元数据（标题/作者）。无 pdfium 或读取失败返回 `None`（调用方回退文件名）。
pub fn pdf_metadata(path: &Path) -> Option<(Option<String>, Option<String>)> {
    let pdfium = parse::bind_pdfium().ok()?;
    let document = pdfium.load_pdf_from_file(path, None).ok()?;
    let metadata = document.metadata();
    let title = metadata
        .get(pdfium_render::prelude::PdfDocumentMetadataTagType::Title)
        .map(|tag| tag.value().trim().to_string())
        .filter(|value| !value.is_empty());
    let author = metadata
        .get(pdfium_render::prelude::PdfDocumentMetadataTagType::Author)
        .map(|tag| tag.value().trim().to_string())
        .filter(|value| !value.is_empty());
    Some((title, author))
}

/// 将 PDF 重排为块序列。
pub fn reflow(path: &Path, options: &ReflowOptions, cancel: &Cancel) -> Result<Document> {
    let doc_hash = content_hash(&std::fs::read(path)?);
    std::fs::create_dir_all(&options.out_dir)?;

    let pdfium = parse::bind_pdfium().map_err(Error::Unsupported)?;
    let document = pdfium
        .load_pdf_from_file(path, None)
        .map_err(|error| Error::Parse(format!("PDF 打开失败：{error}")))?;
    let total = document.pages().len() as u32;
    let (start, end) = options.page_range.unwrap_or((1, total));
    let end = end.min(total);

    let mut blocks = Vec::new();
    let mut order = 0u32;

    for page_number in start..=end {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let index = page_number - 1;
        let page = document
            .pages()
            .get(index as i32)
            .map_err(|error| Error::Parse(format!("第 {page_number} 页读取失败：{error}")))?;
        let raw = parse::extract_page(&page, cancel)?;

        // 无文本层 → 整页位图降级。
        if raw.glyphs.is_empty() {
            push_page_bitmap(
                &mut blocks,
                &page,
                &options.out_dir,
                page_number,
                index,
                doc_hash,
                &mut order,
            )?;
            continue;
        }

        let text_lines = lines::build_lines(&raw.glyphs);
        let layout = columns::detect(raw.width, &text_lines);

        // 版面把握不足 → 整页位图降级。
        if layout.kind == ColumnKind::Multi {
            push_page_bitmap(
                &mut blocks,
                &page,
                &options.out_dir,
                page_number,
                index,
                doc_hash,
                &mut order,
            )?;
            continue;
        }

        let mut paragraphs = match layout.cut {
            Some(cut) => {
                let mut all = paragraphs::build((0.0, cut), &text_lines);
                all.extend(paragraphs::build((cut, raw.width), &text_lines));
                all
            }
            None => paragraphs::build((0.0, raw.width), &text_lines),
        };
        paragraphs.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal));

        let body_size = regions::body_size(&paragraphs);
        let kinds: Vec<RegionKind> = paragraphs
            .iter()
            .map(|paragraph| regions::classify(paragraph, body_size))
            .collect();

        let arrangement = order::arrange(&order::OrderInput {
            paragraphs: &paragraphs,
            images: &raw.images,
            cut: layout.cut,
        });

        let needs_raster = !raw.images.is_empty() || kinds.contains(&RegionKind::Formula);
        let page_raster = if needs_raster {
            Some(raster::render_page(&page, RASTER_SCALE)?)
        } else {
            None
        };

        for (seq, item) in arrangement.into_iter().enumerate() {
            let seq = seq as u32;
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            let (kind, payload) = match item {
                Item::Paragraph(i) => match kinds[i] {
                    RegionKind::Body => (
                        BlockKind::Paragraph,
                        Payload::Text(paragraphs[i].text.clone()),
                    ),
                    RegionKind::Code => {
                        (BlockKind::Code, Payload::Text(paragraphs[i].text.clone()))
                    }
                    RegionKind::Heading { level } => (
                        BlockKind::Heading { level },
                        Payload::Text(paragraphs[i].text.clone()),
                    ),
                    RegionKind::Formula => {
                        let paragraph = &paragraphs[i];
                        let raster = page_raster.as_ref().expect("needs_raster 保证");
                        let file = options.out_dir.join(format!("p{page_number}-f{seq}.png"));
                        let (w, h) = raster.crop_save(
                            (
                                paragraph.x - 2.0,
                                paragraph.y - 2.0,
                                paragraph.w + 4.0,
                                paragraph.h + 4.0,
                            ),
                            &file,
                        )?;
                        (
                            BlockKind::Formula,
                            Payload::Bitmap(BitmapRef {
                                path: file.display().to_string(),
                                width_px: w,
                                height_px: h,
                            }),
                        )
                    }
                },
                Item::Image(i) => {
                    let rect = raw.images[i];
                    let raster = page_raster.as_ref().expect("needs_raster 保证");
                    let file = options.out_dir.join(format!("p{page_number}-i{seq}.png"));
                    let (w, h) = raster.crop_save(
                        (rect.x - 2.0, rect.y - 2.0, rect.w + 4.0, rect.h + 4.0),
                        &file,
                    )?;
                    (
                        BlockKind::Figure,
                        Payload::Bitmap(BitmapRef {
                            path: file.display().to_string(),
                            width_px: w,
                            height_px: h,
                        }),
                    )
                }
            };
            blocks.push(Block {
                id: stable_block_id(doc_hash, index, seq),
                kind,
                order,
                page_range: Some((page_number, page_number)),
                payload,
            });
            order += 1;
        }
    }

    Ok(Document {
        doc_hash,
        title: None,
        blocks,
    })
}

fn push_page_bitmap(
    blocks: &mut Vec<Block>,
    page: &pdfium_render::prelude::PdfPage,
    out_dir: &Path,
    page_number: u32,
    index: u32,
    doc_hash: u64,
    order: &mut u32,
) -> Result<()> {
    let raster = raster::render_page(page, RASTER_SCALE)?;
    let file = out_dir.join(format!("p{page_number}-fallback.png"));
    raster.save_all(&file)?;
    blocks.push(
        Block::bitmap(
            stable_block_id(doc_hash, index, 0),
            BlockKind::PageBitmap,
            *order,
            BitmapRef {
                path: file.display().to_string(),
                width_px: raster.width_px(),
                height_px: raster.height_px(),
            },
        )
        .with_page_range((page_number, page_number)),
    );
    *order += 1;
    Ok(())
}
