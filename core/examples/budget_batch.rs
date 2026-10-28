//! メモリ予算を引数で渡して一括変換し、借用の記録を残す（メモリ予算の計測用）
//!
//! 使い方:
//!
//! ```text
//! budget_batch <input_dir> <output_dir> --budget <bytes> [--limit <bytes>] [--mode crop|pad] [--exif-frame] [--log <csv>]
//! budget_batch --estimate <file>... [--mode crop|pad] [--exif-frame]
//! ```
//!
//! - 一括変換: 1 枚ごとに `Ok <path>` か `Err <path>: <理由>` を出し、最後に
//!   `max_in_use=<bytes> files=<n> failed=<n>` を出す。`--limit` の既定は予算の 3/2
//!   （物理メモリの 1/2 : 3/4 の比を保つ）。`--log` には借用の記録を `at_ms,kind,estimate,in_use,holders` の CSV で書く
//! - `--estimate`: 各ファイルの変換の見積もりを `<bytes> <path>` で出す
//! - 品質 90・最大 8MB は CLI の既定と同じ。`--exif-frame` は既定のフレーム設定と既定の素材
//!
//! CLI はプロセス全体の予算（物理メモリの半分）しか使わないので、8GB の PC 相当の予算で測るにはこれを使う。
//! 結果は `docs/superpowers/plans/2026-10-28-memory-budget.md` の実施メモにある。

use picture_tool_core::exif_frame::{AssetDirs, ExifAssets, ExifFrameConfig};
use picture_tool_core::{
    BackgroundColor, ConversionMode, EventKind, MemoryBudget, ProcessingConfig,
    collect_image_files, estimate_conversion, process_batch_with_budget,
};
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "usage: budget_batch <input_dir> <output_dir> --budget <bytes> [--limit <bytes>] \
[--mode crop|pad] [--exif-frame] [--log <csv>]\n       budget_batch --estimate <file>... [--mode crop|pad] [--exif-frame]";

struct Args {
    positional: Vec<PathBuf>,
    estimate: bool,
    budget: Option<u64>,
    limit: Option<u64>,
    mode: ConversionMode,
    exif_frame: bool,
    log: Option<PathBuf>,
}

fn parse() -> Result<Args, String> {
    let mut args = Args {
        positional: Vec::new(),
        estimate: false,
        budget: None,
        limit: None,
        mode: ConversionMode::Crop,
        exif_frame: false,
        log: None,
    };
    let mut it = std::env::args().skip(1);
    let bytes = |v: Option<String>| -> Result<u64, String> {
        v.ok_or("missing value")?
            .parse()
            .map_err(|e| format!("bad number: {e}"))
    };
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--estimate" => args.estimate = true,
            "--budget" => args.budget = Some(bytes(it.next())?),
            "--limit" => args.limit = Some(bytes(it.next())?),
            "--mode" => {
                args.mode = match it.next().as_deref() {
                    Some("crop") => ConversionMode::Crop,
                    Some("pad") => ConversionMode::Pad,
                    _ => return Err("--mode must be crop or pad".into()),
                }
            }
            "--exif-frame" => args.exif_frame = true,
            "--log" => args.log = Some(it.next().ok_or("missing value")?.into()),
            _ if arg.starts_with("--") => return Err(format!("unknown option {arg}")),
            _ => args.positional.push(arg.into()),
        }
    }
    Ok(args)
}

fn main() -> ExitCode {
    let args = match parse() {
        Ok(args) => args,
        Err(e) => {
            eprintln!("{e}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let config = ProcessingConfig {
        mode: args.mode,
        bg_color: BackgroundColor::White,
        quality: 90,
        max_size_mb: 8,
        delete_originals: false,
        max_width: None,
    };

    if args.estimate {
        let mut failed = false;
        for path in &args.positional {
            match estimate_conversion(path, &config, args.exif_frame) {
                Ok(bytes) => println!("{bytes} {}", path.display()),
                Err(e) => {
                    println!("Err {}: {e:#}", path.display());
                    failed = true;
                }
            }
        }
        return if failed {
            ExitCode::from(1)
        } else {
            ExitCode::SUCCESS
        };
    }

    let (Some(budget), [input, output]) = (args.budget, args.positional.as_slice()) else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let limit = args.limit.unwrap_or(budget / 2 * 3);
    let (frame, assets) = if args.exif_frame {
        match ExifAssets::load(AssetDirs::default()) {
            Ok(assets) => (Some(ExifFrameConfig::default()), Some(assets)),
            Err(e) => {
                eprintln!("failed to load Exif frame assets: {e:#}");
                return ExitCode::from(2);
            }
        }
    } else {
        (None, None)
    };

    let files = collect_image_files(input).files;
    let b = MemoryBudget::with_log(budget, limit);
    let results = process_batch_with_budget(
        &b,
        &files,
        output,
        &config,
        frame.as_ref(),
        assets.as_ref(),
        None,
    );
    let mut failed = 0;
    for (path, result) in files.iter().zip(&results) {
        match result {
            Ok(_) => println!("Ok {}", path.display()),
            Err(e) => {
                failed += 1;
                println!("Err {}: {e:#}", path.display());
            }
        }
    }

    let events = b.events();
    let max_in_use = events.iter().map(|e| e.in_use).max().unwrap_or(0);
    println!(
        "max_in_use={max_in_use} files={} failed={failed}",
        files.len()
    );
    if let Some(log) = args.log {
        let mut csv = String::from("at_ms,kind,estimate,in_use,holders\n");
        for e in &events {
            let kind = match e.kind {
                EventKind::Wait => "wait",
                EventKind::Acquire => "acquire",
                EventKind::Release => "release",
            };
            csv.push_str(&format!(
                "{:.3},{kind},{},{},{}\n",
                e.at.as_secs_f64() * 1000.0,
                e.estimate,
                e.in_use,
                e.holders
            ));
        }
        if let Err(e) = std::fs::File::create(&log).and_then(|mut f| f.write_all(csv.as_bytes())) {
            eprintln!("failed to write {}: {e}", log.display());
            return ExitCode::from(2);
        }
    }
    ExitCode::SUCCESS
}
