//! PDF → 字形/图像对象（DESIGN-ENGINE §4.2 parse）。

use cadmus_core::{Cancel, Error, Result};
use pdfium_render::prelude::*;

/// 字形框（左上原点，单位 pt）。
#[derive(Debug, Clone)]
pub struct GlyphBox {
    pub ch: char,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub size: f32,
    pub font: String,
}

/// 图/矢量对象包围盒（左上原点，单位 pt）。
#[derive(Debug, Clone, Copy)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// 单页原始数据。
#[derive(Debug, Clone)]
pub struct RawPage {
    pub width: f32,
    pub height: f32,
    pub glyphs: Vec<GlyphBox>,
    pub images: Vec<Rect>,
}

/// 绑定 pdfium 动态库。
///
/// 顺序：`CADMUS_PDFIUM_LIB` 环境变量（目录）> `vendor/pdfium/lib` > 系统库。
pub fn bind_pdfium() -> Result<Pdfium> {
    let bindings = if let Ok(dir) = std::env::var("CADMUS_PDFIUM_LIB") {
        Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(&dir))
    } else if std::path::Path::new("vendor/pdfium/lib").exists() {
        Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(
            "vendor/pdfium/lib",
        ))
    } else {
        Pdfium::bind_to_system_library()
    };
    bindings
        .map(Pdfium::new)
        .map_err(|error| Error::Unsupported(format!("pdfium 加载失败：{error}")))
}

/// 提取单页的字形与图像对象。
pub fn extract_page(page: &PdfPage, cancel: &Cancel) -> Result<RawPage> {
    let width = page.width().value;
    let height = page.height().value;

    let mut glyphs = Vec::new();
    let text = page
        .text()
        .map_err(|error| Error::Parse(format!("文本层读取失败：{error}")))?;
    for character in text.chars().iter() {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        if matches!(
            character.render_mode(),
            Ok(PdfPageTextRenderMode::Invisible)
        ) {
            continue;
        }
        // 旋转字形（水印/戳记）不参与正文流 [待定：阈值 45°]。
        if character
            .angle_degrees()
            .is_ok_and(|angle| angle.abs() > 45.0)
        {
            continue;
        }
        let Some(ch) = character.unicode_char() else {
            continue;
        };
        // 空格等无墨迹字形 tight_bounds 为零尺寸，回退 loose_bounds 取前进宽度。
        let bounds = match character.tight_bounds() {
            Ok(rect) if rect.width().value > 0.01 && rect.height().value > 0.01 => rect,
            _ => {
                let Ok(loose) = character.loose_bounds() else {
                    continue;
                };
                loose
            }
        };
        if bounds.width().value <= 0.0 && bounds.height().value <= 0.0 {
            continue;
        }
        glyphs.push(GlyphBox {
            ch,
            x: bounds.left().value,
            y: height - bounds.top().value,
            w: bounds.width().value,
            h: bounds.height().value,
            size: character.scaled_font_size().value,
            font: character.font_name(),
        });
    }

    let mut images = Vec::new();
    for object in page.objects().iter() {
        if object.object_type() == PdfPageObjectType::Image
            && let Ok(quad) = object.bounds()
        {
            let left = quad.left().value;
            let right = quad.right().value;
            let top = quad.top().value;
            let bottom = quad.bottom().value;
            images.push(Rect {
                x: left,
                y: height - top,
                w: right - left,
                h: top - bottom,
            });
        }
    }

    Ok(RawPage {
        width,
        height,
        glyphs,
        images,
    })
}
