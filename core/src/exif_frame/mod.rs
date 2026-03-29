pub mod layout;
pub mod logo;
pub mod preset;
pub mod text;

use anyhow::Result;
use image::DynamicImage;
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
    // TODO: Task 6 で実装。現時点では画像をそのまま返す
    let _ = (exif, config, bg_color, asset_dirs);
    Ok(image.clone())
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
