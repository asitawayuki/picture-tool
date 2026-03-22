use crate::state::ProcessingState;
use crate::types::*;
use picture_tool_core as core;

#[tauri::command]
pub fn list_directory(path: String) -> Result<Vec<FileEntry>, String> {
    Ok(vec![]) // Task 5で実装
}

#[tauri::command]
pub fn list_drives() -> Result<Vec<String>, String> {
    Ok(vec![]) // Task 5で実装
}

#[tauri::command]
pub async fn list_images(path: String) -> Result<Vec<ImageEntry>, String> {
    Ok(vec![]) // Task 5で実装
}

#[tauri::command]
pub async fn get_thumbnail(path: String) -> Result<String, String> {
    Err("Not implemented".to_string()) // Task 5で実装
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
