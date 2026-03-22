use crate::state::ProcessingState;
use crate::types::*;
use picture_tool_core as core;
use std::fs;
use std::path::{Path, PathBuf};

#[tauri::command]
pub fn list_directory(path: String) -> Result<Vec<FileEntry>, String> {
    let dir = Path::new(&path);
    if !dir.is_dir() {
        return Err(format!("Not a directory: {}", path));
    }

    let mut entries = Vec::new();
    let read_dir = fs::read_dir(dir).map_err(|e| e.to_string())?;

    for entry in read_dir.flatten() {
        let file_type = entry.file_type().map_err(|e| e.to_string())?;
        let name = entry.file_name().to_string_lossy().to_string();
        let entry_path = entry.path();
        let path_str = entry_path.to_string_lossy().to_string();

        // 隠しファイル/フォルダーをスキップ
        if name.starts_with('.') {
            continue;
        }

        let is_image = if file_type.is_file() {
            core::is_supported_image(&entry_path)
        } else {
            false
        };

        entries.push(FileEntry {
            name,
            path: path_str,
            is_dir: file_type.is_dir(),
            is_image,
        });
    }

    // フォルダーを先に、ファイルはアルファベット順
    entries.sort_by(|a, b| {
        b.is_dir.cmp(&a.is_dir).then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    Ok(entries)
}

#[tauri::command]
pub fn list_drives() -> Result<Vec<String>, String> {
    #[cfg(target_os = "windows")]
    {
        let mut drives = Vec::new();
        for letter in b'A'..=b'Z' {
            let drive = format!("{}:\\", letter as char);
            if Path::new(&drive).exists() {
                drives.push(drive);
            }
        }
        Ok(drives)
    }

    #[cfg(not(target_os = "windows"))]
    {
        Ok(vec!["/".to_string()])
    }
}

#[tauri::command]
pub async fn list_images(path: String) -> Result<Vec<ImageEntry>, String> {
    let dir = Path::new(&path);
    if !dir.is_dir() {
        return Err(format!("Not a directory: {}", path));
    }

    // 直下の画像のみ取得（再帰しない）
    let read_dir = fs::read_dir(dir).map_err(|e| e.to_string())?;

    let direct_files: Vec<PathBuf> = read_dir
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && core::is_supported_image(p))
        .collect();

    let mut entries = Vec::new();
    for file_path in direct_files {
        let name = file_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let path_str = file_path.to_string_lossy().to_string();

        let (width, height) = match image::image_dimensions(&file_path) {
            Ok(dims) => dims,
            Err(_) => (0, 0),
        };

        let size_bytes = fs::metadata(&file_path)
            .map(|m| m.len())
            .unwrap_or(0);

        entries.push(ImageEntry {
            name,
            path: path_str,
            width,
            height,
            size_bytes,
            thumbnail_base64: None, // 遅延読み込み
        });
    }

    entries.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    Ok(entries)
}

#[tauri::command]
pub async fn get_thumbnail(path: String) -> Result<String, String> {
    core::generate_thumbnail_base64(Path::new(&path), 200)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn process_images(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, ProcessingState>,
    files: Vec<String>,
    output_folder: String,
    config: core::ProcessingConfig,
) -> Result<Vec<core::ProcessResult>, String> {
    Err("Not implemented".to_string()) // Task 6で実装
}

#[tauri::command]
pub fn cancel_processing(state: tauri::State<'_, ProcessingState>) -> Result<(), String> {
    Ok(()) // Task 6で実装
}

// 注: pick_folderはTauriコマンドとしては実装しない。
// フロントエンドから@tauri-apps/plugin-dialogのopen()を直接呼び出す。
