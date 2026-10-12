//! cadmus App：Tauri 壳 + 阅读引擎 + 书库。
//!
//! 能力：导入（EPUB/TXT）→ 书库管理（列表/删除/进度）→ 阅读（主题/字号/行距/行宽/字体/目录）。

use std::path::PathBuf;

use serde::Serialize;
use tauri::Manager;

mod annotations;
mod library;

#[derive(Serialize)]
struct BookView {
    id: String,
    title: String,
    author: Option<String>,
    format: String,
    size: u64,
    added_at: u64,
    last_read_at: u64,
    progress: f32,
    anchor_block: Option<String>,
    anchor_offset: f32,
    builtin: bool,
    /// 封面数据 URL（可能没有或过大而省略）。
    cover_data: Option<String>,
}

fn to_view(dir: &std::path::Path, entry: &library::BookEntry) -> BookView {
    // 列表封面给较小上限（400 KB），避免书库一次性铺满内存。
    let cover_data = entry
        .cover
        .as_ref()
        .map(|cover| library::resolve_stored(dir, cover))
        .and_then(|path| library::data_url(&path.display().to_string(), 400_000));
    BookView {
        id: entry.id.clone(),
        title: entry.title.clone(),
        author: entry.author.clone(),
        format: entry.format.clone(),
        size: entry.size,
        added_at: entry.added_at,
        last_read_at: entry.last_read_at,
        progress: entry.progress,
        anchor_block: entry.anchor_block.clone(),
        anchor_offset: entry.anchor_offset,
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
    Ok(library::load(&dir)
        .iter()
        .map(|entry| to_view(&dir, entry))
        .collect())
}

#[tauri::command]
fn import_book(app: tauri::AppHandle, path: String) -> Result<BookView, String> {
    let dir = data_dir(&app)?;
    let picked = path;
    let ext = std::path::Path::new(&picked)
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| format!("无法识别文件类型：{picked}"))?;
    match ext.as_str() {
        "epub" | "txt" | "pdf" => {}
        other => return Err(format!("暂不支持格式：{other}")),
    }
    let bytes = read_picked(&picked)?;
    let fallback = std::path::Path::new(picked.rsplit('/').next().unwrap_or(&picked))
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "未命名".to_string());
    let entry = library::ingest(&dir, &bytes, &ext, &fallback, false, None)?;
    Ok(to_view(&dir, &entry))
}

/// 读取文件选择器带回的文件。
///
/// iOS：选择器返回的通常是 `file://` 形式的沙箱外路径，必须先开启
/// security-scoped 访问权限才能读（与官方 fs 插件一致的官方做法）。
#[cfg(target_os = "ios")]
fn read_picked(picked: &str) -> Result<Vec<u8>, String> {
    use objc2_foundation::{NSString, NSURL};

    if !picked.starts_with("file:") {
        // 沙箱内纯路径，直接读。
        return std::fs::read(picked).map_err(|error| format!("无法读取文件（{picked}）：{error}"));
    }

    let ns_url = NSURL::URLWithString(&NSString::from_str(picked));
    if let Some(url) = &ns_url {
        unsafe {
            url.startAccessingSecurityScopedResource();
        }
    }

    let path = url::Url::parse(picked)
        .ok()
        .and_then(|parsed| parsed.to_file_path().ok())
        .ok_or_else(|| format!("无法解析文件 URL：{picked}"))?;
    let result = std::fs::read(&path)
        .map_err(|error| format!("无法读取文件（{}）：{error}", path.display()));

    if let Some(url) = &ns_url {
        unsafe {
            url.stopAccessingSecurityScopedResource();
        }
    }
    result
}

#[cfg(not(target_os = "ios"))]
fn read_picked(picked: &str) -> Result<Vec<u8>, String> {
    let path = picked.strip_prefix("file://").unwrap_or(picked);
    std::fs::read(path).map_err(|error| format!("无法读取文件（{path}）：{error}"))
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
fn save_progress(
    app: tauri::AppHandle,
    id: String,
    progress: f32,
    anchor_block: Option<String>,
    anchor_offset: Option<f32>,
) -> Result<(), String> {
    let dir = data_dir(&app)?;
    library::set_progress(&dir, &id, progress, anchor_block.as_deref(), anchor_offset)
}

#[tauri::command]
fn search_book(
    app: tauri::AppHandle,
    id: String,
    query: String,
) -> Result<Vec<library::SearchHit>, String> {
    let dir = data_dir(&app)?;
    library::search(&dir, &id, &query)
}

#[tauri::command]
fn list_annotations(
    app: tauri::AppHandle,
    id: String,
) -> Result<annotations::AnnotationStore, String> {
    let dir = data_dir(&app)?;
    Ok(annotations::load(&dir, &id))
}

#[tauri::command]
fn add_annotation(
    app: tauri::AppHandle,
    id: String,
    block_id: String,
    start: u32,
    end: u32,
    text: String,
    color: String,
) -> Result<annotations::Annotation, String> {
    let dir = data_dir(&app)?;
    annotations::add(&dir, &id, &block_id, start, end, &text, &color)
}

#[tauri::command]
fn update_annotation(
    app: tauri::AppHandle,
    id: String,
    ann_id: String,
    color: Option<String>,
    note: Option<String>,
) -> Result<annotations::Annotation, String> {
    let dir = data_dir(&app)?;
    annotations::update(&dir, &id, &ann_id, color.as_deref(), note.as_deref())
}

#[tauri::command]
fn delete_annotation(app: tauri::AppHandle, id: String, ann_id: String) -> Result<(), String> {
    let dir = data_dir(&app)?;
    annotations::delete(&dir, &id, &ann_id)
}

#[tauri::command]
fn resize_annotation(
    app: tauri::AppHandle,
    id: String,
    ann_id: String,
    start: u32,
    end: u32,
    text: String,
) -> Result<annotations::Annotation, String> {
    let dir = data_dir(&app)?;
    annotations::resize(&dir, &id, &ann_id, start, end, &text)
}

#[tauri::command]
fn add_bookmark(
    app: tauri::AppHandle,
    id: String,
    block_id: Option<String>,
    label: String,
    progress: f32,
) -> Result<annotations::Bookmark, String> {
    let dir = data_dir(&app)?;
    annotations::add_bookmark(&dir, &id, block_id.as_deref(), &label, progress)
}

#[tauri::command]
fn delete_bookmark(app: tauri::AppHandle, id: String, bookmark_id: String) -> Result<(), String> {
    let dir = data_dir(&app)?;
    annotations::delete_bookmark(&dir, &id, &bookmark_id)
}

#[tauri::command]
fn export_markdown(app: tauri::AppHandle, id: String) -> Result<String, String> {
    let dir = data_dir(&app)?;
    let export_dir = app
        .path()
        .document_dir()
        .map_err(|error| format!("无法定位导出目录：{error}"))?
        .join("cadmus-exports");
    library::export_markdown(&dir, &id, &export_dir)
}

#[tauri::command]
fn pdf_page(app: tauri::AppHandle, id: String, page: u32) -> Result<PageView, String> {
    let dir = data_dir(&app)?;
    let (src, total) = library::pdf_page(&dir, &id, page)?;
    Ok(PageView { src, total, page })
}

#[derive(Serialize)]
struct PageView {
    src: String,
    total: u32,
    page: u32,
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
            save_progress,
            search_book,
            list_annotations,
            add_annotation,
            update_annotation,
            delete_annotation,
            resize_annotation,
            add_bookmark,
            delete_bookmark,
            export_markdown,
            pdf_page
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
