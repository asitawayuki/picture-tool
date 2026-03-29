use ab_glyph::{Font, FontArc, Glyph, PxScale, ScaleFont, point};
use anyhow::{Context, Result};
use image::{Rgba, RgbaImage};
use rust_embed::Embed;
use std::sync::OnceLock;

#[derive(Embed)]
#[folder = "assets/fonts/"]
struct FontAssets;

static BUNDLED_FONT: OnceLock<FontArc> = OnceLock::new();

pub fn load_font(path: Option<&str>) -> Result<FontArc> {
    match path {
        Some(p) => {
            let data = std::fs::read(p).context("failed to read font file")?;
            FontArc::try_from_vec(data).map_err(|_| anyhow::anyhow!("invalid font file"))
        }
        None => {
            let font = BUNDLED_FONT.get_or_init(|| {
                let font_file = FontAssets::iter()
                    .find(|f| f.ends_with(".ttf") || f.ends_with(".otf"))
                    .expect("no bundled font found");
                let data = FontAssets::get(&font_file)
                    .expect("failed to load bundled font");
                FontArc::try_from_vec(data.data.to_vec())
                    .expect("invalid bundled font")
            });
            Ok(font.clone())
        }
    }
}

pub fn measure_text_width(font: &FontArc, size: f32, text: &str) -> f32 {
    if text.is_empty() {
        return 0.0;
    }
    let scaled = font.as_scaled(PxScale::from(size));
    let mut width = 0.0;
    let mut prev = None;
    for ch in text.chars() {
        let glyph_id = scaled.glyph_id(ch);
        if let Some(prev_id) = prev {
            width += scaled.kern(prev_id, glyph_id);
        }
        width += scaled.h_advance(glyph_id);
        prev = Some(glyph_id);
    }
    width
}

pub fn truncate_text(font: &FontArc, size: f32, text: &str, max_width: f32) -> String {
    let full_width = measure_text_width(font, size, text);
    if full_width <= max_width {
        return text.to_string();
    }
    let ellipsis = "...";
    let ellipsis_width = measure_text_width(font, size, ellipsis);
    let target_width = max_width - ellipsis_width;
    if target_width <= 0.0 {
        return ellipsis.to_string();
    }
    let mut result = String::new();
    let scaled = font.as_scaled(PxScale::from(size));
    let mut width = 0.0;
    let mut prev = None;
    for ch in text.chars() {
        let glyph_id = scaled.glyph_id(ch);
        if let Some(prev_id) = prev {
            width += scaled.kern(prev_id, glyph_id);
        }
        width += scaled.h_advance(glyph_id);
        if width > target_width {
            break;
        }
        result.push(ch);
        prev = Some(glyph_id);
    }
    result.push_str(ellipsis);
    result
}

pub fn draw_text_on_image(
    img: &mut RgbaImage,
    font: &FontArc,
    size: f32,
    text: &str,
    x: i32,
    y: i32,
    color: Rgba<u8>,
) {
    let scaled = font.as_scaled(PxScale::from(size));
    let ascent = scaled.ascent();
    let img_width = img.width() as i32;
    let img_height = img.height() as i32;

    let mut cursor_x = 0.0f32;
    let mut prev_glyph_id = None;

    for ch in text.chars() {
        let glyph_id = scaled.glyph_id(ch);

        if let Some(prev_id) = prev_glyph_id {
            cursor_x += scaled.kern(prev_id, glyph_id);
        }

        let glyph: Glyph = glyph_id.with_scale_and_position(
            PxScale::from(size),
            point(cursor_x, ascent),
        );

        if let Some(outlined) = font.outline_glyph(glyph) {
            let bounds = outlined.px_bounds();
            outlined.draw(|gx, gy, coverage| {
                if coverage <= 0.0 {
                    return;
                }
                let px = x + bounds.min.x as i32 + gx as i32;
                let py = y + bounds.min.y as i32 + gy as i32;
                if px < 0 || py < 0 || px >= img_width || py >= img_height {
                    return;
                }
                let existing = img.get_pixel(px as u32, py as u32);
                let alpha = coverage * (color[3] as f32 / 255.0);
                let inv = 1.0 - alpha;
                let blended = Rgba([
                    (color[0] as f32 * alpha + existing[0] as f32 * inv) as u8,
                    (color[1] as f32 * alpha + existing[1] as f32 * inv) as u8,
                    (color[2] as f32 * alpha + existing[2] as f32 * inv) as u8,
                    (255.0_f32.min(existing[3] as f32 + coverage * color[3] as f32)) as u8,
                ]);
                img.put_pixel(px as u32, py as u32, blended);
            });
        }

        cursor_x += scaled.h_advance(glyph_id);
        prev_glyph_id = Some(glyph_id);
    }
}

/// テキストを90度時計回りに回転して描画
/// (center_x, center_y) を中心に、テキストが右方向を向くよう回転
pub fn draw_text_rotated_90(
    img: &mut RgbaImage,
    font: &FontArc,
    size: f32,
    text: &str,
    center_x: i32,
    center_y: i32,
    color: Rgba<u8>,
) {
    if text.is_empty() {
        return;
    }

    let scaled = font.as_scaled(PxScale::from(size));
    let text_width = measure_text_width(font, size, text).ceil() as u32 + 2;
    // height: ascent + descent (line height)
    let ascent = scaled.ascent();
    let descent = scaled.descent().abs();
    let text_height = (ascent + descent).ceil() as u32 + 2;

    if text_width == 0 || text_height == 0 {
        return;
    }

    // 水平バッファにテキストを描画
    let mut buf = RgbaImage::new(text_width, text_height);
    draw_text_on_image(&mut buf, font, size, text, 0, 0, color);

    // 90度時計回り回転: (x, y) → (text_height-1-y, x)
    // 回転後のサイズ: 幅=text_height, 高さ=text_width
    let rot_w = text_height;
    let rot_h = text_width;

    // ターゲット画像への配置オフセット（中心合わせ）
    let offset_x = center_x - rot_w as i32 / 2;
    let offset_y = center_y - rot_h as i32 / 2;

    let img_w = img.width() as i32;
    let img_h = img.height() as i32;

    for y in 0..text_height {
        for x in 0..text_width {
            let pixel = buf.get_pixel(x, y);
            if pixel[3] == 0 {
                continue;
            }
            // 90度時計回り回転後の座標
            let rx = (text_height - 1 - y) as i32;
            let ry = x as i32;

            let px = offset_x + rx;
            let py = offset_y + ry;

            if px < 0 || py < 0 || px >= img_w || py >= img_h {
                continue;
            }

            // アルファブレンド
            let existing = img.get_pixel(px as u32, py as u32);
            let alpha = pixel[3] as f32 / 255.0;
            let inv = 1.0 - alpha;
            let blended = Rgba([
                (pixel[0] as f32 * alpha + existing[0] as f32 * inv) as u8,
                (pixel[1] as f32 * alpha + existing[1] as f32 * inv) as u8,
                (pixel[2] as f32 * alpha + existing[2] as f32 * inv) as u8,
                255u8.min((existing[3] as f32 + pixel[3] as f32 * alpha) as u8),
            ]);
            img.put_pixel(px as u32, py as u32, blended);
        }
    }
}

/// テキストを指定幅に収める（フォント縮小 → 省略）
/// min_size_ratio: primary_size に対する最小比率（例: 0.7 = 70%まで縮小）
/// 戻り値: (収まったテキスト, 最終フォントサイズ)
pub fn auto_fit_text(
    font: &FontArc,
    size: f32,
    text: &str,
    max_width: f32,
    min_size_ratio: f32,
) -> (String, f32) {
    let min_size = size * min_size_ratio;

    // 0.5 刻みでフォントサイズを縮小しながらフィット確認
    let mut current_size = size;
    loop {
        let width = measure_text_width(font, current_size, text);
        if width <= max_width {
            return (text.to_string(), current_size);
        }
        let next_size = current_size - 0.5;
        if next_size < min_size {
            break;
        }
        current_size = next_size;
    }

    // min_size でも収まらない場合は truncate_text で省略
    let truncated = truncate_text(font, min_size, text, max_width);
    (truncated, min_size)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::RgbaImage;

    fn test_font() -> FontArc {
        load_font(None).expect("bundled font should load")
    }

    #[test]
    fn load_bundled_font() {
        let font = load_font(None).unwrap();
        let scaled = font.as_scaled(PxScale::from(24.0));
        assert!(scaled.ascent() > 0.0);
    }

    #[test]
    fn measure_text_non_zero() {
        let font = load_font(None).unwrap();
        let width = measure_text_width(&font, 24.0, "Hello World");
        assert!(width > 0.0);
    }

    #[test]
    fn measure_empty_text() {
        let font = load_font(None).unwrap();
        let width = measure_text_width(&font, 24.0, "");
        assert_eq!(width, 0.0);
    }

    #[test]
    fn truncate_long_text() {
        let font = load_font(None).unwrap();
        let result = truncate_text(&font, 24.0, "This is a very long text that should be truncated", 100.0);
        assert!(result.ends_with("..."));
        let width = measure_text_width(&font, 24.0, &result);
        assert!(width <= 100.0 + 1.0);
    }

    #[test]
    fn draw_text_no_panic() {
        let mut img = image::RgbaImage::new(200, 50);
        let font = load_font(None).unwrap();
        draw_text_on_image(&mut img, &font, 16.0, "Test Text", 10, 10, image::Rgba([0, 0, 0, 255]));
    }

    // --- New tests for draw_text_rotated_90 and auto_fit_text ---

    #[test]
    fn draw_rotated_text_does_not_panic() {
        let font = test_font();
        let mut img = RgbaImage::new(200, 800);
        draw_text_rotated_90(
            &mut img, &font, 16.0,
            "ILCE-7M4 | FE 24-70mm F2.8 GM II | 35mm f/2.8",
            100, 400,
            Rgba([255, 255, 255, 255]),
        );
    }

    #[test]
    fn measure_text_width_positive() {
        let font = test_font();
        let width = measure_text_width(&font, 20.0, "Hello");
        assert!(width > 0.0);
    }

    #[test]
    fn truncate_text_with_ellipsis() {
        let font = test_font();
        let full_width = measure_text_width(&font, 20.0, "Very long text that should be truncated");
        let truncated = truncate_text(&font, 20.0, "Very long text that should be truncated", full_width * 0.5);
        assert!(truncated.ends_with("..."));
        assert!(truncated.len() < "Very long text that should be truncated".len());
    }

    #[test]
    fn auto_fit_text_shrinks_font() {
        let font = test_font();
        let text = "ILCE-7M4 | FE 24-70mm F2.8 GM II | 35mm f/2.8 1/250s ISO400";
        let narrow_width = 200.0;
        let (fitted, final_size) = auto_fit_text(&font, 20.0, text, narrow_width, 0.7);
        assert!(final_size < 20.0, "Font size should be reduced");
        let fitted_width = measure_text_width(&font, final_size, &fitted);
        assert!(fitted_width <= narrow_width + 1.0);
    }

    #[test]
    fn auto_fit_text_no_shrink_needed() {
        let font = test_font();
        let text = "Hi";
        let (fitted, final_size) = auto_fit_text(&font, 20.0, text, 500.0, 0.7);
        assert_eq!(fitted, "Hi");
        assert!((final_size - 20.0).abs() < 0.01, "Font size should not change");
    }

    #[test]
    fn draw_rotated_text_pixels_within_bounds() {
        let font = test_font();
        let mut img = RgbaImage::new(100, 500);
        draw_text_rotated_90(
            &mut img, &font, 14.0, "Test text",
            50, 250, Rgba([255, 0, 0, 255]),
        );
        // Should not panic - bounds checking is implicit
    }
}
