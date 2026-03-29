//! Exifフレーム v2 統合テスト
use picture_tool_core::*;
use picture_tool_core::exif_frame::*;
use std::path::PathBuf;
use tempfile::TempDir;

fn create_test_image(width: u32, height: u32) -> image::DynamicImage {
    image::DynamicImage::ImageRgb8(image::RgbImage::new(width, height))
}

fn default_exif() -> ExifInfo {
    ExifInfo {
        camera_make: Some("SONY".to_string()),
        camera_model: Some("ILCE-7M4".to_string()),
        lens_model: Some("FE 24-70mm F2.8 GM II".to_string()),
        focal_length: Some("35mm".to_string()),
        f_number: Some("f/2.8".to_string()),
        shutter_speed: Some("1/250s".to_string()),
        iso: Some(400),
        date_taken: None,
    }
}

fn default_asset_dirs() -> AssetDirs {
    AssetDirs {
        user_logos_dir: None,
        user_fonts_dir: None,
        user_model_map: None,
    }
}

/// 一時ディレクトリにテスト用JPEGファイルを書き出して PathBuf を返す
fn write_test_jpeg(dir: &TempDir, width: u32, height: u32, name: &str) -> PathBuf {
    let path = dir.path().join(name);
    let img = create_test_image(width, height);
    img.save(&path).expect("Failed to save test JPEG");
    path
}

// ---- Test 1: 横構図 (1200x800) → 4:5 ----

#[test]
fn pad_exif_landscape_produces_4_5() {
    let img = create_test_image(1200, 800);
    let exif = default_exif();
    let config = ExifFrameConfig::default();
    let bg = BackgroundColor::Black;
    let dirs = default_asset_dirs();
    let result = render_exif_frame(&img, &exif, &config, &bg, &dirs).unwrap();
    let ratio = result.width() as f32 / result.height() as f32;
    assert!(
        (ratio - 0.8).abs() < 0.02,
        "Expected 4:5 (0.8), got {:.3}",
        ratio
    );
}

// ---- Test 2: 縦構図 (800x1200) → 4:5 ----

#[test]
fn pad_exif_portrait_produces_4_5() {
    let img = create_test_image(800, 1200);
    let exif = default_exif();
    let config = ExifFrameConfig::default();
    let bg = BackgroundColor::White;
    let dirs = default_asset_dirs();
    let result = render_exif_frame(&img, &exif, &config, &bg, &dirs).unwrap();
    let ratio = result.width() as f32 / result.height() as f32;
    assert!(
        (ratio - 0.8).abs() < 0.02,
        "Expected 4:5 (0.8), got {:.3}",
        ratio
    );
}

// ---- Test 3: 既に4:5 (800x1000) → 正常に処理できる ----

#[test]
fn pad_exif_already_4_5_still_works() {
    let img = create_test_image(800, 1000);
    let exif = default_exif();
    let config = ExifFrameConfig::default();
    let bg = BackgroundColor::Black;
    let dirs = default_asset_dirs();
    let result = render_exif_frame(&img, &exif, &config, &bg, &dirs).unwrap();
    let ratio = result.width() as f32 / result.height() as f32;
    assert!(
        (ratio - 0.8).abs() < 0.02,
        "Expected 4:5 (0.8), got {:.3}",
        ratio
    );
}

// ---- Test 4: EXIF情報なし → クラッシュしない ----

#[test]
fn pad_exif_no_exif_data_doesnt_crash() {
    let img = create_test_image(1200, 800);
    let exif = ExifInfo::default(); // 全フィールドNone
    let config = ExifFrameConfig::default();
    let bg = BackgroundColor::Black;
    let dirs = default_asset_dirs();
    let result = render_exif_frame(&img, &exif, &config, &bg, &dirs);
    assert!(result.is_ok(), "render_exif_frame should not crash with empty ExifInfo");
}

// ---- Test 5: Cropモードは exif_frame 設定を無視する ----

#[test]
fn crop_mode_ignores_exif_frame_config() {
    let tmp = TempDir::new().unwrap();
    let input = write_test_jpeg(&tmp, 1200, 800, "input_crop.jpg");
    let out_dir = tmp.path().to_path_buf();

    let config = ProcessingConfig {
        mode: ConversionMode::Crop,
        bg_color: BackgroundColor::Black,
        quality: 85,
        max_size_mb: 8,
        delete_originals: false,
    };
    let ef_config = ExifFrameConfig::default();
    let dirs = default_asset_dirs();

    let result = process_image(&input, &out_dir, &config, Some(&ef_config), Some(&dirs));
    assert!(result.is_ok(), "Crop mode with exif config should succeed: {:?}", result.err());
}

// ---- Test 6: Qualityモードは exif_frame 設定を無視する ----

#[test]
fn quality_mode_ignores_exif_frame_config() {
    let tmp = TempDir::new().unwrap();
    let input = write_test_jpeg(&tmp, 800, 1000, "input_quality.jpg");
    let out_dir = tmp.path().to_path_buf();

    let config = ProcessingConfig {
        mode: ConversionMode::Quality,
        bg_color: BackgroundColor::White,
        quality: 90,
        max_size_mb: 8,
        delete_originals: false,
    };
    let ef_config = ExifFrameConfig::default();
    let dirs = default_asset_dirs();

    let result = process_image(&input, &out_dir, &config, Some(&ef_config), Some(&dirs));
    assert!(result.is_ok(), "Quality mode with exif config should succeed: {:?}", result.err());
}
