use crate::security;
use crate::state::ProcessingState;
use crate::types::*;
use picture_tool_core as core;
use picture_tool_core::exif_frame::{self, AssetDirs, ExifFrameConfig, FontInfo};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
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

/// `SystemTime` を UNIX epoch からのミリ秒へ。
///
/// epoch より前の日時（時計が狂った機材で撮った写真に実在する）は `duration_since`
/// が `Err` になるので `None` にする。並び順では `None` は末尾に送られるだけで、
/// 一覧から消えることはない。
fn epoch_ms(time: std::time::SystemTime) -> Option<i64> {
    time.duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_millis()).ok())
}

/// 一覧の順序。**小文字化した名前 → 元の名前**の 2 段で比較する。
///
/// 1 段だけだと安定ソート任せになり、同じ小文字名を持つ `A.jpg` と `a.jpg` の
/// 相対順が `read_dir` の返すファイルシステム順のままになる（＝実行ごとに変わりうる）。
/// フロントの `sortImages` が同値を名前の昇順で解決しているのと同じ規則に揃える。
fn sort_entries(entries: &mut [ImageEntry]) {
    entries.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.name.cmp(&b.name))
    });
}

/// フォルダー直下の画像を `ImageEntry` にする。**再帰しない。**
///
/// **`par_iter` であることが設計の一部。** 1 枚あたり `image_dimensions_oriented`
/// ＋ `fs::metadata` ＋ ★の読み取りで open が 3 回になる。逐次のままだと
/// 1,000 枚で 576 → 650ms の悪化になり、`list_images` が返るまでグリッドは空なので
/// フォルダーを開いた瞬間の停止時間がそのまま伸びる。並列なら 169ms で現行より速い
/// （実測は絞り込み spec の「実測」節）。
///
/// **順序は崩れない。** `&Vec<T>` の `par_iter()` は `IndexedParallelIterator` で、
/// `map` は indexed 性を保ち、`Vec` への `collect` は index どおりに詰める。
///
/// コマンド本体（`#[tauri::command]` かつ `async`）は単体テストから素直に呼べないので、
/// 中身をここへ出してテスト可能にしている（`metadata_target` と同じ形）。
fn collect_image_entries(dir: &Path) -> Result<Vec<ImageEntry>, String> {
    use rayon::prelude::*;

    let read_dir = fs::read_dir(dir).map_err(|e| e.to_string())?;
    let direct_files: Vec<PathBuf> = read_dir
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && core::is_supported_image(p))
        .collect();

    let mut entries: Vec<ImageEntry> = direct_files
        .par_iter()
        .map(|file_path| {
            let name = file_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let path_str = file_path.to_string_lossy().to_string();

            // 生ピクセルの縦横は Orientation 5-8 で入れ替わるため、表示値も揃える。
            let (width, height) = core::image_dimensions_oriented(file_path).unwrap_or_default();
            // 1 度の stat から大きさも日時も取る。並び順のために stat をもう 1 回
            // 回すと、3,000 枚のフォルダーで走査量が倍になる
            let meta = fs::metadata(file_path).ok();
            let size_bytes = meta.as_ref().map(|m| m.len()).unwrap_or(0);
            let modified_ms = meta
                .as_ref()
                .and_then(|m| m.modified().ok())
                .and_then(epoch_ms);
            let created_ms = meta
                .as_ref()
                .and_then(|m| m.created().ok())
                .and_then(epoch_ms);

            // **★は読めなくても一覧を落とさない。** グリッドの飾りであって、
            // 一覧そのものの前提ではない（PNG も壊れた JPEG も並ぶ）。
            // **`warnings` はここで捨てる** ── 3,000 枚ぶんの警告は利用者に
            // 見せる形が無い。同じ写真を情報モードで開けば `read_image_metadata`
            // が同じ警告を 1 枚分だけ返すので、情報は失われず出す場所が寄るだけ
            let rating = core::metadata::read_metadata_fast(file_path)
                .ok()
                .and_then(|read| read.metadata.rating);

            ImageEntry {
                name,
                path: path_str,
                width,
                height,
                size_bytes,
                modified_ms,
                created_ms,
                rating,
            }
        })
        .collect();

    sort_entries(&mut entries);
    Ok(entries)
}

#[tauri::command]
pub async fn list_images(path: String) -> Result<Vec<ImageEntry>, String> {
    let dir = security::existing_dir(&path)?;
    tokio::task::spawn_blocking(move || collect_image_entries(&dir))
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
        core::generate_thumbnail_base64(&file, max_dimension).map_err(|e| format!("{e:#}"))
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
        core::generate_full_image_base64(&file, max_width, max_height).map_err(|e| format!("{e:#}"))
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
    if let Some(ref raw) = default_path
        && let Ok(dir) = security::existing_dir(raw)
    {
        builder = builder.set_directory(dir);
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
    core::validate_config(&config).map_err(|e| format!("{e:#}"))?;

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
        ) && !emit_failure_logged.swap(true, Ordering::Relaxed)
        {
            eprintln!("Failed to emit processing-progress: {}", e);
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
    tokio::task::spawn_blocking(move || core::read_exif_info(&file).map_err(|e| format!("{e:#}")))
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
    exif_frame::preset::save_preset(&dir, &config).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub async fn delete_preset(name: String) -> Result<(), String> {
    let dir = user_presets_dir()?;
    let name = security::preset_name(&name)?;
    exif_frame::preset::delete_preset(&dir, &name).map_err(|e| format!("{e:#}"))
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
        fonts.extend(scan_user_fonts(&user_dir));
    }

    Ok(fonts)
}

/// ユーザーフォントディレクトリを走査して一覧を作る。
///
/// **受け入れる集合を `security::readable_font` と一致させること。** GUI からフォントを
/// 選ぶ経路はこの一覧しか無いので、ここに出ないフォントは事実上使えない。逆にここに
/// 出るのに `readable_font` が弾くと「選べるのに保存できない」になる。拡張子の大小を
/// 区別しないのと、ディレクトリを除くのはそのため（`security` 側は `is_file` を見ている）。
///
/// 並び順は表示名で固定する。`read_dir` の順序は保証が無く、起動のたびに
/// ドロップダウンの並びが変わる。
fn scan_user_fonts(dir: &Path) -> Vec<FontInfo> {
    let mut fonts: Vec<FontInfo> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter(|path| {
            path.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("ttf") || e.eq_ignore_ascii_case("otf"))
        })
        .map(|path| FontInfo {
            display_name: format!(
                "User: {}",
                path.file_stem().unwrap_or_default().to_string_lossy()
            ),
            path: Some(path.to_string_lossy().to_string()),
            is_bundled: false,
        })
        .collect();
    fonts.sort_by(|a, b| a.display_name.cmp(&b.display_name));
    fonts
}

/// 編集対象の現在値と、箇所ごとの値の食い違いを返す
#[tauri::command]
pub async fn read_image_metadata(path: String) -> Result<core::metadata::MetadataRead, String> {
    let file = security::readable_jpeg(&path)?;
    tokio::task::spawn_blocking(move || {
        core::metadata::read_metadata_fast(&file).map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// メタデータの書き込み先を検証する。
///
/// **見るのは `metadata_roots` だけ。** 変換の出力先の承認（`pick_output_folder`）は
/// ここでは効かない ── 「出力先フォルダーを選択」のダイアログは既存写真の書き換えに
/// ついて何も言っていないので、それを書き込みの同意として使ってはならない。
///
/// コマンド本体（`#[tauri::command]` かつ `tauri::State` を取る）は単体テストから
/// 呼べないので、**どちらの集合を見るかという判断だけ**をここへ出してテスト可能にしている。
fn metadata_target(state: &ProcessingState, path: &str) -> Result<PathBuf, String> {
    security::writable_image(&state.metadata_roots, path)
}

/// メタデータ編集の承認を記録する。**足すのは `metadata_roots` だけ。**
///
/// 承認ダイアログ（`grant_metadata_editing`）は「タイトル・コメント・評価を
/// 書き込めるようにします」としか言っておらず、変換の出力先になることには触れていない。
/// `metadata_target` と対にしてここへ出してあるのは、同じ理由（コマンド本体を
/// 単体テストから呼べない）でテスト可能にするため。
fn approve_metadata_root(state: &ProcessingState, dir: PathBuf) {
    state.metadata_roots.grant(dir);
}

/// 検証付き原子的置換で書き込み、**保存直後の実際の中身を返す**
///
/// 戻り値を保存済みの値としてそのまま使えるので、フロント側が
/// 「送った値」と「ファイルの中身」を取り違えることが無い。
/// 食い違いの警告も、書き込みで揃った結果が返る。
#[tauri::command]
pub async fn write_image_metadata(
    state: tauri::State<'_, ProcessingState>,
    path: String,
    metadata: core::metadata::EditableMetadata,
) -> Result<core::metadata::MetadataRead, String> {
    let file = metadata_target(&state, &path)?;
    tokio::task::spawn_blocking(move || save_and_read_back(&file, &metadata))
        .await
        .map_err(|e| e.to_string())?
}

/// `write_image_metadata` の本体。コマンド本体（`tauri::State` を取る）は単体テストから
/// 呼べないので、ここへ出してある（`metadata_target` と同じ理由）。
///
/// **保存失敗は文脈のチェーン全体（`{e:#}`）で文字列化する。** `to_string()` だと
/// 最上位の「メタデータを保存できませんでした…」しか届かず、なぜ止めたか
/// （「既存の Exif が保存で失われるか変わるため」等）が利用者に見えない。
/// 保存が返した警告（MakerNote の位置の変化）は、読み戻した結果の `warnings` に足す ──
/// フロントは保存後の `warnings` をそのまま表示している（`metadataDraft.svelte.ts`）
fn save_and_read_back(
    file: &Path,
    metadata: &core::metadata::EditableMetadata,
) -> Result<core::metadata::MetadataRead, String> {
    let saved = core::metadata::write_metadata(file, metadata).map_err(|e| format!("{e:#}"))?;
    // **読み戻しも入口を通す。** ここだけ経路が違うと、★の書き戻しの
    // 「値の出所」（絞り込み spec §5）がグリッドの★と別経路になる
    let mut read = core::metadata::read_metadata_fast(file).map_err(|e| format!("{e:#}"))?;
    read.warnings.extend(saved);
    Ok(read)
}

/// フォルダー単位でメタデータの書き込みを承認させる
///
/// **確認を OS ネイティブのダイアログで出すことが本質。** webview 内の確認は
/// 乗っ取られた状態では素通りする（`confirm_delete_originals` と同型）。
/// `folder` は webview から来るが、承認されるのは `existing_dir` が解決した実体パスで、
/// その実体パスをダイアログの文面に出すので、利用者は「見覚えのない場所」に気づける。
/// 戻り値も**解決後の実体パス**にして、フロントの表示と境界の判定を一致させる。
#[tauri::command]
pub async fn grant_metadata_editing(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, ProcessingState>,
    folder: String,
) -> Result<Option<String>, String> {
    let dir = security::existing_dir(&folder)?;

    let mut builder = app_handle
        .dialog()
        .message(format!(
            "このフォルダーの写真に、タイトル・コメント・評価を書き込めるようにします。\n\n{}\n\n書き換えるのはメタデータだけで、写真そのもの（画素）は変わりません。\n許可はアプリを終了すると消えます。",
            dir.display()
        ))
        .title("このフォルダーへの書き込みを許可しますか？")
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom(
            "許可する".to_string(),
            "許可しない".to_string(),
        ));

    if let Some(window) = app_handle.get_webview_window("main") {
        builder = builder.parent(&window);
    }

    if !builder.blocking_show() {
        return Ok(None);
    }
    // **返すのは解決後の実体パス。** 呼び出し側が送った生の文字列を保持すると、
    // シンボリックリンク越しなどで境界とフロントの表示がずれ、
    // 「承認済みと出ているのに保存が弾かれる」になる
    let granted = dir.to_string_lossy().to_string();
    approve_metadata_root(&state, dir);
    Ok(Some(granted))
}

/// 未保存の有無を Rust 側へ通知する
///
/// フロントで `onCloseRequested` を使わないのは、`core:default` に
/// `window:allow-destroy` が含まれず、webview に追加権限が要るため。
/// 「webview にプラグイン権限を与えない」という既存の前提（`security.rs`）に反する。
#[tauri::command]
pub fn set_unsaved_state(state: tauri::State<'_, ProcessingState>, unsaved: bool) {
    state.unsaved.store(unsaved, Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
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

    // --- フォント一覧: security::readable_font が受ける集合と一致させる ---

    #[test]
    fn user_fonts_are_listed_regardless_of_extension_case() {
        // `readable_font` は拡張子を大小無視で受ける。一覧が区別すると
        // 「置いたのに選べない」になる（GUI からフォントを選ぶ経路はこの一覧だけ）
        let dir = TempDir::new().unwrap();
        for name in ["a.ttf", "b.TTF", "c.otf", "d.OtF"] {
            fs::write(dir.path().join(name), b"font").unwrap();
        }

        let names: Vec<String> = scan_user_fonts(dir.path())
            .into_iter()
            .map(|f| f.display_name)
            .collect();

        assert_eq!(
            names,
            vec!["User: a", "User: b", "User: c", "User: d"],
            "4 つとも出て、表示名で並ぶ"
        );
    }

    #[test]
    fn files_that_are_not_fonts_are_not_listed() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("readme.txt"), b"x").unwrap();
        fs::write(dir.path().join("noext"), b"x").unwrap();
        fs::write(dir.path().join("real.ttf"), b"font").unwrap();

        let listed = scan_user_fonts(dir.path());
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].display_name, "User: real");
    }

    #[test]
    fn a_directory_named_like_a_font_is_not_listed() {
        // `readable_font` は is_file を見るので、一覧に出すと選んだ瞬間に弾かれる
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join("trap.ttf")).unwrap();

        assert!(scan_user_fonts(dir.path()).is_empty());
    }

    #[test]
    fn a_missing_font_folder_lists_nothing_instead_of_failing() {
        let dir = TempDir::new().unwrap();
        assert!(scan_user_fonts(&dir.path().join("does-not-exist")).is_empty());
    }

    #[test]
    fn a_listed_font_carries_the_path_that_can_be_selected() {
        let dir = TempDir::new().unwrap();
        let font = dir.path().join("MyFont.ttf");
        fs::write(&font, b"font").unwrap();

        let listed = scan_user_fonts(dir.path());
        assert_eq!(listed[0].path.as_deref(), Some(as_str(&font).as_str()));
        assert!(!listed[0].is_bundled);
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

    // --- 書き込みの承認: 変換の出力先とメタデータ編集は別の同意 ---

    /// **出力先フォルダーの選択は、メタデータ書き込みの承認を兼ねない。**
    ///
    /// `pick_output_folder` のダイアログは「出力先フォルダーを選択」としか言っておらず、
    /// 既存写真の書き換えについては何も述べていない。利用者が出力先に自分の写真
    /// フォルダーを選ぶのは普通の操作なので、そこを兼ねてしまうと追加の同意なしに
    /// 配下の全 JPEG が書き換え可能になる。
    #[test]
    fn approving_an_output_folder_does_not_approve_metadata_writing() {
        let dir = TempDir::new().unwrap();
        let photo = dir.path().join("a.jpg");
        write_real_jpeg(&photo);
        let root = fs::canonicalize(dir.path()).unwrap();

        let state = ProcessingState::new();
        // `pick_output_folder` が承認する側だけを埋める
        state.writable_roots.grant(root.clone());

        // 前提条件: 変換の出力先としては承認済みである
        // （＝「何も承認していないから拒否された」ではない）
        assert!(security::writable_dir(&state.writable_roots, &as_str(dir.path())).is_ok());

        assert!(
            metadata_target(&state, &as_str(&photo)).is_err(),
            "出力先の承認だけでメタデータを書き換えられてはならない"
        );

        // 前提条件: `grant_metadata_editing` が承認する側を埋めれば通る
        // （＝ファイルそのものが理由で落ちているのではない）
        approve_metadata_root(&state, root);
        assert!(metadata_target(&state, &as_str(&photo)).is_ok());
    }

    /// 逆向きも兼ねない。メタデータ編集の承認は変換の出力先にならない
    #[test]
    fn approving_metadata_editing_does_not_approve_an_output_folder() {
        let dir = TempDir::new().unwrap();
        let photo = dir.path().join("a.jpg");
        write_real_jpeg(&photo);
        let root = fs::canonicalize(dir.path()).unwrap();

        let state = ProcessingState::new();
        // `grant_metadata_editing` が承認する側だけを埋める
        approve_metadata_root(&state, root);

        // 前提条件: メタデータ編集としては承認済みである
        assert!(metadata_target(&state, &as_str(&photo)).is_ok());

        assert!(security::writable_dir(&state.writable_roots, &as_str(dir.path())).is_err());
    }

    fn entry_named(name: &str) -> ImageEntry {
        ImageEntry {
            name: name.to_string(),
            path: format!("/photos/{name}"),
            width: 4000,
            height: 3000,
            size_bytes: 4_500_000,
            modified_ms: Some(1_755_000_000_000),
            created_ms: None,
            rating: None,
        }
    }

    /// **同じ小文字名を持つ 2 枚の相対順が、入力の並びに依存しない。**
    ///
    /// 現行の `sort_by_key(|a| a.name.to_lowercase())` は安定ソートなので、
    /// `A.jpg` と `a.jpg` の順は `read_dir` が返したファイルシステム順のままになる。
    /// `par_iter` にしても収集順は保たれる（`IndexedParallelIterator`）ので、
    /// これは並列化が持ち込む問題ではなく、**ついでに直す既存の欠陥**である。
    #[test]
    fn entries_with_the_same_lowercased_name_are_ordered_deterministically() {
        let mut forward = vec![entry_named("A.jpg"), entry_named("a.jpg")];
        let mut backward = vec![entry_named("a.jpg"), entry_named("A.jpg")];
        // 前提条件: 入力は実際に別の並びである（同じものを 2 回並べていない）
        assert_ne!(forward[0].name, backward[0].name);

        sort_entries(&mut forward);
        sort_entries(&mut backward);

        let names = |v: &[ImageEntry]| v.iter().map(|e| e.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(&forward), names(&backward));
        // 小文字化した名前で同値なら、元の名前の昇順で決める
        assert_eq!(
            names(&forward),
            vec!["A.jpg".to_string(), "a.jpg".to_string()]
        );
    }

    /// 小文字化した比較が第 1 キーであること（`Z.jpg` が `a.jpg` より前に来ない）
    #[test]
    fn entries_are_ordered_case_insensitively_first() {
        let mut entries = vec![entry_named("Z.jpg"), entry_named("a.jpg")];
        sort_entries(&mut entries);
        assert_eq!(entries[0].name, "a.jpg");
    }

    /// ★がファイルから読めて `ImageEntry` に載る
    #[test]
    fn list_reads_the_rating_from_the_file() {
        let dir = TempDir::new().unwrap();
        let rated = dir.path().join("rated.jpg");
        write_real_jpeg(&rated);
        core::metadata::write_metadata(
            &rated,
            &core::metadata::EditableMetadata {
                rating: Some(4),
                ..Default::default()
            },
        )
        .unwrap();
        let plain = dir.path().join("plain.jpg");
        write_real_jpeg(&plain);

        let entries = collect_image_entries(dir.path()).unwrap();

        assert_eq!(entries.len(), 2);
        let rating_of = |name: &str| {
            entries
                .iter()
                .find(|e| e.name == name)
                .unwrap_or_else(|| panic!("{name} が一覧に無い"))
                .rating
        };
        assert_eq!(rating_of("rated.jpg"), Some(4));
        assert_eq!(rating_of("plain.jpg"), None);
    }

    /// **★の読み取り失敗で一覧ごと失敗しない。**
    ///
    /// グリッドは PNG / WebP / JPEG XL / AVIF も並べるし、壊れた JPEG も混ざる。
    /// ★は一覧表示の副作用であって、一覧そのものの前提ではない
    #[test]
    fn list_survives_files_whose_rating_cannot_be_read() {
        let dir = TempDir::new().unwrap();
        write_real_jpeg(&dir.path().join("ok.jpg"));
        fs::write(dir.path().join("broken.jpg"), b"not a jpeg at all").unwrap();
        fs::write(
            dir.path().join("shot.png"),
            [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A],
        )
        .unwrap();

        let entries = collect_image_entries(dir.path()).unwrap();

        // 前提条件: 3 枚とも一覧に載っている（壊れた分が消えていない）
        assert_eq!(entries.len(), 3);
        for entry in &entries {
            if entry.name != "ok.jpg" {
                assert_eq!(entry.rating, None, "{} で★が付いた", entry.name);
            }
        }
    }

    /// **保存の拒否理由が境界を越えて届く。** `to_string()` では最上位の文脈
    /// （「メタデータを保存できませんでした…」）しか残らず、内側の理由が落ちる。
    /// 内側の文面には `build` の「解析できない」を使う（実書き込み経路で `preserve` の
    /// 拒否を起こす入力は作れない。計画 A-4）
    #[test]
    fn a_refused_save_reports_the_inner_reason() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("broken-exif.jpg");
        write_real_jpeg(&path);
        // SOI の直後に、TIFF として読めない Exif APP1 を差し込む
        let mut bytes = fs::read(&path).unwrap();
        let contents = b"Exif\0\0\xAA\xAA\xAA\xAA\xAA\xAA\xAA\xAA";
        let mut app1 = vec![0xFF, 0xE1];
        app1.extend(((contents.len() + 2) as u16).to_be_bytes());
        app1.extend(contents);
        bytes.splice(2..2, app1);
        fs::write(&path, &bytes).unwrap();

        let error = save_and_read_back(
            &path,
            &core::metadata::EditableMetadata {
                rating: Some(3),
                ..Default::default()
            },
        )
        .unwrap_err();

        assert!(
            error.contains("メタデータを保存できませんでした"),
            "{error}"
        );
        assert!(error.contains("既存の Exif を解析できない"), "{error}");
        assert_eq!(fs::read(&path).unwrap(), bytes, "元ファイルが変わった");
    }

    /// **保存が返した警告は、保存後に返す `warnings` に載る。**
    /// フロントは保存後の `warnings` だけを表示するので、ここで落とすと利用者に届かない
    #[test]
    fn a_save_warning_reaches_the_returned_warnings() {
        use little_exif::exif_tag::ExifTag;
        use little_exif::filetype::FileExtension;
        use little_exif::metadata::Metadata;

        let dir = TempDir::new().unwrap();
        let path = dir.path().join("maker-note.jpg");
        write_real_jpeg(&path);
        let mut bytes = fs::read(&path).unwrap();
        let mut meta = Metadata::new();
        meta.set_tag(ExifTag::MakerNote(
            b"SONY DSC \0\0\0maker-note-body".to_vec(),
        ));
        meta.write_to_vec(&mut bytes, FileExtension::JPEG).unwrap();
        fs::write(&path, &bytes).unwrap();

        let read = save_and_read_back(
            &path,
            &core::metadata::EditableMetadata {
                title: Some("Harbor at dusk".into()),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(read.metadata.title.as_deref(), Some("Harbor at dusk"));
        assert!(
            read.warnings.iter().any(|w| w.contains("MakerNote")),
            "{:?}",
            read.warnings
        );
    }
}
