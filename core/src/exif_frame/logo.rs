use anyhow::{Context, Result};
use image::DynamicImage;
use rust_embed::Embed;
use std::path::{Path, PathBuf};
use usvg::TreeParsing;

/// バンドルロゴのみを埋め込む
#[derive(Embed)]
#[folder = "assets/logos/"]
struct LogoAssets;

/// バンドルロゴからDynamicImageを読み込み
pub fn load_bundled_logo(filename: &str, target_size: u32) -> Result<DynamicImage> {
    let data = LogoAssets::get(filename)
        .with_context(|| format!("bundled logo not found: {}", filename))?;
    let ext = filename.rsplit('.').next().unwrap_or("");
    match ext {
        "svg" => load_svg_from_bytes(&data.data, target_size),
        _ => {
            let img = image::load_from_memory(&data.data)
                .context("failed to decode bundled logo")?;
            Ok(img.resize(target_size, target_size, image::imageops::FilterType::Lanczos3))
        }
    }
}

/// ロゴファイルを読み込み、指定サイズにリサイズ（アスペクト比維持）
pub fn load_logo_file(path: &Path, target_size: u32) -> Result<DynamicImage> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let img = match ext.to_lowercase().as_str() {
        "svg" => load_svg(path, target_size)?,
        _ => {
            let img = image::open(path).context("failed to open logo file")?;
            img.resize(target_size, target_size, image::imageops::FilterType::Lanczos3)
        }
    };
    Ok(img)
}

fn load_svg(path: &Path, target_size: u32) -> Result<DynamicImage> {
    let svg_data = std::fs::read(path)?;
    load_svg_from_bytes(&svg_data, target_size)
}

fn load_svg_from_bytes(svg_data: &[u8], target_size: u32) -> Result<DynamicImage> {
    let options = usvg::Options::default();
    let utree = usvg::Tree::from_data(svg_data, &options)?;
    let rtree = resvg::Tree::from_usvg(&utree);

    let orig_size = rtree.size;
    let orig_w = orig_size.width() as f32;
    let orig_h = orig_size.height() as f32;
    let scale = target_size as f32 / orig_w.max(orig_h);
    let width = (orig_w * scale).round() as u32;
    let height = (orig_h * scale).round() as u32;

    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)
        .context("failed to create pixmap")?;
    let transform = resvg::tiny_skia::Transform::from_scale(scale, scale);
    rtree.render(transform, &mut pixmap.as_mut());

    let rgba = image::RgbaImage::from_raw(width, height, pixmap.data().to_vec())
        .context("failed to create image from pixmap")?;
    Ok(DynamicImage::ImageRgba8(rgba))
}

/// ロゴファイルのパスを解決（SVG優先、lightバリアント対応）
pub fn resolve_logo_file(
    dir: Option<&Path>,
    base_name: &str,
    use_light: bool,
) -> Option<PathBuf> {
    let dir = dir?;
    if !dir.exists() {
        return None;
    }

    let candidates = if use_light {
        vec![
            format!("{}_light.svg", base_name),
            format!("{}_light.png", base_name),
            format!("{}.svg", base_name),
            format!("{}.png", base_name),
        ]
    } else {
        vec![
            format!("{}.svg", base_name),
            format!("{}.png", base_name),
        ]
    };

    for candidate in candidates {
        let path = dir.join(&candidate);
        if path.exists() {
            return Some(path);
        }
    }
    None
}

/// レンズモデル名からレンズブランドロゴを解決して読み込む
pub fn resolve_lens_brand_logo(
    user_dir: Option<&Path>,
    lens_model: &str,
    model_map: &crate::model_map::ModelMap,
    use_light: bool,
    target_size: u32,
) -> Option<DynamicImage> {
    model_map
        .lens_brand_logo(lens_model)
        .and_then(|logo_file| resolve_and_load_logo(user_dir, logo_file, use_light, target_size))
}

/// ロゴを解決（ユーザーディレクトリ優先 → バンドルフォールバック）
pub fn resolve_and_load_logo(
    user_dir: Option<&Path>,
    filename: &str,
    use_light: bool,
    target_size: u32,
) -> Option<DynamicImage> {
    let base_name = filename.trim_end_matches(".svg").trim_end_matches(".png");

    // 1. ユーザーディレクトリから検索
    if let Some(path) = resolve_logo_file(user_dir, base_name, use_light) {
        if let Ok(img) = load_logo_file(&path, target_size) {
            return Some(img);
        }
    }

    // 2. バンドルロゴからフォールバック
    let candidates = if use_light {
        vec![
            format!("{}_light.svg", base_name),
            format!("{}_light.png", base_name),
            format!("{}.svg", base_name),
            format!("{}.png", base_name),
        ]
    } else {
        vec![
            format!("{}.svg", base_name),
            format!("{}.png", base_name),
        ]
    };
    for candidate in candidates {
        if let Ok(img) = load_bundled_logo(&candidate, target_size) {
            return Some(img);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn load_svg_logo() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/test_logo.svg");
        let img = load_logo_file(&path, 64).unwrap();
        assert_eq!(img.width(), 64);
        assert_eq!(img.height(), 64);
    }

    #[test]
    fn load_png_logo() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.png");
        let img = image::RgbaImage::from_pixel(100, 100, image::Rgba([255, 0, 0, 255]));
        img.save(&path).unwrap();
        let loaded = load_logo_file(&path, 32).unwrap();
        assert_eq!(loaded.width(), 32);
        assert_eq!(loaded.height(), 32);
    }

    #[test]
    fn resolve_logo_prefers_svg() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("test.svg"),
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10" fill="blue"/></svg>"#,
        )
        .unwrap();
        let png = image::RgbaImage::from_pixel(10, 10, image::Rgba([255, 0, 0, 255]));
        png.save(dir.path().join("test.png")).unwrap();
        let resolved = resolve_logo_file(Some(dir.path()), "test", false);
        assert!(resolved.is_some());
        assert!(resolved.unwrap().to_str().unwrap().ends_with(".svg"));
    }

    #[test]
    fn resolve_logo_light_variant() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("test_light.svg"),
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10" fill="white"/></svg>"#,
        )
        .unwrap();
        std::fs::write(
            dir.path().join("test.svg"),
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10" fill="black"/></svg>"#,
        )
        .unwrap();
        let resolved = resolve_logo_file(Some(dir.path()), "test", true);
        assert!(resolved.unwrap().to_str().unwrap().contains("_light"));
    }
}
