//! ★でグリッドを絞り込むための「フォルダー全件走査」のコスト実測（spike）。
//!
//! いまの `read_metadata` は `std::fs::read` で**ファイル全体**をメモリに載せる。
//! ★1つを知るために 5MB の JPEG を丸ごと読むのは、可視範囲 40 枚なら気にならないが、
//! 1,000 枚のフォルダーを一度に走査するなら効いてくる。
//!
//! Exif も XMP も SOS（画素データの開始）より前の APP1 セグメントにある。
//! **先頭のセグメントだけ読めば済む**はずで、その差を数字にするのがここ。
//!
//! 通常のテスト実行では走らない（実写真をリポジトリに入れないため）。
//!
//! 実行:
//! ```text
//! # 1. コーパスを作る（元にする実写真 1 枚から N 枚のコピーを作り、★を散らす）
//! PICTURE_TOOL_SPIKE_SOURCE=docs/tmp/20260328-DSC05024.jpg \
//! PICTURE_TOOL_SPIKE_DIR=/var/tmp/rating-spike \
//! PICTURE_TOOL_SPIKE_COUNT=1000 \
//!   cargo test -p picture-tool-core --release --test rating_scan_spike \
//!     -- --ignored --nocapture generate_corpus
//!
//! # 2. 測る
//! PICTURE_TOOL_SPIKE_DIR=/var/tmp/rating-spike \
//!   cargo test -p picture-tool-core --release --test rating_scan_spike \
//!     -- --ignored --nocapture scan_benchmark
//! ```

use std::fs::File;
use std::io::{BufReader, Read};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use picture_tool_core::metadata::{read_metadata, read_metadata_bytes, EditableMetadata};
use rayon::prelude::*;

// --- 測る対象 ---------------------------------------------------------------

/// **軽量経路**: JPEG の先頭から SOS までを辿り、APP1 だけを写した最小の JPEG を組み立てる。
///
/// SOS 以降（＝ファイルの 99% を占める画素データ）には Exif も XMP も無い。
/// APP1 以外のセグメント（APP0/APP2 ICC/APP13 IPTC/DQT/DHT/SOF）も
/// 読まずに `seek` で飛ばす。組み立てた buffer は既存の解析経路
/// （`little_exif` と `img_parts`）がそのまま食える形にしておく ──
/// 解析の実装を変えずに「読む量」だけを変えた差を測るため。
fn header_jpeg(path: &Path) -> std::io::Result<Vec<u8>> {
    let mut file = BufReader::with_capacity(64 * 1024, File::open(path)?);
    let mut out = Vec::with_capacity(64 * 1024);

    let mut soi = [0u8; 2];
    file.read_exact(&mut soi)?;
    if soi != [0xFF, 0xD8] {
        return Err(broken("SOI ではない"));
    }
    out.extend_from_slice(&soi);

    let mut byte = [0u8; 1];
    loop {
        file.read_exact(&mut byte)?;
        if byte[0] != 0xFF {
            return Err(broken("セグメント境界が 0xFF で始まっていない"));
        }
        // 0xFF のパディング（fill byte）を読み飛ばす
        let mut marker = 0xFF;
        while marker == 0xFF {
            file.read_exact(&mut byte)?;
            marker = byte[0];
        }
        match marker {
            // 長さを持たないマーカー
            0xD9 => break, // EOI（APP1 が 1 つも無い JPEG）
            0x01 | 0xD0..=0xD7 => continue,
            _ => {}
        }

        let mut len_bytes = [0u8; 2];
        file.read_exact(&mut len_bytes)?;
        let len = u16::from_be_bytes(len_bytes) as usize;
        if len < 2 {
            return Err(broken("セグメント長が短すぎる"));
        }
        let payload = len - 2;

        if marker == 0xDA {
            // SOS。ヘッダだけ写し、画素は 1 バイトも読まずに EOI で閉じる
            let mut body = vec![0u8; payload];
            file.read_exact(&mut body)?;
            out.extend_from_slice(&[0xFF, 0xDA]);
            out.extend_from_slice(&len_bytes);
            out.extend_from_slice(&body);
            out.extend_from_slice(&[0xFF, 0xD9]);
            break;
        }

        if marker == 0xE1 {
            // APP1 = Exif と XMP。**これだけが要る**
            let mut body = vec![0u8; payload];
            file.read_exact(&mut body)?;
            out.extend_from_slice(&[0xFF, marker]);
            out.extend_from_slice(&len_bytes);
            out.extend_from_slice(&body);
        } else {
            // **`Seek::seek` ではなく `seek_relative`。** 前者は BufReader の
            // バッファを無条件に捨てるので、飛ばした先を読むために同じ 64KB を
            // 読み直すことになる（最初の実測で B が B' の 2 倍遅かった原因）
            file.seek_relative(payload as i64)?;
        }
    }
    Ok(out)
}

/// **前方一括経路**: 先頭 `PREFIX` バイトを 1 回で読み、その中でセグメントを辿る。
///
/// `header_jpeg` は APP1 とそれ以外で read と seek を切り替える。1 枚あたりの
/// syscall が増えるので、「1 回だけ大きく読む」方が速い可能性がある。どちらが速いかは
/// 測らないと分からないので両方測る。
///
/// SOS が `PREFIX` 内に無ければ（大きな MakerNote や Extended XMP）**倍々に伸ばす**。
/// ファイル全体を読み直す実装にすると、その 1 枚だけ現行と同じコストに戻る。
fn prefix_jpeg(path: &Path) -> std::io::Result<Vec<u8>> {
    const PREFIX: usize = 64 * 1024;

    let mut file = File::open(path)?;
    let mut buf = Vec::with_capacity(PREFIX);
    let mut want = PREFIX;
    loop {
        let base = buf.len();
        buf.resize(want, 0);
        let mut filled = base;
        while filled < want {
            match file.read(&mut buf[filled..])? {
                0 => break,
                n => filled += n,
            }
        }
        let eof = filled < want;
        buf.truncate(filled);

        if let Some(at) = sos_offset(&buf) {
            buf.truncate(at);
            buf.extend_from_slice(&[0xFF, 0xD9]);
            return Ok(buf);
        }
        if eof {
            // SOS が無いまま終端。壊れているか JPEG ではない。そのまま返して
            // 既存の解析経路に判断させる
            return Ok(buf);
        }
        want *= 2;
    }
}

/// 1 枚あたり実際に読んだ「先頭」の大きさ。定数 `PREFIX` を決める根拠にする
fn header_size(path: &Path) -> std::io::Result<usize> {
    header_jpeg(path).map(|b| b.len())
}

/// バイト列の中の SOS マーカー開始位置。`None` は「この長さの中には無い」
fn sos_offset(bytes: &[u8]) -> Option<usize> {
    let mut at = 2;
    loop {
        if at + 1 >= bytes.len() || bytes[at] != 0xFF {
            return None;
        }
        let mut marker_at = at + 1;
        while *bytes.get(marker_at)? == 0xFF {
            marker_at += 1;
        }
        let marker = bytes[marker_at];
        match marker {
            0xDA | 0xD9 => return Some(at),
            0x01 | 0xD0..=0xD7 => {
                at = marker_at + 1;
                continue;
            }
            _ => {}
        }
        let len = u16::from_be_bytes([*bytes.get(marker_at + 1)?, *bytes.get(marker_at + 2)?]);
        at = marker_at + 1 + len as usize;
    }
}

fn broken(why: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, why.to_string())
}

// --- ページキャッシュ -------------------------------------------------------

/// 対象ファイルのページキャッシュを落とす。
///
/// **これが本番の状況である。** フォルダーを開いた直後は、`list_images` が
/// stat しただけで中身は読まれておらず、サムネイルを取った可視範囲の数十枚を
/// 除いてすべてコールド。「★で絞り込む」を押した瞬間のコストはこの状態で決まる。
///
/// `POSIX_FADV_DONTNEED` は clean なページしか落とせないので、先に `sync` する。
fn drop_page_cache(paths: &[PathBuf]) {
    unsafe { libc::sync() };
    for path in paths {
        if let Ok(file) = File::open(path) {
            unsafe {
                libc::posix_fadvise(file.as_raw_fd(), 0, 0, libc::POSIX_FADV_DONTNEED);
            }
        }
    }
}

// --- 実測 -------------------------------------------------------------------

fn spike_dir() -> PathBuf {
    PathBuf::from(
        std::env::var("PICTURE_TOOL_SPIKE_DIR")
            .expect("PICTURE_TOOL_SPIKE_DIR にコーパスのディレクトリを渡すこと"),
    )
}

fn corpus(dir: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("コーパスのディレクトリを読めない")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("jpg")))
        .collect();
    paths.sort();
    assert!(
        !paths.is_empty(),
        "コーパスが空。先に generate_corpus を走らせる"
    );
    paths
}
#[test]
#[ignore = "実写真が要る spike"]
fn generate_corpus() {
    let source = PathBuf::from(
        std::env::var("PICTURE_TOOL_SPIKE_SOURCE")
            .expect("PICTURE_TOOL_SPIKE_SOURCE に元にする実写真を渡すこと"),
    );
    let dir = spike_dir();
    let count: usize = std::env::var("PICTURE_TOOL_SPIKE_COUNT")
        .unwrap_or_else(|_| "1000".to_string())
        .parse()
        .expect("PICTURE_TOOL_SPIKE_COUNT は整数");

    std::fs::create_dir_all(&dir).expect("ディレクトリを作れない");

    // ★を散らした種を作る。None も混ぜる ── ★の付いていない写真の方が
    // 実際には多く、「タグが無い」経路のコストも走査に含まれる
    let seeds: Vec<PathBuf> = [None, Some(1u8), Some(2), Some(3), Some(4), Some(5)]
        .iter()
        .enumerate()
        .map(|(at, stars)| {
            let seed = dir.join(format!(".seed-{at}.jpg"));
            std::fs::copy(&source, &seed).expect("種をコピーできない");
            if let Some(stars) = stars {
                picture_tool_core::metadata::write_metadata(
                    &seed,
                    &EditableMetadata {
                        title: None,
                        comment: None,
                        rating: Some(*stars),
                    },
                )
                .expect("種に★を書けない");
            }
            seed
        })
        .collect();

    // **`std::fs::copy` を使ってはならない。** Linux では `copy_file_range` に落ち、
    // btrfs はそれを reflink（CoW の共有）で満たす。1,000 枚が同じ物理ブロックを
    // 指すので、コールドキャッシュのつもりの走査が 2 枚目以降すべてキャッシュヒット
    // になり、**軽量経路の効果を実際より小さく見せる**（実測で確認した罠）。
    // 種を 1 度メモリに載せ、1 枚ずつ実体を書く
    let bodies: Vec<Vec<u8>> = seeds
        .iter()
        .map(|seed| std::fs::read(seed).expect("種を読めない"))
        .collect();
    for at in 0..count {
        let target = dir.join(format!("photo-{at:05}.jpg"));
        std::fs::write(&target, &bodies[at % bodies.len()]).expect("書けない");
    }

    let bytes: u64 = corpus(&dir)
        .iter()
        .filter_map(|p| p.metadata().ok())
        .map(|m| m.len())
        .sum();
    println!(
        "コーパス: {} 枚 / {:.1} MB（1 枚 {:.1} MB）",
        count,
        bytes as f64 / 1e6,
        bytes as f64 / 1e6 / count as f64
    );
}

/// 何度か回して**中央値**を採る。1 回だけだと、たまたま走った別プロセスの
/// I/O に引きずられた数字を設計の根拠にしてしまう
fn reps() -> usize {
    std::env::var("PICTURE_TOOL_SPIKE_REPS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3)
}

/// ページキャッシュに全ファイルを載せる（計測しない）。
///
/// **warm の計測にはこれが要る。** 直前に走った cold の計測は先頭しか読んでいない
/// ので、素直に続けると「warm のはずの 1 本目だけ実質 cold」という順序依存の数字が出る
/// （最初の実測で `I/O のみ` の warm が A の warm より 4 倍遅かった原因）。
fn prime_page_cache(paths: &[PathBuf]) {
    paths.par_iter().for_each(|p| {
        let _ = std::hint::black_box(std::fs::read(p));
    });
}

/// 1 回の走査。`read` が 1 枚あたりの処理
fn run_once(
    paths: &[PathBuf],
    parallel: bool,
    read: &(impl Fn(&Path) -> Option<u8> + Sync),
) -> Duration {
    let started = Instant::now();
    let sum: u64 = if parallel {
        paths.par_iter().map(|p| read(p).unwrap_or(0) as u64).sum()
    } else {
        paths.iter().map(|p| read(p).unwrap_or(0) as u64).sum()
    };
    let elapsed = started.elapsed();
    std::hint::black_box(sum);
    elapsed
}

fn measure(
    label: &str,
    paths: &[PathBuf],
    cold: bool,
    parallel: bool,
    read: impl Fn(&Path) -> Option<u8> + Sync,
) {
    let reps = reps();
    let mut runs: Vec<Duration> = (0..reps)
        .map(|_| {
            if cold {
                drop_page_cache(paths);
            } else {
                prime_page_cache(paths);
            }
            run_once(paths, parallel, &read)
        })
        .collect();
    runs.sort();
    let median = runs[reps / 2].as_secs_f64() * 1000.0;
    println!(
        "  {label:<24} {median:>7.0} ms  ({:>5.2} ms/枚)   min {:>7.0} / max {:>7.0}",
        median / paths.len() as f64,
        runs[0].as_secs_f64() * 1000.0,
        runs[reps - 1].as_secs_f64() * 1000.0
    );
}

/// このプロセスがこれまでに使った物理メモリの最大値（KB）
fn peak_rss_kb() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("VmHWM:"))?
                .split_whitespace()
                .nth(1)?
                .parse()
                .ok()
        })
        .unwrap_or(0)
}

/// `list_images` が 1 枚ごとにやっていること（★は読まない）。
/// `image_dimensions_oriented` は画像ヘッダ読み ＋ `read_exif_info`（kamadak-exif の
/// `read_from_container`＝Exif APP1 だけを読む）で、**既に先頭を触っている**
fn read_listing(path: &Path) -> Option<u8> {
    let dims = picture_tool_core::image_dimensions_oriented(path).ok()?;
    let meta = std::fs::metadata(path).ok()?;
    std::hint::black_box((dims, meta.len()));
    Some(0)
}

/// `list_images` の仕事 ＋ ★。畳んだ場合の 1 枚あたりのコスト
fn read_listing_with_rating(path: &Path) -> Option<u8> {
    read_listing(path)?;
    read_header(path)
}

fn read_full(path: &Path) -> Option<u8> {
    read_metadata(path).ok().and_then(|r| r.metadata.rating)
}
fn read_header(path: &Path) -> Option<u8> {
    header_jpeg(path)
        .ok()
        .and_then(|b| read_metadata_bytes(b, path).ok())
        .and_then(|r| r.metadata.rating)
}
fn read_prefix(path: &Path) -> Option<u8> {
    prefix_jpeg(path)
        .ok()
        .and_then(|b| read_metadata_bytes(b, path).ok())
        .and_then(|r| r.metadata.rating)
}

#[test]
#[ignore = "実写真が要る spike"]
fn scan_benchmark() {
    let dir = spike_dir();
    let paths = corpus(&dir);
    let bytes: u64 = paths
        .iter()
        .filter_map(|p| p.metadata().ok())
        .map(|m| m.len())
        .sum();

    // --- 1. 答えが一致すること。速い経路が違う★を返したら数字に意味は無い ---
    let mut sizes: Vec<usize> = Vec::with_capacity(paths.len());
    let mut rated = 0usize;
    for path in &paths {
        let full = read_full(path);
        assert_eq!(
            full,
            read_header(path),
            "軽量経路の★が違う: {}",
            path.display()
        );
        assert_eq!(
            full,
            read_prefix(path),
            "前方一括経路の★が違う: {}",
            path.display()
        );
        if full.is_some() {
            rated += 1;
        }
        sizes.push(header_size(path).expect("先頭を読めない"));
    }
    sizes.sort_unstable();
    // **前提**: ★の付いた写真と付いていない写真が両方コーパスに居ること。
    // 片方しか無いと「どちらの経路も常に None を返す」でも一致してしまう
    assert!(rated > 0 && rated < paths.len(), "★の有無が混ざっていない");

    let header_total: u64 = sizes.iter().map(|s| *s as u64).sum();
    println!(
        "\nコーパス {} 枚 / {:.1} MB（1 枚 {:.2} MB）。★あり {} 枚 / なし {} 枚",
        paths.len(),
        bytes as f64 / 1e6,
        bytes as f64 / 1e6 / paths.len() as f64,
        rated,
        paths.len() - rated
    );
    println!(
        "先頭（SOI〜SOS）の大きさ: 最小 {:.1} KB / 中央 {:.1} KB / 最大 {:.1} KB。\
         \n合計 {:.1} MB = 全体の {:.2}%\n",
        sizes[0] as f64 / 1024.0,
        sizes[sizes.len() / 2] as f64 / 1024.0,
        sizes[sizes.len() - 1] as f64 / 1024.0,
        header_total as f64 / 1e6,
        header_total as f64 / bytes as f64 * 100.0
    );

    // --- 2. 走査時間 ---
    // I/O の下限。解析を一切しない
    let io_full = |p: &Path| std::fs::read(p).ok().map(|b| b.len() as u8);
    let io_head = |p: &Path| header_jpeg(p).ok().map(|b| b.len() as u8);

    for parallel in [true, false] {
        println!(
            "--- {} / {} 回の中央値 ---",
            if parallel {
                "rayon 並列（read_image_ratings と同じ形）"
            } else {
                "逐次（1 枚あたりのコスト）"
            },
            reps()
        );
        for cold in [true, false] {
            println!(
                "  [{}]",
                if cold {
                    "cold: フォルダーを開いた直後"
                } else {
                    "warm: 2 回目以降"
                }
            );
            measure("I/O のみ・全体", &paths, cold, parallel, io_full);
            measure("I/O のみ・先頭", &paths, cold, parallel, io_head);
            measure("A: 現行 read_metadata", &paths, cold, parallel, read_full);
            measure("B: 軽量経路(seek)", &paths, cold, parallel, read_header);
            measure("B': 前方 64KB 一括", &paths, cold, parallel, read_prefix);
            measure("L: list_images 相当", &paths, cold, parallel, read_listing);
            measure(
                "L+B: list_images に畳む",
                &paths,
                cold,
                parallel,
                read_listing_with_rating,
            );
        }
        println!();
    }
}

/// 経路ごとの**ピークメモリ**。1 プロセスに 1 経路しか走らせない
/// （`VmHWM` は取り消せないので、同じプロセスで比べると常に最初の経路の値になる）。
///
/// 実行: `PICTURE_TOOL_SPIKE_ONLY=a|b|bp cargo test ... peak_memory`
#[test]
#[ignore = "実写真が要る spike"]
fn peak_memory() {
    let which = std::env::var("PICTURE_TOOL_SPIKE_ONLY").unwrap_or_else(|_| "a".to_string());
    let paths = corpus(&spike_dir());
    let before = peak_rss_kb();
    let elapsed = match which.as_str() {
        "a" => run_once(&paths, true, &(read_full as fn(&Path) -> Option<u8>)),
        "b" => run_once(&paths, true, &(read_header as fn(&Path) -> Option<u8>)),
        "bp" => run_once(&paths, true, &(read_prefix as fn(&Path) -> Option<u8>)),
        other => panic!("PICTURE_TOOL_SPIKE_ONLY は a / b / bp のいずれか（{other}）"),
    };
    println!(
        "経路 {which}: {} 枚を {:.0} ms。ピーク RSS {:.1} MB（走査前 {:.1} MB）",
        paths.len(),
        elapsed.as_secs_f64() * 1000.0,
        peak_rss_kb() as f64 / 1024.0,
        before as f64 / 1024.0
    );
}

/// **フォールバックへ落ちる頻度と、SOI〜SOS の大きさの分布**（絞り込み spec の未検証 3）。
///
/// `read_metadata_header` は手書きのセグメント走査なので、大きな MakerNote や
/// Extended XMP を持つファイルで失敗しうる。正しさは `read_metadata_fast` の
/// フォールバックが担保するが、**常時落ちるなら軽量経路の意味が無い。**
///
/// ```text
/// PICTURE_TOOL_SPIKE_DIR=/path/to/real/photos \
///   cargo test -p picture-tool-core --release --test rating_scan_spike \
///     -- --ignored --nocapture header_fallback_rate
/// ```
#[test]
#[ignore]
fn header_fallback_rate() {
    let dir = std::env::var("PICTURE_TOOL_SPIKE_DIR").expect("PICTURE_TOOL_SPIKE_DIR が要る");
    let files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("jpg") || e.eq_ignore_ascii_case("jpeg"))
        })
        .collect();
    assert!(!files.is_empty(), "JPEG が 1 枚も無い: {dir}");

    let mut failed: Vec<(PathBuf, String)> = Vec::new();
    let mut sizes: Vec<usize> = Vec::new();
    for path in &files {
        if let Err(error) = picture_tool_core::metadata::read_metadata_header(path) {
            failed.push((path.clone(), error.to_string()));
        }
        if let Ok(bytes) = header_jpeg(path) {
            sizes.push(bytes.len());
        }
    }
    sizes.sort_unstable();

    println!("枚数: {}", files.len());
    println!(
        "フォールバックへ落ちた: {} 枚（{:.2}%）",
        failed.len(),
        failed.len() as f64 * 100.0 / files.len() as f64
    );
    for (path, error) in failed.iter().take(10) {
        println!("  {} — {}", path.display(), error);
    }
    if !sizes.is_empty() {
        println!(
            "SOI〜SOS: 最小 {} / 中央 {} / 最大 {} バイト",
            sizes[0],
            sizes[sizes.len() / 2],
            sizes[sizes.len() - 1]
        );
    }
}

/// **コーパスのページキャッシュだけを落とす**（絞り込み spec の未検証 4 の下ごしらえ）。
///
/// 実機（`make dev`）でフォルダーを開く直前に叩く。`/proc/sys/vm/drop_caches` と違い
/// **昇格した権限が要らず、開発機の他のキャッシュも巻き添えにしない。**
///
/// ```text
/// PICTURE_TOOL_SPIKE_DIR=/var/tmp/rating-spike \
///   cargo test -p picture-tool-core --release --test rating_scan_spike \
///     -- --ignored --nocapture drop_corpus_cache
/// ```
#[test]
#[ignore]
fn drop_corpus_cache() {
    let dir = spike_dir();
    let files = corpus(&dir);
    assert!(!files.is_empty(), "コーパスが空: {}", dir.display());
    drop_page_cache(&files);
    println!("{} 枚のページキャッシュを落とした", files.len());
}
