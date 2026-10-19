// avif-decode 3.0.0（https://github.com/kornelski/avif-decode）からの移植。原著作権表示とライセンス:
//
// Copyright 2021 Kornel Lesiński
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

//! libdav1d がデコードした YUV の [`Picture`]（主画像と、あればアルファ）を [`DynamicImage`] にする
//!
//! **差し替え前と同じ画素を出すための移植。** 移植元は avif-decode 3.0.0 の `src/lib.rs` の
//! `to_image`、`src/image.rs`（変換・アルファ・プリマルチプライドの解除）、`src/rav1d.rs` の
//! `single_plane_img`・`matrix_coefficients`・`range`。Ok/Err が分かれる条件と画素の計算式は
//! 移植元に合わせてあり、変えると差し替え前の出力と一致しなくなる（照合は `jxl_avif_input` の画素一致）。
//! 変換本体は移植元と同じ `yuv` クレート（SIMD なしのスカラー経路。理由は core/Cargo.toml）。
//!
//! **`Picture` から読んでよいのは、寸法・`pixel_layout`・`bits_per_component`・
//! `matrix_coefficients`・`color_range`・`stride`・`plane` だけ。** dav1d 0.11.1 の
//! `color_primaries()`・`transfer_characteristic()`・`chroma_location()` は予約値で `unreachable!()`
//! に落ちる。移植元はこれらを読まないので、読むと差し替え前に開けたファイルが Err になる。

use anyhow::{Context, Result, anyhow, bail, ensure};
use dav1d::pixel::{MatrixCoefficients as Mc, YUVRange};
use dav1d::{BitsPerComponent, Picture, PixelLayout, PlanarImageComponent as Component};
use image::{DynamicImage, ImageBuffer, Luma, Pixel, Rgba};
use std::borrow::Cow;
use yuv::{YuvPlanarImage, YuvRange, YuvStandardMatrix};

/// 主画像（とアルファ）を出力の画素形式に変換する
///
/// 出力形式は主画像の深度で決まる（8bit: `Rgb8`/`Rgba8`/`Luma8`、10/12bit: 同じく 16bit 版）。
/// グレー＋アルファは `Rgba` になる。
pub(super) fn to_image(
    color: &Picture,
    alpha: Option<&Picture>,
    premultiplied: bool,
) -> Result<DynamicImage> {
    let has_alpha = alpha.is_some();
    let depth = Depth::of(color)?;
    let layout = color.pixel_layout();
    let gray = layout == PixelLayout::I400;
    let (width, height) = (color.width(), color.height());

    // デコード前の判定は宣言寸法によるもの。実際のフレームで出力の大きさを確かめ直す
    let channels: u64 = match (has_alpha, gray) {
        (true, _) => 4,
        (false, true) => 1,
        (false, false) => 3,
    };
    let bytes_per_sample: u64 = if depth == Depth::Eight { 1 } else { 2 };
    image::Limits::default()
        .reserve(u64::from(width) * u64::from(height) * channels * bytes_per_sample)
        .context("decoded AVIF exceeds the 512MB output limit")?;

    let range = color.color_range();
    let mut img = if gray {
        let y = PlaneData::new(color, Component::Y, depth)?;
        match depth {
            Depth::Eight => gray_image(y.u8(), &y.geom, depth, range, has_alpha),
            _ => gray_image(&y.u16(), &y.geom, depth, range, has_alpha),
        }
    } else {
        let conv = conversion(matrix(color).unwrap_or(Matrix::Bt601))?;
        let yuv_range = match range {
            YUVRange::Full => YuvRange::Full,
            YUVRange::Limited => YuvRange::Limited,
        };
        let [y, u, v] =
            [Component::Y, Component::U, Component::V].map(|c| PlaneData::new(color, c, depth));
        let (y, u, v) = (y?, u?, v?);
        if depth == Depth::Eight {
            let planar = planar(y.u8(), u.u8(), v.u8(), &y.geom, &u.geom, &v.geom);
            let convert = |dst: &mut [u8], stride, alpha| {
                convert8(&planar, dst, stride, layout, conv, yuv_range, alpha)
            };
            if has_alpha {
                DynamicImage::ImageRgba8(filled(width, height, |d, s| convert(d, s, true))?)
            } else {
                DynamicImage::ImageRgb8(filled(width, height, |d, s| convert(d, s, false))?)
            }
        } else {
            let (y16, u16_, v16) = (y.u16(), u.u16(), v.u16());
            let planar = planar(&y16, &u16_, &v16, &y.geom, &u.geom, &v.geom);
            let convert = |dst: &mut [u16], stride, alpha| {
                convert16(&planar, dst, stride, layout, depth, conv, yuv_range, alpha)
            };
            if has_alpha {
                DynamicImage::ImageRgba16(filled(width, height, |d, s| convert(d, s, true))?)
            } else {
                DynamicImage::ImageRgb16(filled(width, height, |d, s| convert(d, s, false))?)
            }
        }
    };

    if let Some(alpha) = alpha {
        // 主画像の「なし」は Identity とみなす。アルファの「なし」（未指定を含む）は検査しない
        let color_matrix = matrix(color).unwrap_or(Matrix::Identity);
        if let Some(alpha_matrix) = matrix(alpha).filter(|&m| m != Matrix::Identity)
            && alpha_matrix != color_matrix
        {
            bail!("alpha image has color info");
        }
        // アルファは Y 平面だけを使い、レイアウトは問わない。値はアルファ自身の深度と範囲で変換する
        let alpha_depth = Depth::of(alpha)?;
        let y = PlaneData::new(alpha, Component::Y, alpha_depth)?;
        let range = alpha.color_range();
        match alpha_depth {
            Depth::Eight => {
                write_alpha(&mut img, y.u8(), &y.geom, alpha_depth, range, premultiplied)?
            }
            _ => write_alpha(
                &mut img,
                &y.u16(),
                &y.geom,
                alpha_depth,
                range,
                premultiplied,
            )?,
        }
    }
    Ok(img)
}

/// 1 サンプルの深度
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Depth {
    Eight,
    Ten,
    Twelve,
}

impl Depth {
    fn of(p: &Picture) -> Result<Self> {
        Ok(match p.bits_per_component() {
            Some(BitsPerComponent(8)) => Self::Eight,
            Some(BitsPerComponent(10)) => Self::Ten,
            Some(BitsPerComponent(12)) => Self::Twelve,
            other => bail!("unsupported AV1 bit depth: {other:?}"),
        })
    }

    fn bits(self) -> u32 {
        match self {
            Self::Eight => 8,
            Self::Ten => 10,
            Self::Twelve => 12,
        }
    }
}

/// 平面の幾何（サンプル単位）
#[derive(Debug, Clone, Copy)]
struct Geometry {
    width: usize,
    height: usize,
    /// サンプル単位の行の間隔
    stride: usize,
}

/// 1 つの平面と、その幾何
struct PlaneData {
    plane: dav1d::Plane,
    geom: Geometry,
}

impl PlaneData {
    /// 移植元 `single_plane_img` の検査（stride が正・サンプル幅の倍数・平面の幅以上）を通して取り出す
    fn new(p: &Picture, component: Component, depth: Depth) -> Result<Self> {
        // クロマの平面はレイアウトに応じて間引かれている
        let (ss_x, ss_y) = match (component, p.pixel_layout()) {
            (Component::Y, _) | (_, PixelLayout::I444) => (0, 0),
            (_, PixelLayout::I420) => (1, 1),
            (_, PixelLayout::I422) => (1, 0),
            (_, PixelLayout::I400) => bail!("AV1 picture is missing a chroma plane"),
        };
        let width = (p.width() as usize + ss_x) >> ss_x;
        let height = (p.height() as usize + ss_y) >> ss_y;

        let sample_bytes = if depth == Depth::Eight { 1 } else { 2 };
        // dav1d クレートは符号付きの stride を `u32` に切り詰めて返すので、負の値は i32 で戻して弾く
        let stride_bytes = i32::try_from(p.stride(component))
            .ok()
            .filter(|&s| s > 0)
            .ok_or_else(|| anyhow!("invalid AV1 plane stride"))?
            as usize;
        ensure!(
            stride_bytes.is_multiple_of(sample_bytes),
            "invalid AV1 plane stride"
        );
        let stride = stride_bytes / sample_bytes;
        ensure!(stride >= width && width != 0, "invalid AV1 plane stride");

        let plane = p.plane(component);
        ensure!(
            plane.len() >= stride_bytes * height,
            "AV1 plane is shorter than its geometry"
        );
        Ok(Self {
            plane,
            geom: Geometry {
                width,
                height,
                stride,
            },
        })
    }

    /// 8bit の平面
    fn u8(&self) -> &[u8] {
        &self.plane
    }

    /// 10/12bit の平面。整列していなければ複製する（移植元 `imgref_align_to_u16`）
    fn u16(&self) -> Cow<'_, [u16]> {
        match bytemuck::try_cast_slice(&self.plane) {
            Ok(samples) => Cow::Borrowed(samples),
            Err(_) => Cow::Owned(
                self.plane
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|&b| u16::from_ne_bytes(b))
                    .collect(),
            ),
        }
    }
}

fn planar<'a, T: Copy + std::fmt::Debug>(
    y: &'a [T],
    u: &'a [T],
    v: &'a [T],
    yd: &Geometry,
    ud: &Geometry,
    vd: &Geometry,
) -> YuvPlanarImage<'a, T> {
    // 寸法は `Picture` の u32 から作ったもので、stride は平面の長さ以下なので u32 に収まる
    YuvPlanarImage {
        y_plane: y,
        y_stride: yd.stride as u32,
        u_plane: u,
        u_stride: ud.stride as u32,
        v_plane: v,
        v_stride: vd.stride as u32,
        width: yd.width as u32,
        height: yd.height as u32,
    }
}

/// 出力の画素バッファを作り、その中身へ直接書かせる（`fill` の第 2 引数は行のサンプル数）
fn filled<P: Pixel>(
    width: u32,
    height: u32,
    fill: impl FnOnce(&mut [P::Subpixel], u32) -> Result<()>,
) -> Result<ImageBuffer<P, Vec<P::Subpixel>>> {
    let mut buf = ImageBuffer::<P, Vec<P::Subpixel>>::new(width, height);
    fill(&mut buf, width * u32::from(P::CHANNEL_COUNT))?;
    Ok(buf)
}

/// 行列係数の種別（H.273 の値の意味で区別する）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Matrix {
    Identity,
    Bt709,
    Fcc,
    Bt601,
    Smpte240,
    YCgCo,
    Bt2020Ncl,
    Smpte2085,
    ChromatNcl,
    ChromatCl,
    ICtCp,
}

/// 移植元 `matrix_coefficients`。未指定は `None`
///
/// dav1d クレートは値 4 を `BT470M`、値 5・6 をどちらも `BT470BG` という名前で返すので、名前ではなく
/// 値の意味で対応づける（5 と 6 は同じ BT.601 の行列）。BT.2020 の CL は移植元と同じく NCL として扱う。
/// 予約値 15 以上は dav1d クレートが `Unspecified` にし、予約値 3 はアクセサ自体が panic する
fn matrix(p: &Picture) -> Option<Matrix> {
    Some(match p.matrix_coefficients() {
        Mc::Unspecified | Mc::Reserved => return None,
        Mc::Identity => Matrix::Identity,
        Mc::BT709 => Matrix::Bt709,
        Mc::BT470M => Matrix::Fcc,
        Mc::BT470BG | Mc::ST170M => Matrix::Bt601,
        Mc::ST240M => Matrix::Smpte240,
        Mc::YCgCo => Matrix::YCgCo,
        Mc::BT2020NonConstantLuminance | Mc::BT2020ConstantLuminance => Matrix::Bt2020Ncl,
        Mc::ST2085 => Matrix::Smpte2085,
        Mc::ChromaticityDerivedNonConstantLuminance => Matrix::ChromatNcl,
        Mc::ChromaticityDerivedConstantLuminance => Matrix::ChromatCl,
        Mc::ICtCp => Matrix::ICtCp,
    })
}

/// RGB への変換の上での平面の意味
#[derive(Debug, Clone, Copy, PartialEq)]
enum Conversion {
    /// 平面が G・B・R（Identity）。クロマの間引きがあるときは使えない
    Gbr,
    YCgCo,
    Matrix(YuvStandardMatrix),
}

fn conversion(m: Matrix) -> Result<Conversion> {
    Ok(match m {
        Matrix::Identity => Conversion::Gbr,
        Matrix::YCgCo => Conversion::YCgCo,
        Matrix::Bt709 => Conversion::Matrix(YuvStandardMatrix::Bt709),
        Matrix::Fcc => Conversion::Matrix(YuvStandardMatrix::Fcc),
        // BT.470 System B,G は BT.601 と同じ行列（`yuv` の `Bt470_6` は別物）
        Matrix::Bt601 => Conversion::Matrix(YuvStandardMatrix::Bt601),
        Matrix::Smpte240 => Conversion::Matrix(YuvStandardMatrix::Smpte240),
        Matrix::Bt2020Ncl => Conversion::Matrix(YuvStandardMatrix::Bt2020),
        Matrix::Smpte2085 | Matrix::ChromatNcl | Matrix::ChromatCl | Matrix::ICtCp => {
            bail!("unsupported matrix coefficients: {m:?}")
        }
    })
}

const IDENTITY_SUBSAMPLED: &str = "identity matrix with chroma subsampling";

/// 8bit の平面を `dst`（`alpha` が偽なら RGB、真なら RGBA）へ変換する
fn convert8(
    planar: &YuvPlanarImage<u8>,
    dst: &mut [u8],
    stride: u32,
    layout: PixelLayout,
    conv: Conversion,
    range: YuvRange,
    alpha: bool,
) -> Result<()> {
    use Conversion::{Gbr, Matrix, YCgCo};
    use PixelLayout::{I400, I420, I422, I444};
    macro_rules! call {
        ($rgb:ident, $rgba:ident $(, $m:expr)?) => {
            if alpha {
                yuv::$rgba(planar, dst, stride, range $(, $m)?)
            } else {
                yuv::$rgb(planar, dst, stride, range $(, $m)?)
            }
        };
    }
    let res = match (conv, layout) {
        (Gbr, I444) => call!(gbr_to_rgb, gbr_to_rgba),
        (Gbr, I420 | I422) => bail!(IDENTITY_SUBSAMPLED),
        (YCgCo, I444) => call!(ycgco444_to_rgb, ycgco444_to_rgba),
        (YCgCo, I422) => call!(ycgco422_to_rgb, ycgco422_to_rgba),
        (YCgCo, I420) => call!(ycgco420_to_rgb, ycgco420_to_rgba),
        (Matrix(m), I444) => call!(yuv444_to_rgb, yuv444_to_rgba, m),
        (Matrix(m), I422) => call!(yuv422_to_rgb, yuv422_to_rgba, m),
        (Matrix(m), I420) => call!(yuv420_to_rgb, yuv420_to_rgba, m),
        (_, I400) => bail!("monochrome in a color conversion"),
    };
    Ok(res?)
}

/// 10/12bit の平面を `dst` へ変換し、16bit の全域に広げる
#[allow(clippy::too_many_arguments)]
fn convert16(
    planar: &YuvPlanarImage<u16>,
    dst: &mut [u16],
    stride: u32,
    layout: PixelLayout,
    depth: Depth,
    conv: Conversion,
    range: YuvRange,
    alpha: bool,
) -> Result<()> {
    use Conversion::{Gbr, Matrix, YCgCo};
    use Depth::{Eight, Ten, Twelve};
    use PixelLayout::{I400, I420, I422, I444};
    macro_rules! call {
        ($rgb:ident, $rgba:ident $(, $m:expr)?) => {
            if alpha {
                yuv::$rgba(planar, dst, stride, range $(, $m)?)
            } else {
                yuv::$rgb(planar, dst, stride, range $(, $m)?)
            }
        };
    }
    let res = match (conv, layout, depth) {
        (Gbr, I444, Ten) => call!(gb10_to_rgb10, gb10_to_rgba10),
        (Gbr, I444, Twelve) => call!(gb12_to_rgb12, gb12_to_rgba12),
        (Gbr, I420 | I422, _) => bail!(IDENTITY_SUBSAMPLED),
        (YCgCo, I444, Ten) => call!(icgc410_to_rgb10, icgc410_to_rgba10),
        (YCgCo, I422, Ten) => call!(icgc210_to_rgb10, icgc210_to_rgba10),
        (YCgCo, I420, Ten) => call!(icgc010_to_rgb10, icgc010_to_rgba10),
        (YCgCo, I444, Twelve) => call!(icgc412_to_rgb12, icgc412_to_rgba12),
        (YCgCo, I422, Twelve) => call!(icgc212_to_rgb12, icgc212_to_rgba12),
        (YCgCo, I420, Twelve) => call!(icgc012_to_rgb12, icgc012_to_rgba12),
        (Matrix(m), I444, Ten) => call!(i410_to_rgb10, i410_to_rgba10, m),
        (Matrix(m), I444, Twelve) => call!(i412_to_rgb12, i412_to_rgba12, m),
        (Matrix(m), I422, Ten) => call!(i210_to_rgb10, i210_to_rgba10, m),
        (Matrix(m), I422, Twelve) => call!(i212_to_rgb12, i212_to_rgba12, m),
        (Matrix(m), I420, Ten) => call!(i010_to_rgb10, i010_to_rgba10, m),
        (Matrix(m), I420, Twelve) => call!(i012_to_rgb12, i012_to_rgba12, m),
        (_, I400, _) => bail!("monochrome in a color conversion"),
        (_, _, Eight) => bail!("8-bit planes in a 16-bit conversion"),
    };
    res?;

    // 変換は元の深度の値を出すが、出力は 16bit の全域を前提にするのでビット複製で広げる
    let shift = 16 - depth.bits();
    let low_shift = depth.bits() - shift;
    for v in dst.iter_mut() {
        *v = (*v << shift) | (*v >> low_shift);
    }
    Ok(())
}

/// 輝度のサンプルを 16bit の全域に広げる（ビット複製ではない）。8bit の出力はこの上位 8bit
fn luma16(v: u16, depth: Depth, range: YUVRange) -> u16 {
    let bits = depth.bits();
    let (min, span) = match range {
        YUVRange::Full => (0, (1 << bits) - 1),
        YUVRange::Limited => (16 << (bits - 8), 219 << (bits - 8)),
    };
    let v = u32::from(v).saturating_sub(min).min(span);
    ((v * 65535 + span / 2) / span) as u16
}

/// グレーの主画像。8bit は移植元の `luma8`（= `luma16(.., 8bit, ..) >> 8`）、10/12bit は `luma16`。
/// アルファ付きはアルファを 0 の仮置きにした `Rgba`
fn gray_image<T: Copy + Into<u16>>(
    samples: &[T],
    y: &Geometry,
    depth: Depth,
    range: YUVRange,
    has_alpha: bool,
) -> DynamicImage {
    let (w, h) = (y.width as u32, y.height as u32);
    let g = |px: u32, py: u32| {
        luma16(
            samples[py as usize * y.stride + px as usize].into(),
            depth,
            range,
        )
    };
    let g8 = |px, py| (g(px, py) >> 8) as u8;
    match (depth == Depth::Eight, has_alpha) {
        (true, false) => {
            DynamicImage::ImageLuma8(ImageBuffer::from_fn(w, h, |x, y| Luma([g8(x, y)])))
        }
        (true, true) => DynamicImage::ImageRgba8(ImageBuffer::from_fn(w, h, |x, y| {
            let v = g8(x, y);
            Rgba([v, v, v, 0])
        })),
        (false, false) => {
            DynamicImage::ImageLuma16(ImageBuffer::from_fn(w, h, |x, y| Luma([g(x, y)])))
        }
        (false, true) => DynamicImage::ImageRgba16(ImageBuffer::from_fn(w, h, |x, y| {
            let v = g(x, y);
            Rgba([v, v, v, 0])
        })),
    }
}

/// アルファの Y 平面を `img` のアルファに書く（移植元 `write_alpha8` / `write_alpha16`）
///
/// **ここは意図して移植元と違う。** アルファは主画像と同じ寸法でなければならず、幅と高さの
/// どちらが違っても Err にする（spec「アルファの寸法」）。移植元は行を `zip` で突き合わせて
/// 幅だけを比べ、高さが違うものは短いほうの行数だけ書いて Ok にしていた。正常なファイルは
/// 寸法が等しいので、出力の画素はこの違いの影響を受けない。
fn write_alpha<T: Copy + Into<u16>>(
    img: &mut DynamicImage,
    samples: &[T],
    y: &Geometry,
    depth: Depth,
    range: YUVRange,
    premultiplied: bool,
) -> Result<()> {
    ensure!(
        y.width == img.width() as usize && y.height == img.height() as usize,
        "invalid alpha size"
    );
    let rows = |row: usize| &samples[row * y.stride..][..y.width];
    match img {
        DynamicImage::ImageRgba8(buf) => {
            let row_len = buf.width() as usize * 4;
            for (row, out) in buf.chunks_exact_mut(row_len).enumerate() {
                for (px, &v) in out.as_chunks_mut::<4>().0.iter_mut().zip(rows(row)) {
                    px[3] = (luma16(v.into(), depth, range) >> 8) as u8;
                }
                if premultiplied {
                    unpremultiply8(out);
                }
            }
        }
        DynamicImage::ImageRgba16(buf) => {
            let row_len = buf.width() as usize * 4;
            for (row, out) in buf.chunks_exact_mut(row_len).enumerate() {
                for (px, &v) in out.as_chunks_mut::<4>().0.iter_mut().zip(rows(row)) {
                    px[3] = luma16(v.into(), depth, range);
                }
                if premultiplied {
                    unpremultiply16(out);
                }
            }
        }
        _ => bail!("alpha written to an image without an alpha channel"),
    }
    Ok(())
}

/// プリマルチプライドの解除。a=0 と a=最大値は触らない
fn unpremultiply8(row: &mut [u8]) {
    for px in row.as_chunks_mut::<4>().0 {
        let a = px[3];
        if a != 255 && a != 0 {
            for c in &mut px[..3] {
                *c = (u16::from(*c) * 255 / u16::from(a)).min(255) as u8;
            }
        }
    }
}

fn unpremultiply16(row: &mut [u16]) {
    for px in row.as_chunks_mut::<4>().0 {
        let a = px[3];
        if a != 0xFFFF && a != 0 {
            for c in &mut px[..3] {
                *c = (u32::from(*c) * 0xFFFF / u32::from(a)).min(0xFFFF) as u16;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2x2 の不透明な白（主画像の変換が済んだ状態）に、`w`x`h` の 8bit 全レンジのアルファを書く
    fn write(w: usize, h: usize) -> Result<DynamicImage> {
        let mut img = DynamicImage::ImageRgba8(ImageBuffer::from_pixel(2, 2, Rgba([255; 4])));
        let samples = vec![128u8; w * h];
        let geom = Geometry {
            width: w,
            height: h,
            stride: w,
        };
        write_alpha(
            &mut img,
            &samples,
            &geom,
            Depth::Eight,
            YUVRange::Full,
            false,
        )?;
        Ok(img)
    }

    #[test]
    fn alpha_of_a_different_size_is_rejected() {
        // 前提: 同じ寸法なら書ける（アルファが 128 に置き換わる）
        let ok = write(2, 2).unwrap();
        assert!(ok.to_rgba8().pixels().all(|p| p[3] == 128));
        // 移植元は行を zip して幅だけを比べていたので、高さだけ違うものを Ok にしていた
        for (w, h) in [(2, 1), (2, 3), (1, 2), (3, 2)] {
            let err = write(w, h).unwrap_err();
            assert!(
                format!("{err:#}").contains("invalid alpha size"),
                "{w}x{h}: {err:#}"
            );
        }
    }
}
