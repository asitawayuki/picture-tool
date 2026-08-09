pub mod layout;
pub mod logo;
pub mod preset;
pub mod text;

use ab_glyph::FontArc;
use anyhow::Result;
use image::{DynamicImage, Rgba, RgbaImage};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Exif情報の配置位置
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ExifPosition {
    /// デフォルト: 横構図→下、縦構図→右
    #[default]
    Auto,
    Bottom,
    Top,
    Right,
    Left,
}

/// 表示項目のON/OFF
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayItems {
    pub maker_logo: bool,
    pub lens_brand_logo: bool,
    pub camera_model: bool,
    pub lens_model: bool,
    pub focal_length: bool,
    pub f_number: bool,
    pub shutter_speed: bool,
    pub iso: bool,
    pub date_taken: bool,
    pub custom_text: bool,
}

impl Default for DisplayItems {
    fn default() -> Self {
        Self {
            maker_logo: true,
            lens_brand_logo: true,
            camera_model: true,
            lens_model: true,
            focal_length: true,
            f_number: true,
            shutter_speed: true,
            iso: true,
            date_taken: false,
            custom_text: false,
        }
    }
}

/// フォント設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FontConfig {
    pub font_path: Option<String>,
    pub primary_size: f32,
    pub secondary_size: f32,
}

impl Default for FontConfig {
    fn default() -> Self {
        Self {
            font_path: None,
            primary_size: 0.025,
            secondary_size: 0.018,
        }
    }
}

/// Exifフレーム設定（1プリセット = この構造体1つ）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExifFrameConfig {
    pub name: String,
    pub position: ExifPosition,
    pub items: DisplayItems,
    pub font: FontConfig,
    pub custom_text: String,
}

impl Default for ExifFrameConfig {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            position: ExifPosition::Auto,
            items: DisplayItems::default(),
            font: FontConfig::default(),
            custom_text: String::new(),
        }
    }
}

/// ユーザー設定ディレクトリ（`<OS の config dir>/picture-tool`）
///
/// **設定ディレクトリのパスを組み立てる唯一の場所。** 以前は CLI・GUI・core が
/// それぞれ `"picture-tool/presets"` のような文字列を直書きしており、
/// 片方だけ変えると設定が読めなくなる構造だった（S6-M17）。
///
/// 外向きの入口は `AssetDirs::default()` の方。こちらを公開すると
/// 「自分でサブディレクトリを組み立てる」経路が復活するので crate 内に閉じる。
pub(crate) fn user_config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("picture-tool"))
}

/// アセットディレクトリの検索パス
#[derive(Debug, Clone)]
pub struct AssetDirs {
    pub user_logos_dir: Option<PathBuf>,
    pub user_fonts_dir: Option<PathBuf>,
    pub user_model_map: Option<PathBuf>,
    /// ユーザープリセットの保存先。`preset::list_all_presets` 等に渡す。
    pub user_presets_dir: Option<PathBuf>,
}

impl Default for AssetDirs {
    fn default() -> Self {
        let config_dir = user_config_dir();
        Self {
            user_logos_dir: config_dir.as_ref().map(|d| d.join("assets/logos")),
            user_fonts_dir: config_dir.as_ref().map(|d| d.join("assets/fonts")),
            user_model_map: config_dir.as_ref().map(|d| d.join("model_map_custom.json")),
            user_presets_dir: config_dir.as_ref().map(|d| d.join("presets")),
        }
    }
}

/// Exifフレーム描画に必要なアセット一式。
///
/// **画像1枚ごとではなく、バッチの前に1回だけ構築すること。** 以前は
/// `render_exif_frame` の中で毎回 `ModelMap` を組み立てており、並列ワーカーが
/// それぞれ同じ埋め込み JSON をパースし、同じユーザーファイルを読み直していた。
pub struct ExifAssets {
    pub dirs: AssetDirs,
    model_map: crate::model_map::ModelMap,
    /// 構築時の非致命的な問題（カスタム model_map の読み込み失敗など）。
    /// core は `eprintln!` しないので、呼び出し元が利用者へ伝えること。
    pub warnings: Vec<String>,
}

impl ExifAssets {
    pub fn load(dirs: AssetDirs) -> Result<Self> {
        let mut model_map = crate::model_map::ModelMap::load_bundled()?;
        let mut warnings = Vec::new();

        if let Some(ref custom_path) = dirs.user_model_map {
            if custom_path.exists() {
                // カスタムマップが壊れていても描画自体は続行するが、
                // 「書いたのに効いていない」に気づけるよう必ず警告を残す。
                match std::fs::read_to_string(custom_path) {
                    Ok(json_str) => {
                        if let Err(e) = model_map.merge_custom(&json_str) {
                            warnings.push(format!(
                                "Ignoring custom model map {}: {:#}",
                                custom_path.display(),
                                e
                            ));
                        }
                    }
                    Err(e) => warnings.push(format!(
                        "Failed to read custom model map {}: {}",
                        custom_path.display(),
                        e
                    )),
                }
            }
        }

        Ok(Self {
            dirs,
            model_map,
            warnings,
        })
    }
}

/// フォント情報（GUI一覧表示用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FontInfo {
    pub display_name: String,
    pub path: Option<String>,
    pub is_bundled: bool,
}

/// Exifフレーム付き画像を生成
pub fn render_exif_frame(
    image: &DynamicImage,
    exif: &crate::ExifInfo,
    config: &ExifFrameConfig,
    bg_color: &crate::BackgroundColor,
    assets: &ExifAssets,
) -> Result<DynamicImage> {
    let photo_w = image.width();
    let photo_h = image.height();

    // 1. レイアウト計算
    let layout = layout::calculate_pad_exif_layout(photo_w, photo_h, config, bg_color);

    // 2. skip_exif: 4:5キャンバスに写真を中央配置して返す
    if layout.skip_exif {
        let bg_pixel = bg_color.to_rgba();
        let mut canvas = RgbaImage::from_pixel(layout.canvas_width, layout.canvas_height, bg_pixel);
        image::imageops::overlay(
            &mut canvas,
            image,
            layout.photo_x as i64,
            layout.photo_y as i64,
        );
        return Ok(DynamicImage::ImageRgba8(canvas));
    }

    // 3. 写真リサイズ（必要な場合）
    let resized;
    let photo = if layout.photo_width != photo_w || layout.photo_height != photo_h {
        resized = image.resize_exact(
            layout.photo_width,
            layout.photo_height,
            image::imageops::FilterType::Lanczos3,
        );
        &resized
    } else {
        image
    };

    // 4. キャンバス作成
    let bg_pixel = bg_color.to_rgba();
    let mut canvas = RgbaImage::from_pixel(layout.canvas_width, layout.canvas_height, bg_pixel);

    // 5. 写真をオーバーレイ
    image::imageops::overlay(
        &mut canvas,
        photo,
        layout.photo_x as i64,
        layout.photo_y as i64,
    );

    // 6. ModelMap は ExifAssets として呼び出し元が1回だけ構築済み
    let model_map = &assets.model_map;

    // 7. テキスト色（背景輝度に基づく）
    let luminance =
        0.299 * bg_pixel[0] as f32 + 0.587 * bg_pixel[1] as f32 + 0.114 * bg_pixel[2] as f32;
    let is_dark = luminance < 128.0;
    let primary_color = if is_dark {
        Rgba([255u8, 255, 255, 255])
    } else {
        Rgba([0x33u8, 0x33, 0x33, 255])
    };
    let secondary_color = if is_dark {
        Rgba([0xaau8, 0xaa, 0xaa, 255])
    } else {
        Rgba([0x88u8, 0x88, 0x88, 255])
    };

    // 8. フォント読み込み
    let font = text::load_font(config.font.font_path.as_deref())?;

    // 9. ロゴ読み込み
    let logo_size = layout.exif_area_height.min(layout.exif_area_width) * 3 / 5;
    let logo_size = logo_size.max(16);
    let user_logos = assets.dirs.user_logos_dir.as_deref();

    let maker_logo = if config.items.maker_logo {
        if let Some(ref make) = exif.camera_make {
            if let Some(entry) = model_map.maker_logo(make) {
                let filename = entry.maker.clone();
                logo::resolve_and_load_logo(user_logos, &filename, is_dark, logo_size)
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    let lens_logo = if config.items.lens_brand_logo {
        if let Some(ref lens) = exif.lens_model {
            logo::resolve_lens_brand_logo(user_logos, lens, model_map, is_dark, logo_size)
        } else {
            None
        }
    } else {
        None
    };

    // 10. テキスト構築
    let primary_text = build_primary_text(exif, &config.items);
    let secondary_text = build_secondary_text(exif, &config.items, &config.custom_text);

    // 11. 描画
    let bar = ExifBar {
        font: &font,
        config,
        primary_text: &primary_text,
        secondary_text: &secondary_text,
        primary_color,
        secondary_color,
        maker_logo: maker_logo.as_ref(),
        lens_logo: lens_logo.as_ref(),
        photo_short_side: layout.photo_width.min(layout.photo_height),
    };
    draw_exif_area(&mut canvas, &layout, &bar);

    Ok(DynamicImage::ImageRgba8(canvas))
}

/// Exifバー1本を描くのに必要な情報。
/// 水平（Bottom/Top）と回転（Right/Left）で共通。
struct ExifBar<'a> {
    font: &'a FontArc,
    config: &'a ExifFrameConfig,
    primary_text: &'a str,
    secondary_text: &'a str,
    primary_color: Rgba<u8>,
    secondary_color: Rgba<u8>,
    maker_logo: Option<&'a DynamicImage>,
    lens_logo: Option<&'a DynamicImage>,
    photo_short_side: u32,
}

/// レイアウトが示す Exif エリアにバーを描き込む。
///
/// バーの中身は常に「横長」として1つの透明バッファに描画し、
/// Right/Left のときだけ90度回転してから合成する。
/// 水平版と回転版を別々に実装していた頃は、セパレータ線が
/// 片方だけ不透明になる等の不整合が生まれていた（S4-H5/H7）。
fn draw_exif_area(canvas: &mut RgbaImage, layout: &layout::PadExifLayout, bar: &ExifBar) {
    let area_w = layout.exif_area_width;
    let area_h = layout.exif_area_height;
    if area_w == 0 || area_h == 0 {
        return;
    }

    // 回転後に Exif エリアと一致するよう、バッファは長辺×短辺で作る
    let (buf_w, buf_h) = if layout.is_rotated {
        (area_h, area_w)
    } else {
        (area_w, area_h)
    };

    let buf = render_exif_bar(buf_w, buf_h, bar);
    let bar_image = if layout.is_rotated {
        image::imageops::rotate90(&buf)
    } else {
        buf
    };

    // overlay はアルファ合成する。半透明のセパレータ線が
    // 背景と正しく混ざるのはこの経路を通るため（S4-H5）。
    image::imageops::overlay(
        canvas,
        &bar_image,
        layout.exif_area_x as i64,
        layout.exif_area_y as i64,
    );
}

/// 横長のExifバーを透明バッファに描画する。
///
/// レイアウト（左から右）:
///   [余白][メーカーロゴ][余白][セパレータ][余白][2段テキスト ... ][レンズロゴ][余白]
fn render_exif_bar(buf_w: u32, buf_h: u32, bar: &ExifBar) -> RgbaImage {
    let mut buf = RgbaImage::new(buf_w, buf_h);

    let margin = (buf_h as f32 * 0.1) as u32;
    let separator_width = 2u32;
    let logo_display_h = ((buf_h as f32 * 0.45) as u32).max(1);

    // --- 左: メーカーロゴ + セパレータ ---
    let mut text_start_x = margin;
    if let Some(logo) = bar.maker_logo {
        let logo_scaled = logo.resize(
            u32::MAX,
            logo_display_h,
            image::imageops::FilterType::Lanczos3,
        );
        let logo_y = (buf_h.saturating_sub(logo_scaled.height())) / 2;
        image::imageops::overlay(&mut buf, &logo_scaled, margin as i64, logo_y as i64);
        text_start_x = margin + logo_scaled.width() + margin;

        let sep_x = text_start_x;
        let sep_top = buf_h / 6;
        let sep_bot = buf_h * 5 / 6;
        let sep_color = Rgba([
            bar.primary_color[0],
            bar.primary_color[1],
            bar.primary_color[2],
            100,
        ]);
        for py in sep_top..sep_bot.min(buf_h) {
            for px in sep_x..(sep_x + separator_width).min(buf_w) {
                buf.put_pixel(px, py, sep_color);
            }
        }
        text_start_x += separator_width + margin;
    }

    // --- フォントサイズの基準（写真の短辺ベース、バー高さで頭打ち） ---
    let max_font = buf_h as f32 * 0.4;
    let primary_size_base = (bar.photo_short_side as f32 * bar.config.font.primary_size)
        .max(10.0)
        .min(max_font);
    let secondary_size_base = (bar.photo_short_side as f32 * bar.config.font.secondary_size)
        .max(8.0)
        .min(max_font * 0.75);

    // --- 右: レンズブランドロゴ ---
    // テキストをフィットさせる *前* に確定させ、その幅をテキスト領域から差し引く。
    // 後から重ねるだけだと、テキストが領域幅いっぱいに広がったときに
    // ロゴがテキストの上に被る（S4-H6）。
    let lens_logo_scaled = bar.lens_logo.map(|llogo| {
        llogo.resize(
            u32::MAX,
            ((secondary_size_base * 1.2) as u32).max(1),
            image::imageops::FilterType::Lanczos3,
        )
    });

    let mut text_end_x = buf_w.saturating_sub(margin);
    if let Some(ref ll) = lens_logo_scaled {
        text_end_x = text_end_x.saturating_sub(ll.width() + margin);
    }

    if text_start_x >= text_end_x {
        // テキストを置く幅が残っていない。ロゴだけ描いて返す。
        overlay_lens_logo(&mut buf, buf_w, buf_h, margin, lens_logo_scaled.as_ref());
        return buf;
    }
    let text_area_w = (text_end_x - text_start_x) as f32;

    let (primary_fitted, primary_size) = if bar.primary_text.is_empty() {
        (String::new(), primary_size_base)
    } else {
        text::auto_fit_text(
            bar.font,
            primary_size_base,
            bar.primary_text,
            text_area_w,
            0.7,
        )
    };
    let (secondary_fitted, secondary_size) = if bar.secondary_text.is_empty() {
        (String::new(), secondary_size_base)
    } else {
        text::auto_fit_text(
            bar.font,
            secondary_size_base,
            bar.secondary_text,
            text_area_w,
            0.7,
        )
    };

    // 2行まとめて縦中央
    let total_text_h = primary_size + secondary_size + 2.0;
    let text_block_y = (buf_h as f32 - total_text_h) / 2.0;

    if !primary_fitted.is_empty() {
        text::draw_text_on_image(
            &mut buf,
            bar.font,
            primary_size,
            &primary_fitted,
            text_start_x as i32,
            text_block_y as i32,
            bar.primary_color,
        );
    }
    if !secondary_fitted.is_empty() {
        text::draw_text_on_image(
            &mut buf,
            bar.font,
            secondary_size,
            &secondary_fitted,
            text_start_x as i32,
            (text_block_y + primary_size + 2.0) as i32,
            bar.secondary_color,
        );
    }

    overlay_lens_logo(&mut buf, buf_w, buf_h, margin, lens_logo_scaled.as_ref());
    buf
}

fn overlay_lens_logo(
    buf: &mut RgbaImage,
    buf_w: u32,
    buf_h: u32,
    margin: u32,
    lens_logo: Option<&DynamicImage>,
) {
    if let Some(ll) = lens_logo {
        let x = buf_w.saturating_sub(ll.width() + margin);
        let y = (buf_h.saturating_sub(ll.height())) / 2;
        image::imageops::overlay(buf, ll, x as i64, y as i64);
    }
}

fn build_primary_text(exif: &crate::ExifInfo, items: &DisplayItems) -> String {
    let mut parts = Vec::new();
    if items.camera_model {
        if let Some(ref model) = exif.camera_model {
            parts.push(model.clone());
        }
    }
    if items.lens_model {
        if let Some(ref lens) = exif.lens_model {
            parts.push(lens.clone());
        }
    }
    parts.join(" | ")
}

fn build_secondary_text(exif: &crate::ExifInfo, items: &DisplayItems, custom_text: &str) -> String {
    let mut parts = Vec::new();
    if items.focal_length {
        if let Some(ref v) = exif.focal_length {
            parts.push(v.clone());
        }
    }
    if items.f_number {
        if let Some(ref v) = exif.f_number {
            parts.push(v.clone());
        }
    }
    if items.shutter_speed {
        if let Some(ref v) = exif.shutter_speed {
            parts.push(v.clone());
        }
    }
    if items.iso {
        if let Some(v) = exif.iso {
            parts.push(format!("ISO {}", v));
        }
    }
    if items.date_taken {
        if let Some(ref v) = exif.date_taken {
            parts.push(v.clone());
        }
    }
    if items.custom_text && !custom_text.is_empty() {
        parts.push(custom_text.to_string());
    }
    parts.join("  ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ExifInfo;

    #[test]
    fn exif_frame_config_json_roundtrip() {
        let config = ExifFrameConfig {
            name: "test".to_string(),
            position: ExifPosition::Bottom,
            items: DisplayItems::default(),
            font: FontConfig::default(),
            custom_text: "@user".to_string(),
        };
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: ExifFrameConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "test");
        assert_eq!(deserialized.custom_text, "@user");
        assert_eq!(deserialized.position, ExifPosition::Bottom);
    }

    #[test]
    fn exif_position_default() {
        let pos = ExifPosition::default();
        assert_eq!(pos, ExifPosition::Auto);
    }

    #[test]
    fn build_primary_text_with_camera_and_lens() {
        let exif = ExifInfo {
            camera_model: Some("ILCE-7M4".to_string()),
            lens_model: Some("FE 24-70mm f/2.8 GM II".to_string()),
            ..ExifInfo::default()
        };
        let items = DisplayItems::default();
        let text = build_primary_text(&exif, &items);
        assert_eq!(text, "ILCE-7M4 | FE 24-70mm f/2.8 GM II");
    }

    #[test]
    fn build_secondary_text_params() {
        let exif = ExifInfo {
            focal_length: Some("35mm".to_string()),
            f_number: Some("f/2.8".to_string()),
            shutter_speed: Some("1/250s".to_string()),
            iso: Some(400),
            ..ExifInfo::default()
        };
        let items = DisplayItems::default();
        let text = build_secondary_text(&exif, &items, "");
        assert!(text.contains("35mm"));
        assert!(text.contains("f/2.8"));
        assert!(text.contains("ISO 400"));
    }

    /// 仕様: レンズブランドロゴと Exif テキストは重ならない（S4-H6）。
    ///
    /// 以前は `auto_fit_text` にバー幅いっぱいを渡してから、あとでロゴを
    /// 上に重ねていたため、テキストが幅いっぱいまで広がるとロゴがテキストに被った。
    ///
    /// 検証方法: レンズロゴを一目で分かる単色（マゼンタ）にして描画し、
    /// ロゴが置かれる右端の帯にテキストの画素が1つも入らないことを確認する。
    #[cfg(feature = "bundled-font")]
    #[test]
    fn lens_logo_never_overlaps_the_exif_text() {
        const MAGENTA: Rgba<u8> = Rgba([255, 0, 255, 255]);
        // バー幅を確実に使い切る長さにする。短いと「たまたま重ならなかった」
        // だけでテストが通ってしまい、回帰を検出できない。
        const PRIMARY: &str = "ILCE-7M4 | FE 24-70mm F2.8 GM II | ILCE-7M4 | FE 24-70mm F2.8 GM II | ILCE-7M4 | FE 24-70mm F2.8 GM II";
        const SECONDARY: &str = "35mm  f/2.8  1/250s  ISO 400  2026-08-05 12:34:56  @photographer  35mm  f/2.8  1/250s  ISO 400";
        let font = text::load_font(None).unwrap();
        let lens_logo = DynamicImage::ImageRgba8(RgbaImage::from_pixel(120, 40, MAGENTA));

        let bar = ExifBar {
            font: &font,
            config: &ExifFrameConfig::default(),
            // 縮小しても入りきらないほど長いテキスト（＝幅いっぱいに広がる状況）
            primary_text: PRIMARY,
            secondary_text: SECONDARY,
            primary_color: Rgba([255, 255, 255, 255]),
            secondary_color: Rgba([170, 170, 170, 255]),
            maker_logo: None,
            lens_logo: Some(&lens_logo),
            photo_short_side: 1000,
        };

        let buf_w = 800;
        let buf_h = 60;
        let with_logo = render_exif_bar(buf_w, buf_h, &bar);
        let without_logo = render_exif_bar(
            buf_w,
            buf_h,
            &ExifBar {
                lens_logo: None,
                ..bar
            },
        );

        // ロゴが占める帯を、実際に描かれたマゼンタ画素から特定する
        let band_start = (0..buf_w)
            .find(|&x| (0..buf_h).any(|y| with_logo.get_pixel(x, y).0[0..3] == [255, 0, 255]))
            .expect("the lens logo must actually be drawn");

        let opaque_pixels_in_band = |img: &RgbaImage| {
            (band_start..buf_w)
                .flat_map(|x| (0..buf_h).map(move |y| (x, y)))
                .filter(|&(x, y)| img.get_pixel(x, y)[3] > 0)
                .collect::<Vec<_>>()
        };

        // 前提条件: ロゴが無ければテキストはこの帯まで伸びる。
        // ここが空だとテストが何も検出できていないことになる。
        assert!(
            !opaque_pixels_in_band(&without_logo).is_empty(),
            "precondition failed: the text does not reach the logo band even without a logo, \
             so this test cannot detect an overlap"
        );

        // 本題: ロゴがあるとき、その帯にはロゴ以外の画素があってはならない
        for (x, y) in opaque_pixels_in_band(&with_logo) {
            let p = with_logo.get_pixel(x, y);
            assert_eq!(
                p.0[0..3],
                [255, 0, 255],
                "text pixel {:?} at ({}, {}) intrudes into the lens logo band (x >= {})",
                p,
                x,
                y,
                band_start
            );
        }
    }
}
