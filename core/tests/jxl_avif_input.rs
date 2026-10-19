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
//! | `split.jxl` | `exif_o6.jxl` のコードストリームを `jxlp` 2 つに分けたもの（Exif あり） |
//! | `broken_brob.jxl` | Brotli として壊れた `brob`（種類は Exif）を持つ |
//! | `exif_bomb.jxl` | 展開すると 8MiB になる Exif の `brob` を持つ |
//! | `nested.jxl` | コンテナの `jxlc` の中にさらにコンテナ（`brob` の Exif ＋ `jxlc`）を入れた不正なもの |
//! | `over_limit.jxl` | 14000x13000 の単色の非可逆。出力が 546MB で、上限の 512MB を超える |
//! | `lossy_24mp.jxl` | 6000x4000 の非可逆。左半分が赤・右半分が青 |
//! | `orient_N.avif` | 64x32 を 4 分割（左上=赤、右上=緑、左下=青、右下=黄）し、Exif の向き N を Pillow が `irot`/`imir` に変換して書いたもの |
//! | `avif/*.avif` | `tests/fixtures/make_avif_decoder.py` が生成した、AVIF デコーダ差し替え用の 25 件。元画像は 33x17 の非単色の図柄。以下の行はその内訳 |
//! | `avif/<主画像>[_<アルファ>].avif`（20 件） | 画素一致用。名前は `p<深度>_<間引き>_<lim/full>_<行列>`（`gray<深度>_...` は 4:0:0）に、アルファ付きなら `_a<深度>` などが続く。行列・範囲・深度の全組はスクリプトの表を参照 |
//! | `avif/base_64x32.avif` | 4 色の 64x32。`crash.avif` の元 |
//! | `avif/crash.avif` | `base_64x32.avif` のタイルデータ内（オフセット 340・341）を壊したもの。abort の回帰。計画の frame OBU ペイロード先頭の位置は、現行の base では abort しない |
//! | `avif/over_limit.avif` | 14000x13000 の単色 8bit 4:2:0。出力が 546MB で 512MB を超える |
//! | `avif/alpha_huge.avif` | 主画像 64x64、アルファは実際にエンコードした 23200x23200。主画像だけなら上限の内側 |
//! | `avif/two_seqhdr.avif` | 1 アイテムの AV1 データにシーケンスヘッダを 2 個持つ（`p8_420_lim_unspec.avif` の主画像データの複製） |
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

fn is_green(p: Rgba<u8>) -> bool {
    p[1] > 200 && p[0] < 60 && p[2] < 60
}

fn is_yellow(p: Rgba<u8>) -> bool {
    p[0] > 200 && p[1] > 200 && p[2] < 60
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
    // 前提: Exif は読めている（読めずに無視されたのなら、このテストは何も確かめていない）
    let info = read_exif_info(&fixture("exif_o6.jxl")).unwrap();
    assert_eq!(info.camera_make.as_deref(), Some("TestMake"));

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
        ("exif_o6.jxl", "jxl_as.avif"),
        ("exif_o6.avif", "avif_as.jxl"),
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

#[test]
fn jpeg_named_as_avif_is_still_decoded() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("jpeg_as.avif");
    std::fs::copy(fixture("exif-only.jpg"), &path).unwrap();
    let expected = image::open(fixture("exif-only.jpg")).unwrap().dimensions();
    assert_eq!(open_image_oriented(&path).unwrap().dimensions(), expected);
    assert_eq!(image_dimensions_oriented(&path).unwrap(), expected);
}

#[test]
fn lossy_jxl_at_a_typical_camera_resolution_is_converted() {
    let img = open_image_oriented(&fixture("lossy_24mp.jxl")).unwrap();
    assert_eq!(img.dimensions(), (6000, 4000));
    assert!(is_red(img.get_pixel(1500, 2000)));
    assert!(is_blue(img.get_pixel(4500, 2000)));

    let out = tempfile::tempdir().unwrap();
    let result = process_image(
        &fixture("lossy_24mp.jxl"),
        out.path(),
        &quality_config(),
        None,
        None,
    )
    .unwrap();
    assert_eq!(
        image::open(&result.output_path).unwrap().dimensions(),
        (6000, 4000)
    );
}

/// Exif の向き N で表示したときの 4 隅の色（左上, 右上, 左下, 右下）。元は 赤 緑 / 青 黄
fn expected_corners(orientation: u16) -> [&'static str; 4] {
    match orientation {
        1 => ["red", "green", "blue", "yellow"],
        2 => ["green", "red", "yellow", "blue"], // 左右反転
        3 => ["yellow", "blue", "green", "red"], // 180 度
        4 => ["blue", "yellow", "red", "green"], // 上下反転
        5 => ["red", "blue", "green", "yellow"], // 転置
        6 => ["blue", "red", "yellow", "green"], // 時計回り 90 度
        7 => ["yellow", "green", "blue", "red"], // 反転置
        8 => ["green", "yellow", "red", "blue"], // 反時計回り 90 度
        _ => unreachable!(),
    }
}

fn color_name(p: Rgba<u8>) -> &'static str {
    if is_red(p) {
        "red"
    } else if is_green(p) {
        "green"
    } else if is_blue(p) {
        "blue"
    } else if is_yellow(p) {
        "yellow"
    } else {
        "other"
    }
}

#[test]
fn avif_orientation_follows_irot_and_imir() {
    let out = tempfile::tempdir().unwrap();
    for orientation in 1..=8u16 {
        let path = fixture(&format!("orient_{orientation}.avif"));
        let expected_size = if orientation >= 5 { (32, 64) } else { (64, 32) };

        let img = open_image_oriented(&path).unwrap();
        assert_eq!(img.dimensions(), expected_size, "orientation {orientation}");
        let (w, h) = img.dimensions();
        let corners = [
            (w / 4, h / 4),
            (w * 3 / 4, h / 4),
            (w / 4, h * 3 / 4),
            (w * 3 / 4, h * 3 / 4),
        ]
        .map(|(x, y)| color_name(img.get_pixel(x, y)));
        assert_eq!(
            corners,
            expected_corners(orientation),
            "orientation {orientation}"
        );

        assert_eq!(
            image_dimensions_oriented(&path).unwrap(),
            expected_size,
            "orientation {orientation}"
        );
        let result = process_image(&path, out.path(), &quality_config(), None, None).unwrap();
        assert_eq!(
            image::open(&result.output_path).unwrap().dimensions(),
            expected_size,
            "orientation {orientation}"
        );
    }
}

#[test]
fn split_jxl_codestream_is_decoded() {
    let img = open_image_oriented(&fixture("split.jxl")).unwrap();
    assert_eq!(img.dimensions(), (40, 20));
    assert!(is_red(img.get_pixel(5, 10)));
    assert!(is_blue(img.get_pixel(35, 10)));
    let info = read_exif_info(&fixture("split.jxl")).unwrap();
    assert_eq!(info.camera_make.as_deref(), Some("TestMake"));
}

#[test]
fn broken_auxiliary_box_does_not_affect_pixels_or_dimensions() {
    let path = fixture("broken_brob.jxl");
    let img = open_image_oriented(&path).unwrap();
    assert_eq!(img.dimensions(), (40, 20));
    assert!(is_red(img.get_pixel(5, 10)));
    assert_eq!(image_dimensions_oriented(&path).unwrap(), (40, 20));
    // Exif は読めないので無いものとして扱う（エラーにはしない）
    assert_eq!(read_exif_info(&path).unwrap().camera_make, None);
}

#[test]
fn oversized_compressed_exif_is_ignored() {
    // 前提: 同じ形（brob に入った Exif）で大きさが普通なら読める
    let normal = read_exif_info(&fixture("exif_brob.jxl")).unwrap();
    assert_eq!(normal.camera_make.as_deref(), Some("TestMake"));

    let path = fixture("exif_bomb.jxl");
    assert_eq!(read_exif_info(&path).unwrap().camera_make, None);
    assert_eq!(open_image_oriented(&path).unwrap().dimensions(), (40, 20));
}

#[test]
fn jxl_whose_output_exceeds_the_memory_limit_is_rejected() {
    let path = fixture("over_limit.jxl");
    // 寸法はヘッダだけで読めるので一覧には出る
    assert_eq!(image_dimensions_oriented(&path).unwrap(), (14000, 13000));
    assert!(open_image_oriented(&path).is_err());
    let out = tempfile::tempdir().unwrap();
    assert!(process_image(&path, out.path(), &quality_config(), None, None).is_err());
}

#[test]
fn dimensions_are_read_from_the_header_alone() {
    let dir = tempfile::tempdir().unwrap();

    // JPEG XL: コードストリームのヘッダより後ろを切り落とす
    let jxl = std::fs::read(fixture("lossy_24mp.jxl")).unwrap();
    let truncated_jxl = dir.path().join("head.jxl");
    std::fs::write(&truncated_jxl, &jxl[..200]).unwrap();

    // AVIF: 画像データ（mdat）の中身を切り落とす
    let avif = std::fs::read(fixture("orient_6.avif")).unwrap();
    let mdat = avif.windows(4).position(|w| w == b"mdat").unwrap();
    let truncated_avif = dir.path().join("head.avif");
    std::fs::write(&truncated_avif, &avif[..mdat + 4]).unwrap();

    for (path, expected) in [(&truncated_jxl, (6000, 4000)), (&truncated_avif, (32, 64))] {
        // 前提: 画素は本当に読めない
        assert!(open_image_oriented(path).is_err(), "{}", path.display());
        assert_eq!(
            image_dimensions_oriented(path).unwrap(),
            expected,
            "{}",
            path.display()
        );
    }
}

#[test]
fn container_nested_in_a_codestream_box_is_rejected() {
    // jxlc の中身はコードストリームでなければならない。受け入れると、内側の brob が
    // 上限なしで展開される経路が開く
    let path = fixture("nested.jxl");
    assert!(open_image_oriented(&path).is_err());
    assert!(image_dimensions_oriented(&path).is_err());
}

fn avif(name: &str) -> PathBuf {
    fixture(&format!("avif/{name}"))
}

fn open_err(name: &str) -> String {
    format!("{:#}", open_image_oriented(&avif(name)).expect_err(name))
}

#[test]
fn avif_over_the_output_limit_is_rejected_before_decoding() {
    assert!(
        open_err("over_limit.avif").contains("AVIF declares a size over the 512MB output limit")
    );
}

#[test]
fn avif_alpha_is_counted_against_the_output_limit() {
    // 前提: 主画像だけなら上限の内側（64x64）
    assert_eq!(
        image_dimensions_oriented(&avif("alpha_huge.avif")).unwrap(),
        (64, 64)
    );
    assert!(
        open_err("alpha_huge.avif").contains("AVIF declares a size over the 512MB output limit")
    );
}

#[test]
fn avif_with_two_sequence_headers_in_one_item_is_rejected() {
    assert!(open_err("two_seqhdr.avif").contains("exactly one sequence header (found 2)"));
    // アイテムごとに 1 個（合計 2 個）の正常なアルファ付きは開ける
    assert!(open_image_oriented(&avif("p8_420_lim_709_a8.avif")).is_ok());
}
