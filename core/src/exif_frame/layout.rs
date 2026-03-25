// 3レイアウトの座標計算と描画

use crate::exif_frame::{ExifFrameConfig, FrameLayout, OutputAspectRatio};

/// レイアウト計算結果
#[derive(Debug)]
pub struct FrameDimensions {
    /// 最終出力幅
    pub total_width: u32,
    /// 最終出力高さ
    pub total_height: u32,
    /// 写真の配置X座標
    pub photo_x: u32,
    /// 写真の配置Y座標
    pub photo_y: u32,
    /// フレーム領域の高さ（BottomBar, FullBorder用）
    pub frame_height: u32,
    /// フレーム領域の幅（SideBar用）
    pub frame_width: u32,
    /// ロゴの配置座標とサイズ
    pub logo_x: u32,
    pub logo_y: u32,
    pub logo_size: u32,
    /// テキストの配置座標
    pub primary_text_x: u32,
    pub primary_text_y: u32,
    pub secondary_text_x: u32,
    pub secondary_text_y: u32,
    /// フレーム付加をスキップすべきか
    pub skip_frame: bool,
}

const MIN_SHORT_SIDE: u32 = 200;

/// レイアウトに応じたフレーム寸法を計算
pub fn calculate_frame_dimensions(
    photo_width: u32,
    photo_height: u32,
    config: &ExifFrameConfig,
) -> FrameDimensions {
    let short_side = photo_width.min(photo_height);

    // 極小画像チェック
    if short_side < MIN_SHORT_SIDE {
        return FrameDimensions {
            total_width: photo_width,
            total_height: photo_height,
            photo_x: 0,
            photo_y: 0,
            frame_height: 0,
            frame_width: 0,
            logo_x: 0,
            logo_y: 0,
            logo_size: 0,
            primary_text_x: 0,
            primary_text_y: 0,
            secondary_text_x: 0,
            secondary_text_y: 0,
            skip_frame: true,
        };
    }

    let padding = (short_side as f32 * config.frame_padding) as u32;

    let (mut total_w, mut total_h, photo_x, photo_y, frame_h, frame_w) = match config.layout {
        FrameLayout::BottomBar => {
            let frame_h = padding;
            (photo_width, photo_height + frame_h, 0, 0, frame_h, 0)
        }
        FrameLayout::SideBar => {
            let frame_w = padding * 2; // サイドバーは幅方向に2倍
            (photo_width + frame_w, photo_height, 0, 0, 0, frame_w)
        }
        FrameLayout::FullBorder => {
            let frame_h = padding; // 下部EXIF領域
            (
                photo_width + padding * 2,
                photo_height + padding * 2 + frame_h,
                padding,
                padding,
                frame_h,
                0,
            )
        }
    };

    // アスペクト比調整
    if let OutputAspectRatio::Fixed(target_w, target_h) = config.aspect_ratio {
        let target_ratio = target_w as f64 / target_h as f64;
        let current_ratio = total_w as f64 / total_h as f64;
        if current_ratio > target_ratio {
            // 幅が広すぎる → 高さを増やす
            total_h = (total_w as f64 / target_ratio) as u32;
        } else {
            // 高さが高すぎる → 幅を増やす
            total_w = (total_h as f64 * target_ratio) as u32;
        }
    }

    // ロゴ・テキスト配置（レイアウト依存）
    let logo_size = (frame_h.max(frame_w) as f32 * 0.6) as u32;
    let (logo_x, logo_y) = match config.layout {
        FrameLayout::BottomBar => {
            (padding / 2, photo_height + (frame_h.saturating_sub(logo_size)) / 2)
        }
        FrameLayout::SideBar => {
            let center_x = photo_width + (frame_w.saturating_sub(logo_size)) / 2;
            (center_x, padding)
        }
        FrameLayout::FullBorder => {
            (padding + padding / 2, photo_height + padding * 2 + (frame_h.saturating_sub(logo_size)) / 2)
        }
    };

    let text_offset_x = logo_x + logo_size + padding / 2;
    let (primary_text_x, primary_text_y, secondary_text_x, secondary_text_y) = match config.layout {
        FrameLayout::BottomBar | FrameLayout::FullBorder => {
            (text_offset_x, logo_y, text_offset_x, logo_y + logo_size / 2)
        }
        FrameLayout::SideBar => {
            let x = photo_width + frame_w / 4;
            (x, logo_y + logo_size + padding / 2, x, logo_y + logo_size + padding)
        }
    };

    FrameDimensions {
        total_width: total_w,
        total_height: total_h,
        photo_x,
        photo_y,
        frame_height: frame_h,
        frame_width: frame_w,
        logo_x,
        logo_y,
        logo_size,
        primary_text_x,
        primary_text_y,
        secondary_text_x,
        secondary_text_y,
        skip_frame: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exif_frame::*;

    fn test_config(layout: FrameLayout) -> ExifFrameConfig {
        ExifFrameConfig {
            layout,
            frame_padding: 0.05,
            aspect_ratio: OutputAspectRatio::Free,
            ..ExifFrameConfig::default()
        }
    }

    #[test]
    fn bottom_bar_dimensions() {
        let config = test_config(FrameLayout::BottomBar);
        let dims = calculate_frame_dimensions(1000, 1250, &config);
        // 短辺1000 × padding 0.05 = 50px のフレーム高さ
        assert_eq!(dims.frame_height, 50);
        assert_eq!(dims.total_width, 1000);
        assert_eq!(dims.total_height, 1250 + 50);
        assert_eq!(dims.photo_x, 0);
        assert_eq!(dims.photo_y, 0);
    }

    #[test]
    fn sidebar_dimensions() {
        let config = test_config(FrameLayout::SideBar);
        let dims = calculate_frame_dimensions(1000, 1250, &config);
        let sidebar_width = (1000.0 * 0.05 * 2.0) as u32; // サイドバーは幅方向
        assert_eq!(dims.total_width, 1000 + sidebar_width);
        assert_eq!(dims.total_height, 1250);
    }

    #[test]
    fn full_border_dimensions() {
        let config = test_config(FrameLayout::FullBorder);
        let dims = calculate_frame_dimensions(1000, 1250, &config);
        let padding = 50; // 短辺1000 × 0.05
        // 上下左右にパディング + 下部にEXIF情報エリア
        assert_eq!(dims.total_width, 1000 + padding * 2);
        assert!(dims.total_height > 1250 + padding * 2); // EXIF分が追加
    }

    #[test]
    fn fixed_aspect_ratio_adjustment() {
        let config = ExifFrameConfig {
            aspect_ratio: OutputAspectRatio::Fixed(4, 5),
            ..test_config(FrameLayout::BottomBar)
        };
        let dims = calculate_frame_dimensions(1000, 1250, &config);
        let ratio = dims.total_width as f64 / dims.total_height as f64;
        let target = 4.0 / 5.0;
        assert!((ratio - target).abs() < 0.01,
            "Expected ratio ~{}, got {}", target, ratio);
    }

    #[test]
    fn free_aspect_ratio_no_adjustment() {
        let config = ExifFrameConfig {
            aspect_ratio: OutputAspectRatio::Free,
            ..test_config(FrameLayout::BottomBar)
        };
        let dims = calculate_frame_dimensions(1000, 1250, &config);
        // Freeなら写真幅と同じ
        assert_eq!(dims.total_width, 1000);
    }

    #[test]
    fn minimum_image_skips_frame() {
        let config = test_config(FrameLayout::BottomBar);
        // 短辺200px未満はフレームスキップ
        let dims = calculate_frame_dimensions(150, 200, &config);
        assert!(dims.skip_frame);
    }
}
