pub mod exif_frame;
pub mod model_map;

use anyhow::{Context, Result};
use image::{DynamicImage, GenericImageView, RgbaImage};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use walkdir::WalkDir;

// --- 型定義 ---

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ConversionMode {
    Crop,
    Pad,
    Quality,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessResult {
    pub input_path: String,
    pub output_path: String,
    pub final_size_mb: f64,
    pub final_quality: Option<u8>,
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
}

/// 進捗コールバック: (current, total) -> bool（falseでキャンセル）
pub type ProgressCallback = Box<dyn Fn(usize, usize) -> bool + Send + Sync>;

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
        exif_data
            .get_field(tag, exif::In::PRIMARY)
            .map(|f| {
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

    Ok(ExifInfo {
        camera_make: get_string(exif::Tag::Make).map(|s| s.trim().to_string()),
        camera_model: get_string(exif::Tag::Model).map(|s| s.trim().to_string()),
        lens_model: get_string(exif::Tag::LensModel).map(|s| s.trim().to_string()),
        focal_length,
        f_number,
        shutter_speed,
        iso,
        date_taken: get_string(exif::Tag::DateTimeOriginal),
    })
}

/// 設定を検証する
pub fn validate_config(config: &ProcessingConfig) -> Result<()> {
    if config.quality == 0 || config.quality > 100 {
        anyhow::bail!("Quality must be between 1 and 100");
    }
    if config.max_size_mb == 0 {
        anyhow::bail!("max_size_mb must be at least 1");
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
pub fn collect_image_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    for entry in WalkDir::new(dir)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.is_file() && is_supported_image(path) {
            files.push(path.to_path_buf());
        }
    }

    Ok(files)
}

/// 画像を処理
pub fn process_image(
    input_path: &Path,
    output_folder: &Path,
    config: &ProcessingConfig,
    exif_frame_config: Option<&exif_frame::ExifFrameConfig>,
    asset_dirs: Option<&exif_frame::AssetDirs>,
) -> Result<ProcessResult> {
    let img = image::open(input_path)
        .with_context(|| format!("Failed to open image: {}", input_path.display()))?;

    let converted = match config.mode {
        ConversionMode::Crop => convert_aspect_ratio_crop(img),
        ConversionMode::Pad => convert_aspect_ratio_pad(img, config.bg_color),
        ConversionMode::Quality => img,
    };

    // Exifフレーム付加（オプション）
    // EXIF読み取り失敗でもフレーム生成は続行
    let framed = if let (Some(fc), Some(ad)) = (exif_frame_config, asset_dirs) {
        let exif = read_exif_info(input_path).unwrap_or_default();
        match exif_frame::render_exif_frame(&converted, &exif, fc, ad) {
            Ok(result) => result,
            Err(e) => {
                eprintln!("Warning: Exif frame rendering failed for {}: {}", input_path.display(), e);
                converted
            }
        }
    } else {
        converted
    };

    let output_path = generate_output_path(input_path, output_folder)?;
    let max_size_bytes = config.max_size_mb * 1024 * 1024;

    let (final_size, final_quality) =
        save_with_size_limit(&framed, &output_path, config.quality, max_size_bytes)?;

    // 成功時のみ元ファイルを削除
    if config.delete_originals {
        if let Err(e) = fs::remove_file(input_path) {
            eprintln!(
                "Warning: Failed to delete original file {}: {}",
                input_path.display(),
                e
            );
        }
    }

    let final_size_mb = final_size as f64 / (1024.0 * 1024.0);

    Ok(ProcessResult {
        input_path: input_path.to_string_lossy().to_string(),
        output_path: output_path.to_string_lossy().to_string(),
        final_size_mb,
        final_quality: if final_quality < config.quality {
            Some(final_quality)
        } else {
            None
        },
    })
}

/// バッチ処理（並列）
pub fn process_batch(
    files: &[PathBuf],
    output_folder: &Path,
    config: &ProcessingConfig,
    exif_frame_config: Option<&exif_frame::ExifFrameConfig>,
    asset_dirs: Option<&exif_frame::AssetDirs>,
    on_progress: Option<ProgressCallback>,
) -> Vec<Result<ProcessResult>> {
    let total = files.len();
    let cancelled = Arc::new(AtomicBool::new(false));
    let processed_count = AtomicUsize::new(0);

    files
        .par_iter()
        .map(|path| {
            if cancelled.load(Ordering::Relaxed) {
                return Err(anyhow::anyhow!("Processing cancelled"));
            }

            let result = process_image(path, output_folder, config, exif_frame_config, asset_dirs);

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

/// サムネイルをbase64エンコードされたJPEG文字列として生成
pub fn generate_thumbnail_base64(path: &Path, max_dimension: u32) -> Result<String> {
    use base64::Engine as _;
    let max_dimension = max_dimension.min(1024);

    let img = image::open(path)
        .with_context(|| format!("Failed to open image for thumbnail: {}", path.display()))?;

    let thumbnail = img.thumbnail(max_dimension, max_dimension);
    let rgb = thumbnail.to_rgb8();

    let mut jpeg_bytes = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut jpeg_bytes);
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut cursor, 75);
    encoder.encode(
        rgb.as_raw(),
        rgb.width(),
        rgb.height(),
        image::ColorType::Rgb8,
    )?;

    Ok(base64::engine::general_purpose::STANDARD.encode(&jpeg_bytes))
}

/// フル解像度画像をbase64エンコードされたJPEG文字列として生成（プレビュー用）
pub fn generate_full_image_base64(path: &Path, max_width: u32, max_height: u32) -> Result<String> {
    use base64::Engine as _;

    let max_width = max_width.min(2560);
    let max_height = max_height.min(1600);

    let img =
        image::open(path).with_context(|| format!("Failed to open image: {}", path.display()))?;

    let (w, h) = img.dimensions();

    let resized = if w > max_width || h > max_height {
        img.resize(max_width, max_height, image::imageops::FilterType::Lanczos3)
    } else {
        img
    };

    let rgb = resized.to_rgb8();

    let mut jpeg_bytes = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut jpeg_bytes);
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut cursor, 90);
    encoder.encode(
        rgb.as_raw(),
        rgb.width(),
        rgb.height(),
        image::ColorType::Rgb8,
    )?;

    Ok(base64::engine::general_purpose::STANDARD.encode(&jpeg_bytes))
}

// --- プライベートヘルパー ---

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
fn convert_aspect_ratio_pad(img: DynamicImage, bg_color: BackgroundColor) -> DynamicImage {
    let (width, height) = img.dimensions();
    let target_ratio = 4.0 / 5.0;
    let current_ratio = width as f64 / height as f64;

    if (current_ratio - target_ratio).abs() < 0.001 {
        return img;
    }

    let (new_width, new_height) = if current_ratio > target_ratio {
        // 横長すぎる → 上下にパディング
        let new_height = (width as f64 / target_ratio).round() as u32;
        (width, new_height)
    } else {
        // 縦長すぎる → 左右にパディング
        let new_width = (height as f64 * target_ratio).round() as u32;
        (new_width, height)
    };

    let mut canvas = RgbaImage::from_pixel(new_width, new_height, bg_color.to_rgba());

    let x = (new_width.saturating_sub(width)) / 2;
    let y = (new_height.saturating_sub(height)) / 2;

    image::imageops::overlay(&mut canvas, &img.to_rgba8(), x.into(), y.into());

    DynamicImage::ImageRgba8(canvas)
}

/// 出力パスを生成（重複時は連番を追加）
fn generate_output_path(input_path: &Path, output_folder: &Path) -> Result<PathBuf> {
    let stem = input_path
        .file_stem()
        .context("Failed to get file stem")?
        .to_string_lossy();

    let output_filename = format!("{}_processed.jpg", stem);
    let mut output_path = output_folder.join(&output_filename);

    let mut counter = 1;
    while output_path.exists() {
        let numbered_filename = format!("{}_processed_{}.jpg", stem, counter);
        output_path = output_folder.join(numbered_filename);
        counter += 1;
    }

    Ok(output_path)
}

/// サイズ制限付きで画像を保存
fn save_with_size_limit(
    img: &DynamicImage,
    output_path: &Path,
    initial_quality: u8,
    max_size_bytes: usize,
) -> Result<(usize, u8)> {
    const MIN_QUALITY: u8 = 60;
    const QUALITY_STEP: u8 = 5;

    let rgb_img = img.to_rgb8();
    let mut quality = initial_quality;

    loop {
        let temp_path = output_path.with_extension("tmp.jpg");

        if let Err(e) = save_jpeg_rgb(&rgb_img, &temp_path, quality) {
            let _ = fs::remove_file(&temp_path);
            return Err(e);
        }

        let metadata = fs::metadata(&temp_path)
            .with_context(|| format!("Failed to get metadata: {}", temp_path.display()))?;
        let file_size = metadata.len() as usize;

        if file_size <= max_size_bytes || quality <= MIN_QUALITY {
            fs::rename(&temp_path, output_path)
                .with_context(|| format!("Failed to rename file: {}", output_path.display()))?;
            return Ok((file_size, quality));
        }

        fs::remove_file(&temp_path).ok();
        quality = quality.saturating_sub(QUALITY_STEP).max(MIN_QUALITY);
    }
}

/// JPEG形式で画像を保存（RgbImageを直接受け取る低レベル関数）
fn save_jpeg_rgb(rgb_img: &image::RgbImage, path: &Path, quality: u8) -> Result<()> {
    let file =
        File::create(path).with_context(|| format!("Failed to create file: {}", path.display()))?;
    let mut writer = BufWriter::new(file);
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut writer, quality);
    encoder
        .encode(
            rgb_img.as_raw(),
            rgb_img.width(),
            rgb_img.height(),
            image::ColorType::Rgb8,
        )
        .with_context(|| format!("Failed to encode JPEG: {}", path.display()))?;
    Ok(())
}

/// JPEG形式で画像を保存
#[allow(dead_code)]
fn save_jpeg(img: &DynamicImage, path: &Path, quality: u8) -> Result<()> {
    let rgb_img = img.to_rgb8();
    save_jpeg_rgb(&rgb_img, path, quality)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgb};
    use std::fs;
    use std::sync::atomic::AtomicUsize;

    fn test_config() -> ProcessingConfig {
        ProcessingConfig {
            mode: ConversionMode::Crop,
            bg_color: BackgroundColor::White,
            quality: 90,
            max_size_mb: 8,
            delete_originals: false,
        }
    }

    /// テスト用のRGB画像を指定サイズで生成しJPEGとして保存
    fn create_test_image(path: &Path, width: u32, height: u32) {
        let img = ImageBuffer::from_fn(width, height, |x, y| {
            Rgb([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8])
        });
        img.save(path).unwrap();
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

        let files = collect_image_files(dir.path()).unwrap();
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn collect_image_files_returns_empty_for_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let files = collect_image_files(dir.path()).unwrap();
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
        let ratio = w as f64 / h as f64;
        assert!(
            (ratio - 0.8).abs() < 0.02,
            "pad結果のアスペクト比が4:5でない: {}x{} (ratio={})",
            w,
            h,
            ratio
        );
        // パディングは元画像以上のサイズになる
        assert!(w >= 800, "幅が元画像より小さい");
        assert!(h >= 400, "高さが元画像より小さい");
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

        let output_img = image::open(&result.output_path).unwrap();
        let (w, h) = output_img.dimensions();
        let ratio = w as f64 / h as f64;
        assert!((ratio - 0.8).abs() < 0.02);
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
                    .map_or(false, |e| e.to_string().contains("cancelled"))
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
        assert!(bytes.len() > 0);
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
}
