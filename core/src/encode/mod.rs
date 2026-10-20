//! 出力 JPEG のエンコード（spec 2026-10-20「エンコーダ」）。
//!
//! 本番の出力もサムネイル・プレビューも [`encode_jpeg`] を通す。違いは [`Embed`] だけで、
//! サムネイル・プレビューは webview に表示するだけなので ICC も Exif も埋め込まない（[`Embed::NONE`]）。
//!
//! `jpeg-encoder` の `Encoder::new` の既定（品質 90 未満で 4:2:0・色差の間引きは左上 1 画素の
//! `Nearest`・Annex K のテーブル・標準ハフマン表）には頼らず、毎回すべて明示する。

mod quant;

use anyhow::{Context, Result, anyhow};
use jpeg_encoder::{ChromaSubsamplingMethod, ColorType, Encoder, QuantizationTableType};

/// JPEG に埋め込むもの
pub(crate) struct Embed<'a> {
    /// ICC プロファイル本体（APP2 に分割して書く）
    pub icc: Option<&'a [u8]>,
    /// Exif の TIFF 本体（`"Exif\0\0"` は含まない）
    pub exif: Option<&'a [u8]>,
}

impl Embed<'static> {
    /// 何も埋め込まない（サムネイル・プレビュー）
    pub(crate) const NONE: Embed<'static> = Embed {
        icc: None,
        exif: None,
    };
}

/// RGB の画素を品質 `quality`（1..=100）の JPEG にする
pub(crate) fn encode_jpeg(rgb: &image::RgbImage, quality: u8, embed: &Embed) -> Result<Vec<u8>> {
    let (width, height) = rgb.dimensions();
    let too_large = || anyhow!("Image of {width}x{height} is too large to encode as JPEG");
    let w = u16::try_from(width).map_err(|_| too_large())?;
    let h = u16::try_from(height).map_err(|_| too_large())?;

    let tables = quant::quant_tables(quality);
    let mut bytes = Vec::new();
    let mut encoder = Encoder::new(&mut bytes, quality);
    encoder.set_quantization_tables(
        QuantizationTableType::Custom(Box::new(tables.luma)),
        QuantizationTableType::Custom(Box::new(tables.chroma)),
    );
    encoder.set_sampling_factor(quant::sampling(quality));
    encoder.set_chroma_subsampling_method(ChromaSubsamplingMethod::Average);
    encoder.set_optimized_huffman_tables(true);
    if let Some(icc) = embed.icc {
        encoder
            .add_icc_profile(icc)
            .context("Failed to embed ICC profile")?;
    }
    if let Some(exif) = embed.exif {
        encoder
            .add_exif_metadata(exif)
            .context("Failed to embed EXIF")?;
    }
    encoder
        .encode(rgb.as_raw(), w, h, ColorType::Rgb)
        .context("Failed to encode JPEG")?;
    Ok(bytes)
}

/// サイズ制限を満たす JPEG バイト列
pub(crate) struct EncodedJpeg {
    pub bytes: Vec<u8>,
    /// 実際に採用した品質
    pub quality: u8,
    /// `max_bytes` を満たせたか（false なら下限品質でも超過している）
    pub within_limit: bool,
}

/// サイズ制限を満たすまで品質を下げながらメモリ上で JPEG にエンコードする
///
/// ディスクへの書き出しは確定した1回分だけを呼び出し元が行う。
/// 試行のたびに書いて消す方式は最大7往復の I/O を生み、一時ファイル名の衝突源でもあった。
pub(crate) fn encode_within_size_limit(
    rgb: &image::RgbImage,
    initial: u8,
    max_bytes: usize,
    embed: &Embed,
) -> Result<EncodedJpeg> {
    const MIN_QUALITY: u8 = 60;
    const QUALITY_STEP: u8 = 5;

    let mut quality = initial;

    loop {
        let bytes = encode_jpeg(rgb, quality, embed)?;

        if bytes.len() <= max_bytes {
            return Ok(EncodedJpeg {
                bytes,
                quality,
                within_limit: true,
            });
        }
        if quality <= MIN_QUALITY {
            return Ok(EncodedJpeg {
                bytes,
                quality,
                within_limit: false,
            });
        }

        quality = quality.saturating_sub(QUALITY_STEP).max(MIN_QUALITY);
    }
}
