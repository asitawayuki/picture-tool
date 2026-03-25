pub mod layout;
pub mod logo;
pub mod preset;
pub mod text;

use anyhow::Result;
use image::{DynamicImage, Rgba, RgbaImage};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// フレームレイアウトの種類（3パターン）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameLayout {
    BottomBar,
    SideBar,
    FullBorder,
}

/// フレーム背景色
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameColor {
    White,
    Black,
    Custom(u8, u8, u8),
}

impl FrameColor {
    pub fn to_rgba(&self) -> image::Rgba<u8> {
        match self {
            FrameColor::White => image::Rgba([255, 255, 255, 255]),
            FrameColor::Black => image::Rgba([0, 0, 0, 255]),
            FrameColor::Custom(r, g, b) => image::Rgba([*r, *g, *b, 255]),
        }
    }

    pub fn is_dark(&self) -> bool {
        match self {
            FrameColor::Black => true,
            FrameColor::White => false,
            FrameColor::Custom(r, g, b) => {
                (*r as f32 * 0.299 + *g as f32 * 0.587 + *b as f32 * 0.114) < 128.0
            }
        }
    }
}

/// 出力アスペクト比
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputAspectRatio {
    Fixed(u32, u32),
    Free,
}

/// 表示項目のON/OFF
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DisplayItems {
    pub maker_logo: bool,
    pub brand_logo: bool,
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

impl DisplayItems {
    pub fn all_enabled() -> Self {
        Self {
            maker_logo: true,
            brand_logo: true,
            lens_brand_logo: true,
            camera_model: true,
            lens_model: true,
            focal_length: true,
            f_number: true,
            shutter_speed: true,
            iso: true,
            date_taken: true,
            custom_text: true,
        }
    }
}

/// フォント設定
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExifFrameConfig {
    pub name: String,
    pub layout: FrameLayout,
    pub color: FrameColor,
    pub aspect_ratio: OutputAspectRatio,
    pub items: DisplayItems,
    pub font: FontConfig,
    pub custom_text: String,
    pub frame_padding: f32,
}

impl Default for ExifFrameConfig {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            layout: FrameLayout::BottomBar,
            color: FrameColor::White,
            aspect_ratio: OutputAspectRatio::Fixed(4, 5),
            items: DisplayItems::all_enabled(),
            font: FontConfig::default(),
            custom_text: String::new(),
            frame_padding: 0.05,
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
    asset_dirs: &AssetDirs,
) -> Result<DynamicImage> {
    let (photo_w, photo_h) = (image.width(), image.height());

    // レイアウト計算
    let dims = layout::calculate_frame_dimensions(photo_w, photo_h, config);
    if dims.skip_frame {
        return Ok(image.clone());
    }

    // モデルマッピング読み込み
    let mut model_map = crate::model_map::ModelMap::load_bundled();
    if let Some(ref map_path) = asset_dirs.user_model_map {
        if map_path.exists() {
            if let Ok(custom_json) = std::fs::read_to_string(map_path) {
                let _ = model_map.merge_custom(&custom_json);
            }
        }
    }

    // キャンバス生成
    let bg_color = config.color.to_rgba();
    let mut canvas = RgbaImage::from_pixel(dims.total_width, dims.total_height, bg_color);

    // 写真配置
    image::imageops::overlay(
        &mut canvas,
        &image.to_rgba8(),
        dims.photo_x as i64,
        dims.photo_y as i64,
    );

    // ロゴ描画（ユーザーディレクトリ優先 → バンドルフォールバック）
    if let Some(ref make) = exif.camera_make {
        let use_light = config.color.is_dark();
        if config.items.maker_logo {
            if let Some(logo_entry) = model_map.maker_logo(make) {
                if let Some(logo_img) = logo::resolve_and_load_logo(
                    asset_dirs.user_logos_dir.as_deref(),
                    &logo_entry.maker,
                    use_light,
                    dims.logo_size,
                ) {
                    image::imageops::overlay(
                        &mut canvas,
                        &logo_img.to_rgba8(),
                        dims.logo_x as i64,
                        dims.logo_y as i64,
                    );
                }
            }
        }
    }

    // テキスト描画
    // FontArcは内部でArcを持つためcloneは軽量。バンドルフォントはOnceLockキャッシュ済み
    let font = text::load_font(config.font.font_path.as_deref())
        .unwrap_or_else(|_| text::load_font(None).expect("bundled font must exist"));

    let short_side = photo_w.min(photo_h);
    let primary_size = short_side as f32 * config.font.primary_size;
    let secondary_size = short_side as f32 * config.font.secondary_size;
    let text_color = if config.color.is_dark() {
        Rgba([255, 255, 255, 255])
    } else {
        Rgba([51, 51, 51, 255])
    };
    let secondary_text_color = if config.color.is_dark() {
        Rgba([170, 170, 170, 255])
    } else {
        Rgba([136, 136, 136, 255])
    };

    // プライマリテキスト（カメラ + レンズ）
    let primary_text = build_primary_text(exif, &model_map, &config.items);
    if !primary_text.is_empty() {
        let max_width = (dims.total_width - dims.primary_text_x - 10) as f32;
        let truncated = text::truncate_text(&font, primary_size, &primary_text, max_width);
        text::draw_text_on_image(
            &mut canvas,
            &font,
            primary_size,
            &truncated,
            dims.primary_text_x as i32,
            dims.primary_text_y as i32,
            text_color,
        );
    }

    // セカンダリテキスト（撮影パラメータ）
    let secondary_text = build_secondary_text(exif, &config.items, &config.custom_text);
    if !secondary_text.is_empty() {
        let max_width = (dims.total_width - dims.secondary_text_x - 10) as f32;
        let truncated = text::truncate_text(&font, secondary_size, &secondary_text, max_width);
        text::draw_text_on_image(
            &mut canvas,
            &font,
            secondary_size,
            &truncated,
            dims.secondary_text_x as i32,
            dims.secondary_text_y as i32,
            secondary_text_color,
        );
    }

    Ok(DynamicImage::ImageRgba8(canvas))
}

fn build_primary_text(
    exif: &crate::ExifInfo,
    model_map: &crate::model_map::ModelMap,
    items: &DisplayItems,
) -> String {
    let mut parts = Vec::new();
    if items.camera_model {
        if let Some(ref model) = exif.camera_model {
            parts.push(model_map.camera_display_name(model).to_string());
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
    use image::{DynamicImage, Rgba, RgbaImage};
    use crate::ExifInfo;

    #[test]
    fn render_bottom_bar_white() {
        let img = DynamicImage::ImageRgba8(RgbaImage::from_pixel(800, 1000, Rgba([128, 128, 128, 255])));
        let exif = ExifInfo {
            camera_make: Some("SONY".to_string()),
            camera_model: Some("ILCE-7M4".to_string()),
            lens_model: Some("FE 24-70mm f/2.8 GM II".to_string()),
            focal_length: Some("35mm".to_string()),
            f_number: Some("f/2.8".to_string()),
            shutter_speed: Some("1/250s".to_string()),
            iso: Some(400),
            date_taken: None,
        };
        let config = ExifFrameConfig::default();
        let asset_dirs = AssetDirs::default();
        let result = render_exif_frame(&img, &exif, &config, &asset_dirs);
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.height() > img.height());
        let ratio = output.width() as f64 / output.height() as f64;
        assert!((ratio - 0.8).abs() < 0.02);
    }

    #[test]
    fn render_with_all_none_exif() {
        let img = DynamicImage::ImageRgba8(RgbaImage::from_pixel(800, 1000, Rgba([128, 128, 128, 255])));
        let exif = ExifInfo::default();
        let config = ExifFrameConfig::default();
        let asset_dirs = AssetDirs::default();
        let result = render_exif_frame(&img, &exif, &config, &asset_dirs);
        assert!(result.is_ok());
    }

    #[test]
    fn render_skips_for_tiny_image() {
        let img = DynamicImage::ImageRgba8(RgbaImage::from_pixel(100, 150, Rgba([128, 128, 128, 255])));
        let exif = ExifInfo::default();
        let config = ExifFrameConfig::default();
        let asset_dirs = AssetDirs::default();
        let result = render_exif_frame(&img, &exif, &config, &asset_dirs);
        assert!(result.is_ok());
        let output = result.unwrap();
        assert_eq!(output.width(), 100);
        assert_eq!(output.height(), 150);
    }

    #[test]
    fn exif_frame_config_json_roundtrip() {
        let config = ExifFrameConfig {
            name: "test".to_string(),
            layout: FrameLayout::BottomBar,
            color: FrameColor::White,
            aspect_ratio: OutputAspectRatio::Fixed(4, 5),
            items: DisplayItems::all_enabled(),
            font: FontConfig::default(),
            custom_text: "@user".to_string(),
            frame_padding: 0.05,
        };
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: ExifFrameConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "test");
        assert_eq!(deserialized.custom_text, "@user");
    }

    #[test]
    fn frame_color_custom_json() {
        let color = FrameColor::Custom(255, 128, 0);
        let json = serde_json::to_string(&color).unwrap();
        let deserialized: FrameColor = serde_json::from_str(&json).unwrap();
        match deserialized {
            FrameColor::Custom(r, g, b) => {
                assert_eq!((r, g, b), (255, 128, 0));
            }
            _ => panic!("Expected Custom variant"),
        }
    }

    #[test]
    fn output_aspect_ratio_json() {
        let fixed = OutputAspectRatio::Fixed(4, 5);
        let json = serde_json::to_string(&fixed).unwrap();
        assert!(json.contains("fixed"));

        let free = OutputAspectRatio::Free;
        let json = serde_json::to_string(&free).unwrap();
        let deserialized: OutputAspectRatio = serde_json::from_str(&json).unwrap();
        matches!(deserialized, OutputAspectRatio::Free);
    }
}
