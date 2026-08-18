pub mod exif_frame;
pub mod model_map;

use anyhow::{Context, Result};
use image::{DynamicImage, GenericImageView, RgbaImage};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use walkdir::WalkDir;

// --- 型定義 ---

/// 変換モード
///
/// `clap` feature を有効にすると `ValueEnum` が derive され、CLI が
/// この enum を直接引数型に使える。以前は cli 側に同じ enum が複製されており、
/// core にモードを増やしてもコンパイルエラーにならなかった（S6-CLI-3）。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[serde(rename_all = "lowercase")]
pub enum ConversionMode {
    Crop,
    Pad,
    Quality,
}

/// パディング時の背景色
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[serde(rename_all = "lowercase")]
pub enum BackgroundColor {
    White,
    Black,
}

impl BackgroundColor {
    pub fn to_rgba(&self) -> image::Rgba<u8> {
        match self {
            BackgroundColor::White => image::Rgba([255, 255, 255, 255]),
            BackgroundColor::Black => image::Rgba([0, 0, 0, 255]),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingConfig {
    pub mode: ConversionMode,
    pub bg_color: BackgroundColor,
    pub quality: u8,
    pub max_size_mb: usize,
    pub delete_originals: bool,
    /// 出力 4:5 キャンバスの幅の上限 (px)。None なら無制限（元の画素数を保つ）。
    /// 実効値は 4 の倍数に切り捨てられる。quality モードでは無視される。
    ///
    /// `#[serde(default)]` は GUI から来る JSON にこのフィールドが無くても
    /// デシリアライズが壊れないようにするため。
    #[serde(default)]
    pub max_width: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessResult {
    pub input_path: String,
    pub output_path: String,
    pub final_size_mb: f64,
    pub final_quality: Option<u8>,
    /// 品質を下限まで下げても `max_size_mb` を満たせなかった。
    /// 主機能であるサイズ制限が破られたことをサイレントにしないためのフラグ。
    #[serde(default)]
    pub size_limit_exceeded: bool,
    /// 処理は成功したが利用者に伝えるべき事象（EXIF 読み取り失敗、Exif フレームの
    /// フォールバック、元ファイル削除の失敗など）。core は自ら出力せず呼び出し元に委ねる。
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ExifInfo {
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens_model: Option<String>,
    pub focal_length: Option<String>,
    pub f_number: Option<String>,
    pub shutter_speed: Option<String>,
    pub iso: Option<u32>,
    pub date_taken: Option<String>,
    /// EXIF Orientation (1-8)。`None` は「タグ無し」＝変換不要。
    /// `image` クレートは読み込み時にこれを適用しないため、core 側で明示的に適用する。
    pub orientation: Option<u16>,
}

/// 進捗コールバック: (current, total) -> bool（falseでキャンセル）
///
/// `process_batch` は rayon で並列処理するため、このコールバックは**複数のワーカースレッドから
/// 同時に呼ばれる**。`current` は完了順の通し番号であり、`files` の添字とも昇順とも対応しない
/// （例: 3, 1, 2 の順で呼ばれうる）。実装側で共有状態を触る場合は自前で同期すること。
pub type ProgressCallback = Box<dyn Fn(usize, usize) -> bool + Send + Sync>;

/// `collect_image_files` の結果
#[derive(Debug, Default)]
pub struct CollectedImages {
    /// 収集できた画像ファイル
    pub files: Vec<PathBuf>,
    /// 権限エラー等で走査できずスキップしたエントリの説明。
    /// 空でない場合、`files` は入力フォルダーの全画像を網羅していない。
    pub skipped: Vec<String>,
}

// --- 公開API ---

/// 画像ファイルからEXIF情報を読み取る
/// ファイルが存在しない、またはEXIFデータがない場合はデフォルト値（Noneフィールド）を返す
pub fn read_exif_info(path: &Path) -> Result<ExifInfo> {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(ExifInfo::default()),
        Err(e) => {
            return Err(e).with_context(|| format!("Failed to open for EXIF: {}", path.display()))
        }
    };
    let mut bufreader = std::io::BufReader::new(file);
    let exif_data = match exif::Reader::new().read_from_container(&mut bufreader) {
        Ok(e) => e,
        Err(_) => return Ok(ExifInfo::default()),
    };

    let get_string = |tag: exif::Tag| -> Option<String> {
        exif_data.get_field(tag, exif::In::PRIMARY).map(|f| {
            f.display_value()
                .with_unit(&exif_data)
                .to_string()
                .trim_matches('"')
                .trim()
                .to_string()
        })
    };

    let iso = exif_data
        .get_field(exif::Tag::PhotographicSensitivity, exif::In::PRIMARY)
        .and_then(|f| match f.value {
            exif::Value::Short(ref v) => v.first().map(|&x| x as u32),
            exif::Value::Long(ref v) => v.first().copied(),
            _ => f.display_value().to_string().parse::<u32>().ok(),
        });

    let shutter_speed = exif_data
        .get_field(exif::Tag::ExposureTime, exif::In::PRIMARY)
        .map(|f| {
            let s = f.display_value().to_string();
            if s.ends_with(" s") {
                s.replace(" s", "s")
            } else {
                format!("{s}s")
            }
        });

    let focal_length = exif_data
        .get_field(exif::Tag::FocalLength, exif::In::PRIMARY)
        .map(|f| {
            let s = f.display_value().to_string();
            if s.ends_with(" mm") {
                s.replace(" mm", "mm")
            } else if s.ends_with("mm") {
                s
            } else {
                format!("{s}mm")
            }
        });

    let f_number = exif_data
        .get_field(exif::Tag::FNumber, exif::In::PRIMARY)
        .map(|f| {
            let s = f.display_value().to_string();
            if s.starts_with("f/") {
                s
            } else {
                format!("f/{s}")
            }
        });

    let orientation = exif_data
        .get_field(exif::Tag::Orientation, exif::In::PRIMARY)
        .and_then(|f| f.value.get_uint(0))
        .and_then(|v| u16::try_from(v).ok())
        .filter(|v| (1..=8).contains(v));

    Ok(ExifInfo {
        camera_make: get_string(exif::Tag::Make).map(|s| s.trim().to_string()),
        camera_model: get_string(exif::Tag::Model).map(|s| s.trim().to_string()),
        lens_model: get_string(exif::Tag::LensModel).map(|s| s.trim().to_string()),
        focal_length,
        f_number,
        shutter_speed,
        iso,
        date_taken: get_string(exif::Tag::DateTimeOriginal),
        orientation,
    })
}

/// EXIF Orientation を画像に適用して「見たままの向き」に正規化する
///
/// `image` 0.24 には Orientation の自動適用が無い。適用しないと以下がすべて壊れる:
/// 縦横判定（`auto_placement` が Exif バーを誤った辺に付ける）、4:5 変換の基準、
/// そして出力（再エンコードで元 EXIF が失われるため 90 度傾いたまま残る）。
/// パイプラインの入口で一度だけ適用し、以降は正立した画像だけを扱う。
pub fn apply_orientation(img: DynamicImage, orientation: Option<u16>) -> DynamicImage {
    match orientation {
        Some(2) => img.fliph(),
        Some(3) => img.rotate180(),
        Some(4) => img.flipv(),
        Some(5) => img.rotate90().fliph(),
        Some(6) => img.rotate90(),
        Some(7) => img.rotate270().fliph(),
        Some(8) => img.rotate270(),
        // 1（無変換）、タグ無し、範囲外はそのまま
        _ => img,
    }
}

/// EXIF Orientation 適用後の (幅, 高さ)
///
/// Orientation 5-8 は 90 度回転を含むため、生ピクセルの縦横が入れ替わる。
pub fn oriented_dimensions((width, height): (u32, u32), orientation: Option<u16>) -> (u32, u32) {
    match orientation {
        Some(5..=8) => (height, width),
        _ => (width, height),
    }
}

/// 画像を開き、EXIF Orientation を適用した状態で返す
///
/// 生の `image::open` を直接使うと向きが正規化されず、プレビューと出力がずれる。
/// 表示・処理を問わず、画像を開く入口はこの関数に揃えること。
pub fn open_image_oriented(path: &Path) -> Result<DynamicImage> {
    let img =
        image::open(path).with_context(|| format!("Failed to open image: {}", path.display()))?;
    let orientation = read_exif_info(path).ok().and_then(|info| info.orientation);
    Ok(apply_orientation(img, orientation))
}

/// 設定を検証する
pub fn validate_config(config: &ProcessingConfig) -> Result<()> {
    if config.quality == 0 || config.quality > 100 {
        anyhow::bail!("Quality must be between 1 and 100");
    }
    if config.max_size_mb == 0 {
        anyhow::bail!("max_size_mb must be at least 1");
    }
    // 上限そのものが壊れていると `k * 5` が u32 で溢れる。
    // 20000 は 20000x25000 の RGBA キャンバス（約 2GB）というメモリ側の線。
    if let Some(max_width) = config.max_width {
        if !(4..=20000).contains(&max_width) {
            anyhow::bail!("max_width must be between 4 and 20000");
        }
    }
    Ok(())
}

/// サポートされている画像形式かチェック
pub fn is_supported_image(path: &Path) -> bool {
    if let Some(ext) = path.extension() {
        let ext = ext.to_string_lossy().to_lowercase();
        matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "webp")
    } else {
        false
    }
}

/// 指定フォルダー内の画像ファイルを収集
///
/// 走査できなかったエントリは戻り値の `skipped` に載せる。
/// シンボリックリンクは辿らない（`follow_links(false)`）。
pub fn collect_image_files(dir: &Path) -> CollectedImages {
    let mut result = CollectedImages::default();

    for entry in WalkDir::new(dir).follow_links(false) {
        match entry {
            Ok(entry) => {
                // `Path::is_file` は fs::metadata 経由でリンクを常に解決してしまうため、
                // follow_links(false) の設定に従う file_type() で判定する。
                if entry.file_type().is_file() && is_supported_image(entry.path()) {
                    result.files.push(entry.into_path());
                }
            }
            Err(e) => result.skipped.push(e.to_string()),
        }
    }

    result
}

/// 画像を処理
pub fn process_image(
    input_path: &Path,
    output_folder: &Path,
    config: &ProcessingConfig,
    exif_frame_config: Option<&exif_frame::ExifFrameConfig>,
    assets: Option<&exif_frame::ExifAssets>,
) -> Result<ProcessResult> {
    // core はライブラリであり stdout/stderr を持たない（GUI には届かない）。
    // 処理は続行するが利用者に伝えるべき事象はここに積み、ProcessResult で呼び出し元へ返す。
    let mut warnings = Vec::new();

    // EXIF は Orientation の適用と Exif フレーム描画の両方で使うため一度だけ読む。
    let exif = match read_exif_info(input_path) {
        Ok(info) => info,
        Err(e) => {
            // 「EXIF が無い」は read_exif_info が default を返す正常系。
            // ここに来るのは破損・I/O エラーなので黙って握りつぶさない。
            warnings.push(format!(
                "Failed to read EXIF from {}: {}",
                input_path.display(),
                e
            ));
            ExifInfo::default()
        }
    };

    let img = image::open(input_path)
        .with_context(|| format!("Failed to open image: {}", input_path.display()))?;
    let img = apply_orientation(img, exif.orientation);

    // 出力キャンバス幅の上限。quality モードはアスペクト比を変えないため対象外
    // （「幅」を指定しても縦写真の長辺を縛れない / spec §2）。
    let target = match config.mode {
        ConversionMode::Quality => None,
        _ => target_canvas(config.max_width),
    };

    // 前段: pad は巨大な RGBA キャンバスを確保してから文字とロゴを描くため、
    // その前に写真を目標ボックスへ縮小してメモリを抑える。crop に入れないのは、
    // 切り落として捨てる画素まで Lanczos3 で再サンプルすることになるため
    // （crop → 最終縮小の順の方が安く、丸めも一度で済む / spec §4）。
    // 縮小方向にしか働かない: ガードを満たさなければ何もしない。
    let img = match (target, config.mode) {
        (Some((target_w, target_h)), ConversionMode::Pad)
            if exif_frame::layout::fit_to_4_5(img.width(), img.height()).0 > target_w =>
        {
            img.resize(target_w, target_h, image::imageops::FilterType::Lanczos3)
        }
        _ => img,
    };

    let converted = match config.mode {
        ConversionMode::Crop => convert_aspect_ratio_crop(img),
        ConversionMode::Pad => {
            if let (Some(ef_config), Some(assets)) = (exif_frame_config, assets) {
                match exif_frame::render_exif_frame(
                    &img,
                    &exif,
                    ef_config,
                    &config.bg_color,
                    assets,
                ) {
                    Ok(framed) => {
                        // core は自ら出力しない。フレームを諦めた等の事象は呼び出し元へ運ぶ。
                        warnings.extend(framed.warnings);
                        framed.image
                    }
                    Err(e) => {
                        warnings.push(format!(
                            "Exif frame failed, falling back to pad only: {}",
                            e
                        ));
                        convert_aspect_ratio_pad(img, config.bg_color)
                    }
                }
            } else {
                convert_aspect_ratio_pad(img, config.bg_color)
            }
        }
        ConversionMode::Quality => img,
    };

    // 最終: crop には前段が無いのでここがすべてを担う。pad では no-op になるが、
    // それはレイアウト実装に依存した不変条件なので、契約としてモードを問わず適用する。
    // 比較だけなので効いていないときの実行コストは無い（spec §4）。
    let converted = match target {
        Some((target_w, target_h)) if converted.width() > target_w => {
            converted.resize_exact(target_w, target_h, image::imageops::FilterType::Lanczos3)
        }
        _ => converted,
    };

    let max_size_bytes = config.max_size_mb * 1024 * 1024;
    let encoded = encode_within_size_limit(&converted, config.quality, max_size_bytes)?;
    let output_path = write_new_output_file(input_path, output_folder, &encoded.bytes)?;

    let final_size_mb = encoded.bytes.len() as f64 / (1024.0 * 1024.0);

    if !encoded.within_limit {
        warnings.push(format!(
            "Size limit not met: {:.1} MB > {} MB even at minimum quality {}",
            final_size_mb, config.max_size_mb, encoded.quality
        ));
    }

    // 成功時のみ元ファイルを削除
    if config.delete_originals {
        if let Err(e) = fs::remove_file(input_path) {
            warnings.push(format!(
                "Failed to delete original file {}: {}",
                input_path.display(),
                e
            ));
        }
    }

    Ok(ProcessResult {
        input_path: input_path.to_string_lossy().to_string(),
        output_path: output_path.to_string_lossy().to_string(),
        final_size_mb,
        final_quality: if encoded.quality < config.quality {
            Some(encoded.quality)
        } else {
            None
        },
        size_limit_exceeded: !encoded.within_limit,
        warnings,
    })
}

/// バッチ処理（並列）
/// キャンセル要求のため着手されなかった、を表すエラーメッセージ
///
/// `process_batch` は入力と同数・同順の結果を返すので、キャンセル後の分も
/// `Err` として並ぶ。呼び出し側が「変換に失敗した」と「そもそも着手していない」を
/// 区別できるよう、文言を定数として公開する（GUI はこれを未処理として扱う）。
pub const CANCELLED_ERROR: &str = "Processing cancelled";

pub fn process_batch(
    files: &[PathBuf],
    output_folder: &Path,
    config: &ProcessingConfig,
    exif_frame_config: Option<&exif_frame::ExifFrameConfig>,
    assets: Option<&exif_frame::ExifAssets>,
    on_progress: Option<ProgressCallback>,
) -> Vec<Result<ProcessResult>> {
    let total = files.len();
    let cancelled = Arc::new(AtomicBool::new(false));
    let processed_count = AtomicUsize::new(0);

    files
        .par_iter()
        .map(|path| {
            if cancelled.load(Ordering::Relaxed) {
                return Err(anyhow::anyhow!(CANCELLED_ERROR));
            }

            let result = process_image(path, output_folder, config, exif_frame_config, assets);

            let current = processed_count.fetch_add(1, Ordering::SeqCst) + 1;
            if let Some(ref cb) = on_progress {
                if !cb(current, total) {
                    cancelled.store(true, Ordering::Relaxed);
                }
            }

            result
        })
        .collect()
}

/// サムネイルの一辺の上限。これを超える要求は丸められる。
///
/// 呼び出し側がサムネイルをキャッシュする場合、キーには丸めた後の値を使うこと。
/// 生の要求値をキーにすると、同一内容のエントリを要求値の数だけ作れてしまう。
pub const THUMBNAIL_MAX_DIMENSION: u32 = 1024;

/// サムネイルをbase64エンコードされたJPEG文字列として生成
pub fn generate_thumbnail_base64(path: &Path, max_dimension: u32) -> Result<String> {
    use base64::Engine as _;
    let max_dimension = max_dimension.min(THUMBNAIL_MAX_DIMENSION);

    // 出力と同じく Orientation を適用する。適用しないとサムネイルと変換結果の向きがずれる。
    let img = open_image_oriented(path)
        .with_context(|| format!("Failed to open image for thumbnail: {}", path.display()))?;

    let thumbnail = img.thumbnail(max_dimension, max_dimension);
    let jpeg_bytes = encode_jpeg_rgb(&thumbnail.to_rgb8(), 75)?;

    Ok(base64::engine::general_purpose::STANDARD.encode(&jpeg_bytes))
}

/// `generate_exif_frame_preview_base64` の結果
#[derive(Debug)]
pub struct ExifFramePreview {
    /// data URI の prefix を含まない生の base64 JPEG
    pub base64: String,
    /// フレーム描画由来の警告。
    ///
    /// **GUI はこれを利用者に出さない。** プレビューは長辺 400px 固定なので、
    /// 実出力ではフレームが出る写真でも `skip_exif` に落ちて偽陽性になる。
    /// その判断は GUI 固有の事情なので境界（`gui/src/commands.rs`）が行う。
    /// core 側で握り潰すと、将来 CLI プレビューを作ったときに理由の分からない
    /// 握り潰しが残る（spec §8）。
    pub warnings: Vec<String>,
}

/// Exifフレームのプレビューをbase64エンコードされたJPEG文字列として生成
///
/// GUI 専用ではなく core に置く。以前は「縮小 → 描画 → JPEG → base64」の一連が
/// Tauri コマンドの中だけに書かれており、CLI からプレビューを作れず、
/// GUI が `image` / `base64` に直接依存する原因にもなっていた（S6-M16）。
pub fn generate_exif_frame_preview_base64(
    path: &Path,
    config: &exif_frame::ExifFrameConfig,
    bg_color: &BackgroundColor,
    assets: &exif_frame::ExifAssets,
    max_dimension: u32,
) -> Result<ExifFramePreview> {
    use base64::Engine as _;
    let max_dimension = max_dimension.clamp(1, 1024);

    // Orientation を適用してから縮小する。生の image::open だと縦横が実際の
    // 処理結果と食い違い、auto_placement が別の辺を選んでしまう。
    let img = open_image_oriented(path)?;
    let thumbnail = img.resize(
        max_dimension,
        max_dimension,
        image::imageops::FilterType::Triangle,
    );

    let exif = read_exif_info(path).unwrap_or_default();
    let framed = exif_frame::render_exif_frame(&thumbnail, &exif, config, bg_color, assets)?;
    let jpeg_bytes = encode_jpeg_rgb(&framed.image.to_rgb8(), 85)?;

    Ok(ExifFramePreview {
        base64: base64::engine::general_purpose::STANDARD.encode(&jpeg_bytes),
        warnings: framed.warnings,
    })
}

/// 画像ファイルの表示上の (幅, 高さ) を返す（デコードせずヘッダのみ読む）
///
/// EXIF Orientation 5-8 では生ピクセルの縦横が入れ替わるため、
/// 一覧表示の値も `open_image_oriented` 後の見え方に揃える。
pub fn image_dimensions_oriented(path: &Path) -> Result<(u32, u32)> {
    let raw = image::image_dimensions(path)
        .with_context(|| format!("Failed to read image dimensions: {}", path.display()))?;
    let orientation = read_exif_info(path).ok().and_then(|info| info.orientation);
    Ok(oriented_dimensions(raw, orientation))
}

/// フル解像度画像をbase64エンコードされたJPEG文字列として生成（プレビュー用）
pub fn generate_full_image_base64(path: &Path, max_width: u32, max_height: u32) -> Result<String> {
    use base64::Engine as _;

    let max_width = max_width.min(2560);
    let max_height = max_height.min(1600);

    let img = open_image_oriented(path)?;

    let (w, h) = img.dimensions();

    let resized = if w > max_width || h > max_height {
        img.resize(max_width, max_height, image::imageops::FilterType::Lanczos3)
    } else {
        img
    };

    let jpeg_bytes = encode_jpeg_rgb(&resized.to_rgb8(), 90)?;

    Ok(base64::engine::general_purpose::STANDARD.encode(&jpeg_bytes))
}

// --- プライベートヘルパー ---

/// 目標キャンバスサイズ。キャンバスは常に k*4 × k*5（S4 で確立した不変条件）。
///
/// 丸めは切り捨てのみ。切り上げて 1002 → 1004 になったら「指定値を超えない」という
/// 機能の目的を果たさない。
///
/// 範囲チェックの本線は `validate_config`（4..=20000）。`.max(1)` はそれを通さずに
/// core を直接使う利用者への安全網で、目標が 0x0 になって `resize_exact` が
/// 壊れるのを release ビルドでも防ぐ。`debug_assert!` では release で消える。
fn target_canvas(max_width: Option<u32>) -> Option<(u32, u32)> {
    let k = (max_width? / 4).max(1); // 切り捨て
    Some((k * 4, k * 5))
}

/// 4:5のアスペクト比に変換 (中央クロップ)
fn convert_aspect_ratio_crop(img: DynamicImage) -> DynamicImage {
    let (width, height) = img.dimensions();
    let target_ratio = 4.0 / 5.0;
    let current_ratio = width as f64 / height as f64;

    if (current_ratio - target_ratio).abs() < 0.001 {
        return img;
    }

    let (crop_width, crop_height) = if current_ratio > target_ratio {
        // 横長すぎる → 幅を削る
        let new_width = (height as f64 * target_ratio).round() as u32;
        (new_width, height)
    } else {
        // 縦長すぎる → 高さを削る
        let new_height = (width as f64 / target_ratio).round() as u32;
        (width, new_height)
    };

    let x = (width.saturating_sub(crop_width)) / 2;
    let y = (height.saturating_sub(crop_height)) / 2;

    img.crop_imm(x, y, crop_width, crop_height)
}

/// 4:5のアスペクト比に変換 (パディング)
///
/// キャンバスサイズは `fit_to_4_5` に一本化してある。以前はここだけ `width / 0.8` を
/// 自前計算し、比率差 0.001 未満は素通りしていたため、「pad が作るキャンバス幅」の
/// 答えが2つあった。`--max-width` の前段スケールはこの値に依存するので、
/// 食い違うと上限の保証が崩れる（spec 2026-08-12 §4）。
fn convert_aspect_ratio_pad(img: DynamicImage, bg_color: BackgroundColor) -> DynamicImage {
    let (width, height) = img.dimensions();
    let (new_width, new_height) = exif_frame::layout::fit_to_4_5(width, height);

    // 既に厳密な 4:5 ならコピーを作らない
    if (new_width, new_height) == (width, height) {
        return img;
    }

    let mut canvas = RgbaImage::from_pixel(new_width, new_height, bg_color.to_rgba());

    let x = (new_width.saturating_sub(width)) / 2;
    let y = (new_height.saturating_sub(height)) / 2;

    image::imageops::overlay(&mut canvas, &img.to_rgba8(), x.into(), y.into());

    DynamicImage::ImageRgba8(canvas)
}

/// 出力ファイルを新規作成し、内容を書き込んで確定したパスを返す（重複時は連番）
///
/// `exists()` で調べてから作る方式は TOCTOU であり、`process_batch` は常に並列なので
/// `sub1/photo.jpg` と `sub2/photo.jpg` を処理する2スレッドが同じ `photo_processed.jpg` を
/// 掴んで片方を破壊しうる。`create_new(true)` は「作成」と「不在の確認」を OS 側で
/// 不可分に行うため、勝者が1スレッドに定まる。
fn write_new_output_file(input_path: &Path, output_folder: &Path, bytes: &[u8]) -> Result<PathBuf> {
    // to_string_lossy は非UTF-8名を U+FFFD に丸め、異なる元ファイルを同じ stem に潰す。
    // OsString のまま組み立てて元の名前を保つ。
    let stem = input_path
        .file_stem()
        .context("Failed to get file stem")?
        .to_os_string();

    let mut counter: u32 = 0;
    loop {
        let mut filename = stem.clone();
        if counter == 0 {
            filename.push("_processed.jpg");
        } else {
            filename.push(format!("_processed_{}.jpg", counter));
        }
        let candidate = output_folder.join(filename);

        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => {
                // 予約はできたので、以降の失敗では中途半端なファイルを残さない。
                if let Err(e) = write_all_and_flush(file, bytes) {
                    let _ = fs::remove_file(&candidate);
                    return Err(e)
                        .with_context(|| format!("Failed to write: {}", candidate.display()));
                }
                return Ok(candidate);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => counter += 1,
            Err(e) => {
                return Err(e)
                    .with_context(|| format!("Failed to create file: {}", candidate.display()))
            }
        }
    }
}

fn write_all_and_flush(file: File, bytes: &[u8]) -> std::io::Result<()> {
    let mut writer = BufWriter::new(file);
    writer.write_all(bytes)?;
    writer.flush()
}

/// サイズ制限を満たす JPEG バイト列
struct EncodedJpeg {
    bytes: Vec<u8>,
    /// 実際に採用した品質
    quality: u8,
    /// `max_size_bytes` を満たせたか（false なら下限品質でも超過している）
    within_limit: bool,
}

/// サイズ制限を満たすまで品質を下げながらメモリ上で JPEG にエンコードする
///
/// ディスクへの書き出しは確定した1回分だけを呼び出し元が行う。
/// 試行のたびに書いて消す方式は最大7往復の I/O を生み、一時ファイル名の衝突源でもあった。
fn encode_within_size_limit(
    img: &DynamicImage,
    initial_quality: u8,
    max_size_bytes: usize,
) -> Result<EncodedJpeg> {
    const MIN_QUALITY: u8 = 60;
    const QUALITY_STEP: u8 = 5;

    let rgb_img = img.to_rgb8();
    let mut quality = initial_quality;

    loop {
        let bytes = encode_jpeg_rgb(&rgb_img, quality)?;

        if bytes.len() <= max_size_bytes {
            return Ok(EncodedJpeg {
                bytes,
                quality,
                within_limit: true,
            });
        }
        if quality <= MIN_QUALITY {
            return Ok(EncodedJpeg {
                bytes,
                quality,
                within_limit: false,
            });
        }

        quality = quality.saturating_sub(QUALITY_STEP).max(MIN_QUALITY);
    }
}

/// RgbImage を JPEG バイト列にエンコード
fn encode_jpeg_rgb(rgb_img: &image::RgbImage, quality: u8) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut bytes);
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut cursor, quality)
        .encode(
            rgb_img.as_raw(),
            rgb_img.width(),
            rgb_img.height(),
            image::ColorType::Rgb8,
        )
        .context("Failed to encode JPEG")?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgb};
    use std::fs;
    use std::sync::atomic::AtomicUsize;
    use tempfile::TempDir;

    fn test_config() -> ProcessingConfig {
        ProcessingConfig {
            mode: ConversionMode::Crop,
            bg_color: BackgroundColor::White,
            quality: 90,
            max_size_mb: 8,
            delete_originals: false,
            max_width: None,
        }
    }

    /// テスト用のRGB画像を指定サイズで生成しJPEGとして保存
    fn create_test_image(path: &Path, width: u32, height: u32) {
        let img = ImageBuffer::from_fn(width, height, |x, y| {
            Rgb([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8])
        });
        img.save(path).unwrap();
    }

    /// JPEGがほとんど圧縮できない高エントロピー画像を生成する
    /// （決定的な線形合同法。サイズ制限を満たせないケースを再現するため）
    fn create_incompressible_image(path: &Path, width: u32, height: u32) {
        let mut state: u32 = 0x1234_5678;
        let img = ImageBuffer::from_fn(width, height, |_, _| {
            let mut next = || {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (state >> 24) as u8
            };
            Rgb([next(), next(), next()])
        });
        img.save(path).unwrap();
    }

    /// 指定した EXIF Orientation を持つ JPEG を生成する
    ///
    /// kamadak-exif は読み取り専用なので、SOI 直後に APP1 セグメントを差し込んで
    /// Orientation タグ1件だけの最小 TIFF 構造を手で組み立てる。
    fn create_test_image_with_orientation(path: &Path, width: u32, height: u32, orientation: u16) {
        let img = ImageBuffer::from_fn(width, height, |x, y| {
            Rgb([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8])
        });
        let mut jpeg = Vec::new();
        img.write_to(
            &mut std::io::Cursor::new(&mut jpeg),
            image::ImageOutputFormat::Jpeg(90),
        )
        .unwrap();

        let mut app1 = Vec::new();
        app1.extend_from_slice(b"Exif\0\0");
        app1.extend_from_slice(b"II\x2a\x00"); // リトルエンディアンTIFFヘッダ
        app1.extend_from_slice(&8u32.to_le_bytes()); // IFD0 のオフセット
        app1.extend_from_slice(&1u16.to_le_bytes()); // エントリ数
        app1.extend_from_slice(&0x0112u16.to_le_bytes()); // Orientation タグ
        app1.extend_from_slice(&3u16.to_le_bytes()); // 型: SHORT
        app1.extend_from_slice(&1u32.to_le_bytes()); // 個数
        app1.extend_from_slice(&orientation.to_le_bytes());
        app1.extend_from_slice(&0u16.to_le_bytes()); // 値領域の余り
        app1.extend_from_slice(&0u32.to_le_bytes()); // 次IFDなし

        let mut out = Vec::new();
        out.extend_from_slice(&jpeg[..2]); // SOI
        out.extend_from_slice(&[0xFF, 0xE1]);
        out.extend_from_slice(&((app1.len() + 2) as u16).to_be_bytes());
        out.extend_from_slice(&app1);
        out.extend_from_slice(&jpeg[2..]);

        fs::write(path, out).unwrap();
    }

    // =========================================================
    // バリデーション
    // =========================================================

    #[test]
    fn validate_config_accepts_boundary_values() {
        let mut config = test_config();
        config.quality = 1;
        assert!(validate_config(&config).is_ok());
        config.quality = 100;
        assert!(validate_config(&config).is_ok());
    }

    #[test]
    fn validate_config_rejects_zero_and_over_100() {
        let mut config = test_config();
        config.quality = 0;
        assert!(validate_config(&config).is_err());
        config.quality = 101;
        assert!(validate_config(&config).is_err());
    }

    // =========================================================
    // 画像形式判定
    // =========================================================

    #[test]
    fn is_supported_image_recognizes_all_formats() {
        for ext in &["jpg", "jpeg", "JPG", "JPEG", "png", "PNG", "webp", "WEBP"] {
            assert!(
                is_supported_image(Path::new(&format!("photo.{}", ext))),
                "should accept .{}",
                ext
            );
        }
    }

    #[test]
    fn is_supported_image_rejects_non_image_formats() {
        for ext in &["pdf", "txt", "mp4", "gif", "bmp", "tiff", ""] {
            let path = if ext.is_empty() {
                "noext".to_string()
            } else {
                format!("file.{}", ext)
            };
            assert!(
                !is_supported_image(Path::new(&path)),
                "should reject .{}",
                ext
            );
        }
    }

    // =========================================================
    // ファイル収集
    // =========================================================

    #[test]
    fn collect_image_files_finds_images_in_subdirectories() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("subdir");
        fs::create_dir(&sub).unwrap();

        create_test_image(&dir.path().join("root.jpg"), 10, 10);
        create_test_image(&sub.join("nested.png"), 10, 10);
        fs::write(dir.path().join("readme.txt"), b"text").unwrap();

        let files = collect_image_files(dir.path()).files;
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn collect_image_files_returns_empty_for_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let files = collect_image_files(dir.path()).files;
        assert!(files.is_empty());
    }

    // =========================================================
    // Cropモード: 実際の画像でアスペクト比を検証
    // =========================================================

    #[test]
    fn crop_mode_produces_4_5_aspect_ratio_from_landscape() {
        let dir = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        let input = dir.path().join("landscape.jpg");
        // 横長画像 (1000x600, ratio=1.67)
        create_test_image(&input, 1000, 600);

        let config = ProcessingConfig {
            mode: ConversionMode::Crop,
            ..test_config()
        };
        let result = process_image(&input, out.path(), &config, None, None).unwrap();

        let output_img = image::open(&result.output_path).unwrap();
        let (w, h) = output_img.dimensions();
        let ratio = w as f64 / h as f64;
        assert!(
            (ratio - 0.8).abs() < 0.02,
            "crop結果のアスペクト比が4:5でない: {}x{} (ratio={})",
            w,
            h,
            ratio
        );
    }

    #[test]
    fn crop_mode_produces_4_5_aspect_ratio_from_portrait() {
        let dir = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        let input = dir.path().join("portrait.jpg");
        // 縦長画像 (600x1200, ratio=0.5)
        create_test_image(&input, 600, 1200);

        let config = ProcessingConfig {
            mode: ConversionMode::Crop,
            ..test_config()
        };
        let result = process_image(&input, out.path(), &config, None, None).unwrap();

        let output_img = image::open(&result.output_path).unwrap();
        let (w, h) = output_img.dimensions();
        let ratio = w as f64 / h as f64;
        assert!(
            (ratio - 0.8).abs() < 0.02,
            "crop結果のアスペクト比が4:5でない: {}x{} (ratio={})",
            w,
            h,
            ratio
        );
    }

    // =========================================================
    // Padモード: アスペクト比とサイズが元画像以上であることを検証
    // =========================================================

    #[test]
    fn pad_mode_produces_4_5_and_preserves_original_content() {
        let dir = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        let input = dir.path().join("wide.jpg");
        // 横長画像
        create_test_image(&input, 800, 400);

        let config = ProcessingConfig {
            mode: ConversionMode::Pad,
            bg_color: BackgroundColor::White,
            ..test_config()
        };
        let result = process_image(&input, out.path(), &config, None, None).unwrap();

        let output_img = image::open(&result.output_path).unwrap();
        let (w, h) = output_img.dimensions();
        // 4:5 は「おおむね 0.8」ではなく厳密な整数比（k*4 x k*5）
        assert_eq!(
            (w, h),
            (800, 1000),
            "800x400 は 800x1000 にパディングされる"
        );
        assert_eq!(w * 5, h * 4, "canvas must be exactly 4:5");
        // パディングは元画像以上のサイズになる（写真が欠けない）
        assert!(w >= 800 && h >= 400, "元画像がキャンバスに収まっていない");
    }

    #[test]
    fn pad_mode_with_black_background() {
        let dir = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        let input = dir.path().join("tall.jpg");
        create_test_image(&input, 400, 800);

        let config = ProcessingConfig {
            mode: ConversionMode::Pad,
            bg_color: BackgroundColor::Black,
            ..test_config()
        };
        let result = process_image(&input, out.path(), &config, None, None).unwrap();
        assert!(Path::new(&result.output_path).exists());

        let (w, h) = image::open(&result.output_path).unwrap().dimensions();
        assert_eq!(
            (w, h),
            (640, 800),
            "400x800 は左右にパディングされて 640x800"
        );
        assert_eq!(w * 5, h * 4, "canvas must be exactly 4:5");
    }

    /// 仕様: pad の出力キャンバスは `fit_to_4_5` と同じ厳密な k*4 x k*5（spec §4 / §9 #5）。
    ///
    /// 400x501 は比率差 0.0016 で旧実装の早期 return 帯の外にあるが、
    /// `round(501 * 0.8) = 401` により 401x501（401*5=2005, 501*4=2004）を出していた。
    #[test]
    fn pad_mode_produces_an_exact_4_5_canvas_for_a_rounded_size() {
        let dir = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        let input = dir.path().join("near_4_5.jpg");
        create_test_image(&input, 400, 501);

        let config = ProcessingConfig {
            mode: ConversionMode::Pad,
            ..test_config()
        };
        let result = process_image(&input, out.path(), &config, None, None).unwrap();

        let (w, h) = image::open(&result.output_path).unwrap().dimensions();
        assert_eq!(
            (w, h),
            (404, 505),
            "400x501 が収まる最小の 4:5 キャンバスは k=101"
        );
        assert_eq!(w * 5, h * 4, "canvas must be exactly 4:5");
    }

    /// 仕様: 「ほぼ 4:5」の入力もパディングを省略されない（spec §4）。
    ///
    /// 800x1001 は比率差 0.0008 で旧実装の早期 return 帯に入り、
    /// 800x1001（800*5=4000, 1001*4=4004）のまま素通りしていた。
    #[test]
    fn pad_mode_does_not_pass_through_almost_4_5_input() {
        let dir = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        let input = dir.path().join("almost_4_5.jpg");
        create_test_image(&input, 800, 1001);

        let config = ProcessingConfig {
            mode: ConversionMode::Pad,
            ..test_config()
        };
        let result = process_image(&input, out.path(), &config, None, None).unwrap();

        let (w, h) = image::open(&result.output_path).unwrap().dimensions();
        assert_eq!(
            (w, h),
            (804, 1005),
            "800x1001 が収まる最小の 4:5 キャンバスは k=201"
        );
        assert_eq!(w * 5, h * 4, "canvas must be exactly 4:5");
    }

    // =========================================================
    // Qualityモード: アスペクト比は変わらない
    // =========================================================

    #[test]
    fn quality_mode_preserves_original_aspect_ratio() {
        let dir = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        let input = dir.path().join("original.jpg");
        create_test_image(&input, 1600, 900);

        let config = ProcessingConfig {
            mode: ConversionMode::Quality,
            ..test_config()
        };
        let result = process_image(&input, out.path(), &config, None, None).unwrap();

        let output_img = image::open(&result.output_path).unwrap();
        let (w, h) = output_img.dimensions();
        let original_ratio = 1600.0 / 900.0;
        let output_ratio = w as f64 / h as f64;
        assert!(
            (output_ratio - original_ratio).abs() < 0.02,
            "quality modeでアスペクト比が変わった: original={}, output={}",
            original_ratio,
            output_ratio
        );
    }

    // =========================================================
    // 出力ファイル: 命名規則と重複回避
    // =========================================================

    #[test]
    fn output_file_naming_adds_processed_suffix() {
        let dir = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        let input = dir.path().join("photo.jpg");
        create_test_image(&input, 400, 500);

        let result = process_image(&input, out.path(), &test_config(), None, None).unwrap();
        assert!(
            result.output_path.ends_with("photo_processed.jpg"),
            "出力ファイル名が不正: {}",
            result.output_path
        );
    }

    #[test]
    fn output_file_naming_handles_duplicate_names() {
        let dir = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        let input = dir.path().join("dup.jpg");
        create_test_image(&input, 400, 500);

        // 1回目
        let r1 = process_image(&input, out.path(), &test_config(), None, None).unwrap();
        assert!(r1.output_path.ends_with("dup_processed.jpg"));

        // 2回目 — 同じ入力で重複
        let r2 = process_image(&input, out.path(), &test_config(), None, None).unwrap();
        assert!(
            r2.output_path.ends_with("dup_processed_1.jpg"),
            "重複時の連番が不正: {}",
            r2.output_path
        );
    }

    // =========================================================
    // delete_originals: 成功時に削除、失敗時は保持
    // =========================================================

    #[test]
    fn delete_originals_removes_source_on_success() {
        let dir = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        let input = dir.path().join("to_delete.jpg");
        create_test_image(&input, 400, 500);

        let config = ProcessingConfig {
            delete_originals: true,
            ..test_config()
        };
        let result = process_image(&input, out.path(), &config, None, None);
        assert!(result.is_ok());
        assert!(
            !input.exists(),
            "delete_originals=trueなのに元ファイルが残っている"
        );
    }

    #[test]
    fn delete_originals_false_keeps_source() {
        let dir = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        let input = dir.path().join("keep.jpg");
        create_test_image(&input, 400, 500);

        let config = ProcessingConfig {
            delete_originals: false,
            ..test_config()
        };
        process_image(&input, out.path(), &config, None, None).unwrap();
        assert!(
            input.exists(),
            "delete_originals=falseなのに元ファイルが削除された"
        );
    }

    // =========================================================
    // process_batch: 並列処理と進捗コールバック
    // =========================================================

    #[test]
    fn process_batch_processes_all_files() {
        let dir = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();

        let files: Vec<PathBuf> = (0..5)
            .map(|i| {
                let p = dir.path().join(format!("img_{}.jpg", i));
                create_test_image(&p, 400, 500);
                p
            })
            .collect();

        let results = process_batch(&files, out.path(), &test_config(), None, None, None);
        let success_count = results.iter().filter(|r| r.is_ok()).count();
        assert_eq!(success_count, 5, "5枚すべて処理成功すべき");
    }

    #[test]
    fn process_batch_progress_callback_receives_correct_counts() {
        let dir = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();

        let files: Vec<PathBuf> = (0..3)
            .map(|i| {
                let p = dir.path().join(format!("cb_{}.jpg", i));
                create_test_image(&p, 400, 500);
                p
            })
            .collect();

        let max_seen = Arc::new(AtomicUsize::new(0));
        let max_clone = Arc::clone(&max_seen);
        let total_seen = Arc::new(AtomicUsize::new(0));
        let total_clone = Arc::clone(&total_seen);

        let cb: ProgressCallback = Box::new(move |current, total| {
            max_clone.fetch_max(current, Ordering::SeqCst);
            total_clone.store(total, Ordering::SeqCst);
            true
        });

        process_batch(&files, out.path(), &test_config(), None, None, Some(cb));

        assert_eq!(
            max_seen.load(Ordering::SeqCst),
            3,
            "最大currentは3であるべき"
        );
        assert_eq!(total_seen.load(Ordering::SeqCst), 3, "totalは3であるべき");
    }

    #[test]
    fn process_batch_cancellation_stops_remaining_items() {
        let dir = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();

        // 多めの画像を作成（rayon並列でもキャンセルが効くように）
        let count = 100;
        let files: Vec<PathBuf> = (0..count)
            .map(|i| {
                let p = dir.path().join(format!("cancel_{}.jpg", i));
                create_test_image(&p, 400, 500);
                p
            })
            .collect();

        // 1枚処理完了後にキャンセル
        let cb: ProgressCallback = Box::new(|current, _total| current < 1);

        let results = process_batch(&files, out.path(), &test_config(), None, None, Some(cb));
        let cancelled_count = results
            .iter()
            .filter(|r| {
                r.as_ref()
                    .err()
                    .is_some_and(|e| e.to_string().contains("cancelled"))
            })
            .count();

        // キャンセルされた結果が少なくとも1つ存在する
        assert!(
            cancelled_count > 0,
            "キャンセルされた処理が1つもない（success={}, cancelled={}, total={}）",
            results.iter().filter(|r| r.is_ok()).count(),
            cancelled_count,
            count
        );
    }

    // =========================================================
    // サムネイル生成
    // =========================================================

    #[test]
    fn generate_thumbnail_returns_valid_base64_jpeg() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("thumb_src.jpg");
        create_test_image(&input, 2000, 2500);

        let base64_str = generate_thumbnail_base64(&input, 200).unwrap();

        // base64デコードしてJPEGとして読めることを確認
        use base64::Engine as _;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&base64_str)
            .expect("base64デコード失敗");

        let cursor = std::io::Cursor::new(bytes);
        let thumb =
            image::load(cursor, image::ImageFormat::Jpeg).expect("サムネイルがJPEGとして読めない");

        let (w, h) = thumb.dimensions();
        assert!(
            w <= 200 && h <= 200,
            "サムネイルが200px以内に収まっていない: {}x{}",
            w,
            h
        );
    }

    #[test]
    fn generate_thumbnail_for_nonexistent_file_returns_error() {
        let result = generate_thumbnail_base64(Path::new("/nonexistent/image.jpg"), 200);
        assert!(result.is_err());
    }

    #[test]
    fn generate_full_image_returns_valid_base64_jpeg() {
        let dir = tempfile::tempdir().unwrap();
        let img_path = dir.path().join("test.jpg");
        let img = image::RgbImage::from_fn(100, 100, |_, _| image::Rgb([128, 128, 128]));
        img.save(&img_path).unwrap();

        let result = generate_full_image_base64(&img_path, 50, 50).unwrap();
        assert!(!result.is_empty());

        use base64::Engine as _;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&result)
            .unwrap();
        assert!(!bytes.is_empty());
    }

    #[test]
    fn generate_full_image_clamps_resolution_to_max() {
        let dir = tempfile::tempdir().unwrap();
        let img_path = dir.path().join("test.jpg");
        let img = image::RgbImage::from_fn(100, 100, |_, _| image::Rgb([128, 128, 128]));
        img.save(&img_path).unwrap();

        let result = generate_full_image_base64(&img_path, 10000, 10000).unwrap();
        assert!(!result.is_empty());
    }

    // =========================================================
    // ファイルサイズ制限
    // =========================================================

    #[test]
    fn output_file_is_valid_jpeg() {
        let dir = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        let input = dir.path().join("valid.jpg");
        create_test_image(&input, 800, 1000);

        let result = process_image(&input, out.path(), &test_config(), None, None).unwrap();
        let output_img = image::open(&result.output_path);
        assert!(output_img.is_ok(), "出力ファイルが有効な画像として開けない");
        assert!(result.final_size_mb > 0.0, "ファイルサイズが0");
    }

    // =========================================================
    // ExifInfo
    // =========================================================

    #[test]
    fn read_exif_info_returns_default_for_nonexistent_file() {
        let result = read_exif_info(Path::new("/nonexistent/image.jpg"));
        assert!(result.is_ok());
        let info = result.unwrap();
        assert!(info.camera_make.is_none());
        assert!(info.camera_model.is_none());
        assert!(info.iso.is_none());
    }

    // =========================================================
    // serde: JSON直列化がTauriと互換
    // =========================================================

    #[test]
    fn processing_config_serializes_to_expected_json() {
        let config = test_config();
        let json = serde_json::to_value(&config).unwrap();
        assert_eq!(json["mode"], "crop");
        assert_eq!(json["bg_color"], "white");
        assert_eq!(json["quality"], 90);
        assert_eq!(json["delete_originals"], false);
    }

    #[test]
    fn processing_config_deserializes_from_frontend_json() {
        // フロントエンドから送られてくるJSON形式
        let json = r#"{
            "mode": "pad",
            "bg_color": "black",
            "quality": 75,
            "max_size_mb": 4,
            "delete_originals": true
        }"#;
        let config: ProcessingConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.mode, ConversionMode::Pad);
        assert_eq!(config.bg_color, BackgroundColor::Black);
        assert_eq!(config.quality, 75);
        assert!(config.delete_originals);
    }

    // =========================================================
    // validate_config: max_size_mb バリデーション
    // =========================================================

    #[test]
    fn validate_config_rejects_zero_max_size() {
        let mut config = test_config();
        config.max_size_mb = 0;
        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn validate_config_accepts_valid_max_size() {
        let mut config = test_config();
        config.max_size_mb = 1;
        assert!(validate_config(&config).is_ok());
        config.max_size_mb = 50;
        assert!(validate_config(&config).is_ok());
    }

    // =========================================================
    // ファイルサイズ制限の実効性
    // =========================================================

    #[test]
    fn save_with_size_limit_actually_reduces_quality() {
        let dir = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        let input = dir.path().join("large.jpg");
        // 大きめの画像を生成
        create_test_image(&input, 4000, 5000);

        let config = ProcessingConfig {
            mode: ConversionMode::Quality,
            max_size_mb: 1,
            quality: 95,
            ..test_config()
        };
        let result = process_image(&input, out.path(), &config, None, None).unwrap();

        // 1MB以下または品質がMIN_QUALITYまで下がっていること
        assert!(
            result.final_size_mb <= 1.0 || result.final_quality == Some(60),
            "サイズ制限が機能していない: size={:.2}MB, quality={:?}",
            result.final_size_mb,
            result.final_quality
        );
    }

    // =========================================================
    // エッジケース: 極小画像
    // =========================================================

    #[test]
    fn crop_mode_handles_tiny_image() {
        let dir = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        let input = dir.path().join("tiny.jpg");
        create_test_image(&input, 2, 3);

        let config = ProcessingConfig {
            mode: ConversionMode::Crop,
            ..test_config()
        };
        let result = process_image(&input, out.path(), &config, None, None);
        assert!(result.is_ok());
    }

    #[test]
    fn pad_mode_handles_tiny_image() {
        let dir = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        let input = dir.path().join("tiny.jpg");
        create_test_image(&input, 2, 3);

        let config = ProcessingConfig {
            mode: ConversionMode::Pad,
            ..test_config()
        };
        let result = process_image(&input, out.path(), &config, None, None);
        assert!(result.is_ok());
    }

    // =========================================================
    // PNG入力の変換
    // =========================================================

    #[test]
    fn process_image_handles_png_input() {
        let dir = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        let input = dir.path().join("photo.png");
        create_test_image(&input, 800, 1000);

        let result = process_image(&input, out.path(), &test_config(), None, None).unwrap();
        assert!(
            result.output_path.ends_with(".jpg"),
            "出力はJPEGであるべき: {}",
            result.output_path
        );
        let output_img = image::open(&result.output_path);
        assert!(output_img.is_ok());
    }

    // =========================================================
    // 並列バッチ処理: 同名入力でも出力を潰し合わない (C2)
    // =========================================================

    #[test]
    fn process_batch_gives_each_input_a_distinct_output() {
        // collect_image_files はサブディレクトリを再帰走査するので
        // sub_0/photo.jpg と sub_1/photo.jpg は日常的に同時処理される。
        // 幅を1枚ずつ変えることで「別の画像に上書きされた」ことも検出する。
        const COUNT: u32 = 20;
        let dir = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();

        let files: Vec<PathBuf> = (0..COUNT)
            .map(|i| {
                let sub = dir.path().join(format!("sub_{}", i));
                fs::create_dir(&sub).unwrap();
                let p = sub.join("photo.jpg");
                create_test_image(&p, 100 + i * 10, 200);
                p
            })
            .collect();

        let config = ProcessingConfig {
            mode: ConversionMode::Quality, // 寸法を保つことで内容の同一性を追える
            ..test_config()
        };
        let results = process_batch(&files, out.path(), &config, None, None, None);

        let ok: Vec<&ProcessResult> = results.iter().filter_map(|r| r.as_ref().ok()).collect();
        assert_eq!(ok.len(), COUNT as usize, "全件成功すべき");

        let unique_paths: std::collections::HashSet<&str> =
            ok.iter().map(|r| r.output_path.as_str()).collect();
        assert_eq!(
            unique_paths.len(),
            COUNT as usize,
            "同名入力に同じ出力パスが割り当てられた"
        );

        let mut widths: Vec<u32> = ok
            .iter()
            .map(|r| {
                image::open(&r.output_path)
                    .expect("出力が有効な画像でない")
                    .width()
            })
            .collect();
        widths.sort_unstable();
        let expected: Vec<u32> = (0..COUNT).map(|i| 100 + i * 10).collect();
        assert_eq!(widths, expected, "並列処理で画像の内容が失われている");
    }

    // =========================================================
    // EXIF Orientation (C3)
    // =========================================================

    #[test]
    fn orientation_6_is_uprighted_before_conversion() {
        // Orientation=6 は「90度CW回転して表示せよ」の意。
        // ビューア上では 400x800 の生ピクセルが 800x400 の横位置写真として見える。
        let dir = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        let input = dir.path().join("rotated.jpg");
        create_test_image_with_orientation(&input, 400, 800, 6);

        let config = ProcessingConfig {
            mode: ConversionMode::Quality, // アスペクト比を変えないモードで向きだけを見る
            ..test_config()
        };
        let result = process_image(&input, out.path(), &config, None, None).unwrap();

        let (w, h) = image::open(&result.output_path).unwrap().dimensions();
        assert_eq!(
            (w, h),
            (800, 400),
            "Orientation が適用されず傾いたまま出力されている"
        );
    }

    #[test]
    fn image_without_orientation_tag_is_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        let input = dir.path().join("plain.jpg");
        create_test_image(&input, 400, 800);

        let config = ProcessingConfig {
            mode: ConversionMode::Quality,
            ..test_config()
        };
        let result = process_image(&input, out.path(), &config, None, None).unwrap();

        let (w, h) = image::open(&result.output_path).unwrap().dimensions();
        assert_eq!((w, h), (400, 800), "タグ無しの画像が回転させられた");
    }

    #[test]
    fn read_exif_info_extracts_orientation() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("tagged.jpg");
        create_test_image_with_orientation(&input, 100, 100, 8);

        let info = read_exif_info(&input).unwrap();
        assert_eq!(info.orientation, Some(8));
    }

    #[test]
    fn oriented_dimensions_swaps_only_for_rotated_orientations() {
        // EXIF Orientation 5-8 は 90 度回転を含むため縦横が入れ替わる。
        for o in [1, 2, 3, 4] {
            assert_eq!(
                oriented_dimensions((400, 800), Some(o)),
                (400, 800),
                "Orientation {} で縦横が入れ替わった",
                o
            );
        }
        for o in [5, 6, 7, 8] {
            assert_eq!(
                oriented_dimensions((400, 800), Some(o)),
                (800, 400),
                "Orientation {} で縦横が入れ替わっていない",
                o
            );
        }
        assert_eq!(oriented_dimensions((400, 800), None), (400, 800));
    }

    #[test]
    fn image_dimensions_oriented_reports_the_upright_size_of_a_file() {
        // 一覧表示の縦横は「利用者が見る向き」と一致していなければならない。
        // ファイル経由で EXIF を読むところまで含めて確認する。
        let dir = TempDir::new().unwrap();

        let upright = dir.path().join("upright.jpg");
        create_test_image(&upright, 400, 800);
        // 前提: Orientation タグが無ければ生ピクセルの縦横がそのまま出る
        assert_eq!(image_dimensions_oriented(&upright).unwrap(), (400, 800));

        let rotated = dir.path().join("rotated.jpg");
        create_test_image_with_orientation(&rotated, 400, 800, 6);
        assert_eq!(image_dimensions_oriented(&rotated).unwrap(), (800, 400));
    }

    #[test]
    fn image_dimensions_oriented_reports_an_error_for_a_non_image() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("not_an_image.jpg");
        fs::write(&path, b"this is not a JPEG").unwrap();

        assert!(image_dimensions_oriented(&path).is_err());
    }

    // =========================================================
    // サイズ制限を満たせなかったことの通知 (H1 / H2)
    // =========================================================

    #[test]
    fn size_limit_exceeded_is_reported_when_unreachable() {
        let dir = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        let input = dir.path().join("noise.jpg");
        create_incompressible_image(&input, 2000, 2500);

        let config = ProcessingConfig {
            mode: ConversionMode::Quality,
            max_size_mb: 1,
            quality: 95,
            ..test_config()
        };
        let result = process_image(&input, out.path(), &config, None, None).unwrap();

        assert!(
            result.final_size_mb > 1.0,
            "前提が崩れている: この画像は1MBに収まってしまった ({:.2} MB)",
            result.final_size_mb
        );
        assert!(
            result.size_limit_exceeded,
            "制限を満たせなかったのに成功として黙殺されている"
        );
    }

    #[test]
    fn size_limit_exceeded_is_false_when_limit_is_met() {
        let dir = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        let input = dir.path().join("small.jpg");
        create_test_image(&input, 400, 500);

        let result = process_image(&input, out.path(), &test_config(), None, None).unwrap();
        assert!(result.final_size_mb <= 8.0);
        assert!(!result.size_limit_exceeded);
        assert!(result.warnings.is_empty(), "正常系で警告が出ている");
    }

    #[test]
    fn size_limit_failure_is_accompanied_by_a_warning() {
        // core はライブラリで stderr を持たない（GUI には届かない）。
        // 伝えるべき事象は必ず戻り値に載っていること。
        let dir = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        let input = dir.path().join("noise.jpg");
        create_incompressible_image(&input, 2000, 2500);

        let config = ProcessingConfig {
            mode: ConversionMode::Quality,
            max_size_mb: 1,
            quality: 95,
            ..test_config()
        };
        let result = process_image(&input, out.path(), &config, None, None).unwrap();

        assert!(result.size_limit_exceeded, "前提が崩れている");
        assert!(
            !result.warnings.is_empty(),
            "制限を満たせなかったのに呼び出し元へ伝える警告が無い"
        );
    }

    // =========================================================
    // ファイル収集: リンク非追従とスキップの可視化 (H3 / M2)
    // =========================================================

    #[cfg(unix)]
    #[test]
    fn collect_image_files_does_not_follow_symlinks() {
        let outside = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();

        let target = outside.path().join("outside.jpg");
        create_test_image(&target, 10, 10);
        create_test_image(&root.path().join("real.jpg"), 10, 10);
        std::os::unix::fs::symlink(&target, root.path().join("link.jpg")).unwrap();

        let collected = collect_image_files(root.path());
        let names: Vec<String> = collected
            .files
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();

        assert_eq!(
            names,
            vec!["real.jpg".to_string()],
            "リンクを辿らない設定なのにリンク先が収集されている"
        );
    }

    #[test]
    fn collect_image_files_reports_unreadable_paths_as_skipped() {
        // 走査できなかったエントリが黙って消えると「件数が減った」ことに気づけない。
        let collected = collect_image_files(Path::new("/nonexistent/folder"));
        assert!(collected.files.is_empty());
        assert!(
            !collected.skipped.is_empty(),
            "走査失敗が呼び出し元へ一切伝わっていない"
        );
    }

    // =========================================================
    // max_width: 範囲検証と serde 互換（spec §7 / §9 #8）
    // =========================================================

    /// 仕様: 上限の指定は 4..=20000 px。
    /// 下限 4 はキャンバス幅が 4 の倍数であることの最小値、
    /// 上限 20000 は 20000x25000 の RGBA キャンバスが約 2GB に達するという実メモリ上の線。
    #[test]
    fn validate_config_accepts_max_width_boundaries() {
        let mut config = test_config();
        config.max_width = Some(4);
        assert!(validate_config(&config).is_ok(), "下限 4 は有効な指定");
        config.max_width = Some(20000);
        assert!(validate_config(&config).is_ok(), "上限 20000 は有効な指定");
        config.max_width = None;
        assert!(
            validate_config(&config).is_ok(),
            "無指定は無制限であって不正ではない"
        );
    }

    #[test]
    fn validate_config_rejects_max_width_outside_the_supported_range() {
        let mut config = test_config();
        config.max_width = Some(3);
        assert!(validate_config(&config).is_err(), "3 は下限未満");
        config.max_width = Some(20001);
        assert!(validate_config(&config).is_err(), "20001 は上限超過");
    }

    /// 仕様: GUI から来る JSON に `max_width` が無くてもデシリアライズは壊れない。
    /// 無指定は「無制限」（従来どおり原寸）を意味する（spec §3）。
    #[test]
    fn processing_config_defaults_max_width_to_none_when_absent() {
        let json = r#"{
            "mode": "pad",
            "bg_color": "black",
            "quality": 75,
            "max_size_mb": 4,
            "delete_originals": true
        }"#;
        let config: ProcessingConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.max_width, None);
    }

    #[test]
    fn processing_config_reads_max_width_from_frontend_json() {
        let json = r#"{
            "mode": "pad",
            "bg_color": "white",
            "quality": 90,
            "max_size_mb": 8,
            "delete_originals": false,
            "max_width": 1080
        }"#;
        let config: ProcessingConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.max_width, Some(1080));
    }

    // =========================================================
    // max_width: 出力キャンバス幅の上限（spec §9 #1〜#4, #6）
    // =========================================================

    /// テスト用: max_width つきの pad / crop 設定
    fn config_with_max_width(mode: ConversionMode, max_width: u32) -> ProcessingConfig {
        ProcessingConfig {
            mode,
            max_width: Some(max_width),
            ..test_config()
        }
    }

    /// 指定サイズの画像を1枚処理し、出力の (幅, 高さ) を返す
    fn process_and_measure(w: u32, h: u32, config: &ProcessingConfig) -> (u32, u32) {
        let dir = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        let input = dir.path().join(format!("in_{}x{}.jpg", w, h));
        create_test_image(&input, w, h);
        let result = process_image(&input, out.path(), config, None, None).unwrap();
        image::open(&result.output_path).unwrap().dimensions()
    }

    /// 仕様: 目標より大きい入力に対しては、pad の出力は目標幅ちょうどに着地する（#1）。
    /// 横位置・縦位置の両方を見る（spec §4 の導出は k を決める辺で場合分けしている）。
    #[test]
    fn pad_with_max_width_lands_exactly_on_the_target_canvas() {
        let config = config_with_max_width(ConversionMode::Pad, 1080);
        for (w, h) in [(3000, 2000), (2000, 3000)] {
            let (out_w, out_h) = process_and_measure(w, h, &config);
            assert_eq!(
                (out_w, out_h),
                (1080, 1350),
                "{}x{} + max_width=1080 は 1080x1350 になるべき",
                w,
                h
            );
            assert_eq!(out_w * 5, out_h * 4, "canvas must be exactly 4:5");
        }
    }

    /// 仕様: crop も同じ契約。crop には前段が無く、最終リサイズだけが上限を保証する（#2）。
    #[test]
    fn crop_with_max_width_lands_exactly_on_the_target_canvas() {
        let config = config_with_max_width(ConversionMode::Crop, 1080);
        for (w, h) in [(3000, 2000), (2000, 3000)] {
            let (out_w, out_h) = process_and_measure(w, h, &config);
            assert_eq!(
                (out_w, out_h),
                (1080, 1350),
                "{}x{} + max_width=1080 は 1080x1350 になるべき",
                w,
                h
            );
            assert_eq!(out_w * 5, out_h * 4, "canvas must be exactly 4:5");
        }
    }

    /// 仕様: 指定値は上限であって目標ではない。元が小さければ拡大しない（#3）。
    /// 契約は不等式なので、ここでは等値を要求しない。
    #[test]
    fn max_width_never_upscales_a_smaller_image() {
        // pad: 800x533 の 4:5 キャンバスは 800x1000 で、上限 1080 に既に収まっている
        let (w, h) =
            process_and_measure(800, 533, &config_with_max_width(ConversionMode::Pad, 1080));
        assert_eq!((w, h), (800, 1000), "上限より小さい入力は引き伸ばされない");

        // crop: 中央クロップの結果も元の高さを保ったまま
        let (w, h) =
            process_and_measure(800, 533, &config_with_max_width(ConversionMode::Crop, 1080));
        assert_eq!((w, h), (426, 533), "crop も拡大されない");
    }

    /// 仕様: 4 の倍数でない指定は切り捨てる。切り上げると指定値を超えてしまう（#4）。
    #[test]
    fn max_width_is_rounded_down_to_a_multiple_of_four() {
        let (w, h) = process_and_measure(
            3000,
            2000,
            &config_with_max_width(ConversionMode::Pad, 1002),
        );
        assert_eq!((w, h), (1000, 1250), "1002 は 1000 に切り捨てられる");
        assert!(w <= 1002, "実効値が指定値を超えてはならない");
    }

    /// 仕様: quality モードは 4:5 に変換しないため max_width の対象外（#6）。
    /// 「幅」を指定しても縦写真では長辺が幅*5/4 を大きく超え、上限の意味を持たない。
    #[test]
    fn quality_mode_ignores_max_width() {
        let config = config_with_max_width(ConversionMode::Quality, 1080);
        let (w, h) = process_and_measure(1600, 900, &config);
        assert_eq!((w, h), (1600, 900), "quality モードでは寸法が変わらない");
    }
}
