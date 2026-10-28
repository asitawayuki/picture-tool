//! 1 枚の JPEG XL が通る最小の `max_alloc`（jxl-oxide の作業領域の上限）を二分探索して表示する
//!
//! 使い方: `cargo run --release -p picture-tool-core --example jxl_alloc_probe -- <path>`
//!
//! - 標準出力: `<path> <w>x<h> <色型> min_max_alloc=<bytes> per_pixel=<bytes/画素>`
//! - 探索の上端（64B/画素 + 64MiB）でも通らなければ、その旨を出して終了コード 1
//!
//! メモリ予算の見積もり（`core/src/budget/estimate.rs`）の JPEG XL の係数を再計測するためのもの。
//! 結果は `docs/superpowers/plans/2026-10-28-memory-budget.md` の実施メモにある。
//! jxl-oxide の結果は入力と上限だけで決まる（スレッドプールを使わない）ので、1 回測れば足りる。

use image::{DynamicImage, ImageDecoder as _};
use jxl_oxide::JxlThreadPool;
use jxl_oxide::integration::JxlDecoder;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const MIB: u64 = 1 << 20;

fn decoder(path: &Path) -> Result<JxlDecoder<BufReader<File>>, Box<dyn std::error::Error>> {
    Ok(JxlDecoder::with_thread_pool(
        BufReader::new(File::open(path)?),
        JxlThreadPool::none(),
    )?)
}

/// `max_alloc = m` で最後までデコードできるか
fn passes(path: &Path, m: u64) -> bool {
    let Ok(mut d) = decoder(path) else {
        return false;
    };
    let mut limits = image::Limits::no_limits();
    limits.max_alloc = Some(m);
    d.set_limits(limits).is_ok() && DynamicImage::from_decoder(d).is_ok()
}

fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);
    let (Some(path), None) = (args.next().map(PathBuf::from), args.next()) else {
        eprintln!("usage: jxl_alloc_probe <path>");
        return ExitCode::from(2);
    };
    let d = match decoder(&path) {
        Ok(d) => d,
        Err(err) => {
            eprintln!("failed to read {}: {err}", path.display());
            return ExitCode::from(2);
        }
    };
    let (w, h) = d.dimensions();
    let color = d.color_type();
    drop(d);
    let pixels = u64::from(w) * u64::from(h);

    // 1MiB 単位で探す。lo は通らない側、hi は通る側
    let mut hi = (pixels * 64 + 64 * MIB).div_ceil(MIB);
    if !passes(&path, hi * MIB) {
        println!(
            "{} {w}x{h} {color:?} does not decode within max_alloc={}",
            path.display(),
            hi * MIB
        );
        return ExitCode::from(1);
    }
    let mut lo = 0;
    while hi - lo > 1 {
        let mid = lo + (hi - lo) / 2;
        if passes(&path, mid * MIB) {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    let min = hi * MIB;
    println!(
        "{} {w}x{h} {color:?} min_max_alloc={min} per_pixel={:.2}",
        path.display(),
        min as f64 / pixels as f64
    );
    ExitCode::SUCCESS
}
