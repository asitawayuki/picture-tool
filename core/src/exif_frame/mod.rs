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
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ExifPosition {
    /// デフォルト: 横構図→下、縦構図→右
    Auto,
    Bottom,
    Top,
    Right,
    Left,
}

impl Default for ExifPosition {
    fn default() -> Self {
        Self::Auto
    }
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

/// アセットディレクトリの検索パス
#[derive(Debug, Clone)]
pub struct AssetDirs {
    pub user_logos_dir: Option<PathBuf>,
    pub user_fonts_dir: Option<PathBuf>,
    pub user_model_map: Option<PathBuf>,
}

impl Default for AssetDirs {
    fn default() -> Self {
        let config_dir = dirs_config_dir();
        Self {
            user_logos_dir: config_dir.as_ref().map(|d| d.join("assets/logos")),
            user_fonts_dir: config_dir.as_ref().map(|d| d.join("assets/fonts")),
            user_model_map: config_dir.as_ref().map(|d| d.join("model_map_custom.json")),
        }
    }
}

fn dirs_config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("picture-tool"))
}

/// フォント情報（GUI一覧表示用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FontInfo {
    pub display_name: String,
    pub path: Option<String>,
    pub is_bundled: bool,
}

/// ロゴ情報（GUI一覧表示用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogoInfo {
    pub filename: String,
    pub matched_to: Option<String>,
    pub is_bundled: bool,
}

/// Exifフレーム付き画像を生成
pub fn render_exif_frame(
    image: &DynamicImage,
    exif: &crate::ExifInfo,
    config: &ExifFrameConfig,
    bg_color: &crate::BackgroundColor,
    asset_dirs: &AssetDirs,
) -> Result<DynamicImage> {
    let photo_w = image.width();
    let photo_h = image.height();

    // 1. レイアウト計算
    let layout = layout::calculate_pad_exif_layout(photo_w, photo_h, config, bg_color);

    // 2. skip_exif: 4:5キャンバスに写真を中央配置して返す
    if layout.skip_exif {
        let bg_pixel = bg_color.to_rgba();
        let mut canvas = RgbaImage::from_pixel(layout.canvas_width, layout.canvas_height, bg_pixel);
        image::imageops::overlay(&mut canvas, image, layout.photo_x as i64, layout.photo_y as i64);
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
    image::imageops::overlay(&mut canvas, photo, layout.photo_x as i64, layout.photo_y as i64);

    // 6. ModelMap 読み込み（カスタムマップをオプションでマージ）
    let mut model_map = crate::model_map::ModelMap::load_bundled();
    if let Some(ref custom_path) = asset_dirs.user_model_map {
        if custom_path.exists() {
            if let Ok(json_str) = std::fs::read_to_string(custom_path) {
                let _ = model_map.merge_custom(&json_str);
            }
        }
    }

    // 7. テキスト色（背景輝度に基づく）
    let luminance = 0.299 * bg_pixel[0] as f32
        + 0.587 * bg_pixel[1] as f32
        + 0.114 * bg_pixel[2] as f32;
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
    let user_logos = asset_dirs.user_logos_dir.as_deref();

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
            logo::resolve_lens_brand_logo(user_logos, lens, &model_map, is_dark, logo_size)
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
    if layout.is_rotated {
        draw_exif_rotated(
            &mut canvas,
            &layout,
            &font,
            config,
            &primary_text,
            &secondary_text,
            primary_color,
            maker_logo.as_ref(),
        );
    } else {
        draw_exif_horizontal(
            &mut canvas,
            &layout,
            &font,
            config,
            &primary_text,
            &secondary_text,
            primary_color,
            secondary_color,
            maker_logo.as_ref(),
            lens_logo.as_ref(),
        );
    }

    Ok(DynamicImage::ImageRgba8(canvas))
}

/// 水平レイアウト（Bottom/Top）でExif情報を描画する
fn draw_exif_horizontal(
    canvas: &mut RgbaImage,
    layout: &layout::PadExifLayout,
    font: &FontArc,
    config: &ExifFrameConfig,
    primary_text: &str,
    secondary_text: &str,
    primary_color: Rgba<u8>,
    secondary_color: Rgba<u8>,
    maker_logo: Option<&DynamicImage>,
    lens_logo: Option<&DynamicImage>,
) {
    let area_x = layout.exif_area_x;
    let area_y = layout.exif_area_y;
    let area_w = layout.exif_area_width;
    let area_h = layout.exif_area_height;

    if area_w == 0 || area_h == 0 {
        return;
    }

    // ロゴの高さは exif_area_height の 60%
    let logo_display_h = (area_h as f32 * 0.6) as u32;
    let logo_display_h = logo_display_h.max(1);

    // ロゴ描画 + ロゴが占める横幅
    let mut text_start_x = area_x;
    let separator_width = 2u32;
    let logo_margin = (area_h as f32 * 0.1) as u32;

    if let Some(logo) = maker_logo {
        let logo_scaled = logo.resize(
            u32::MAX,
            logo_display_h,
            image::imageops::FilterType::Lanczos3,
        );
        let logo_w = logo_scaled.width();
        let logo_x = area_x + logo_margin;
        let logo_y = area_y + (area_h.saturating_sub(logo_display_h)) / 2;
        image::imageops::overlay(canvas, &logo_scaled, logo_x as i64, logo_y as i64);
        text_start_x = logo_x + logo_w + logo_margin;

        // セパレータ線
        let sep_x = text_start_x;
        let sep_top = area_y + area_h / 6;
        let sep_bot = area_y + area_h * 5 / 6;
        for py in sep_top..sep_bot.min(canvas.height()) {
            for px in sep_x..(sep_x + separator_width).min(canvas.width()) {
                let sep_color = Rgba([primary_color[0], primary_color[1], primary_color[2], 100]);
                canvas.put_pixel(px, py, sep_color);
            }
        }
        text_start_x += separator_width + logo_margin;
    }

    // テキストエリア幅
    let text_area_w = area_x + area_w - text_start_x;
    if text_area_w == 0 {
        return;
    }

    // テキスト垂直中央配置
    // 2行：primary (上) + secondary (下)
    let primary_size_base = area_h as f32 * config.font.primary_size;
    let secondary_size_base = area_h as f32 * config.font.secondary_size;
    // フォントサイズの最小値を area_h の一定割合に設定（小さすぎ防止）
    let primary_size_base = primary_size_base.max(8.0);
    let secondary_size_base = secondary_size_base.max(6.0);

    let (primary_fitted, primary_size) = if !primary_text.is_empty() {
        text::auto_fit_text(font, primary_size_base, primary_text, text_area_w as f32, 0.7)
    } else {
        (String::new(), primary_size_base)
    };

    let (secondary_fitted, secondary_size) = if !secondary_text.is_empty() {
        text::auto_fit_text(font, secondary_size_base, secondary_text, text_area_w as f32, 0.7)
    } else {
        (String::new(), secondary_size_base)
    };

    // 2行まとめて縦中央
    let total_text_h = primary_size + secondary_size + 2.0;
    let text_block_y = area_y as f32 + (area_h as f32 - total_text_h) / 2.0;

    if !primary_fitted.is_empty() {
        text::draw_text_on_image(
            canvas,
            font,
            primary_size,
            &primary_fitted,
            text_start_x as i32,
            text_block_y as i32,
            primary_color,
        );
    }

    if !secondary_fitted.is_empty() {
        text::draw_text_on_image(
            canvas,
            font,
            secondary_size,
            &secondary_fitted,
            text_start_x as i32,
            (text_block_y + primary_size + 2.0) as i32,
            secondary_color,
        );
    }

    // レンズブランドロゴ（primary textの後ろに追加）
    // 簡易実装: lens_logo は secondary 行の右端付近に表示
    if let Some(llogo) = lens_logo {
        let ll_h = (secondary_size * 1.2) as u32;
        let ll_scaled = llogo.resize(u32::MAX, ll_h.max(1), image::imageops::FilterType::Lanczos3);
        let ll_x = area_x + area_w - ll_scaled.width() - logo_margin;
        let ll_y = area_y + (area_h.saturating_sub(ll_scaled.height())) / 2;
        image::imageops::overlay(canvas, &ll_scaled, ll_x as i64, ll_y as i64);
    }
}

/// 回転レイアウト（Right/Left）でExif情報を描画する
fn draw_exif_rotated(
    canvas: &mut RgbaImage,
    layout: &layout::PadExifLayout,
    font: &FontArc,
    config: &ExifFrameConfig,
    primary_text: &str,
    secondary_text: &str,
    primary_color: Rgba<u8>,
    maker_logo: Option<&DynamicImage>,
) {
    let area_x = layout.exif_area_x;
    let area_y = layout.exif_area_y;
    let area_w = layout.exif_area_width;
    let area_h = layout.exif_area_height;

    if area_w == 0 || area_h == 0 {
        return;
    }

    // 回転レイアウト: exif_area_height が「テキストの最大幅」になる
    let max_text_width = area_h as f32 * 0.9;
    let center_x = (area_x + area_w / 2) as i32;

    // メーカーロゴを exif バーの上端付近に配置
    let logo_margin = (area_w as f32 * 0.1) as u32;
    let logo_display_w = (area_w as f32 * 0.7) as u32;
    let mut text_center_y = (area_y + area_h / 2) as i32;

    if let Some(logo) = maker_logo {
        let logo_scaled = logo.resize(
            logo_display_w.max(1),
            u32::MAX,
            image::imageops::FilterType::Lanczos3,
        );
        let logo_h = logo_scaled.height();
        let logo_x = area_x + (area_w.saturating_sub(logo_scaled.width())) / 2;
        let logo_y = area_y + logo_margin;
        image::imageops::overlay(canvas, &logo_scaled, logo_x as i64, logo_y as i64);
        // ロゴの下からテキスト中央を調整
        let remaining_y_start = logo_y + logo_h + logo_margin;
        let remaining_h = (area_y + area_h).saturating_sub(remaining_y_start);
        text_center_y = (remaining_y_start + remaining_h / 2) as i32;
    }

    // primary + secondary を1行に結合
    let combined = if !primary_text.is_empty() && !secondary_text.is_empty() {
        format!("{}  |  {}", primary_text, secondary_text)
    } else if !primary_text.is_empty() {
        primary_text.to_string()
    } else {
        secondary_text.to_string()
    };

    if combined.is_empty() {
        return;
    }

    let text_size_base = area_w as f32 * config.font.primary_size;
    let text_size_base = text_size_base.max(8.0);

    let (fitted, final_size) =
        text::auto_fit_text(font, text_size_base, &combined, max_text_width, 0.7);

    text::draw_text_rotated_90(
        canvas,
        font,
        final_size,
        &fitted,
        center_x,
        text_center_y,
        primary_color,
    );
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

fn build_secondary_text(
    exif: &crate::ExifInfo,
    items: &DisplayItems,
    custom_text: &str,
) -> String {
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
}
