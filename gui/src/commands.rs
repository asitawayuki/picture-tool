use crate::security;
use crate::state::ProcessingState;
use crate::types::*;
use picture_tool_core as core;
use picture_tool_core::exif_frame::{self, AssetDirs, ExifFrameConfig, FontInfo};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{Emitter, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tauri_plugin_store::StoreExt;

/// 各コマンドはこの関数を通したパスだけを触る。理由は `security` モジュール参照。
#[tauri::command]
pub fn list_directory(path: String) -> Result<Vec<FileEntry>, String> {
    let dir = security::existing_dir(&path)?;

    let mut entries = Vec::new();
    let read_dir = fs::read_dir(&dir).map_err(|e| e.to_string())?;

    for entry in read_dir.flatten() {
        // 壊れたシンボリックリンク等で1エントリが読めなくても一覧全体を失敗させない。
        // 「失敗はスキップして継続」という CLAUDE.md の方針に揃える（S6-M14）。
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
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
        b.is_dir
            .cmp(&a.is_dir)
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
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
            if std::path::Path::new(&drive).exists() {
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
    let dir = security::existing_dir(&path)?;

    tokio::task::spawn_blocking(move || {
        // 直下の画像のみ取得（再帰しない）
        let read_dir = fs::read_dir(&dir).map_err(|e| e.to_string())?;

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

            // 生ピクセルの縦横は Orientation 5-8 で入れ替わるため、表示値も揃える。
            let (width, height) = core::image_dimensions_oriented(&file_path).unwrap_or_default();
            let size_bytes = fs::metadata(&file_path).map(|m| m.len()).unwrap_or(0);

            entries.push(ImageEntry {
                name,
                path: path_str,
                width,
                height,
                size_bytes,
            });
        }

        entries.sort_by_key(|a| a.name.to_lowercase());

        Ok(entries)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn get_thumbnail(
    state: tauri::State<'_, ProcessingState>,
    path: String,
    max_dimension: u32,
) -> Result<String, String> {
    let file = security::readable_image(&path)?;
    // キャッシュキーは実体のパスと、core が実際に使う丸めた後のサイズ。
    // 生の要求値をキーにすると、上限超えの値を変えるだけで同一内容の
    // エントリを LRU の上限まで作れてしまう。
    let max_dimension = max_dimension.min(core::THUMBNAIL_MAX_DIMENSION);
    let cache_key = format!("{}:{}", file.display(), max_dimension);

    {
        let mut cache = state
            .thumbnail_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(cached) = cache.get(&cache_key) {
            return Ok(cached.clone());
        }
    }

    let result = tokio::task::spawn_blocking(move || {
        core::generate_thumbnail_base64(&file, max_dimension).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;

    {
        let mut cache = state
            .thumbnail_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        cache.put(cache_key, result.clone());
    }

    Ok(result)
}

#[tauri::command]
pub async fn get_full_image(
    path: String,
    max_width: u32,
    max_height: u32,
) -> Result<String, String> {
    let file = security::readable_image(&path)?;
    tokio::task::spawn_blocking(move || {
        core::generate_full_image_base64(&file, max_width, max_height).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 出力先フォルダーをネイティブダイアログで選ばせ、書き込み許可を与える
///
/// **ダイアログを Rust 側で開くことが本質。** フロントエンドの
/// `@tauri-apps/plugin-dialog` を使うと、選択結果を webview 経由で受け取ることになり、
/// 乗っ取られた webview が「ユーザーが `/` を選んだ」と自称できてしまう。
/// ここで許可されるのはダイアログが返したパスそのものだけ（S6-H8）。
#[tauri::command]
pub async fn pick_output_folder(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, ProcessingState>,
    default_path: Option<String>,
) -> Result<Option<String>, String> {
    let mut builder = app_handle
        .dialog()
        .file()
        .set_title("出力先フォルダーを選択");

    // 既定位置はあくまで初期表示。ここが許可されるわけではない。
    if let Some(ref raw) = default_path {
        if let Ok(dir) = security::existing_dir(raw) {
            builder = builder.set_directory(dir);
        }
    }

    let Some(picked) = builder.blocking_pick_folder() else {
        return Ok(None); // ユーザーがキャンセルした
    };

    let picked = picked.into_path().map_err(|e| e.to_string())?;
    let resolved = fs::canonicalize(&picked)
        .map_err(|e| format!("Cannot access {}: {}", picked.display(), e))?;

    state.writable_roots.grant(resolved.clone());
    Ok(Some(resolved.to_string_lossy().to_string()))
}

#[tauri::command]
pub async fn process_images(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, ProcessingState>,
    files: Vec<String>,
    output_folder: String,
    config: core::ProcessingConfig,
    exif_frame_config: Option<ExifFrameConfig>,
) -> Result<ProcessBatchResponse, String> {
    core::validate_config(&config).map_err(|e| e.to_string())?;

    // 出力は利用者が明示的に許可したルート配下のみ。
    let output = security::writable_dir(&state.writable_roots, &output_folder)?;

    // 入力は画像ファイルのみ。選択後に1枚消えていた程度で全件を落とさず、
    // 落ちた分は理由付きで failures に載せる（CLAUDE.md「失敗はスキップして継続」）。
    let (requested, inputs, mut failures) = validate_inputs(&files);

    let mut exif_frame_config = exif_frame_config;
    if let Some(ref mut ef) = exif_frame_config {
        resolve_font_path(ef)?;
    }

    let mut config = config;
    let mut warnings = Vec::new();

    // 不可逆な一括削除は、webview が偽装できない OS のダイアログで確認する。
    // フロントエンドの確認ダイアログは webview 内にあるため、乗っ取られた
    // 状態では素通りしてしまう（S6-H8）。
    if config.delete_originals
        && !inputs.is_empty()
        && !confirm_delete_originals(&app_handle, &inputs)
    {
        config.delete_originals = false;
        warnings.push("元ファイルの削除はキャンセルされました。変換のみ実行しました。".to_string());
    }

    if !output.exists() {
        fs::create_dir_all(&output).map_err(|e| e.to_string())?;
    }

    state.cancel_flag.store(false, Ordering::Relaxed);

    let cancel_flag = Arc::clone(&state.cancel_flag);
    let batch = run_batch(
        app_handle,
        cancel_flag,
        inputs,
        output,
        config,
        exif_frame_config,
    )
    .await?;

    warnings.extend(batch.warnings);

    let (results, batch_failures) = split_results(&requested, batch.results);
    failures.extend(batch_failures);

    Ok(ProcessBatchResponse {
        results,
        failures,
        warnings,
    })
}

/// webview が渡したパスを検証し、通ったものと落ちたものに分ける
///
/// 戻り値の1つ目は検証を通ったパスの**要求時の表記**で、`inputs` と同じ並び。
/// 結果の対応づけに使う。
fn validate_inputs(files: &[String]) -> (Vec<String>, Vec<PathBuf>, Vec<ProcessFailure>) {
    let mut requested = Vec::new();
    let mut inputs = Vec::new();
    let mut failures = Vec::new();

    for raw in files {
        match security::readable_image(raw) {
            Ok(path) => {
                requested.push(raw.clone());
                inputs.push(path);
            }
            Err(error) => failures.push(ProcessFailure {
                input_path: raw.clone(),
                error,
            }),
        }
    }

    (requested, inputs, failures)
}

/// バッチ結果を成功と失敗に振り分ける
///
/// 結果はフロントが渡してきたパス表記で返す。core が返す `input_path` は
/// canonicalize 済みの実体パスなので、シンボリックリンク経由の画像だと
/// 要求時の文字列と一致せず、UI 側で「どのファイルの結果か」を辿れなくなる。
fn split_results(
    requested: &[String],
    results: Vec<anyhow::Result<core::ProcessResult>>,
) -> (Vec<core::ProcessResult>, Vec<ProcessFailure>) {
    let mut succeeded = Vec::new();
    let mut failures = Vec::new();

    for (raw, result) in requested.iter().zip(results) {
        match result {
            Ok(mut r) => {
                r.input_path.clone_from(raw);
                succeeded.push(r);
            }
            // キャンセルで着手すらしなかった分は「失敗」ではない。どちらにも
            // 載せないことで、フロントが要求リストとの差分から「未処理」として
            // 表示する（`ResultDialog`）。
            Err(e) if format!("{:#}", e) == core::CANCELLED_ERROR => {}
            Err(e) => failures.push(ProcessFailure {
                input_path: raw.clone(),
                error: format!("{:#}", e),
            }),
        }
    }

    (succeeded, failures)
}

/// webview が指定したフォントを検証し、実体のパスへ置き換える
///
/// `ExifFrameConfig` は webview が丸ごと組み立てる構造体なので、`font_path` は
/// 画像パスと同じく境界を通す必要がある（S6-H8）。
fn resolve_font_path(config: &mut ExifFrameConfig) -> Result<(), String> {
    if let Some(ref raw) = config.font.font_path {
        let font = security::readable_font(raw)?;
        config.font.font_path = Some(font.to_string_lossy().into_owned());
    }
    Ok(())
}

struct BatchOutcome {
    results: Vec<anyhow::Result<core::ProcessResult>>,
    warnings: Vec<String>,
}

/// rayon の並列処理を tokio のワーカーから追い出して実行する
async fn run_batch(
    app_handle: tauri::AppHandle,
    cancel_flag: Arc<AtomicBool>,
    inputs: Vec<PathBuf>,
    output: PathBuf,
    config: core::ProcessingConfig,
    exif_frame_config: Option<ExifFrameConfig>,
) -> Result<BatchOutcome, String> {
    // process_batch は rayon で並列処理するためブロッキング。
    // Tauri の async runtime をブロックしないよう spawn_blocking でオフロードする。
    tokio::task::spawn_blocking(move || {
        let names: Vec<String> = inputs
            .iter()
            .map(|p| p.file_name().unwrap_or_default().to_string_lossy().into())
            .collect();
        let on_progress = progress_callback(app_handle, cancel_flag, names);

        // Exif アセットはバッチ全体で1回だけ構築する（画像ごとに model_map を
        // 読み直さないため）。Exif フレームを使わない場合は構築自体を行わない。
        let assets = match exif_frame_config.as_ref() {
            Some(_) => Some(
                exif_frame::ExifAssets::load(AssetDirs::default())
                    .map_err(|e| format!("{:#}", e))?,
            ),
            None => None,
        };
        // core は eprintln! しない方針なので、提示は呼び出し元の責務。
        // 戻り値に載せてフロントの ResultDialog に出す（S5-F8 / S6-M15）。
        let warnings = assets
            .as_ref()
            .map(|a| a.warnings.clone())
            .unwrap_or_default();

        let results = core::process_batch(
            &inputs,
            &output,
            &config,
            exif_frame_config.as_ref(),
            assets.as_ref(),
            Some(on_progress),
        );

        Ok(BatchOutcome { results, warnings })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 進捗をフロントへ流し、キャンセル要求を返すコールバック
fn progress_callback(
    app_handle: tauri::AppHandle,
    cancel_flag: Arc<AtomicBool>,
    file_names: Vec<String>,
) -> core::ProgressCallback {
    // emit が失敗すると進捗が止まったように見える。原因を追えるよう記録するが、
    // 1件ごとに出すと壊れた状態でログが埋まるため最初の1回だけにする（S6-M15）。
    let emit_failure_logged = AtomicBool::new(false);

    Box::new(move |current, total| -> bool {
        let file_name = file_names
            .get(current.saturating_sub(1))
            .cloned()
            .unwrap_or_default();

        if let Err(e) = app_handle.emit(
            "processing-progress",
            ProgressPayload {
                current,
                total,
                file_name,
            },
        ) {
            if !emit_failure_logged.swap(true, Ordering::Relaxed) {
                eprintln!("Failed to emit processing-progress: {}", e);
            }
        }

        !cancel_flag.load(Ordering::Relaxed)
    })
}

/// 元ファイル削除の最終確認（OS ネイティブ）
///
/// **枚数だけでなく場所を出す。** 削除対象は入力そのものであり、入力はツリーから
/// 自由に選ぶ設計上「許可ルート配下」には縛れない。乗っ取られた webview が
/// ライブラリ全体を `files` に詰めても、枚数だけの文面では利用者が自分の操作との
/// 差異を検知できない。フォルダー一覧があれば「見覚えのない場所」で気づける（S6-H8）。
fn confirm_delete_originals(app_handle: &tauri::AppHandle, inputs: &[PathBuf]) -> bool {
    const SHOWN_DIRS: usize = 3;

    let mut dirs: Vec<String> = inputs
        .iter()
        .filter_map(|p| p.parent())
        .map(|d| d.display().to_string())
        .collect();
    dirs.sort();
    dirs.dedup();

    let mut locations: String = dirs
        .iter()
        .take(SHOWN_DIRS)
        .map(|d| format!("\n・{}", d))
        .collect();
    if dirs.len() > SHOWN_DIRS {
        locations.push_str(&format!("\n・他 {} フォルダー", dirs.len() - SHOWN_DIRS));
    }

    let mut builder = app_handle
        .dialog()
        .message(format!(
            "変換に成功した {} 枚の元ファイルを削除します。\nゴミ箱には入らず、元に戻せません。\n\n削除する場所:{}",
            inputs.len(),
            locations
        ))
        .title("元ファイルを削除しますか？")
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom(
            "削除する".to_string(),
            "削除しない".to_string(),
        ));

    if let Some(window) = app_handle.get_webview_window("main") {
        builder = builder.parent(&window);
    }

    builder.blocking_show()
}

/// お気に入りフォルダーの保存先
///
/// **ファイル名は Rust 側に固定する。** store プラグインの JS API は webview から
/// パスを受け取り、それを AppData 配下として解決するが `..` も絶対パスも
/// 正規化しない。つまり `store:allow-load` 等を webview に与えると、
/// `load("../../../.ssh/config.json")` の形で `security` モジュールの境界を
/// まるごと迂回して任意の JSON を読み書きできる。capabilities から store の
/// 権限を落とし、代わりに用途を固定したこの2コマンドだけを開ける（S6-H8）。
const FAVORITES_STORE: &str = "favorites.json";
const FAVORITES_KEY: &str = "favorites";

#[tauri::command]
pub fn load_favorites(app_handle: tauri::AppHandle) -> Result<Vec<String>, String> {
    let store = app_handle
        .store(FAVORITES_STORE)
        .map_err(|e| e.to_string())?;

    // 読み出しでは実在確認をしない。外付けドライブが外れている間だけ
    // お気に入りが消える、という挙動になるのを避けるため。
    // 不正な値が入り込む経路は save 側で塞いである。
    match store.get(FAVORITES_KEY) {
        Some(value) => serde_json::from_value(value).map_err(|e| e.to_string()),
        None => Ok(Vec::new()),
    }
}

#[tauri::command]
pub fn save_favorites(app_handle: tauri::AppHandle, favorites: Vec<String>) -> Result<(), String> {
    // 保存するのは実在するディレクトリの実体パスだけ。
    let validated: Vec<String> = favorites
        .iter()
        .map(|raw| security::existing_dir(raw).map(|d| d.to_string_lossy().into_owned()))
        .collect::<Result<_, _>>()?;

    let store = app_handle
        .store(FAVORITES_STORE)
        .map_err(|e| e.to_string())?;
    store.set(FAVORITES_KEY, serde_json::json!(validated));
    store.save().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn cancel_processing(state: tauri::State<'_, ProcessingState>) -> Result<(), String> {
    state.cancel_flag.store(true, Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
pub async fn get_exif_info(path: String) -> Result<core::ExifInfo, String> {
    let file = security::readable_image(&path)?;
    tokio::task::spawn_blocking(move || core::read_exif_info(&file).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

/// プレビュー用の低解像度 Exif フレームを生成する
///
/// 縮小・描画・エンコードは core が持つ（S6-M16）。ここは境界の検証と、
/// webview がそのまま `<img src>` に使える data URI への変換だけを行う。
#[tauri::command]
pub async fn render_exif_frame_preview(
    path: String,
    config: ExifFrameConfig,
    bg_color: core::BackgroundColor,
) -> Result<PreviewImage, String> {
    let file = security::readable_image(&path)?;
    let mut config = config;
    resolve_font_path(&mut config)?;

    tokio::task::spawn_blocking(move || {
        let assets =
            exif_frame::ExifAssets::load(AssetDirs::default()).map_err(|e| format!("{:#}", e))?;

        let preview =
            core::generate_exif_frame_preview_base64(&file, &config, &bg_color, &assets, 400)
                .map_err(|e| format!("{:#}", e))?;

        Ok(PreviewImage {
            data_url: format!("data:image/jpeg;base64,{}", preview.base64),
            // フレーム描画由来の警告（preview.warnings）は載せない。プレビューは
            // 長辺 400px 固定なので、実出力ではフレームが出る写真でも skip_exif に
            // 落ち、出すと偽陽性になる。捨てる判断は境界の責務（spec §8）。
            warnings: assets.warnings,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn list_presets() -> Result<Vec<ExifFrameConfig>, String> {
    let dirs = AssetDirs::default();
    Ok(exif_frame::preset::list_all_presets(
        dirs.user_presets_dir.as_deref(),
    ))
}

/// プリセットを保存する
///
/// 名前もフォントも webview 由来なので、保存前に境界を通す。検証せずに書くと
/// 不正な `font_path` が設定ファイルに焼き付き、CLI が同じプリセットを読むため
/// webview の生存期間を越えて残る。
#[tauri::command]
pub async fn save_preset(config: ExifFrameConfig) -> Result<(), String> {
    let dir = user_presets_dir()?;
    let mut config = config;
    security::preset_name(&config.name)?;
    resolve_font_path(&mut config)?;
    exif_frame::preset::save_preset(&dir, &config).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_preset(name: String) -> Result<(), String> {
    let dir = user_presets_dir()?;
    let name = security::preset_name(&name)?;
    exif_frame::preset::delete_preset(&dir, &name).map_err(|e| e.to_string())
}

fn user_presets_dir() -> Result<PathBuf, String> {
    AssetDirs::default()
        .user_presets_dir
        .ok_or_else(|| "config dir not found".to_string())
}

#[tauri::command]
pub async fn list_available_fonts() -> Result<Vec<FontInfo>, String> {
    let mut fonts = vec![FontInfo {
        display_name: "Noto Sans JP (bundled)".to_string(),
        path: None,
        is_bundled: true,
    }];

    if let Some(user_dir) = AssetDirs::default().user_fonts_dir {
        for entry in fs::read_dir(&user_dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "ttf" || e == "otf") {
                fonts.push(FontInfo {
                    display_name: format!(
                        "User: {}",
                        path.file_stem().unwrap_or_default().to_string_lossy()
                    ),
                    path: Some(path.to_string_lossy().to_string()),
                    is_bundled: false,
                });
            }
        }
    }

    Ok(fonts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    fn as_str(path: &Path) -> String {
        path.to_string_lossy().into_owned()
    }

    fn write_real_jpeg(path: &Path) {
        let img = image::RgbImage::from_pixel(4, 4, image::Rgb([120, 120, 120]));
        image::DynamicImage::ImageRgb8(img).save(path).unwrap();
    }

    fn succeeded(input_path: &str) -> core::ProcessResult {
        core::ProcessResult {
            input_path: input_path.to_string(),
            output_path: "/out/photo.jpg".to_string(),
            final_size_mb: 1.0,
            final_quality: Some(90),
            size_limit_exceeded: false,
            warnings: Vec::new(),
        }
    }

    // --- 入力の仕分け: 通らなかった1件でバッチ全体を落とさない ---

    #[test]
    fn validate_inputs_processes_the_valid_images_and_reports_the_rest() {
        let dir = TempDir::new().unwrap();
        let photo = dir.path().join("photo.jpg");
        write_real_jpeg(&photo);
        let secret = dir.path().join("id_rsa");
        fs::write(&secret, b"-----BEGIN OPENSSH PRIVATE KEY-----").unwrap();
        let missing = dir.path().join("gone.jpg");

        let files = vec![as_str(&secret), as_str(&photo), as_str(&missing)];
        let (requested, inputs, failures) = validate_inputs(&files);

        // 通ったものだけが処理対象になり、実体のパスで渡る
        assert_eq!(inputs, vec![fs::canonicalize(&photo).unwrap()]);
        // 対応づけ用に、要求時の表記が同じ並びで返る
        assert_eq!(requested, vec![as_str(&photo)]);
        // 落ちた分は「無かったこと」にせず理由付きで返す
        assert_eq!(failures.len(), 2);
        assert_eq!(failures[0].input_path, as_str(&secret));
        assert!(!failures[0].error.is_empty());
        assert_eq!(failures[1].input_path, as_str(&missing));
    }

    #[test]
    fn validate_inputs_handles_an_empty_selection() {
        let (requested, inputs, failures) = validate_inputs(&[]);
        assert!(requested.is_empty());
        assert!(inputs.is_empty());
        assert!(failures.is_empty());
    }

    // --- 結果の仕分け: 成功 / 失敗 / 未処理 ---

    #[test]
    fn split_results_reports_success_under_the_path_the_caller_asked_for() {
        // core は canonicalize 済みの実体パスを返すが、UI が対応づけられるのは
        // 自分が渡した表記だけ（シンボリックリンク経由だと両者は一致しない）
        let requested = vec!["/photos/link.jpg".to_string()];
        let results = vec![Ok(succeeded("/elsewhere/real.jpg"))];

        let (succeeded_results, failures) = split_results(&requested, results);

        assert_eq!(succeeded_results.len(), 1);
        assert_eq!(succeeded_results[0].input_path, "/photos/link.jpg");
        assert!(failures.is_empty());
    }

    #[test]
    fn split_results_reports_failures_with_their_reason() {
        let requested = vec!["/photos/broken.jpg".to_string()];
        let results = vec![Err(anyhow::anyhow!("Failed to open image"))];

        let (succeeded_results, failures) = split_results(&requested, results);

        assert!(succeeded_results.is_empty());
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].input_path, "/photos/broken.jpg");
        assert!(failures[0].error.contains("Failed to open image"));
    }

    #[test]
    fn split_results_does_not_count_cancelled_files_as_failures() {
        let requested = vec!["/photos/a.jpg".to_string(), "/photos/b.jpg".to_string()];
        let results = vec![
            Ok(succeeded("/photos/a.jpg")),
            Err(anyhow::anyhow!(core::CANCELLED_ERROR)),
        ];

        let (succeeded_results, failures) = split_results(&requested, results);

        // 前提: キャンセル分も結果として並んでいる（そもそも来ていない訳ではない）
        assert_eq!(requested.len(), 2);
        assert_eq!(succeeded_results.len(), 1);
        // 着手していないものは「失敗」ではない。要求リストとの差分で
        // 「未処理」として表示させるため、どちらにも載せない。
        assert!(failures.is_empty());
    }
}
