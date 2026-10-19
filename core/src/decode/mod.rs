//! 画像ファイルのデコード（`image` クレートが読めない JPEG XL / AVIF の振り分け）
//!
//! 形式は**拡張子ではなく中身で判定する**。JPEG XL / AVIF は先頭バイトで見分け、
//! それ以外も `image` の中身による推定を優先する（拡張子は推定できないときの手がかり）。
//!
//! # 向き（Orientation）
//!
//! JPEG XL はコードストリームの向き、AVIF は `irot`/`imir` が正で、どちらの仕様も
//! Exif の Orientation を無視するよう定めている。この 2 形式は [`open`] / [`dimensions`] が
//! 向きを適用済みで返すので、[`Format::exif_orientation_applies`] が偽の形式では
//! Exif の向きを使わない（使うと二重回転になる）。
//!
//! # メモリの上限
//!
//! **出力の画素バッファは `image::open` と同じ `Limits::default()`（512MB）までにする。**
//! 寸法だけ巨大に偽装したファイルで OOM になると abort で `catch_unwind` でも止まらないため、
//! 画素を確保する前に宣言された寸法で弾く。
//!
//! デコーダ内部の作業領域はこの 512MB に含めない（JPEG XL は 1 画素あたり出力の数倍を使い、
//! 含めると 20MP 前後の普通の写真が開けなくなる）。ただし作業領域が宣言された寸法より
//! 大きくなりうる箇所には、それぞれ別に上限を置く（[`JXL_WORKING_BYTES_PER_PIXEL`]、
//! [`frame_size_limits`]）。

mod av1;
mod avif;
mod isobmff;
mod jxl;
mod yuv;

use anyhow::{Context, Result, anyhow, ensure};
use image::{DynamicImage, ImageDecoder as _, ImageReader};
use jxl_oxide::integration::JxlDecoder;
use std::fs::File;
use std::io::{BufReader, Cursor, Read};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Format {
    Jxl,
    Avif,
    /// `image` クレートに任せるもの（JPEG / PNG / WebP）
    Other,
}

impl Format {
    /// Exif の Orientation を画素に適用すべき形式か
    pub(crate) fn exif_orientation_applies(self) -> bool {
        self == Format::Other
    }
}

/// 先頭バイトから形式を判定する。読めない・短いファイルは `Other`（後段の `image` が報告する）
pub(crate) fn sniff(path: &Path) -> Result<Format> {
    let mut head = Vec::with_capacity(64);
    File::open(path)
        .with_context(|| format!("Failed to open image: {}", path.display()))?
        .take(64)
        .read_to_end(&mut head)
        .with_context(|| format!("Failed to read image: {}", path.display()))?;
    Ok(sniff_bytes(&head))
}

const JXL_CODESTREAM: [u8; 2] = [0xFF, 0x0A];

fn sniff_bytes(head: &[u8]) -> Format {
    const JXL_CONTAINER: &[u8] = b"\0\0\0\x0CJXL \r\n\x87\n";
    if head.starts_with(&JXL_CODESTREAM) || head.starts_with(JXL_CONTAINER) {
        return Format::Jxl;
    }
    // ISOBMFF の ftyp: [size:4][b"ftyp"][major:4]...。avif-parse は major brand が
    // `avif` のものしか読まないので、互換ブランドだけに avif を持つものは AVIF として扱わない。
    // `avis`（アニメーション）は AVIF に回し、非対応である旨を avif-parse に報告させる
    if matches!(head.get(4..12), Some(b"ftypavif" | b"ftypavis")) {
        return Format::Avif;
    }
    Format::Other
}

/// 画像をデコードする。JPEG XL / AVIF は形式固有の向きを適用済み、それ以外は向き未適用
pub(crate) fn open(path: &Path, format: Format) -> Result<DynamicImage> {
    match format {
        Format::Jxl => guarded(|| {
            let decoder = jxl_decoder(path)?;
            image::Limits::default().reserve(decoder.total_bytes())?;
            Ok(DynamicImage::from_decoder(decoder)?)
        }),
        Format::Avif => guarded(|| {
            let data = std::fs::read(path)?;
            let layout = avif::read_layout(Cursor::new(&data))?;
            let parsed = avif_parse::read_avif(&mut data.as_slice())?;
            let limits = frame_size_limits(
                avif::declared(&parsed.primary_item)?,
                parsed
                    .alpha_item
                    .as_deref()
                    .map(avif::declared)
                    .transpose()?,
            )?;
            let color = av1::decode(&parsed.primary_item, limits.primary)?;
            let alpha = match (&parsed.alpha_item, limits.alpha) {
                (Some(a), Some(limit)) => Some(av1::decode(a, limit)?),
                _ => None,
            };
            Ok(layout.apply(yuv::to_image(
                &color,
                alpha.as_ref(),
                parsed.premultiplied_alpha,
            )?))
        }),
        Format::Other => Ok(ImageReader::open(path)?.with_guessed_format()?.decode()?),
    }
    .with_context(|| format!("Failed to open image: {}", path.display()))
}

/// 画素を読まずに (幅, 高さ) を得る。JPEG XL / AVIF は向き適用後の寸法
///
/// 一覧表示で全ファイルに対して呼ばれるので、ヘッダより先は読まない。
pub(crate) fn dimensions(path: &Path, format: Format) -> Result<(u32, u32)> {
    match format {
        Format::Jxl => guarded(|| Ok(jxl_decoder(path)?.dimensions())),
        Format::Avif => guarded(|| {
            Ok(avif::read_layout(BufReader::new(File::open(path)?))?.oriented_dimensions())
        }),
        Format::Other => Ok(ImageReader::open(path)?
            .with_guessed_format()?
            .into_dimensions()?),
    }
    .with_context(|| format!("Failed to read image dimensions: {}", path.display()))
}

/// JPEG XL の Exif ボックスを `exif` クレートで読める生 TIFF として取り出す。無ければ `None`
pub(crate) fn jxl_raw_exif(path: &Path) -> Option<Vec<u8>> {
    guarded(|| jxl::raw_exif(BufReader::new(File::open(path)?)))
        .ok()
        .flatten()
}

/// コードストリームだけを渡した jxl-oxide のデコーダ（ヘッダまで読んだ状態）
///
/// 剥がした中身が素のコードストリームで始まることを確かめる。コンテナの中にコンテナを
/// 入れられると、jxl-oxide が内側の `brob` を上限なしで展開してしまう（`jxl` モジュール冒頭）。
///
/// **スレッドプールは使わない（`JxlThreadPool::none()`）。** jxl-oxide は参照フレームの
/// 描画を rayon のプールへ切り離して投げる。そこで panic すると rayon がプロセスを
/// abort させ、`catch_unwind` が効かない。プールなしならその場で実行されるので捕まえられる。
/// 一括変換はファイル単位で並列に動くので全体は遅くならない（1 枚だけを開くときは、
/// 24MP の非可逆 JPEG XL で約 0.5 秒遅くなる）。
fn jxl_decoder(path: &Path) -> Result<JxlDecoder<impl Read>> {
    let mut codestream = jxl::Codestream::new(BufReader::new(File::open(path)?))?;
    let mut signature = [0u8; 2];
    codestream.read_exact(&mut signature)?;
    ensure!(
        signature == JXL_CODESTREAM,
        "JPEG XL container does not hold a bare codestream"
    );
    let mut decoder = JxlDecoder::with_thread_pool(
        Cursor::new(signature).chain(codestream),
        jxl_oxide::JxlThreadPool::none(),
    )?;
    let (width, height) = decoder.dimensions();
    let mut limits = image::Limits::no_limits();
    // 上限値そのものの見直しは別の spec で扱う。ここでは算術が溢れないようにするだけ
    limits.max_alloc = Some(
        (u64::from(width) * u64::from(height))
            .saturating_mul(JXL_WORKING_BYTES_PER_PIXEL)
            .saturating_add(JXL_WORKING_BASE),
    );
    decoder.set_limits(limits)?;
    Ok(decoder)
}

/// jxl-oxide の作業領域の上限（画像の 1 画素あたり）
///
/// 作業領域は画像ヘッダの寸法ではなくフレームの寸法で確保される。フレームは画像より
/// 大きく宣言できる（`have_crop`）ので、出力の 512MB 判定だけでは、画像 1x1・フレーム
/// 巨大の数 KB のファイルで abort する。上限を画像の寸法に比例させて、超えたら jxl-oxide に
/// Err を返させる。6000x4000 で実測した必要量は、非可逆 RGB 8bit で 24〜32B、
/// 16bit RGBA で 32〜40B（可逆はこれ以下）。余裕を見て 64B
const JXL_WORKING_BYTES_PER_PIXEL: u64 = 64;
/// 寸法によらない作業領域（小さい画像でも要るぶん）
const JXL_WORKING_BASE: u64 = 64 << 20;

/// AVIF のデコーダに渡す `frame_size_limit`（画素数）。主画像とアルファで別々に持つ
#[derive(Debug)]
struct FrameSizeLimits {
    primary: u32,
    alpha: Option<u32>,
}

/// AVIF のデコードに先立って、宣言された寸法で弾き、デコーダに渡す `frame_size_limit` を決める
///
/// 判定は出力の画素バッファ（主画像 RGB ＋ アルファ 1 チャンネル）の合計を 1 つの
/// `Limits::default()`（512MB）に数える。理由:
/// - シーケンスヘッダの数は [`avif::declared`] が 1 に限っている。宣言の最大寸法より大きい
///   フレームは `frame_size_limit` でデコーダが拒否するので、宣言が上限の根拠になる
/// - アルファの深度は主画像のものを使う。変換後の出力は主画像の深度で作られるため
/// - `frame_size_limit` は 0 が無制限なので、`u32` に収まらない値や 0 を渡す前に、
///   必ず 512MB 判定を先に通す（`u32::try_from` はその後）。判定を通れば 2^32 画素未満である
/// - アルファには `min(アルファの宣言, 主画像の宣言)` を渡す。アルファは主画像と同じ寸法で
///   なければならず、主画像より大きい宣言を許す理由がない
fn frame_size_limits(
    primary: avif::Declared,
    alpha: Option<avif::Declared>,
) -> Result<FrameSizeLimits> {
    let bytes_per_sample: u64 = if primary.bit_depth > 8 { 2 } else { 1 };
    let mut budget = image::Limits::default();
    let over =
        |e| anyhow::Error::new(e).context("AVIF declares a size over the 512MB output limit");
    budget
        .reserve(primary.pixels.saturating_mul(3 * bytes_per_sample))
        .map_err(over)?;
    if let Some(alpha) = &alpha {
        budget
            .reserve(alpha.pixels.saturating_mul(bytes_per_sample))
            .map_err(over)?;
    }
    let limit = |pixels: u64| u32::try_from(pixels).context("AVIF declares too many pixels");
    Ok(FrameSizeLimits {
        primary: limit(primary.pixels)?,
        alpha: alpha
            .map(|a| limit(a.pixels.min(primary.pixels)))
            .transpose()?,
    })
}

/// JPEG XL / AVIF のデコーダを panic から隔離する
///
/// 外部デコーダは壊れた入力で panic しうる（例: avif-parse は途中で切れたファイルで
/// `debug_assert!` に落ちる）。core が panic すると GUI ごと落ちるので、1 枚の失敗として
/// 扱えるよう `Err` に変える。`metadata::write_metadata` と同じ方針。
///
/// **捕まえられるのは呼び出したスレッドで起きた panic だけ。** そのため jxl-oxide には
/// スレッドプールを使わせない（[`jxl_decoder`]）。rav1d（AVIF）は avif-decode が自前の
/// ワーカースレッドを起こし、その数を変える API がない。ワーカーで panic すると
/// 呼び出し側が止まる可能性は残る（rav1d は dav1d の移植で、入力から panic に至る経路は
/// 確認できていない）。
fn guarded<T>(f: impl FnOnce() -> Result<T>) -> Result<T> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f))
        .unwrap_or_else(|_| Err(anyhow!("decoder panicked on malformed input")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniff_recognises_signatures() {
        assert_eq!(sniff_bytes(&[0xFF, 0x0A, 0x00]), Format::Jxl);
        assert_eq!(sniff_bytes(b"\0\0\0\x0CJXL \r\n\x87\n\0\0"), Format::Jxl);
        assert_eq!(sniff_bytes(b"\0\0\0\x14ftypavif\0\0\0\0mif1"), Format::Avif);
        assert_eq!(sniff_bytes(b"\0\0\0\x14ftypavis\0\0\0\0mif1"), Format::Avif);
        // 互換ブランドにしか avif がないものは、デコーダ（avif-parse）が読めないので AVIF にしない
        assert_eq!(
            sniff_bytes(b"\0\0\0\x18ftypmif1\0\0\0\0miafavif"),
            Format::Other
        );
        // HEIC は AVIF ではない
        assert_eq!(
            sniff_bytes(b"\0\0\0\x18ftypheic\0\0\0\0mif1heic"),
            Format::Other
        );
        assert_eq!(sniff_bytes(&[0xFF, 0xD8, 0xFF]), Format::Other);
        assert_eq!(sniff_bytes(&[]), Format::Other);
    }

    fn d(pixels: u64, bit_depth: u8) -> avif::Declared {
        avif::Declared { pixels, bit_depth }
    }

    #[test]
    fn frame_size_limits_passes_the_declared_sizes() {
        let l = frame_size_limits(d(2048, 8), None).unwrap();
        assert_eq!((l.primary, l.alpha), (2048, None));
        // アルファは主画像との小さいほう
        let l = frame_size_limits(d(4096, 8), Some(d(1_000_000, 8))).unwrap();
        assert_eq!(l.alpha, Some(4096));
        let l = frame_size_limits(d(4096, 8), Some(d(1024, 8))).unwrap();
        assert_eq!(l.alpha, Some(1024));
    }

    #[test]
    fn frame_size_limits_counts_alpha_and_depth_against_one_budget() {
        let over = |r: Result<FrameSizeLimits>| {
            format!("{:#}", r.unwrap_err())
                .contains("AVIF declares a size over the 512MB output limit")
        };
        // 100M 画素: 8bit なら 300MB で通り、10bit なら 600MB で落ちる
        assert!(frame_size_limits(d(100_000_000, 8), None).is_ok());
        assert!(over(frame_size_limits(d(100_000_000, 10), None)));
        // 主画像 64x64 ＋ アルファ 23200x23200（1 チャンネルでも合計が上限を超える）
        assert!(over(frame_size_limits(
            d(64 * 64, 8),
            Some(d(23200 * 23200, 8))
        )));
        // アルファは主画像の深度で数える: 主 10bit なら 2 バイト
        // 8bit は 300,012,288 バイトで通り、10bit は 600,024,576 バイトで超える
        assert!(frame_size_limits(d(64 * 64, 8), Some(d(300_000_000, 8))).is_ok());
        assert!(over(frame_size_limits(
            d(64 * 64, 10),
            Some(d(300_000_000, 8))
        )));
    }

    #[test]
    fn frame_size_limits_rejects_before_the_limit_would_wrap() {
        // 65536x65536 = 2^32。u32 に入らず、0（無制限）にもならないまま 512MB 判定で落ちる
        let err = frame_size_limits(d(1 << 32, 8), None).unwrap_err();
        assert!(
            format!("{err:#}").contains("AVIF declares a size over the 512MB output limit"),
            "{err:#}"
        );
    }
}
