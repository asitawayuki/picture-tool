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

#[cfg(test)]
mod tests {
    use super::*;

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
}
