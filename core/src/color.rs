//! 入力の ICC を sRGB へ変換する（spec 2026-10-20「5」）。JPEG / PNG / WebP の経路だけが通る（`decode`）。
//!
//! - ICC が無い・sRGB である → 何もしない
//! - ICC が読めない・画素形式と合わない・変換を作れない → 何もせず（sRGB とみなし）`warnings` に積む。
//!   1 枚の変換は失敗させない（spec「付随処理の失敗」）
//! - 変換は**1 行ずつ**行い、画像 1 枚分の作業バッファを確保しない（メモリ予算を増やして数えずに済む）
//!
//! 出力に埋め込む sRGB の ICC も、ここで moxcms に 1 度だけ作らせる（[`srgb_icc`]）。

use std::path::Path;
use std::sync::OnceLock;

use image::DynamicImage;
use moxcms::{
    ColorProfile, DataColorSpace, Layout, RenderingIntent, TransformExecutor, TransformOptions,
};

/// 出力に埋め込む sRGB の ICC（moxcms の `new_srgb()` を符号化したもの）
pub(crate) fn srgb_icc() -> &'static [u8] {
    static ICC: OnceLock<Vec<u8>> = OnceLock::new();
    ICC.get_or_init(|| {
        ColorProfile::new_srgb()
            .encode()
            .expect("moxcms encodes its own sRGB profile")
    })
}

/// sRGB とみなせるプロファイルか
///
/// RGB のマトリクス型で、原色（`rXYZ`/`gXYZ`/`bXYZ`）と白色点が `new_srgb()` と各成分 0.002 以内なら sRGB とする。
/// moxcms の `ColorProfile` は `PartialEq` を持たず、説明文字列は言語やベンダーで揺れるため、中身で比べる。
/// **TRC は比べない**: 原色が sRGB で TRC だけが違うもの（Rec.709 の ICC など）は変換しない。既知の制限
pub(crate) fn is_srgb(profile: &ColorProfile) -> bool {
    const TOLERANCE: f64 = 0.002;
    let srgb = ColorProfile::new_srgb();
    let close = |a: moxcms::Xyzd, b: moxcms::Xyzd| {
        (a.x - b.x).abs() <= TOLERANCE
            && (a.y - b.y).abs() <= TOLERANCE
            && (a.z - b.z).abs() <= TOLERANCE
    };
    profile.color_space == DataColorSpace::Rgb
        && profile.is_matrix_shaper()
        && close(profile.red_colorant, srgb.red_colorant)
        && close(profile.green_colorant, srgb.green_colorant)
        && close(profile.blue_colorant, srgb.blue_colorant)
        && close(profile.white_point, srgb.white_point)
}

/// `icc` が sRGB 以外なら、画素を sRGB に変換する。画素形式（8bit / 16bit、アルファ）は保つ
///
/// 変換できないときは画素をそのまま返し、警告を `warnings` に積む（文言は計画の「警告の文言」）。
pub(crate) fn to_srgb(
    mut img: DynamicImage,
    icc: Option<&[u8]>,
    path: &Path,
    warnings: &mut Vec<String>,
) -> DynamicImage {
    let Some(icc) = icc else {
        return img;
    };
    let profile = match no_panic(|| ColorProfile::new_from_slice(icc).map_err(|e| e.to_string())) {
        Ok(p) => p,
        Err(e) => {
            warnings.push(format!(
                "Ignoring unreadable ICC profile in {}: {e}",
                path.display()
            ));
            return img;
        }
    };
    if is_srgb(&profile) {
        return img;
    }
    if let Err(e) = convert_in_place(&mut img, &profile) {
        warnings.push(format!(
            "Ignoring ICC profile that cannot be converted to sRGB in {}: {e}",
            path.display()
        ));
    }
    img
}

/// 変換する画素のサンプル列
enum Samples<'a> {
    U8(&'a mut [u8]),
    U16(&'a mut [u16]),
}

/// `img` を `profile` から sRGB へその場で変換する。`Err` のとき画素は変わっていない
fn convert_in_place(img: &mut DynamicImage, profile: &ColorProfile) -> Result<(), String> {
    use DynamicImage as D;
    let color = img.color();
    let width = img.width() as usize;
    let (layout, space, samples) = match img {
        D::ImageRgb8(b) => (Layout::Rgb, DataColorSpace::Rgb, Samples::U8(b.as_mut())),
        D::ImageRgba8(b) => (Layout::Rgba, DataColorSpace::Rgb, Samples::U8(b.as_mut())),
        D::ImageRgb16(b) => (Layout::Rgb, DataColorSpace::Rgb, Samples::U16(b.as_mut())),
        D::ImageRgba16(b) => (Layout::Rgba, DataColorSpace::Rgb, Samples::U16(b.as_mut())),
        D::ImageLuma8(b) => (Layout::Gray, DataColorSpace::Gray, Samples::U8(b.as_mut())),
        D::ImageLumaA8(b) => (
            Layout::GrayAlpha,
            DataColorSpace::Gray,
            Samples::U8(b.as_mut()),
        ),
        D::ImageLuma16(b) => (Layout::Gray, DataColorSpace::Gray, Samples::U16(b.as_mut())),
        D::ImageLumaA16(b) => (
            Layout::GrayAlpha,
            DataColorSpace::Gray,
            Samples::U16(b.as_mut()),
        ),
        _ => return Err(format!("{color:?} pixels are not supported")),
    };
    if profile.color_space != space {
        return Err(format!(
            "{:?} profile does not match {color:?} pixels",
            profile.color_space
        ));
    }
    let dst = match space {
        DataColorSpace::Gray => srgb_gray()?,
        _ => ColorProfile::new_srgb(),
    };
    let options = TransformOptions {
        rendering_intent: RenderingIntent::RelativeColorimetric,
        ..Default::default()
    };
    let row_len = width * layout.channels();
    let err = |e: moxcms::CmsError| e.to_string();
    // 行の変換（`transform_rows`）は包まない: 途中で panic すると一部の行だけ変換された画素が残るので、
    // そこは呼び出し元の `decode::guarded` で 1 枚の `Err` にする
    match samples {
        Samples::U8(s) => {
            let t = no_panic(|| {
                profile
                    .create_transform_8bit(layout, &dst, layout, options)
                    .map_err(err)
            })?;
            transform_rows(s, row_len, t.as_ref())
        }
        Samples::U16(s) => {
            let t = no_panic(|| {
                profile
                    .create_transform_16bit(layout, &dst, layout, options)
                    .map_err(err)
            })?;
            transform_rows(s, row_len, t.as_ref())
        }
    }
}

/// 信頼できない ICC を扱う moxcms の処理（解析と変換の作成）の panic を `Err` に変える
///
/// moxcms の解析は信頼できない入力に対する新しい攻撃面であり、panic でバッチを止めず、
/// ICC の失敗として警告にして画素を変換せずに続けるため（spec 2026-10-20「付随処理の失敗」）。
fn no_panic<T>(f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f))
        .unwrap_or_else(|_| Err("moxcms panicked on the ICC profile".to_string()))
}

/// グレーの変換先: sRGB の白色点と TRC を持つ Gray のプロファイル
fn srgb_gray() -> Result<ColorProfile, String> {
    let mut gray = ColorProfile::new_srgb();
    gray.gray_trc = Some(
        gray.red_trc
            .clone()
            .ok_or("moxcms sRGB profile has no TRC")?,
    );
    gray.color_space = DataColorSpace::Gray;
    Ok(gray)
}

/// 1 行ずつ作業バッファへ変換して書き戻す
///
/// moxcms の `transform` が `Err` を返すのはスライスの長さが合わないときで、行の長さはどの行も同じなので、
/// 失敗するなら最初の行で失敗する（書き戻す前なので画素は変わらない）。
fn transform_rows<T: Copy + Default>(
    samples: &mut [T],
    row_len: usize,
    transform: &(dyn TransformExecutor<T> + Send + Sync),
) -> Result<(), String> {
    if row_len == 0 {
        return Ok(());
    }
    let mut buf = vec![T::default(); row_len];
    for row in samples.chunks_exact_mut(row_len) {
        transform
            .transform(row, &mut buf)
            .map_err(|e| e.to_string())?;
        row.copy_from_slice(&buf);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageDecoder as _, ImageReader};

    /// フィクスチャ（tests/fixtures/icc/。作り方は make_output_quality.py）に埋め込まれた ICC
    fn fixture_icc(name: &str) -> Vec<u8> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/icc")
            .join(name);
        let mut decoder = ImageReader::open(path)
            .unwrap()
            .with_guessed_format()
            .unwrap()
            .into_decoder()
            .unwrap();
        decoder
            .icc_profile()
            .unwrap()
            .expect("フィクスチャに ICC が無い")
    }

    fn parse(icc: &[u8]) -> ColorProfile {
        ColorProfile::new_from_slice(icc).unwrap()
    }

    #[test]
    fn lightroom_srgb_profile_is_recognized_as_srgb() {
        // LrC が埋め込む "sRGB IEC61966-2.1"（サンプル写真から取り出したもの）
        assert!(is_srgb(&parse(&fixture_icc("srgb_lrc.jpg"))));
    }

    #[test]
    fn adobe_and_prophoto_are_not_srgb() {
        assert!(!is_srgb(&parse(&fixture_icc("adobe.png"))));
        assert!(!is_srgb(&parse(&fixture_icc("prophoto.png"))));
    }

    #[test]
    fn embedded_srgb_profile_parses_as_srgb() {
        assert!(is_srgb(&parse(srgb_icc())));
    }

    #[test]
    fn gray_profile_on_gray_pixels_is_converted() {
        // TRC がガンマ 1.0（線形）の Gray の ICC。画素 128 は線形 0.502 で、sRGB の符号化では 188
        let icc = ColorProfile::new_gray_with_gamma(1.0).encode().unwrap();
        let img = DynamicImage::ImageLuma8(image::GrayImage::from_pixel(4, 2, image::Luma([128])));
        let mut warnings = Vec::new();
        let out = to_srgb(img, Some(&icc), Path::new("gray.png"), &mut warnings);
        assert!(warnings.is_empty(), "{warnings:?}");
        let DynamicImage::ImageLuma8(out) = out else {
            panic!("グレーのまま返っていない: {:?}", out.color());
        };
        let v = out.get_pixel(1, 1).0[0];
        assert!(v.abs_diff(188) <= 2, "{v}");
    }
}
