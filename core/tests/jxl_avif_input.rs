//! JPEG XL / AVIF の入力対応（出力は常に JPEG）
//!
//! フィクスチャは `tests/fixtures/make_jxl_avif.py` で生成したもの。元画像はすべて
//! **40x20、左半分が赤・右半分が青**。向きの検証はこの配置で行う。
//!
//! | ファイル | 中身 |
//! |---|---|
//! | `exif_o6.jxl` | コードストリームの向きは 1。Exif ボックスに Make/Model と Orientation=6 |
//! | `exif_offset.jxl` | `exif_o6.jxl` と同じだが Exif ボックスの tiff_header_offset が 6 |
//! | `exif_brob.jxl` | `exif_o6.jxl` と同じだが Exif を Brotli 圧縮ボックス（`brob`）に入れたもの |
//! | `rotated.jxl` | コードストリームの向きが 6（90 度時計回りで正立）。ffmpeg が書いた Exif はポインタが壊れているので読み取りの検証には使わない |
//! | `exif_o6.avif` | `irot` なし。Exif に Make/Model と Orientation=6 |
//!
//! JPEG XL はコードストリームの向き、AVIF は `irot`/`imir` が正で、
//! **どちらも Exif の Orientation は無視する**（適用すると二重回転になる）。

use image::{GenericImageView, Rgba};
use picture_tool_core::{
    BackgroundColor, ConversionMode, ProcessingConfig, collect_image_files,
    image_dimensions_oriented, is_supported_image, open_image_oriented, process_image,
    read_exif_info,
};
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// 赤が支配的か（AVIF は非可逆なので厳密一致は見ない）
fn is_red(p: Rgba<u8>) -> bool {
    p[0] > 200 && p[1] < 60 && p[2] < 60
}

fn is_blue(p: Rgba<u8>) -> bool {
    p[2] > 200 && p[0] < 60 && p[1] < 60
}

fn quality_config() -> ProcessingConfig {
    // quality モードは縦横比を変えないので、出力の寸法で向きを確かめられる
    ProcessingConfig {
        mode: ConversionMode::Quality,
        bg_color: BackgroundColor::White,
        quality: 90,
        max_size_mb: 8,
        delete_originals: false,
        max_width: None,
    }
}

fn assert_is_jpeg(path: &Path) {
    let bytes = std::fs::read(path).unwrap();
    assert_eq!(&bytes[..3], &[0xFF, 0xD8, 0xFF], "出力が JPEG ではない");
}

#[test]
fn jxl_and_avif_extensions_are_supported_case_insensitively() {
    for name in ["a.jxl", "a.JXL", "a.avif", "a.AVIF"] {
        assert!(is_supported_image(Path::new(name)), "{name}");
    }
}

#[test]
fn folder_scan_includes_jxl_and_avif() {
    let dir = tempfile::tempdir().unwrap();
    for name in ["exif_o6.jxl", "exif_o6.avif"] {
        std::fs::copy(fixture(name), dir.path().join(name)).unwrap();
    }
    let mut names: Vec<_> = collect_image_files(dir.path())
        .files
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, ["exif_o6.avif", "exif_o6.jxl"]);
}

#[test]
fn jxl_is_converted_to_jpeg() {
    let out = tempfile::tempdir().unwrap();
    let result = process_image(
        &fixture("exif_o6.jxl"),
        out.path(),
        &quality_config(),
        None,
        None,
    )
    .unwrap();
    assert!(result.output_path.ends_with("exif_o6_processed.jpg"));
    assert_is_jpeg(Path::new(&result.output_path));
    let output = image::open(&result.output_path).unwrap();
    assert_eq!(output.dimensions(), (40, 20));
}

#[test]
fn avif_is_converted_to_jpeg() {
    let out = tempfile::tempdir().unwrap();
    let result = process_image(
        &fixture("exif_o6.avif"),
        out.path(),
        &quality_config(),
        None,
        None,
    )
    .unwrap();
    assert!(result.output_path.ends_with("exif_o6_processed.jpg"));
    assert_is_jpeg(Path::new(&result.output_path));
    let output = image::open(&result.output_path).unwrap();
    assert_eq!(output.dimensions(), (40, 20));
}

#[test]
fn jxl_codestream_orientation_is_applied_exactly_once() {
    // 90 度時計回りで正立 → 20x40、元の左（赤）が上、右（青）が下
    let img = open_image_oriented(&fixture("rotated.jxl")).unwrap();
    assert_eq!(img.dimensions(), (20, 40));
    assert!(
        is_red(img.get_pixel(10, 5)),
        "上端が赤でない: {:?}",
        img.get_pixel(10, 5)
    );
    assert!(
        is_blue(img.get_pixel(10, 35)),
        "下端が青でない: {:?}",
        img.get_pixel(10, 35)
    );

    assert_eq!(
        image_dimensions_oriented(&fixture("rotated.jxl")).unwrap(),
        (20, 40)
    );

    let out = tempfile::tempdir().unwrap();
    let result = process_image(
        &fixture("rotated.jxl"),
        out.path(),
        &quality_config(),
        None,
        None,
    )
    .unwrap();
    assert_eq!(
        image::open(&result.output_path).unwrap().dimensions(),
        (20, 40)
    );
}

#[test]
fn jxl_exif_orientation_is_ignored() {
    let img = open_image_oriented(&fixture("exif_o6.jxl")).unwrap();
    assert_eq!(img.dimensions(), (40, 20));
    assert!(is_red(img.get_pixel(5, 10)));
    assert!(is_blue(img.get_pixel(35, 10)));
    assert_eq!(
        image_dimensions_oriented(&fixture("exif_o6.jxl")).unwrap(),
        (40, 20)
    );
}

#[test]
fn avif_exif_orientation_is_ignored() {
    let img = open_image_oriented(&fixture("exif_o6.avif")).unwrap();
    assert_eq!(img.dimensions(), (40, 20));
    assert!(is_red(img.get_pixel(5, 10)), "{:?}", img.get_pixel(5, 10));
    assert!(
        is_blue(img.get_pixel(35, 10)),
        "{:?}",
        img.get_pixel(35, 10)
    );
    assert_eq!(
        image_dimensions_oriented(&fixture("exif_o6.avif")).unwrap(),
        (40, 20)
    );
}

#[test]
fn camera_info_is_read_from_jxl_and_avif_exif() {
    for name in [
        "exif_o6.jxl",
        "exif_offset.jxl",
        "exif_brob.jxl",
        "exif_o6.avif",
    ] {
        let info = read_exif_info(&fixture(name)).unwrap();
        assert_eq!(info.camera_make.as_deref(), Some("TestMake"), "{name}");
        assert_eq!(info.camera_model.as_deref(), Some("TestModel"), "{name}");
        // 向きは画素側で解決済み。Exif の 6 を報告すると呼び出し側が再適用する
        assert_eq!(info.orientation, None, "{name}");
    }
}

#[test]
fn truncated_jxl_and_avif_are_reported_as_errors() {
    let dir = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    for name in ["exif_o6.jxl", "exif_o6.avif"] {
        let bytes = std::fs::read(fixture(name)).unwrap();
        let broken = dir.path().join(name);
        std::fs::write(&broken, &bytes[..bytes.len() / 2]).unwrap();

        assert!(open_image_oriented(&broken).is_err(), "{name}");
        assert!(
            process_image(&broken, out.path(), &quality_config(), None, None).is_err(),
            "{name}"
        );
    }
}

#[test]
fn format_is_detected_from_content_not_extension() {
    let dir = tempfile::tempdir().unwrap();
    for (src, disguised) in [
        ("exif_o6.jxl", "jxl_as.png"),
        ("exif_o6.avif", "avif_as.jpg"),
    ] {
        let path = dir.path().join(disguised);
        std::fs::copy(fixture(src), &path).unwrap();
        assert_eq!(
            open_image_oriented(&path).unwrap().dimensions(),
            (40, 20),
            "{disguised}"
        );
    }
}
