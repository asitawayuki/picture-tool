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

/// 品質の下限。Task 3 で決めた値（これより低いとブロック歪みが目立つ）
pub(crate) const MIN_QUALITY: u8 = 28;

/// サイズ制限を満たす最大の品質で、メモリ上で JPEG にエンコードする
///
/// ディスクへの書き出しは確定した1回分だけを呼び出し元が行う。
/// 試行のたびに書いて消す方式は往復の I/O を生み、一時ファイル名の衝突源でもあった。
pub(crate) fn encode_within_size_limit(
    rgb: &image::RgbImage,
    initial: u8,
    max_bytes: usize,
    embed: &Embed,
) -> Result<EncodedJpeg> {
    search_quality(initial, MIN_QUALITY, max_bytes, |q| {
        encode_jpeg(rgb, q, embed)
    })
}

/// `max_bytes` 以下に収まる最大の品質を探す
///
/// 初期品質を 1 回試し、超えたら `[floor, initial - 1]` を二分探索する。
/// エンコード回数は最悪でも `1 + ceil(log2(initial - floor + 1))`（収まる品質が
/// `initial - floor` 通りと「どれも収まらない」の計 `initial - floor + 1` 通りを区別するため）。
/// 初期品質が下限以下のときは初期品質を 1 回だけ試す。
///
/// サイズが品質に対して単調でないと「最大」とは限らないが、
/// `within_limit == true` の結果が `max_bytes` 以下であることは常に守る。
/// どれも収まらなければ下限の結果を `within_limit == false` で返す。
pub(crate) fn search_quality(
    initial: u8,
    floor: u8,
    max_bytes: usize,
    mut encode: impl FnMut(u8) -> Result<Vec<u8>>,
) -> Result<EncodedJpeg> {
    let bytes = encode(initial)?;
    if bytes.len() <= max_bytes {
        return Ok(EncodedJpeg {
            bytes,
            quality: initial,
            within_limit: true,
        });
    }
    if initial <= floor {
        return Ok(EncodedJpeg {
            bytes,
            quality: initial,
            within_limit: false,
        });
    }

    let mut best: Option<(u8, Vec<u8>)> = None;
    // どれも収まらないとき、最後に試すのは必ず下限（hi が lo を下回るのは mid == lo を試した後）
    let mut floor_bytes = Vec::new();
    let (mut lo, mut hi) = (floor, initial - 1);
    while lo <= hi {
        let mid = lo + (hi - lo) / 2;
        let bytes = encode(mid)?;
        if bytes.len() <= max_bytes {
            best = Some((mid, bytes));
            lo = mid + 1;
        } else {
            if mid == floor {
                floor_bytes = bytes;
            }
            hi = mid - 1;
        }
    }

    Ok(match best {
        Some((quality, bytes)) => EncodedJpeg {
            bytes,
            quality,
            within_limit: true,
        },
        None => EncodedJpeg {
            bytes: floor_bytes,
            quality: floor,
            within_limit: false,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 品質 q で `size(q)` バイトを返す偽のエンコーダで探索し、呼ばれた品質も返す
    fn run(
        initial: u8,
        floor: u8,
        max: usize,
        size: impl Fn(u8) -> usize,
    ) -> (EncodedJpeg, Vec<u8>) {
        let mut calls = Vec::new();
        let r = search_quality(initial, floor, max, |q| {
            calls.push(q);
            Ok(vec![0u8; size(q)])
        })
        .unwrap();
        (r, calls)
    }

    fn linear(q: u8) -> usize {
        q as usize * 1000
    }

    #[test]
    fn initial_quality_that_fits_is_used_with_one_encode() {
        let (r, calls) = run(90, 20, 90_000, linear);
        assert_eq!((r.quality, r.within_limit), (90, true));
        assert_eq!(calls, vec![90]);
    }

    #[test]
    fn picks_the_highest_quality_that_fits() {
        let (r, _) = run(90, 20, 73_500, linear);
        assert_eq!((r.quality, r.within_limit), (73, true));
        assert_eq!(r.bytes.len(), 73_000);
    }

    #[test]
    fn encode_count_is_logarithmic() {
        let (_, calls) = run(90, 20, 73_500, linear);
        assert!(calls.len() <= 8, "{} 回: {calls:?}", calls.len());
    }

    #[test]
    fn encode_count_bound_holds_when_gap_is_a_power_of_two() {
        // initial 90, floor 26: 差 64。結果は 64 通り + 「収まらない」= 65 通り → 1 + ceil(log2 65) = 8
        for max in [1, 26_000, 27_000, 89_000] {
            let (r, calls) = run(90, 26, max, linear);
            assert!(calls.len() <= 8, "max {max}: {} 回: {calls:?}", calls.len());
            if max == 1 {
                assert_eq!((r.quality, r.within_limit), (26, false));
            }
            if max == 27_000 {
                assert_eq!((r.quality, r.within_limit), (27, true));
            }
        }
    }

    #[test]
    fn floor_result_is_written_when_nothing_fits() {
        let (r, _) = run(90, 20, 1, linear);
        assert_eq!((r.quality, r.within_limit), (20, false));
        assert_eq!(r.bytes.len(), 20_000);
    }

    #[test]
    fn initial_below_floor_encodes_once() {
        let (r, calls) = run(10, 20, 1, linear);
        assert_eq!((r.quality, r.within_limit), (10, false));
        assert_eq!(calls, vec![10]);
    }

    #[test]
    fn result_fits_even_if_sizes_are_not_monotonic() {
        // q=50 だけ巨大。どの上限でも within のときは上限以下
        let size = |q: u8| if q == 50 { 1_000_000 } else { linear(q) };
        for max in [1, 20_000, 49_500, 60_000, 73_500, 89_000] {
            let (r, _) = run(90, 20, max, size);
            if r.within_limit {
                assert!(r.bytes.len() <= max, "max {max}");
            }
        }
    }
}
