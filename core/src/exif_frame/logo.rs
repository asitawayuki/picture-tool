use anyhow::{Context, Result};
use image::DynamicImage;
use rust_embed::Embed;
use std::path::{Path, PathBuf};

/// バンドルロゴのみを埋め込む
#[derive(Embed)]
#[folder = "assets/logos/"]
struct LogoAssets;

/// バンドルロゴからDynamicImageを読み込み
///
/// `max` は（最大幅, 最大高さ）。アスペクト比を保ったまま、この箱に収まる大きさで読む。
/// SVG は表示する寸法で直接ラスタライズし、PNG は縮小 1 回だけで収める（再拡大しない）。
pub fn load_bundled_logo(filename: &str, max: (u32, u32)) -> Result<DynamicImage> {
    let data = LogoAssets::get(filename)
        .with_context(|| format!("bundled logo not found: {}", filename))?;
    let ext = filename.rsplit('.').next().unwrap_or("");
    match ext {
        "svg" => load_svg_from_bytes(&data.data, max),
        _ => {
            let img =
                image::load_from_memory(&data.data).context("failed to decode bundled logo")?;
            Ok(img.resize(max.0, max.1, image::imageops::FilterType::Lanczos3))
        }
    }
}

/// ロゴファイルを読み込み、`max`（最大幅, 最大高さ）に収める（アスペクト比維持）
pub fn load_logo_file(path: &Path, max: (u32, u32)) -> Result<DynamicImage> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let img = match ext.to_lowercase().as_str() {
        "svg" => load_svg(path, max)?,
        _ => {
            let img = image::open(path).context("failed to open logo file")?;
            img.resize(max.0, max.1, image::imageops::FilterType::Lanczos3)
        }
    };
    Ok(img)
}

fn load_svg(path: &Path, max: (u32, u32)) -> Result<DynamicImage> {
    let svg_data = std::fs::read(path)?;
    load_svg_from_bytes(&svg_data, max)
}

fn load_svg_from_bytes(svg_data: &[u8], max: (u32, u32)) -> Result<DynamicImage> {
    let options = usvg::Options::default();
    let tree = usvg::Tree::from_data(svg_data, &options)?;

    let orig_size = tree.size();
    let orig_w = orig_size.width();
    let orig_h = orig_size.height();
    // 幅・高さが0のSVGだとスケールが NaN / Inf になり、そのまま as u32 で
    // 0 に潰れて意味不明な "failed to create pixmap" になる。ここで弾く。
    if !(orig_w.is_finite() && orig_h.is_finite()) || orig_w <= 0.0 || orig_h <= 0.0 {
        anyhow::bail!("svg has a non-positive canvas size ({}x{})", orig_w, orig_h);
    }
    if max.0 == 0 || max.1 == 0 {
        anyhow::bail!("svg target size must be greater than 0");
    }
    // 箱（最大幅 × 最大高さ）に収まる倍率。極端に横長でも幅は max.0 で打ち切られる
    let scale = (max.0 as f32 / orig_w).min(max.1 as f32 / orig_h);
    let width = (orig_w * scale).round() as u32;
    let height = (orig_h * scale).round() as u32;

    let mut pixmap = resvg::tiny_skia::Pixmap::new(width.max(1), height.max(1))
        .context("failed to create pixmap")?;
    let (width, height) = (width.max(1), height.max(1));
    let transform = resvg::tiny_skia::Transform::from_scale(scale, scale);
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    // tiny-skia の画素はプリマルチプライド。`RgbaImage` はストレートアルファなので
    // demultiply しないと、縁の RGB が α 倍に暗く痩せる（白ロゴで目立つ）
    let mut data = Vec::with_capacity(pixmap.pixels().len() * 4);
    for p in pixmap.pixels() {
        let c = p.demultiply();
        data.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }
    let rgba = image::RgbaImage::from_raw(width, height, data)
        .context("failed to create image from pixmap")?;
    Ok(DynamicImage::ImageRgba8(rgba))
}

/// ロゴファイルのパスを解決（SVG優先、lightバリアント対応）
pub fn resolve_logo_file(dir: Option<&Path>, base_name: &str, use_light: bool) -> Option<PathBuf> {
    let dir = dir?;
    // base_name はユーザーが差し替え可能な model_map から来るため、
    // `dir.join()` に渡す前に単純なファイル名であることを保証する（S4-M8）。
    crate::model_map::validate_asset_filename(base_name).ok()?;
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
        vec![format!("{}.svg", base_name), format!("{}.png", base_name)]
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
    max: (u32, u32),
) -> Option<DynamicImage> {
    model_map
        .lens_brand_logo(lens_model)
        .and_then(|logo_file| resolve_and_load_logo(user_dir, logo_file, use_light, max))
}

/// ロゴを解決（ユーザーディレクトリ優先 → バンドルフォールバック）
pub fn resolve_and_load_logo(
    user_dir: Option<&Path>,
    filename: &str,
    use_light: bool,
    max: (u32, u32),
) -> Option<DynamicImage> {
    let base_name = filename.trim_end_matches(".svg").trim_end_matches(".png");
    // pub API なので model_map 側の検証に依存せず、ここでも自衛する（S4-M8）
    crate::model_map::validate_asset_filename(base_name).ok()?;

    // 1. ユーザーディレクトリから検索
    if let Some(path) = resolve_logo_file(user_dir, base_name, use_light)
        && let Ok(img) = load_logo_file(&path, max)
    {
        return Some(img);
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
        vec![format!("{}.svg", base_name), format!("{}.png", base_name)]
    };
    for candidate in candidates {
        if let Ok(img) = load_bundled_logo(&candidate, max) {
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
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/test_logo.svg");
        let img = load_logo_file(&path, (64, 64)).unwrap();
        assert_eq!(img.width(), 64);
        assert_eq!(img.height(), 64);
    }

    #[test]
    fn load_png_logo() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.png");
        let img = image::RgbaImage::from_pixel(100, 100, image::Rgba([255, 0, 0, 255]));
        img.save(&path).unwrap();
        let loaded = load_logo_file(&path, (32, 32)).unwrap();
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

    /// `model_map.json` が参照するロゴは、ユーザーが何も配置していなくても
    /// バンドルアセットだけで解決できなければならない
    /// （そうでないとメーカーロゴ項目を有効にしても静かに何も描画されない）。
    /// さらに背景色は White / Black の双方を選べる仕様なので、
    /// 暗い背景で不可視にならないよう `_light` バリアントも実在する必要がある。
    #[test]
    fn every_logo_referenced_by_model_map_is_bundled() {
        let json_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/model_map.json");
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&json_path).unwrap()).unwrap();

        let mut referenced: Vec<String> = json["logo_match"]
            .as_object()
            .expect("logo_match must be an object")
            .values()
            .map(|entry| entry["maker"].as_str().unwrap().to_string())
            .collect();
        referenced.extend(
            json["lens_brand_match"]
                .as_array()
                .expect("lens_brand_match must be an array")
                .iter()
                .map(|rule| rule["logo"].as_str().unwrap().to_string()),
        );
        assert!(!referenced.is_empty());

        for filename in referenced {
            let (stem, ext) = filename.rsplit_once('.').expect("logo needs an extension");
            // `resolve_and_load_logo` は light 欠落時に base へフォールバックするため、
            // light バリアントの実在は個別に検証する
            for candidate in [filename.clone(), format!("{}_light.{}", stem, ext)] {
                assert!(
                    load_bundled_logo(&candidate, (64, 64)).is_ok(),
                    "model_map.json references {:?} but bundled asset {:?} is missing or undecodable",
                    filename,
                    candidate
                );
            }
        }
    }

    /// 仕様: ロゴ名はアセットディレクトリ内のファイルしか指せない（S4-M8）。
    /// `model_map_custom.json` はユーザーが書き換えられるので、
    /// `"../.."` を書かれてディレクトリの外を読めてはならない。
    #[test]
    fn logo_names_cannot_escape_the_asset_directory() {
        let root = tempfile::TempDir::new().unwrap();
        let logos = root.path().join("logos");
        std::fs::create_dir(&logos).unwrap();
        // ロゴディレクトリの「外」に、狙われうるファイルを置く
        let outside = root.path().join("secret.png");
        image::RgbaImage::from_pixel(4, 4, image::Rgba([1, 2, 3, 255]))
            .save(&outside)
            .unwrap();

        for evil in ["../secret", "../../secret", "/etc/passwd", "sub/secret"] {
            assert!(
                resolve_logo_file(Some(&logos), evil, false).is_none(),
                "{:?} must not resolve to a path outside the logo directory",
                evil
            );
            assert!(
                resolve_and_load_logo(Some(&logos), evil, false, (32, 32)).is_none(),
                "{:?} must not load anything",
                evil
            );
        }
    }

    /// 仕様: 壊れたSVG（幅・高さが0）はエラーとして扱う。
    /// ゼロ除算で scale が NaN/Inf になったまま先へ進めてはならない（S4-L4）。
    #[test]
    fn zero_sized_svg_is_rejected() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="0" height="0"></svg>"#;
        let result = load_svg_from_bytes(svg, (64, 64));
        assert!(result.is_err(), "zero-sized svg must be an error");
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

    /// 仕様: 横長の SVG は表示する高さで直接ラスタライズする（再拡大しない）。
    /// 長辺 = 幅で読むと高さが 1/5.68 になり、後で約 4.5 倍に拡大されていた。
    #[test]
    fn wide_svg_logo_is_rasterized_at_the_requested_height() {
        let img = load_bundled_logo("sony.svg", (10_000, 19)).unwrap();
        assert_eq!(img.height(), 19);
        let expected_w = 19.0 * 5.68;
        assert!(
            (img.width() as f32 - expected_w).abs() <= 2.0,
            "width {} should be about {}",
            img.width(),
            expected_w
        );
    }

    /// 仕様（Review Focus 4）: 極端に横長でも幅はバーの長さで打ち切られる。
    #[test]
    fn extremely_wide_svg_is_bounded_by_the_bar_length() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="1000" height="1"><rect width="1000" height="1" fill="black"/></svg>"#;
        let img = load_svg_from_bytes(svg, (500, 40)).unwrap();
        assert!(img.width() <= 500, "width {} exceeds 500", img.width());
        assert!(img.height() >= 1);
    }

    /// 仕様: 白ロゴの縁は、半透明でも RGB が白のまま（プリマルチプライドのままだと α 倍に暗くなる）。
    #[test]
    fn white_logo_edges_are_not_darkened() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect x="0.5" y="0" width="9" height="10" fill="white"/></svg>"#;
        let img = load_svg_from_bytes(svg, (10, 10)).unwrap().to_rgba8();
        let edge: Vec<_> = img.pixels().filter(|p| p[3] > 0 && p[3] < 255).collect();
        assert!(
            !edge.is_empty(),
            "precondition failed: no semi-transparent pixel to inspect"
        );
        for p in edge {
            assert!(
                p[0] >= 254 && p[1] >= 254 && p[2] >= 254,
                "edge pixel {:?} is darkened",
                p
            );
        }
    }
}
