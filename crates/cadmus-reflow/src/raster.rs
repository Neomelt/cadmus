//! 页/区域栅格化（DESIGN-ENGINE §4.2 atomic/raster）。

use std::path::Path;

use cadmus_core::{Error, Result};
use image::DynamicImage;
use pdfium_render::prelude::*;

/// 一页的整页位图，供区域裁剪复用。
pub struct PageRaster {
    image: DynamicImage,
    scale: f32,
}

/// 渲染整页（`scale` 为相对 PDF 点的倍率）。
pub fn render_page(page: &PdfPage, scale: f32) -> Result<PageRaster> {
    let page_width = page.width().value;
    let target_width = (page_width * scale).round().max(1.0) as i32;
    let config = PdfRenderConfig::new().set_target_width(target_width);
    let bitmap = page
        .render_with_config(&config)
        .map_err(|error| Error::Parse(format!("页渲染失败：{error}")))?;
    let image = bitmap
        .as_image()
        .map_err(|error| Error::Parse(format!("位图转换失败：{error}")))?;
    let actual_scale = image.width() as f32 / page_width;
    Ok(PageRaster {
        image,
        scale: actual_scale,
    })
}

impl PageRaster {
    pub fn width_px(&self) -> u32 {
        self.image.width()
    }

    pub fn height_px(&self) -> u32 {
        self.image.height()
    }

    /// 裁剪矩形（左上原点，pt）并保存 PNG；返回像素尺寸。
    pub fn crop_save(&self, rect: (f32, f32, f32, f32), out: &Path) -> Result<(u32, u32)> {
        let x = (rect.0 * self.scale).floor().max(0.0) as u32;
        let y = (rect.1 * self.scale).floor().max(0.0) as u32;
        let w = (rect.2 * self.scale).ceil().max(1.0) as u32;
        let h = (rect.3 * self.scale).ceil().max(1.0) as u32;
        let x2 = (x + w).min(self.image.width());
        let y2 = (y + h).min(self.image.height());
        if x2 <= x || y2 <= y {
            return Err(Error::Parse("裁剪区域越界".into()));
        }
        let cropped = self.image.crop_imm(x, y, x2 - x, y2 - y);
        cropped
            .save_with_format(out, image::ImageFormat::Png)
            .map_err(|error| Error::Parse(format!("位图保存失败：{error}")))?;
        Ok((x2 - x, y2 - y))
    }

    /// 保存整页位图。
    pub fn save_all(&self, out: &Path) -> Result<()> {
        self.image
            .save_with_format(out, image::ImageFormat::Png)
            .map_err(|error| Error::Parse(format!("位图保存失败：{error}")))?;
        Ok(())
    }
}
