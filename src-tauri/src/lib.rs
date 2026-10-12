//! cadmus App：Tauri 壳 + 阅读引擎接入。
//!
//! 当前能力：打开内置示例书（EPUB → 块序列 → 阅读 HTML）。
//! 后续：从系统导入本地文件（EPUB/PDF/TXT）。

use std::path::Path;

use serde::Serialize;

#[derive(Serialize)]
struct SampleBook {
    title: String,
    html: String,
}

/// 打开内置示例书，返回可阅读的 HTML（含主题/字号/目录）。
#[tauri::command]
fn read_sample_book() -> Result<SampleBook, String> {
    let bytes: &[u8] = include_bytes!("../sample/sample.epub");
    let (document, report) =
        cadmus_epub::parse_epub_bytes(bytes, &cadmus_epub::EpubOptions::default())
            .map_err(|error| error.to_string())?;

    let title = document
        .title
        .clone()
        .unwrap_or_else(|| "示例书".to_string());
    let byline = report.creators.join("、");
    let meta = cadmus_preview::PreviewMeta {
        title: &title,
        byline: if byline.is_empty() {
            None
        } else {
            Some(byline.as_str())
        },
        cover: None,
    };
    let html = cadmus_preview::render_to_string(&document, Path::new(""), &meta)
        .map_err(|error| error.to_string())?;

    Ok(SampleBook { title, html })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![read_sample_book])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
