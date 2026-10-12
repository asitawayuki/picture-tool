//! 画像ファイルのデコード（`image` クレートが読めない JPEG XL / AVIF の振り分け）
//!
//! 形式は**拡張子ではなく先頭バイトで判定する**。拡張子と中身が食い違っていても、
//! `image::open` と同じく中身どおりにデコードする。
//!
//! # 向き（Orientation）
//!
//! JPEG XL はコードストリームの向き、AVIF は `irot`/`imir` が正で、どちらの仕様も
//! Exif の Orientation を無視するよう定めている。jxl-oxide は向きを適用済みの画素を
//! 返すので、[`Format::exif_orientation_applies`] が偽の形式では Exif の向きを使わない
//! （使うと二重回転になる）。avif-decode は `irot`/`imir` を適用しない（README の既知の制限）。

use anyhow::{Context, Result, anyhow};
use image::DynamicImage;
use std::fs::File;
use std::io::{BufReader, Read};
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

fn sniff_bytes(head: &[u8]) -> Format {
    const JXL_CODESTREAM: &[u8] = &[0xFF, 0x0A];
    const JXL_CONTAINER: &[u8] = b"\0\0\0\x0CJXL \r\n\x87\n";
    if head.starts_with(JXL_CODESTREAM) || head.starts_with(JXL_CONTAINER) {
        return Format::Jxl;
    }
    // ISOBMFF の ftyp: [size:4][b"ftyp"][major:4][minor:4][compatible:4*n]
    if head.len() >= 12 && &head[4..8] == b"ftyp" {
        let size = match u32::from_be_bytes([head[0], head[1], head[2], head[3]]) {
            // 0 は「ファイル末尾まで」、1 は 64bit の largesize が続く印。どちらも読めた範囲で見る
            0 | 1 => head.len(),
            n => n as usize,
        };
        let ftyp = &head[..size.clamp(12, head.len())];
        let (compatible, _) = ftyp.get(16..).unwrap_or(&[]).as_chunks::<4>();
        let is_avif = |brand: &[u8]| brand == b"avif" || brand == b"avis";
        if is_avif(&ftyp[8..12]) || compatible.iter().any(|b| is_avif(b)) {
            return Format::Avif;
        }
    }
    Format::Other
}

/// 画像をデコードする（向きは未適用。JPEG XL だけはデコーダが適用済み）
///
/// **確保量の上限は `image::open` と同じ `Limits::default()`（512MB）。** 寸法だけ巨大に
/// 偽装したファイルで OOM になると abort で `catch_unwind` でも止まらないため、
/// 画素バッファを確保する前に宣言された寸法で弾く。
pub(crate) fn open(path: &Path, format: Format) -> Result<DynamicImage> {
    use image::ImageDecoder as _;
    match format {
        Format::Jxl => guarded(|| {
            let reader = BufReader::new(File::open(path)?);
            let mut decoder = jxl_oxide::integration::JxlDecoder::new(reader)?;
            let mut limits = image::Limits::default();
            limits.reserve(decoder.total_bytes())?;
            decoder.set_limits(limits)?;
            Ok(DynamicImage::from_decoder(decoder)?)
        }),
        Format::Avif => guarded(|| {
            let data = std::fs::read(path)?;
            let avif = avif_parse::read_avif(&mut data.as_slice())?;
            let meta = avif.primary_item_metadata()?;
            let channels = if avif.alpha_item.is_some() { 4 } else { 3 };
            let bytes_per_sample = if meta.bit_depth > 8 { 2 } else { 1 };
            image::Limits::default().reserve(
                u64::from(meta.max_frame_width.get())
                    * u64::from(meta.max_frame_height.get())
                    * channels
                    * bytes_per_sample,
            )?;
            avif_to_dynamic(avif_decode::Decoder::from_avif(&data)?.to_image()?)
        }),
        Format::Other => Ok(image::open(path)?),
    }
    .with_context(|| format!("Failed to open image: {}", path.display()))
}

/// 画素を読まずに (幅, 高さ) を得る。JPEG XL は向き適用後の寸法
pub(crate) fn dimensions(path: &Path, format: Format) -> Result<(u32, u32)> {
    match format {
        Format::Jxl => guarded(|| {
            let image = jxl_oxide::JxlImage::open_with_defaults(path).map_err(|e| anyhow!(e))?;
            Ok((image.width(), image.height()))
        }),
        Format::Avif => guarded(|| {
            let mut reader = BufReader::new(File::open(path)?);
            let meta = avif_parse::read_avif(&mut reader)?.primary_item_metadata()?;
            Ok((meta.max_frame_width.get(), meta.max_frame_height.get()))
        }),
        Format::Other => Ok(image::image_dimensions(path)?),
    }
    .with_context(|| format!("Failed to read image dimensions: {}", path.display()))
}

/// JPEG XL の Exif ボックスを `exif` クレートで読める生 TIFF として取り出す。無ければ `None`
pub(crate) fn jxl_raw_exif(path: &Path) -> Option<Vec<u8>> {
    guarded(|| {
        let image = jxl_oxide::JxlImage::open_with_defaults(path).map_err(|e| anyhow!(e))?;
        let jxl_oxide::AuxBoxData::Data(exif) =
            image.aux_boxes().first_exif().map_err(|e| anyhow!(e))?
        else {
            return Ok(None);
        };
        // payload の先頭 tiff_header_offset バイトは TIFF ヘッダの前置きなので捨てる
        Ok(exif
            .payload()
            .get(exif.tiff_header_offset() as usize..)
            .map(<[u8]>::to_vec))
    })
    .ok()
    .flatten()
}

/// JPEG XL / AVIF のデコーダを panic から隔離する
///
/// 外部デコーダは壊れた入力で panic しうる（例: avif-parse は途中で切れたファイルで
/// `debug_assert!` に落ちる）。core が panic すると GUI ごと落ちるので、1 枚の失敗として
/// 扱えるよう `Err` に変える。`metadata::write_metadata` と同じ方針。
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
        // major が mif1 でも互換ブランドに avif があれば AVIF
        assert_eq!(
            sniff_bytes(b"\0\0\0\x18ftypmif1\0\0\0\0miafavif"),
            Format::Avif
        );
        // HEIC は AVIF ではない
        assert_eq!(
            sniff_bytes(b"\0\0\0\x18ftypheic\0\0\0\0mif1heic"),
            Format::Other
        );
        // size 0（ファイル末尾まで）の ftyp でも互換ブランドを見る
        assert_eq!(
            sniff_bytes(b"\0\0\0\0ftypmif1\0\0\0\0miafavif"),
            Format::Avif
        );
        assert_eq!(sniff_bytes(&[0xFF, 0xD8, 0xFF]), Format::Other);
        assert_eq!(sniff_bytes(&[]), Format::Other);
    }
}
