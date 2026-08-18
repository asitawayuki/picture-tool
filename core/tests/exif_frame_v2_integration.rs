//! Exifフレーム v2 統合テスト
//!
//! 仕様の出所:
//! - CLAUDE.md「4:5アスペクト比への変換」「元の画像ファイルは上書きしない」
//! - docs/superpowers/specs/2026-03-29-exif-frame-v2-design.md
//! - 全体レビュー修正計画 S4（4:5 は厳密比較、crop/quality の無視は出力比率で検証）
use picture_tool_core::exif_frame::*;
use picture_tool_core::*;
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
        orientation: None,
    }
}

/// ユーザー設定ディレクトリに依存しない（＝実行環境で結果が変わらない）アセット
fn default_assets() -> ExifAssets {
    ExifAssets::load(AssetDirs {
        user_logos_dir: None,
        user_fonts_dir: None,
        user_model_map: None,
        user_presets_dir: None,
    })
    .expect("bundled exif assets must load")
}

/// 一時ディレクトリにテスト用JPEGファイルを書き出して PathBuf を返す
fn write_test_jpeg(dir: &TempDir, width: u32, height: u32, name: &str) -> PathBuf {
    let path = dir.path().join(name);
    let img = create_test_image(width, height);
    img.save(&path).expect("Failed to save test JPEG");
    path
}

/// 出力 4:5 は「おおむね 0.8」ではなく整数比の厳密条件。
/// 許容誤差つきの比較では 1px のずれを原理的に検出できない（S4-C1）。
fn assert_exactly_4_5(img: &image::DynamicImage, ctx: &str) {
    assert_eq!(
        img.width() * 5,
        img.height() * 4,
        "expected an exact 4:5 canvas, got {}x{} ({})",
        img.width(),
        img.height(),
        ctx
    );
}

// ---- Test 1: 横構図 → 4:5 ----

#[test]
fn pad_exif_landscape_produces_4_5() {
    let result = render_exif_frame(
        &create_test_image(1200, 800),
        &default_exif(),
        &ExifFrameConfig::default(),
        &BackgroundColor::Black,
        &default_assets(),
    )
    .unwrap();
    assert_exactly_4_5(&result, "landscape 1200x800");
}

// ---- Test 2: 縦構図 → 4:5 ----

#[test]
fn pad_exif_portrait_produces_4_5() {
    let result = render_exif_frame(
        &create_test_image(800, 1200),
        &default_exif(),
        &ExifFrameConfig::default(),
        &BackgroundColor::White,
        &default_assets(),
    )
    .unwrap();
    assert_exactly_4_5(&result, "portrait 800x1200");
}

// ---- Test 3: 既に4:5 → 正常に処理できる ----

#[test]
fn pad_exif_already_4_5_still_works() {
    let result = render_exif_frame(
        &create_test_image(800, 1000),
        &default_exif(),
        &ExifFrameConfig::default(),
        &BackgroundColor::Black,
        &default_assets(),
    )
    .unwrap();
    assert_exactly_4_5(&result, "already 4:5 800x1000");
}

/// 4の倍数・5の倍数に揃っていない実写サイズでも 4:5 が崩れないこと。
/// 従来のテストが 20 の倍数しか使っていなかったために C1 が素通りした。
#[test]
fn pad_exif_produces_exact_4_5_for_non_round_sizes() {
    let assets = default_assets();
    // 高解像度の網羅は layout 側のユニットテストが担う（あちらは計算だけなので一瞬）。
    // ここは「レイアウト結果どおりにキャンバスが作られるか」の確認なので、
    // Lanczos3 リサイズが現実的な時間で終わるサイズに絞る。
    for (w, h) in [(400, 501), (399, 502), (401, 499), (1001, 1000), (207, 203)] {
        for position in [
            ExifPosition::Auto,
            ExifPosition::Bottom,
            ExifPosition::Top,
            ExifPosition::Right,
            ExifPosition::Left,
        ] {
            let config = ExifFrameConfig {
                position,
                ..Default::default()
            };
            let result = render_exif_frame(
                &create_test_image(w, h),
                &default_exif(),
                &config,
                &BackgroundColor::Black,
                &assets,
            )
            .unwrap();
            assert_exactly_4_5(&result, &format!("{}x{} position={:?}", w, h, position));
        }
    }
}

// ---- Test 4: EXIF情報なし → クラッシュしない ----

#[test]
fn pad_exif_no_exif_data_doesnt_crash() {
    let result = render_exif_frame(
        &create_test_image(1200, 800),
        &ExifInfo::default(), // 全フィールドNone
        &ExifFrameConfig::default(),
        &BackgroundColor::Black,
        &default_assets(),
    );
    assert!(
        result.is_ok(),
        "render_exif_frame should not crash with empty ExifInfo"
    );
}

/// 短辺が閾値未満の画像は Exif フレームを諦めるが、
/// **4:5 への変換は放棄しない**（skip_exif は「Exif を描かない」であって
/// 「何もしない」ではない）。
#[test]
fn tiny_image_skips_exif_but_still_becomes_4_5() {
    let result = render_exif_frame(
        &create_test_image(150, 100),
        &default_exif(),
        &ExifFrameConfig::default(),
        &BackgroundColor::Black,
        &default_assets(),
    )
    .unwrap();
    assert_exactly_4_5(&result, "tiny 150x100 (skip_exif path)");
    assert!(
        result.width() >= 150 && result.height() >= 100,
        "the photo must still fit inside the canvas, got {}x{}",
        result.width(),
        result.height()
    );
}

/// 仕様: フォントが読めなければ Exif フレームを諦めて pad にフォールバックし、
/// その事実を warnings で呼び出し元に伝える。panic してはならない（S4-C5）。
#[test]
fn unloadable_font_falls_back_to_pad_with_a_warning() {
    let tmp = TempDir::new().unwrap();
    let input = write_test_jpeg(&tmp, 1200, 800, "input_badfont.jpg");

    let config = ProcessingConfig {
        mode: ConversionMode::Pad,
        bg_color: BackgroundColor::Black,
        quality: 85,
        max_size_mb: 8,
        delete_originals: false,
        max_width: None,
    };
    let ef_config = ExifFrameConfig {
        font: FontConfig {
            font_path: Some("/nonexistent/definitely-not-a-font.ttf".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };

    let result = process_image(
        &input,
        tmp.path(),
        &config,
        Some(&ef_config),
        Some(&default_assets()),
    )
    .expect("a broken font must not fail the whole conversion");

    assert!(
        result.warnings.iter().any(|w| w.contains("Exif frame")),
        "the fallback must be reported to the caller, got {:?}",
        result.warnings
    );
    let out = image::open(&result.output_path).unwrap();
    assert_exactly_4_5(&out, "pad fallback output");
}

// ---- Test 5: Cropモードは exif_frame 設定を無視する ----

/// 「無視する」を出力で検証する。`is_ok()` だけだと Exif フレームが
/// 誤って適用されてもテストは通ってしまう。
#[test]
fn crop_mode_ignores_exif_frame_config() {
    let tmp = TempDir::new().unwrap();
    let input = write_test_jpeg(&tmp, 1200, 800, "input_crop.jpg");

    let config = ProcessingConfig {
        mode: ConversionMode::Crop,
        bg_color: BackgroundColor::Black,
        quality: 85,
        max_size_mb: 8,
        delete_originals: false,
        max_width: None,
    };

    let result = process_image(
        &input,
        tmp.path(),
        &config,
        Some(&ExifFrameConfig::default()),
        Some(&default_assets()),
    )
    .expect("crop mode with exif config should succeed");

    // crop は元画像から 4:5 を切り出す。パディングもExifバーも足さないので
    // 高さは変わらず、幅だけが 4:5 になるまで削られる。
    let out = image::open(&result.output_path).unwrap();
    assert_exactly_4_5(&out, "crop mode output");
    assert_eq!(out.height(), 800, "crop must not add or remove height");
    assert_eq!(out.width(), 640, "1200x800 cropped to 4:5 is 640x800");
}

// ---- Test 6: Qualityモードは exif_frame 設定を無視する ----

#[test]
fn quality_mode_ignores_exif_frame_config() {
    let tmp = TempDir::new().unwrap();
    // 4:5 でないサイズを使う。800x1000 だと「何もしない」と
    // 「4:5に変換した」を出力から区別できない。
    let input = write_test_jpeg(&tmp, 1200, 800, "input_quality.jpg");

    let config = ProcessingConfig {
        mode: ConversionMode::Quality,
        bg_color: BackgroundColor::White,
        quality: 90,
        max_size_mb: 8,
        delete_originals: false,
        max_width: None,
    };

    let result = process_image(
        &input,
        tmp.path(),
        &config,
        Some(&ExifFrameConfig::default()),
        Some(&default_assets()),
    )
    .expect("quality mode with exif config should succeed");

    let out = image::open(&result.output_path).unwrap();
    assert_eq!(
        (out.width(), out.height()),
        (1200, 800),
        "quality mode must preserve the original dimensions"
    );
}

// =========================================================
// プレビュー生成（S6-M16 で GUI から core に移した経路）
// =========================================================

/// data URI の prefix を持たない生の base64 を JPEG としてデコードする
fn decode_preview(base64_jpeg: &str) -> image::DynamicImage {
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(base64_jpeg)
        .expect("preview must be valid base64");
    image::load_from_memory(&bytes).expect("preview must be a decodable JPEG")
}

#[test]
fn exif_frame_preview_is_exactly_4_5() {
    // プレビューは「出力がどう見えるか」を示すものなので、出力と同じく
    // 厳密な 4:5 でなければならない（S4-C1 と同じ不変条件）。
    // 4 の倍数にも 5 の倍数にも揃っていないサイズを使う。
    let tmp = TempDir::new().unwrap();
    let input = write_test_jpeg(&tmp, 400, 501, "photo.jpg");

    let base64 = generate_exif_frame_preview_base64(
        &input,
        &ExifFrameConfig::default(),
        &BackgroundColor::White,
        &default_assets(),
        400,
    )
    .expect("preview generation must succeed");

    let preview = decode_preview(&base64);
    let (w, h) = (preview.width(), preview.height());
    assert_eq!(
        w * 5,
        h * 4,
        "preview must be exactly 4:5 but was {}x{}",
        w,
        h
    );
}

#[test]
fn exif_frame_preview_fits_within_the_requested_size() {
    // プレビューは実寸ではなく指定した上限に収まる縮小版であること。
    // （縮小を飛ばすと巨大な base64 が webview に流れる）
    let tmp = TempDir::new().unwrap();
    let input = write_test_jpeg(&tmp, 2000, 2500, "big.jpg");

    let base64 = generate_exif_frame_preview_base64(
        &input,
        &ExifFrameConfig::default(),
        &BackgroundColor::White,
        &default_assets(),
        400,
    )
    .expect("preview generation must succeed");

    let preview = decode_preview(&base64);
    // Exif バーのぶんだけ元画像より大きくなるが、桁が変わるほどではない
    assert!(
        preview.width() <= 600 && preview.height() <= 700,
        "preview was not downscaled: {}x{}",
        preview.width(),
        preview.height()
    );
}

#[test]
fn exif_frame_preview_rejects_an_unreadable_file() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("broken.jpg");
    std::fs::write(&path, b"not a JPEG").unwrap();

    assert!(generate_exif_frame_preview_base64(
        &path,
        &ExifFrameConfig::default(),
        &BackgroundColor::White,
        &default_assets(),
        400,
    )
    .is_err());
}
