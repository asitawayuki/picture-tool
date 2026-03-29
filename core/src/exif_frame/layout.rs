// レイアウト計算（Task 3 で本実装予定）

use crate::exif_frame::ExifFrameConfig;

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
    /// フレーム領域の高さ
    pub frame_height: u32,
    /// フレーム領域の幅
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

/// レイアウトに応じたフレーム寸法を計算（TODO: Task 3 で本実装）
pub fn calculate_frame_dimensions(
    photo_width: u32,
    photo_height: u32,
    _config: &ExifFrameConfig,
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

    // TODO: Task 3 で ExifPosition に基づく本実装を行う
    // 現時点ではボトムバー相当の固定レイアウトを返す
    let padding = (short_side as f32 * 0.05) as u32;
    let frame_h = padding;
    let total_w = photo_width;
    let total_h = photo_height + frame_h;

    let logo_size = (frame_h as f32 * 0.6) as u32;
    let logo_x = padding / 2;
    let logo_y = photo_height + (frame_h.saturating_sub(logo_size)) / 2;

    let text_offset_x = logo_x + logo_size + padding / 2;

    FrameDimensions {
        total_width: total_w,
        total_height: total_h,
        photo_x: 0,
        photo_y: 0,
        frame_height: frame_h,
        frame_width: 0,
        logo_x,
        logo_y,
        logo_size,
        primary_text_x: text_offset_x,
        primary_text_y: logo_y,
        secondary_text_x: text_offset_x,
        secondary_text_y: logo_y + logo_size / 2,
        skip_frame: false,
    }
}
