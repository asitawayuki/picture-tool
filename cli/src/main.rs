use anyhow::{Context, Result};
use clap::Parser;
use picture_tool_core::{self as core, BackgroundColor, ConversionMode, ProcessingConfig};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "画像バッチ処理ツール - 4:5のアスペクト比に変換し、サイズ制限付きで保存"
)]
struct Args {
    /// 入力フォルダーパス
    #[arg(short, long)]
    input: PathBuf,

    /// 変換モード (crop, pad, quality)
    #[arg(short, long, default_value = "crop")]
    mode: CliConversionMode,

    /// パディング時の背景色 (white, black)
    #[arg(short, long, default_value = "white")]
    bg_color: CliBgColor,

    /// 初期JPEG品質 (1-100)
    #[arg(short, long, default_value = "90")]
    quality: u8,

    /// 最大ファイルサイズ (MB)
    #[arg(long, default_value = "8")]
    max_size: usize,

    /// 出力先フォルダー
    #[arg(short, long, default_value = "./")]
    output: PathBuf,

    /// 変換完了後に元ファイルを削除
    #[arg(long, default_value = "false")]
    delete_originals: bool,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum CliConversionMode {
    Crop,
    Pad,
    Quality,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum CliBgColor {
    White,
    Black,
}

impl From<CliConversionMode> for ConversionMode {
    fn from(m: CliConversionMode) -> Self {
        match m {
            CliConversionMode::Crop => ConversionMode::Crop,
            CliConversionMode::Pad => ConversionMode::Pad,
            CliConversionMode::Quality => ConversionMode::Quality,
        }
    }
}

impl From<CliBgColor> for BackgroundColor {
    fn from(c: CliBgColor) -> Self {
        match c {
            CliBgColor::White => BackgroundColor::White,
            CliBgColor::Black => BackgroundColor::Black,
        }
    }
}

fn main() -> Result<()> {
    let args = Args::parse();

    let config = ProcessingConfig {
        mode: args.mode.into(),
        bg_color: args.bg_color.into(),
        quality: args.quality,
        max_size_mb: args.max_size,
        delete_originals: args.delete_originals,
    };

    core::validate_config(&config)?;

    if !args.input.exists() {
        anyhow::bail!("Input folder does not exist: {}", args.input.display());
    }
    if !args.input.is_dir() {
        anyhow::bail!("Input path is not a directory: {}", args.input.display());
    }

    if !args.output.exists() {
        fs::create_dir_all(&args.output).with_context(|| {
            format!("Failed to create output folder: {}", args.output.display())
        })?;
        println!("Created output folder: {}", args.output.display());
    } else if !args.output.is_dir() {
        anyhow::bail!("Output path is not a directory: {}", args.output.display());
    }

    println!("Processing images in: {}", args.input.display());
    println!("Output folder: {}", args.output.display());

    let image_files = core::collect_image_files(&args.input)?;
    let total_count = image_files.len();

    if total_count == 0 {
        println!("No image files found.");
        return Ok(());
    }

    println!("Found {} images\n", total_count);

    let start = Instant::now();
    let success_count = AtomicUsize::new(0);
    let failed_count = AtomicUsize::new(0);

    let on_progress = |_current: usize, _total: usize| -> bool {
        true // CLI版はキャンセルなし
    };

    let results = core::process_batch(
        &image_files,
        &args.output,
        &config,
        Some(Box::new(on_progress)),
    );

    for (i, result) in results.iter().enumerate() {
        let path = &image_files[i];
        match result {
            Ok(r) => {
                success_count.fetch_add(1, Ordering::SeqCst);
                let quality_info = r
                    .final_quality
                    .map_or(String::new(), |q| format!(", quality: {}%", q));
                println!(
                    "[{}/{}] {} → {} ({:.1} MB{}) ✓",
                    i + 1,
                    total_count,
                    path.file_name().unwrap().to_string_lossy(),
                    PathBuf::from(&r.output_path)
                        .file_name()
                        .unwrap()
                        .to_string_lossy(),
                    r.final_size_mb,
                    quality_info
                );
            }
            Err(e) => {
                failed_count.fetch_add(1, Ordering::SeqCst);
                eprintln!(
                    "[{}/{}] {} ✗ Error: {}",
                    i + 1,
                    total_count,
                    path.file_name().unwrap().to_string_lossy(),
                    e
                );
            }
        }
    }

    let duration = start.elapsed();
    let success = success_count.load(Ordering::SeqCst);
    let failed = failed_count.load(Ordering::SeqCst);

    println!("\nCompleted: {} successful, {} failed", success, failed);
    println!("Total time: {:.1}s", duration.as_secs_f64());

    Ok(())
}
