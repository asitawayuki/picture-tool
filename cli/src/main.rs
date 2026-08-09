use anyhow::{Context, Result};
use clap::Parser;
use picture_tool_core::{self as core, BackgroundColor, ConversionMode, ProcessingConfig};
use std::fs;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
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
    mode: ConversionMode,

    /// パディング時の背景色 (white, black)
    #[arg(short, long, default_value = "white")]
    bg_color: BackgroundColor,

    /// 初期JPEG品質 (1-100)
    #[arg(short, long, default_value = "90", value_parser = clap::value_parser!(u8).range(1..=100))]
    quality: u8,

    /// 最大ファイルサイズ (MB, 1-1024)
    #[arg(long, default_value = "8", value_parser = clap::value_parser!(u64).range(1..=1024))]
    max_size: u64,

    /// 出力先フォルダー
    #[arg(short, long, default_value = "./")]
    output: PathBuf,

    /// 変換完了後に元ファイルを削除
    #[arg(long, default_value = "false")]
    delete_originals: bool,

    /// Exifフレームを付加
    #[arg(short = 'e', long, default_value = "false")]
    exif_frame: bool,

    /// プリセット名
    #[arg(short, long, default_value = "default")]
    preset: String,

    /// プリセットJSONファイル直接指定
    #[arg(long)]
    preset_file: Option<PathBuf>,

    /// カスタムテキスト（プリセットの値を上書き）
    #[arg(long, default_value = "")]
    custom_text: String,
}

/// 表示用のファイル名。非UTF-8やルートパスでも panic しない（S6-L12）。
fn display_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned()
}

fn main() -> Result<()> {
    let args = Args::parse();

    let config = ProcessingConfig {
        mode: args.mode,
        bg_color: args.bg_color,
        quality: args.quality,
        max_size_mb: args.max_size as usize,
        delete_originals: args.delete_originals,
    };

    core::validate_config(&config)?;

    let exif_frame_requested = if args.exif_frame && config.mode != ConversionMode::Pad {
        eprintln!("Warning: --exif-frame is only supported with --mode pad. Ignoring.");
        false
    } else {
        args.exif_frame
    };

    let (exif_frame_config, exif_assets) = if exif_frame_requested {
        let dirs = core::exif_frame::AssetDirs::default();
        let config = if let Some(ref path) = args.preset_file {
            let json = std::fs::read_to_string(path)
                .with_context(|| format!("Failed to read preset file: {}", path.display()))?;
            serde_json::from_str::<core::exif_frame::ExifFrameConfig>(&json)?
        } else {
            let all = core::exif_frame::preset::list_all_presets(dirs.user_presets_dir.as_deref());
            all.into_iter()
                .find(|p| p.name == args.preset)
                .unwrap_or_else(|| {
                    eprintln!("Preset '{}' not found, using default", args.preset);
                    core::exif_frame::ExifFrameConfig::default()
                })
        };

        let mut config = config;
        if !args.custom_text.is_empty() {
            config.custom_text = args.custom_text.clone();
            config.items.custom_text = true;
        }

        // アセットはバッチ全体で1回だけ構築する（画像ごとに model_map を
        // 読み直さないため）。構築時の警告は core が握りつぶさず返してくるので
        // ここで stderr に出す。
        let assets =
            core::exif_frame::ExifAssets::load(dirs).context("Failed to load exif frame assets")?;
        for warning in &assets.warnings {
            eprintln!("Warning: {}", warning);
        }

        (Some(config), Some(assets))
    } else {
        (None, None)
    };

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

    let collected = core::collect_image_files(&args.input);
    for skipped in &collected.skipped {
        eprintln!("Warning: skipped while scanning: {}", skipped);
    }
    let image_files = collected.files;
    let total_count = image_files.len();

    if total_count == 0 {
        println!("No image files found.");
        return Ok(());
    }

    println!("Found {} images\n", total_count);

    let start = Instant::now();

    // 大量処理で無反応に見えないよう完了件数を出す（S6-CLI-1）。
    // rayon の複数ワーカーから呼ばれるが、`\r` での上書きなので順不同でも支障はない。
    // 一件ごとの結果はバッチ完了後にまとめて出すため、ここでの出力とは混ざらない。
    // リダイレクト先では上書きが効かず制御文字がそのまま残るため端末のときだけ出す。
    let interactive = std::io::stderr().is_terminal();
    let on_progress = move |current: usize, total: usize| -> bool {
        if interactive {
            let mut stderr = std::io::stderr().lock();
            let _ = write!(stderr, "\rProcessing... {}/{}", current, total);
            let _ = stderr.flush();
        }
        true // CLI版はキャンセルなし
    };

    let results = core::process_batch(
        &image_files,
        &args.output,
        &config,
        exif_frame_config.as_ref(),
        exif_assets.as_ref(),
        Some(Box::new(on_progress)),
    );

    // 進捗行を消してから結果一覧に移る
    if interactive {
        eprintln!("\r\x1b[K");
    }

    let mut success_count = 0usize;
    let mut failed_count = 0usize;
    let mut over_limit_count = 0usize;

    for (i, result) in results.iter().enumerate() {
        let path = &image_files[i];
        match result {
            Ok(r) => {
                success_count += 1;
                let quality_info = r
                    .final_quality
                    .map_or(String::new(), |q| format!(", quality: {}%", q));
                println!(
                    "[{}/{}] {} → {} ({:.1} MB{}) ✓",
                    i + 1,
                    total_count,
                    display_name(path),
                    display_name(Path::new(&r.output_path)),
                    r.final_size_mb,
                    quality_info
                );
                // core は自ら出力しないので、警告の提示は呼び出し元の責務。
                for warning in &r.warnings {
                    eprintln!("  Warning: {}", warning);
                }
                if r.size_limit_exceeded {
                    over_limit_count += 1;
                }
            }
            Err(e) => {
                failed_count += 1;
                eprintln!(
                    "[{}/{}] {} ✗ Error: {}",
                    i + 1,
                    total_count,
                    display_name(path),
                    e
                );
            }
        }
    }

    let duration = start.elapsed();

    println!(
        "\nCompleted: {} successful, {} failed",
        success_count, failed_count
    );
    if over_limit_count > 0 {
        eprintln!(
            "Warning: {} file(s) exceed the {} MB limit even at minimum quality",
            over_limit_count, args.max_size
        );
    }
    println!("Total time: {:.1}s", duration.as_secs_f64());

    Ok(())
}
