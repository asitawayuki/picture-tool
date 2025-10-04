use anyhow::{Context, Result};
use clap::Parser;
use image::{DynamicImage, GenericImageView, RgbaImage};
use rayon::prelude::*;
use std::fs::{self, File};
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;
use walkdir::WalkDir;

/// 画像バッチ処理ツール - 4:5のアスペクト比に変換し、8MB以下に圧縮
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// 入力フォルダーパス
    #[arg(short, long)]
    input: PathBuf,

    /// 変換モード (crop または pad)
    #[arg(short, long, default_value = "crop")]
    mode: ConversionMode,

    /// パディング時の背景色 (white または black)
    #[arg(short, long, default_value = "white")]
    bg_color: BackgroundColor,

    /// 初期JPEG品質 (1-100)
    #[arg(short, long, default_value = "90")]
    quality: u8,

    /// 最大ファイルサイズ (MB)
    #[arg(long, default_value = "8")]
    max_size: usize,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum ConversionMode {
    Crop,
    Pad,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum BackgroundColor {
    White,
    Black,
}

impl BackgroundColor {
    fn to_rgba(&self) -> image::Rgba<u8> {
        match self {
            BackgroundColor::White => image::Rgba([255, 255, 255, 255]),
            BackgroundColor::Black => image::Rgba([0, 0, 0, 255]),
        }
    }
}

/// 画像処理の結果
struct ProcessResult {
    input_path: PathBuf,
    output_path: PathBuf,
    final_size_mb: f64,
    final_quality: Option<u8>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // 品質の検証
    if args.quality == 0 || args.quality > 100 {
        anyhow::bail!("Quality must be between 1 and 100");
    }

    // 入力フォルダーの検証
    if !args.input.exists() {
        anyhow::bail!("Input folder does not exist: {}", args.input.display());
    }
    if !args.input.is_dir() {
        anyhow::bail!("Input path is not a directory: {}", args.input.display());
    }

    println!("Processing images in: {}", args.input.display());

    // 画像ファイルを収集
    let image_files = collect_image_files(&args.input)?;
    let total_count = image_files.len();

    if total_count == 0 {
        println!("No image files found.");
        return Ok(());
    }

    println!("Found {} images\n", total_count);

    let start = Instant::now();
    let success_count = AtomicUsize::new(0);
    let failed_count = AtomicUsize::new(0);
    let processed_count = AtomicUsize::new(0);

    // 並列処理で画像を処理
    let _results: Vec<_> = image_files
        .par_iter()
        .filter_map(|path| {
            let current = processed_count.fetch_add(1, Ordering::SeqCst) + 1;

            match process_image(path, &args) {
                Ok(result) => {
                    success_count.fetch_add(1, Ordering::SeqCst);

                    let quality_info = if let Some(q) = result.final_quality {
                        format!(", quality: {}%", q)
                    } else {
                        String::new()
                    };

                    println!(
                        "[{}/{}] {} → {} ({:.1} MB{}) ✓",
                        current,
                        total_count,
                        path.file_name().unwrap().to_string_lossy(),
                        result.output_path.file_name().unwrap().to_string_lossy(),
                        result.final_size_mb,
                        quality_info
                    );

                    Some(result)
                }
                Err(e) => {
                    failed_count.fetch_add(1, Ordering::SeqCst);
                    eprintln!(
                        "[{}/{}] {} ✗ Error: {}",
                        current,
                        total_count,
                        path.file_name().unwrap().to_string_lossy(),
                        e
                    );
                    None
                }
            }
        })
        .collect();

    let duration = start.elapsed();
    let success = success_count.load(Ordering::SeqCst);
    let failed = failed_count.load(Ordering::SeqCst);

    println!(
        "\nCompleted: {} successful, {} failed",
        success, failed
    );
    println!("Total time: {:.1}s", duration.as_secs_f64());

    Ok(())
}

/// 指定フォルダー内の画像ファイルを収集
fn collect_image_files(dir: &Path) -> Result<Vec<PathBuf>> {
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

/// サポートされている画像形式かチェック
fn is_supported_image(path: &Path) -> bool {
    if let Some(ext) = path.extension() {
        let ext = ext.to_string_lossy().to_lowercase();
        matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "webp")
    } else {
        false
    }
}

/// 画像を処理
fn process_image(input_path: &Path, args: &Args) -> Result<ProcessResult> {
    // 画像を読み込む
    let img = image::open(input_path)
        .with_context(|| format!("Failed to open image: {}", input_path.display()))?;

    // 4:5のアスペクト比に変換
    let converted = match args.mode {
        ConversionMode::Crop => convert_aspect_ratio_crop(img),
        ConversionMode::Pad => convert_aspect_ratio_pad(img, args.bg_color),
    };

    // 出力パスを生成
    let output_path = generate_output_path(input_path)?;

    // 最大ファイルサイズ (バイト)
    let max_size_bytes = args.max_size * 1024 * 1024;

    // 品質を調整しながら保存
    let (final_size, final_quality) =
        save_with_size_limit(&converted, &output_path, args.quality, max_size_bytes)?;

    let final_size_mb = final_size as f64 / (1024.0 * 1024.0);

    Ok(ProcessResult {
        input_path: input_path.to_path_buf(),
        output_path,
        final_size_mb,
        final_quality: if final_quality < args.quality {
            Some(final_quality)
        } else {
            None
        },
    })
}

/// 4:5のアスペクト比に変換 (中央クロップ)
fn convert_aspect_ratio_crop(img: DynamicImage) -> DynamicImage {
    let (width, height) = img.dimensions();
    let target_ratio = 4.0 / 5.0; // 0.8
    let current_ratio = width as f64 / height as f64;

    if (current_ratio - target_ratio).abs() < 0.001 {
        // 既に4:5の場合はそのまま
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
    let target_ratio = 4.0 / 5.0; // 0.8
    let current_ratio = width as f64 / height as f64;

    if (current_ratio - target_ratio).abs() < 0.001 {
        // 既に4:5の場合はそのまま
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

/// 出力パスを生成
fn generate_output_path(input_path: &Path) -> Result<PathBuf> {
    let parent = input_path
        .parent()
        .context("Failed to get parent directory")?;

    let stem = input_path
        .file_stem()
        .context("Failed to get file stem")?
        .to_string_lossy();

    let output_filename = format!("{}_processed.jpg", stem);

    Ok(parent.join(output_filename))
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
        // 一時ファイルに保存
        let temp_path = output_path.with_extension("tmp.jpg");
        save_jpeg(img, &temp_path, quality)?;

        // ファイルサイズを確認
        let metadata = fs::metadata(&temp_path)
            .with_context(|| format!("Failed to get metadata: {}", temp_path.display()))?;
        let file_size = metadata.len() as usize;

        if file_size <= max_size_bytes || quality <= MIN_QUALITY {
            // サイズが制限内、または最小品質に達した
            fs::rename(&temp_path, output_path)
                .with_context(|| format!("Failed to rename file: {}", output_path.display()))?;
            return Ok((file_size, quality));
        }

        // 品質を下げて再試行
        fs::remove_file(&temp_path).ok();
        quality = quality.saturating_sub(QUALITY_STEP).max(MIN_QUALITY);
    }
}

/// JPEG形式で画像を保存
fn save_jpeg(img: &DynamicImage, path: &Path, quality: u8) -> Result<()> {
    let file = File::create(path)
        .with_context(|| format!("Failed to create file: {}", path.display()))?;

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
