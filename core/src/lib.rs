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

/// 進捗コールバック: (current, total) -> bool（falseでキャンセル）
pub type ProgressCallback = Box<dyn Fn(usize, usize) -> bool + Send + Sync>;

// --- 公開API ---

/// 設定を検証する
pub fn validate_config(config: &ProcessingConfig) -> Result<()> {
    if config.quality == 0 || config.quality > 100 {
        anyhow::bail!("Quality must be between 1 and 100");
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
        .follow_links(true)
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
) -> Result<ProcessResult> {
    let img = image::open(input_path)
        .with_context(|| format!("Failed to open image: {}", input_path.display()))?;

    let converted = match config.mode {
        ConversionMode::Crop => convert_aspect_ratio_crop(img),
        ConversionMode::Pad => convert_aspect_ratio_pad(img, config.bg_color),
        ConversionMode::Quality => img,
    };

    let output_path = generate_output_path(input_path, output_folder)?;
    let max_size_bytes = config.max_size_mb * 1024 * 1024;

    let (final_size, final_quality) =
        save_with_size_limit(&converted, &output_path, config.quality, max_size_bytes)?;

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

            let result = process_image(path, output_folder, config);

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

    let mut quality = initial_quality;

    loop {
        let temp_path = output_path.with_extension("tmp.jpg");
        save_jpeg(img, &temp_path, quality)?;

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

/// JPEG形式で画像を保存
fn save_jpeg(img: &DynamicImage, path: &Path, quality: u8) -> Result<()> {
    let file =
        File::create(path).with_context(|| format!("Failed to create file: {}", path.display()))?;

    let mut writer = BufWriter::new(file);

    let rgb_img = img.to_rgb8();
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn test_config() -> ProcessingConfig {
        ProcessingConfig {
            mode: ConversionMode::Crop,
            bg_color: BackgroundColor::White,
            quality: 90,
            max_size_mb: 8,
            delete_originals: false,
        }
    }

    #[test]
    fn test_validate_config_valid() {
        let config = test_config();
        assert!(validate_config(&config).is_ok());
    }

    #[test]
    fn test_validate_config_quality_zero() {
        let mut config = test_config();
        config.quality = 0;
        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn test_validate_config_quality_over_100() {
        let mut config = test_config();
        config.quality = 101;
        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn test_is_supported_image() {
        assert!(is_supported_image(Path::new("photo.jpg")));
        assert!(is_supported_image(Path::new("photo.JPEG")));
        assert!(is_supported_image(Path::new("photo.png")));
        assert!(is_supported_image(Path::new("photo.webp")));
        assert!(!is_supported_image(Path::new("doc.pdf")));
        assert!(!is_supported_image(Path::new("noext")));
    }

    #[test]
    fn test_collect_image_files_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let files = collect_image_files(dir.path()).unwrap();
        assert!(files.is_empty());
    }

    #[test]
    fn test_collect_image_files_with_images() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.jpg"), b"fake").unwrap();
        fs::write(dir.path().join("b.png"), b"fake").unwrap();
        fs::write(dir.path().join("c.txt"), b"fake").unwrap();
        let files = collect_image_files(dir.path()).unwrap();
        assert_eq!(files.len(), 2);
    }
}
