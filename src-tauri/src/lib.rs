//! cadmus App：Tauri 壳 + 阅读引擎 + 书库。
//!
//! 能力：导入（EPUB/TXT）→ 书库管理（列表/删除/进度）→ 阅读（主题/字号/行距/行宽/字体/目录）。

use std::path::PathBuf;

use base64::Engine as _;
use serde::Serialize;
use tauri::Manager;

mod library;

#[derive(Serialize)]
struct BookView {
    id: String,
    title: String,
    author: Option<String>,
    format: String,
    size: u64,
    added_at: u64,
    progress: f32,
    builtin: bool,
    /// 封面数据 URL（可能没有或过大而省略）。
    cover_data: Option<String>,
}

fn to_view(entry: &library::BookEntry) -> BookView {
    let cover_data = entry.cover.as_ref().and_then(|path| {
        let bytes = std::fs::read(path).ok()?;
        if bytes.len() > 400_000 {
            return None;
        }
        let mime = if path.to_ascii_lowercase().ends_with(".png") {
            "image/png"
        } else {
            "image/jpeg"
        };
        Some(format!(
            "data:{mime};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        ))
    });
    BookView {
        id: entry.id.clone(),
        title: entry.title.clone(),
        author: entry.author.clone(),
        format: entry.format.clone(),
        size: entry.size,
        added_at: entry.added_at,
        progress: entry.progress,
        builtin: entry.builtin,
        cover_data,
    }
}

fn data_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("无法解析数据目录：{error}"))?;
    library::seed_sample(&dir)?;
    Ok(dir)
}

#[tauri::command]
fn list_books(app: tauri::AppHandle) -> Result<Vec<BookView>, String> {
    let dir = data_dir(&app)?;
    Ok(library::load(&dir).iter().map(to_view).collect())
}

#[tauri::command]
fn import_book(app: tauri::AppHandle, path: String) -> Result<BookView, String> {
    let dir = data_dir(&app)?;
    let source = PathBuf::from(&path);
    let ext = source
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    match ext.as_str() {
        "epub" | "txt" => {}
        "pdf" => return Err("PDF 阅读将在下一版支持（先导入 EPUB / TXT）".into()),
        other => return Err(format!("暂不支持格式：{other}")),
    }
    let bytes = std::fs::read(&source).map_err(|error| format!("无法读取文件：{error}"))?;
    let fallback = source
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "未命名".to_string());
    let entry = library::ingest(&dir, &bytes, &ext, &fallback, false, None)?;
    Ok(to_view(&entry))
}

#[tauri::command]
fn read_book(app: tauri::AppHandle, id: String) -> Result<String, String> {
    let dir = data_dir(&app)?;
    library::read_entry(&dir, &id)
}

#[tauri::command]
fn delete_book(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let dir = data_dir(&app)?;
    library::delete_entry(&dir, &id)
}

#[tauri::command]
fn save_progress(app: tauri::AppHandle, id: String, progress: f32) -> Result<(), String> {
    let dir = data_dir(&app)?;
    library::set_progress(&dir, &id, progress)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            list_books,
            import_book,
            read_book,
            delete_book,
            save_progress
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
