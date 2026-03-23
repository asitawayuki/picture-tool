use crate::state::ProcessingState;
use crate::types::*;
use picture_tool_core as core;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tauri::Emitter;

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
pub async fn get_thumbnail(
    state: tauri::State<'_, ProcessingState>,
    path: String,
) -> Result<String, String> {
    // キャッシュ確認
    {
        let mut cache = state.thumbnail_cache.lock().unwrap();
        if let Some(cached) = cache.get(&path) {
            return Ok(cached.clone());
        }
    }

    // キャッシュミス: 生成
    let result = core::generate_thumbnail_base64(Path::new(&path), 200)
        .map_err(|e| e.to_string())?;

    // キャッシュに保存
    {
        let mut cache = state.thumbnail_cache.lock().unwrap();
        cache.put(path, result.clone());
    }

    Ok(result)
}

#[tauri::command]
pub async fn get_full_image(
    path: String,
    max_width: u32,
    max_height: u32,
) -> Result<String, String> {
    core::generate_full_image_base64(Path::new(&path), max_width, max_height)
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
    core::validate_config(&config).map_err(|e| e.to_string())?;

    // 出力フォルダーを作成
    let output = output_folder.clone();
    let output_path = Path::new(&output);
    if !output_path.exists() {
        fs::create_dir_all(output_path).map_err(|e| e.to_string())?;
    }

    // キャンセルフラグをリセット
    state.cancel_flag.store(false, Ordering::Relaxed);

    let file_paths: Vec<PathBuf> = files.iter().map(PathBuf::from).collect();
    let cancel_flag = Arc::clone(&state.cancel_flag);
    let files_clone = files.clone();
    let app_handle_clone = app_handle.clone();

    // process_batchはrayonで並列処理するためブロッキング。
    // Tauriのasync runtimeをブロックしないようspawn_blockingでオフロード。
    let results = tokio::task::spawn_blocking(move || {
        let on_progress: core::ProgressCallback = Box::new(move |current, total| -> bool {
            let file_name = files_clone
                .get(current.saturating_sub(1))
                .cloned()
                .unwrap_or_default();

            let file_name_short = Path::new(&file_name)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            let _ = app_handle_clone.emit(
                "processing-progress",
                ProgressPayload {
                    current,
                    total,
                    file_name: file_name_short,
                },
            );

            !cancel_flag.load(Ordering::Relaxed)
        });

        let output_path = PathBuf::from(&output);
        core::process_batch(&file_paths, &output_path, &config, Some(on_progress))
    })
    .await
    .map_err(|e| e.to_string())?;

    let successes: Vec<core::ProcessResult> = results
        .into_iter()
        .filter_map(|r| r.ok())
        .collect();

    Ok(successes)
}

#[tauri::command]
pub fn cancel_processing(state: tauri::State<'_, ProcessingState>) -> Result<(), String> {
    state.cancel_flag.store(true, Ordering::Relaxed);
    Ok(())
}

// 注: pick_folderはTauriコマンドとしては実装しない。
// フロントエンドから@tauri-apps/plugin-dialogのopen()を直接呼び出す。
