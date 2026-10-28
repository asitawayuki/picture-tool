//! 1 枚のピーク見積もり（spec 2026-10-28 §2）
//!
//! **ヘッダから読める情報（[`Header`]）だけで、その 1 枚が処理の中で使うピークを上から押さえる。**
//! 下回ると、予算の内側でも実際の合計が予算を超える。
//!
//! 見積もりは**区間ごとの最大**: ある時点で同時に生きているバッファだけを足し、区間の間で最大を取る。
//! 区間は `process_image`（変換）と `generate_full_image_base64` など（[`Work::View`]）の処理の順に並ぶ。
//!
//! JPEG / PNG / WebP は Exif の向きを読まない（向きを読むと Exif の解析が見積もりの前に入る）。
//! 代わりに縦横両方の向きで見積もり、大きいほうを取る。JPEG XL・AVIF の寸法は向き適用後。
//!
//! 係数の根拠は `docs/superpowers/plans/2026-10-28-memory-budget.md` の実施メモ「Task 3」の計測。
//! 再計測には `core/examples/jxl_alloc_probe.rs` と `core/examples/budget_batch.rs` を使う。

use crate::ConversionMode;
use crate::decode::{Format, Header};
use crate::exif_frame::layout::fit_to_4_5;

/// 見積もる処理
#[derive(Debug, Clone, Copy)]
pub(crate) enum Work {
    /// `process_image`。`exif_frame` は実際にフレームを描く条件（設定と素材があり、pad）
    Convert {
        mode: ConversionMode,
        max_width: Option<u32>,
        exif_frame: bool,
    },
    /// 開いて縮小し、JPEG にするだけ（サムネイル・ビューアー・Exif フレームのプレビューの下地・
    /// `open_image_oriented`）
    View,
}

/// jxl-oxide の作業領域（アルファなし、1 画素あたり）。正当な入力が通る最小の `max_alloc` の
/// 最大 24.50B（16bit の可逆）× 1.25 を切り上げ。深度ではなくチャンネル数で決まる（グレーも内部は 3 チャンネル）
const JXL_WORKING_PER_PIXEL: u64 = 31;
/// 同じくアルファあり。最大 32.51B（16bit RGBA の可逆）× 1.25 を切り上げ
const JXL_WORKING_PER_PIXEL_ALPHA: u64 = 41;
/// 寸法によらない作業領域（小さい画像でも要るぶん）
const JXL_WORKING_BASE: u64 = 64 << 20;

/// libdav1d が 1 アイテムのデコード中に持ちうるピクチャの枚数: 参照 8 ＋ 出力 1 ＋ グレインの複製 1。
/// 入力は信頼できないので、1 アイテムに複数のフレームを入れて参照スロットを埋める細工を想定する
const AVIF_PICTURES: u128 = 10;
/// AVIF はファイル全体を読み（`std::fs::read`）、さらに avif-parse がアイテムのデータを複製する
const AVIF_FILE_COPIES: u128 = 2;

/// JPEG / PNG / WebP のデコーダ内部（1 画素あたり）。実測の最大はアルファ付き WebP の 6.50B
/// （プログレッシブ JPEG の係数の保持が 6.13B）× 1.25 を切り上げ
const DECODER_PER_PIXEL: u128 = 9;

/// `jpeg-encoder`（最適化ハフマン）の出力キャンバス 1 画素あたり: 行バッファ 3B ＋ 係数ブロック 6B
/// （`encode_blocks` の `init_rows` と `init_block_buffers`）＝ 9B × 1.25 を切り上げ。出力のバイト列もここに含める
const ENCODER_PER_PIXEL: u128 = 12;

/// `image` の `imageops::resize` が縦方向の標本化で作る中間バッファ（`Rgba32FImage`）の 1 画素あたり。
/// 大きさは「元の幅 × 新しい高さ」
const RESIZE_PER_PIXEL: u128 = 16;

/// ビューアー（`generate_full_image_base64`）の縮小先の上限。サムネイルとプレビューの下地（どちらも 1024）は
/// これより小さいので、ここで押さえる
const VIEW_MAX: (u128, u128) = (2560, 1600);

/// JPEG XL のデコーダに渡す作業領域の上限（`max_alloc`）。見積もりの作業領域の項そのもので、
/// `decode::open` もこの値を使う（値を 2 箇所に書かない）。`max_alloc` は jxl-oxide の追跡下の確保だけを
/// 数え、出力の画素バッファは数えないので、出力は [`estimate`] の別の項にする
pub(crate) fn jxl_working_bytes(header: &Header) -> u64 {
    let per_pixel = if header.color.has_alpha() {
        JXL_WORKING_PER_PIXEL_ALPHA
    } else {
        JXL_WORKING_PER_PIXEL
    };
    (u64::from(header.width) * u64::from(header.height))
        .saturating_mul(per_pixel)
        .saturating_add(JXL_WORKING_BASE)
}

/// 1 枚のピーク見積もり（バイト）
pub(crate) fn estimate(header: &Header, work: &Work) -> u64 {
    let (w, h) = (header.width, header.height);
    let e = if header.format.exif_orientation_applies() {
        oriented(header, w, h, work).max(oriented(header, h, w, work))
    } else {
        oriented(header, w, h, work)
    };
    u64::try_from(e).unwrap_or(u64::MAX)
}

/// 向きを決めた寸法での見積もり。溢れないよう u128 で数える
fn oriented(header: &Header, w: u32, h: u32, work: &Work) -> u128 {
    let (w, h) = (u128::from(w), u128::from(h));
    let mut p = w * h;
    if let Some(avif) = header.avif {
        // デコーダが出しうる大きさは宣言（`frame_size_limit`）で縛られる
        p = p.max(u128::from(avif.primary_pixels));
    }
    let bpp = u128::from(header.color.bytes_per_pixel());
    let alpha = header.color.has_alpha();
    let o = p * bpp;

    let decode = decoding(header, p, o);
    // 向きの複製（JPEG / PNG / WebP。JPEG XL・AVIF はデコード中に適用済みで、上の区間に数えた）
    let mut after = 2 * o;
    match *work {
        Work::View => {
            // 縮小（Lanczos3）: 元画像 ＋ 中間 ＋ 縮小後。縮小しないときは `to_rgb8` が原寸を複製するが、
            // そのとき原寸は上限の内側なので同じ式で押さえられる
            let (vw, vh) = VIEW_MAX;
            let resize = o + w * h.min(vh) * RESIZE_PER_PIXEL + vw * vh * bpp.max(3);
            // エンコード: 縮小後 ＋ `to_rgb8` ＋ エンコーダ（原寸の画像はまだ生きている）
            let encode = o + vw * vh * (bpp.max(3) + 3 + ENCODER_PER_PIXEL);
            decode.max(after).max(resize).max(encode)
        }
        Work::Convert {
            mode,
            max_width,
            exif_frame,
        } => {
            if alpha {
                // `flatten_alpha`: RGBA8 への変換（元が RGBA8 でなければ複製）、続いて RGB8 への変換
                after = after.max(o + 4 * p).max(7 * p);
            }
            // 変換に入る画像の 1 画素あたり。アルファは合成済みで RGB8、他は元の色型（RGB に広げて押さえる）
            let b = if alpha { 3 } else { bpp.max(3) };
            let (convert, canvas) = converting(w, h, b, mode, max_width, exif_frame);
            // `into_rgb8` の複製 ＋ エンコーダ。フレームの RGBA キャンバスから RGB8 へ移す間（7C）もこれ以下
            let encode = canvas * (3 + ENCODER_PER_PIXEL);
            decode.max(after).max(convert).max(encode)
        }
    }
}

/// デコード中の区間: 作業領域 ＋ 固定分 ＋ 出力の画素バッファ ＋ ファイルの大きさの項
fn decoding(header: &Header, p: u128, o: u128) -> u128 {
    let file = u128::from(header.file_len);
    match header.format {
        Format::Jxl => u128::from(jxl_working_bytes(header)) + o + file,
        Format::Avif => {
            let avif = header.avif.unwrap_or(crate::decode::AvifDeclared {
                primary_pixels: 0,
                alpha_pixels: None,
                bit_depth: 16,
            });
            // 1 ピクチャ = 宣言画素 × 3 平面（間引きは宣言から分からないので 4:4:4）× サンプルのバイト数
            let sample = if avif.bit_depth > 8 { 2 } else { 1 };
            let primary = u128::from(avif.primary_pixels).max(p) * 3 * sample;
            // アルファのアイテムの深度は宣言に無いので 2 バイトとみなす
            let alpha = u128::from(avif.alpha_pixels.unwrap_or(0)) * 3 * 2;
            // 主画像のデコーダが返したピクチャは、アルファのデコードと YUV→RGB 変換・向きの適用
            // （`Layout::apply` の複製）が終わるまで生きる。両方のデコーダの全ピクチャを足して押さえる
            (primary + alpha) * AVIF_PICTURES + 2 * o + AVIF_FILE_COPIES * file
        }
        Format::Other => DECODER_PER_PIXEL * p + o + file,
    }
}

/// 変換の区間のピークと、エンコードに渡すキャンバスの画素数。`b` は変換に入る画像の 1 画素あたり
fn converting(
    w: u128,
    h: u128,
    b: u128,
    mode: ConversionMode,
    max_width: Option<u32>,
    exif_frame: bool,
) -> (u128, u128) {
    let p = w * h;
    let i = p * b;
    let target = match mode {
        ConversionMode::Quality => None,
        _ => crate::target_canvas(max_width).map(|(tw, th)| (u128::from(tw), u128::from(th))),
    };
    match mode {
        ConversionMode::Quality => (i, p),
        ConversionMode::Crop => {
            // `convert_aspect_ratio_crop` の切り出し（`crop_imm` は複製を作る）
            let cw = w.min((h * 4).div_ceil(5));
            let ch = h.min((w * 5).div_ceil(4));
            let crop = i + cw * ch * b;
            match target {
                // 最終縮小: 切り出し ＋ 中間 ＋ 縮小後
                Some((tw, th)) if cw > tw => {
                    let resize = cw * ch * b + cw * th * RESIZE_PER_PIXEL + tw * th * b;
                    (crop.max(resize), tw * th)
                }
                _ => (crop, cw * ch),
            }
        }
        ConversionMode::Pad => {
            let (fw, fh) = fit(w, h);
            // 前段縮小: 目標の箱へ縮める。元画像 ＋ 中間（元の幅 × 新しい高さ ≤ 目標の高さ）＋ 縮小後
            let (shrink, p2, canvas) = match target {
                Some((tw, th)) if fw > tw => (
                    i + w * h.min(th) * RESIZE_PER_PIXEL + tw * th * b,
                    p.min(tw * th),
                    tw * th,
                ),
                _ => (0, p, fw * fh),
            };
            let i2 = p2 * b;
            // 余白を付ける: 写真 ＋ RGB キャンバス ＋ 写真の `into_rgb8`（RGB8 なら複製しないが押さえる）
            let place = i2 + canvas * 3 + p2 * 3;
            // Exif フレーム: 写真 ＋ バーの分だけ縮めた写真（中間を伴う）＋ RGBA キャンバス
            let frame = if exif_frame {
                i2 + i2 + (p2 * RESIZE_PER_PIXEL).max(canvas * 4)
            } else {
                0
            };
            (shrink.max(place).max(frame), canvas)
        }
    }
}

fn fit(w: u128, h: u128) -> (u128, u128) {
    match (u32::try_from(w), u32::try_from(h)) {
        (Ok(w), Ok(h)) => {
            let (fw, fh) = fit_to_4_5(w, h);
            (u128::from(fw), u128::from(fh))
        }
        _ => (u128::from(u32::MAX), u128::from(u32::MAX)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ConversionMode;
    use crate::decode::{AvifDeclared, Format, Header};
    use image::ColorType;

    const BUDGET_8GB_PC: u64 = 4_026_531_840; // 3.75GiB（spec §1）

    fn x66(color: ColorType) -> Header {
        Header {
            format: Format::Jxl,
            width: 6656,
            height: 9984,
            color,
            file_len: 5_100_000,
            avif: None,
        }
    }

    const FRAMED_PAD: Work = Work::Convert {
        mode: ConversionMode::Pad,
        max_width: None,
        exif_frame: true,
    };

    fn avif_header() -> Header {
        Header {
            format: Format::Avif,
            width: 4000,
            height: 3000,
            color: ColorType::Rgb8,
            file_len: 1_000_000,
            avif: Some(AvifDeclared {
                primary_pixels: 12_000_000,
                alpha_pixels: None,
                bit_depth: 8,
            }),
        }
    }

    fn jpeg_header(width: u32, height: u32) -> Header {
        Header {
            format: Format::Other,
            width,
            height,
            color: ColorType::Rgb8,
            file_len: 5_500_000,
            avif: None,
        }
    }

    #[test]
    fn x66_fits_the_budget_of_an_8gb_pc_in_rgb16_and_rgba16() {
        for color in [ColorType::Rgb16, ColorType::Rgba16] {
            let e = estimate(&x66(color), &FRAMED_PAD);
            assert!(e <= BUDGET_8GB_PC, "{color:?}: {e}");
        }
    }

    #[test]
    fn jxl_estimate_covers_its_own_working_limit_plus_output() {
        // max_alloc が出力を数えないので、見積もりは「作業領域 + 出力」以上でなければならない
        let h = x66(ColorType::Rgba16);
        assert!(estimate(&h, &Work::View) >= jxl_working_bytes(&h) + 6656 * 9984 * 8);
    }

    #[test]
    fn estimate_grows_with_file_size() {
        let small = Header {
            file_len: 1_000_000,
            ..avif_header()
        };
        let big = Header {
            file_len: 100_000_000,
            ..avif_header()
        };
        assert!(estimate(&big, &Work::View) >= estimate(&small, &Work::View) + 2 * 99_000_000);
    }

    #[test]
    fn avif_estimate_covers_eight_reference_frames() {
        // libdav1d は 1 アイテムの中でも参照フレーム 8 枚を持ちうる（spec §2）。
        // 間引きは宣言から分からないので 4:4:4（3 平面）とみなす
        assert!(estimate(&avif_header(), &Work::View) >= 8 * 12_000_000 * 3);
    }

    #[test]
    fn pad_estimate_covers_either_orientation_for_exif_oriented_formats() {
        // 7008x4672 の JPEG。向きは読まない（D-3）ので、縦に回っても横のままでも同じ値
        let land = jpeg_header(7008, 4672);
        let port = jpeg_header(4672, 7008);
        let pad = Work::Convert {
            mode: ConversionMode::Pad,
            max_width: None,
            exif_frame: false,
        };
        assert_eq!(estimate(&land, &pad), estimate(&port, &pad));
    }

    #[test]
    fn max_width_lowers_the_pad_estimate() {
        let h = jpeg_header(7008, 4672);
        let full = Work::Convert {
            mode: ConversionMode::Pad,
            max_width: None,
            exif_frame: false,
        };
        let capped = Work::Convert {
            mode: ConversionMode::Pad,
            max_width: Some(1080),
            exif_frame: false,
        };
        assert!(estimate(&h, &capped) < estimate(&h, &full));
    }

    /// 見積もりは実際のピーク以上でなければならない（spec §2）。値は計画の実施メモ「Task 3」で
    /// 現行の release ビルドを測ったピーク RSS（5 回の最大、`--max-width` は 3 回の最大。KB）。
    /// RSS は起動時の分（約 6MB）も含むので、見積もりにとって厳しい側の比較になる
    #[test]
    fn estimates_cover_the_measured_peaks() {
        let p33 = |format, color, file_len| Header {
            format,
            width: 7008,
            height: 4672,
            color,
            file_len,
            avif: None,
        };
        let j33 = p33(Format::Other, ColorType::Rgb8, 5_718_119);
        let j33p = p33(Format::Other, ColorType::Rgb8, 4_230_175);
        let p33a = p33(Format::Other, ColorType::Rgba8, 23_411_725);
        let p33x16 = p33(Format::Other, ColorType::Rgb16, 26_451_251);
        let w33a = p33(Format::Other, ColorType::Rgba8, 594_764);
        let x33 = p33(Format::Jxl, ColorType::Rgb8, 1_310_264);
        let a33 = Header {
            avif: Some(AvifDeclared {
                primary_pixels: 7008 * 4672,
                alpha_pixels: None,
                bit_depth: 8,
            }),
            ..p33(Format::Avif, ColorType::Rgb8, 96_842)
        };
        let x66_rgb = Header {
            file_len: 5_330_921,
            ..x66(ColorType::Rgb16)
        };
        let x66_rgba = Header {
            file_len: 2_209_654,
            ..x66(ColorType::Rgba16)
        };
        let bomb = Header {
            format: Format::Jxl,
            width: 12000,
            height: 14000,
            color: ColorType::Rgb8,
            file_len: 39_670,
            avif: None,
        };
        let convert = |mode, max_width, exif_frame| Work::Convert {
            mode,
            max_width,
            exif_frame,
        };
        let crop = convert(ConversionMode::Crop, None, false);
        let pad = convert(ConversionMode::Pad, None, false);
        let pad_e = convert(ConversionMode::Pad, None, true);
        let cases: [(&str, &Header, Work, u64); 22] = [
            ("j33 view", &j33, Work::View, 105_428),
            ("j33p view", &j33p, Work::View, 295_884),
            ("w33a view", &w33a, Work::View, 339_760),
            ("a33 view", &a33, Work::View, 148_460),
            ("x66a view", &x66_rgba, Work::View, 2_629_740),
            ("bomb view", &bomb, Work::View, 4_483_788),
            ("j33 crop", &j33, crop, 211_260),
            ("j33 pad", &j33, pad, 725_564),
            ("j33 pad-e", &j33, pad_e, 740_252),
            ("j33p pad", &j33p, pad, 725_536),
            ("p33a crop", &p33a, crop, 229_608),
            ("p33a pad", &p33a, pad, 725_452),
            ("p33x16 crop", &p33x16, crop, 299_872),
            ("w33a crop", &w33a, crop, 342_084),
            ("x33 pad", &x33, pad, 882_320),
            ("a33 pad", &a33, pad, 726_284),
            ("x66 pad-e", &x66_rgb, pad_e, 1_979_524),
            ("x66a pad-e", &x66_rgba, pad_e, 2_632_344),
            ("bomb crop", &bomb, crop, 4_485_488),
            ("bomb pad", &bomb, pad, 4_485_480),
            (
                "j33 pad --max-width 6000",
                &j33,
                convert(ConversionMode::Pad, Some(6000), false),
                610_460,
            ),
            (
                "j33 crop --max-width 3000",
                &j33,
                convert(ConversionMode::Crop, Some(3000), false),
                309_208,
            ),
        ];
        for (name, header, work, peak_kb) in cases {
            let e = estimate(header, &work);
            assert!(
                e >= peak_kb * 1024,
                "{name}: estimate {e} < peak {}",
                peak_kb * 1024
            );
        }
    }
}
