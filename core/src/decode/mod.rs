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
//! [`reserve_avif`]）。

mod avif;
mod isobmff;
mod jxl;

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
            reserve_avif(&parsed)?;
            let decoded = avif_decode::Decoder::from_avif(&data)?.to_image()?;
            Ok(layout.apply(avif_to_dynamic(decoded)?))
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
    limits.max_alloc =
        Some(u64::from(width) * u64::from(height) * JXL_WORKING_BYTES_PER_PIXEL + JXL_WORKING_BASE);
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

/// AVIF の画素バッファの確保に先立って、寸法で弾く
///
/// 宣言された最大寸法は他形式と同じく出力 512MB で判定する。加えて、フレームヘッダが
/// 宣言を超えて取りうる最大寸法（[`avif::max_frame_pixels`]）も見る。普通のエンコーダは
/// 寸法を最小のビット数で書くので、この最大は縦横それぞれ宣言の 2 倍未満、面積で 4 倍未満に
/// 収まる。上限を 4 倍にしておけば、宣言で通るファイルをここで落とすことはない。
fn reserve_avif(parsed: &avif_parse::AvifData) -> Result<()> {
    const WORST_CASE_FACTOR: u64 = 4;
    let meta = parsed.primary_item_metadata()?;
    let channels = if parsed.alpha_item.is_some() { 4 } else { 3 };
    let bytes_per_sample = if meta.bit_depth > 8 { 2 } else { 1 };
    let declared = u64::from(meta.max_frame_width.get()) * u64::from(meta.max_frame_height.get());
    image::Limits::default().reserve(declared * channels * bytes_per_sample)?;

    let mut worst = image::Limits::default();
    worst.max_alloc = worst.max_alloc.map(|max| max * WORST_CASE_FACTOR);
    worst.reserve(avif::max_frame_pixels(&parsed.primary_item)? * channels * bytes_per_sample)?;
    if let (Some(alpha), Some(alpha_meta)) = (&parsed.alpha_item, parsed.alpha_item_metadata()?) {
        let bytes_per_sample = if alpha_meta.bit_depth > 8 { 2 } else { 1 };
        worst.reserve(avif::max_frame_pixels(alpha)? * bytes_per_sample)?;
    }
    Ok(())
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

fn avif_to_dynamic(decoded: avif_decode::Image) -> Result<DynamicImage> {
    use avif_decode::Image as A;
    fn buffer<P: image::Pixel>(
        width: usize,
        height: usize,
        data: Vec<P::Subpixel>,
    ) -> Result<image::ImageBuffer<P, Vec<P::Subpixel>>> {
        let (w, h) = (u32::try_from(width)?, u32::try_from(height)?);
        image::ImageBuffer::from_raw(w, h, data).context("AVIF pixel buffer size mismatch")
    }
    Ok(match decoded {
        A::Rgb8(img) => {
            let (px, w, h) = img.into_contiguous_buf();
            DynamicImage::ImageRgb8(buffer(
                w,
                h,
                px.iter().flat_map(|p| [p.r, p.g, p.b]).collect(),
            )?)
        }
        A::Rgba8(img) => {
            let (px, w, h) = img.into_contiguous_buf();
            DynamicImage::ImageRgba8(buffer(
                w,
                h,
                px.iter().flat_map(|p| [p.r, p.g, p.b, p.a]).collect(),
            )?)
        }
        A::Rgb16(img) => {
            let (px, w, h) = img.into_contiguous_buf();
            DynamicImage::ImageRgb16(buffer(
                w,
                h,
                px.iter().flat_map(|p| [p.r, p.g, p.b]).collect(),
            )?)
        }
        A::Rgba16(img) => {
            let (px, w, h) = img.into_contiguous_buf();
            DynamicImage::ImageRgba16(buffer(
                w,
                h,
                px.iter().flat_map(|p| [p.r, p.g, p.b, p.a]).collect(),
            )?)
        }
        A::Gray8(img) => {
            let (px, w, h) = img.into_contiguous_buf();
            DynamicImage::ImageLuma8(buffer(w, h, px.iter().map(|p| p.value()).collect())?)
        }
        A::Gray16(img) => {
            let (px, w, h) = img.into_contiguous_buf();
            DynamicImage::ImageLuma16(buffer(w, h, px.iter().map(|p| p.value()).collect())?)
        }
    })
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
}
