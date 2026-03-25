pub mod layout;
pub mod logo;
pub mod preset;
pub mod text;

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

#[cfg(test)]
mod tests {
    use super::*;

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
