//! 出力 JPEG の契約（spec 2026-10-20「エンコーダ」「品質の目盛り」）。
//!
//! `process_image` を quality モードで通し、書き出されたファイルのヘッダーを読む。

mod common;

use std::path::{Path, PathBuf};

use common::jpeg::{dht, dqt, markers, sof_sampling};
use image::{ImageBuffer, Rgb};
use picture_tool_core::{BackgroundColor, ConversionMode, ProcessingConfig, process_image};

/// LrC 20% の輝度テーブル（spec の値を逐語で。DQT のままの zigzag 順）
#[rustfmt::skip]
const LRC_20_LUMA: [u16; 64] = [
    12, 8, 8,13, 9,13,21,12,12,21,26,20,16,20,26,32,27,26,26,27,32,34,23,23,23,23,23,34,17,12,12,12,
    12,12,12,17,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,
];

/// LrC 20% の色差テーブル（spec の値を逐語で。DQT のままの zigzag 順）
#[rustfmt::skip]
const LRC_20_CHROMA: [u16; 64] = [
    13,13,13,17,14,17,27,17,17,27,20,14,14,14,20,20,14,14,14,14,20,17,12,12,12,12,12,17,17,12,12,12,
    12,12,12,17,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,
];

/// ITU-T T.81 Annex K（Table K.3）の標準ハフマン表、輝度 DC の BITS
const ANNEX_K_LUMA_DC_BITS: [u8; 16] = [
    0x00, 0x01, 0x05, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

/// ITU-T T.81 Annex K（Table K.5）の標準ハフマン表、輝度 AC の BITS
const ANNEX_K_LUMA_AC_BITS: [u8; 16] = [
    0x00, 0x02, 0x01, 0x03, 0x03, 0x02, 0x04, 0x03, 0x05, 0x05, 0x04, 0x04, 0x00, 0x00, 0x01, 0x7d,
];

/// 256×256 のノイズを PNG で書く（決定的な線形合同法）。
/// 色差を含め、どの周波数にも係数が立つので、ハフマン表とサンプリングの違いが出る
fn write_noise_png(path: &Path) {
    let mut state: u32 = 0x1234_5678;
    let img = ImageBuffer::from_fn(256, 256, |_, _| {
        let mut next = || {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 24) as u8
        };
        Rgb([next(), next(), next()])
    });
    img.save(path).unwrap();
}

/// ノイズ画像を品質 `quality` の quality モードで書き出し、出力 JPEG のバイト列を返す
fn export(quality: u8) -> Vec<u8> {
    let dir = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    let input = dir.path().join("noise.png");
    write_noise_png(&input);
    let config = ProcessingConfig {
        mode: ConversionMode::Quality,
        bg_color: BackgroundColor::White,
        quality,
        max_size_mb: 8,
        delete_originals: false,
        max_width: None,
    };
    let result = process_image(&input, out.path(), &config, None, None).unwrap();
    assert!(
        !result.size_limit_exceeded,
        "前提が崩れている: サイズ制限で品質が下がった"
    );
    std::fs::read(PathBuf::from(result.output_path)).unwrap()
}

#[test]
fn quality_20_writes_the_lrc_20_percent_tables() {
    let tables = dqt(&export(20));
    let table = |id: u8| {
        tables
            .iter()
            .find(|(tq, _)| *tq == id)
            .unwrap_or_else(|| panic!("DQT に表 {id} が無い: {tables:?}"))
            .1
    };
    assert_eq!(table(0), LRC_20_LUMA, "輝度のテーブルが LrC 20% と違う");
    assert_eq!(table(1), LRC_20_CHROMA, "色差のテーブルが LrC 20% と違う");
}

#[test]
fn quality_20_is_subsampled_420() {
    assert_eq!(sof_sampling(&export(20)), [(2, 2), (1, 1), (1, 1)]);
}

#[test]
fn quality_90_is_444() {
    assert_eq!(sof_sampling(&export(90)), [(1, 1), (1, 1), (1, 1)]);
}

/// `jpeg-encoder` の `Encoder::new` は品質 90 未満で既定を 4:2:0 にする。品質 20 と 90 はその既定と
/// 目盛りの判断が一致してしまうので、食い違う品質で、サンプリングを明示していることを固定する
#[test]
fn quality_60_is_444_although_the_encoder_default_is_420() {
    assert_eq!(sof_sampling(&export(60)), [(1, 1), (1, 1), (1, 1)]);
}

#[test]
fn huffman_tables_are_optimized() {
    let tables = dht(&export(90));
    assert!(!tables.is_empty(), "前提が崩れている: DHT が無い");
    let bits = |class: u8| {
        tables
            .iter()
            .find(|(tc, th, _)| *tc == class && *th == 0)
            .unwrap_or_else(|| panic!("輝度の DHT（クラス {class}）が無い: {tables:?}"))
            .2
    };
    assert_ne!(
        bits(0),
        ANNEX_K_LUMA_DC_BITS,
        "輝度 DC が Annex K の標準表のまま"
    );
    assert_ne!(
        bits(1),
        ANNEX_K_LUMA_AC_BITS,
        "輝度 AC が Annex K の標準表のまま"
    );
}

#[test]
fn output_is_baseline() {
    let markers = markers(&export(90));
    assert!(markers.contains(&0xC0), "SOF0 が無い: {markers:02X?}");
    assert!(
        !markers.contains(&0xC2),
        "プログレッシブ（SOF2）になっている"
    );
}

// ---- 透過の合成（spec: 透過は背景色で塗ってから書き出す。全モード共通） ----

/// 全画素が `rgba` の RGBA8 PNG を書く
fn write_flat_rgba_png(path: &Path, w: u32, h: u32, rgba: [u8; 4]) {
    ImageBuffer::from_pixel(w, h, image::Rgba(rgba))
        .save(path)
        .unwrap();
}

/// `rgba` 一色の `w`×`h` PNG を `mode` / `bg` で書き出し、出力の中央画素を返す。
/// 品質 100 は 4:4:4 になる（4:2:0 だと色が滲み、許容差 ±6 を保てない）
fn center_pixel_after_export(
    mode: ConversionMode,
    bg: BackgroundColor,
    w: u32,
    h: u32,
    rgba: [u8; 4],
) -> [u8; 3] {
    let dir = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    let input = dir.path().join("flat.png");
    write_flat_rgba_png(&input, w, h, rgba);
    let config = ProcessingConfig {
        mode,
        bg_color: bg,
        quality: 100,
        max_size_mb: 8,
        delete_originals: false,
        max_width: None,
    };
    let result = process_image(&input, out.path(), &config, None, None).unwrap();
    let decoded = image::open(PathBuf::from(result.output_path))
        .unwrap()
        .to_rgb8();
    decoded
        .get_pixel(decoded.width() / 2, decoded.height() / 2)
        .0
}

/// ±6 を外れた組み合わせを全部集める（最初の 1 件で止めると、どのモードが壊れているか分からない）
fn mismatch(actual: [u8; 3], expected: [u8; 3], what: &str) -> Option<String> {
    let ok = (0..3).all(|c| actual[c].abs_diff(expected[c]) <= 6);
    (!ok).then(|| format!("{what}: 実際 {actual:?} / 期待 {expected:?}（±6）"))
}

/// 全モード × 全背景色 × 寸法（pad は余白あり 100×80 と、厳密な 4:5 の 80×100）
fn all_cases() -> Vec<(ConversionMode, BackgroundColor, u32, u32, String)> {
    let mut v = Vec::new();
    for (mode, name, dims) in [
        (ConversionMode::Crop, "crop", vec![(100, 80)]),
        (ConversionMode::Quality, "quality", vec![(100, 80)]),
        (ConversionMode::Pad, "pad", vec![(100, 80), (80, 100)]),
    ] {
        for (bg, bg_name) in [
            (BackgroundColor::White, "white"),
            (BackgroundColor::Black, "black"),
        ] {
            for &(w, h) in &dims {
                v.push((mode, bg, w, h, format!("{name}/{bg_name}/{w}x{h}")));
            }
        }
    }
    v
}

#[test]
fn transparent_pixels_become_the_background_color() {
    // 完全に透けた画素の裏は赤。背景色で塗られていれば赤は出ない
    // 前提: 入力の RGB は赤（塗らずに RGB だけ使えば赤が出る入力である）
    let hidden = [255, 0, 0, 0];
    assert_eq!(hidden[..3], [255, 0, 0]);
    let mut failures = Vec::new();
    for (mode, bg, w, h, name) in all_cases() {
        let expected = match bg {
            BackgroundColor::White => [255, 255, 255],
            BackgroundColor::Black => [0, 0, 0],
        };
        let actual = center_pixel_after_export(mode, bg, w, h, hidden);
        failures.extend(mismatch(actual, expected, &name));
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn half_transparent_pixels_are_blended_with_the_background() {
    // (0,255,0) を a=128/255 で重ねる: 白なら (127,255,127)、黒なら (0,128,0)
    let half = [0, 255, 0, 128];
    let mut failures = Vec::new();
    for (mode, bg, w, h, name) in all_cases() {
        let expected = match bg {
            BackgroundColor::White => [127, 255, 127],
            BackgroundColor::Black => [0, 128, 0],
        };
        let actual = center_pixel_after_export(mode, bg, w, h, half);
        failures.extend(mismatch(actual, expected, &name));
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// ---- 入力の ICC（spec「5」: sRGB 以外の ICC は sRGB へ変換し、出力には常に sRGB の ICC を埋め込む） ----
//
// フィクスチャと期待値の作り方は fixtures/make_output_quality.py。パッチは 4 色を 8×8 ずつ横に並べた 32×8 で、
// 画素値のまま ICC を割り当てている。

/// 割り当てた画素値（変換しなければ出力はこの値になる）
const PATCHES: [[u8; 3]; 4] = [
    [200, 60, 60],
    [60, 160, 90],
    [90, 110, 200],
    [210, 170, 120],
];

/// ImageMagick 7.1.2 の `-intent relative -profile sRGB.icc` で AdobeRGB → sRGB に変換した値
const ADOBE_IN_SRGB: [[u8; 3]; 4] = [[232, 57, 57], [0, 161, 85], [79, 110, 204], [225, 172, 118]];

/// 同じく ProPhoto → sRGB。3 色は sRGB の色域外で 0 か 255 に張り付く
const PROPHOTO_IN_SRGB: [[u8; 3]; 4] = [[255, 0, 74], [0, 190, 94], [0, 133, 220], [255, 176, 129]];

/// adobe16.png（アルファ 128/255）を ImageMagick で sRGB に変換し、`-clamp` してから白に `-flatten` した値
/// （`-clamp` の理由は make_output_quality.py 冒頭）。
/// 白に合成してから変換する（順序を誤る）と [249,159,159] / [132,209,172] / [169,184,230] / [240,213,188]
const ADOBE16_IN_SRGB_ON_WHITE: [[u8; 3]; 4] = [
    [243, 156, 156],
    [127, 208, 170],
    [167, 182, 229],
    [240, 213, 186],
];

fn icc_fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/icc")
        .join(name)
}

/// 品質 100（4:4:4）の quality モード・白背景で書き出す。
/// 品質 100 にするのは、4:2:0 の出力を `image`（zune-jpeg 0.5.15）が緑に誤デコードするため（既知の制限）
fn export_file(input: &Path) -> (picture_tool_core::ProcessResult, Vec<u8>) {
    let out = tempfile::tempdir().unwrap();
    let config = ProcessingConfig {
        mode: ConversionMode::Quality,
        bg_color: BackgroundColor::White,
        quality: 100,
        max_size_mb: 8,
        delete_originals: false,
        max_width: None,
    };
    let result = process_image(input, out.path(), &config, None, None).unwrap();
    let bytes = std::fs::read(&result.output_path).unwrap();
    (result, bytes)
}

/// 出力の各パッチの中央の画素
fn patch_centers(jpeg: &[u8]) -> [[u8; 3]; 4] {
    let img = image::load_from_memory(jpeg).unwrap().to_rgb8();
    assert_eq!(img.dimensions(), (32, 8), "前提が崩れている: 出力の寸法");
    std::array::from_fn(|i| img.get_pixel(8 * i as u32 + 4, 4).0)
}

/// 許容差 `tol` を外れたパッチを全部集める
fn patch_mismatches(
    actual: [[u8; 3]; 4],
    expected: [[u8; 3]; 4],
    tol: u8,
    what: &str,
) -> Vec<String> {
    (0..4)
        .filter(|&i| (0..3).any(|c| actual[i][c].abs_diff(expected[i][c]) > tol))
        .map(|i| {
            format!(
                "{what} パッチ {i}: 実際 {:?} / 期待 {:?}（±{tol}）",
                actual[i], expected[i]
            )
        })
        .collect()
}

#[test]
fn adobe_rgb_png_is_converted_to_srgb() {
    let (_, out) = export_file(&icc_fixture("adobe.png"));
    let failures = patch_mismatches(patch_centers(&out), ADOBE_IN_SRGB, 3, "adobe.png");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn adobe_rgb_jpeg_and_webp_are_converted_to_srgb() {
    let mut failures = Vec::new();
    for name in ["adobe.jpg", "adobe.webp"] {
        let (_, out) = export_file(&icc_fixture(name));
        failures.extend(patch_mismatches(
            patch_centers(&out),
            ADOBE_IN_SRGB,
            6,
            name,
        ));
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn sixteen_bit_png_is_converted_before_flattening() {
    let (_, out) = export_file(&icc_fixture("adobe16.png"));
    let failures = patch_mismatches(
        patch_centers(&out),
        ADOBE16_IN_SRGB_ON_WHITE,
        3,
        "adobe16.png",
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn prophoto_png_moves_toward_the_srgb_rendering() {
    let (_, out) = export_file(&icc_fixture("prophoto.png"));
    let actual = patch_centers(&out);
    // 色域内の 4 色目は正解値に合う
    let mut failures = Vec::new();
    if (0..3).any(|c| actual[3][c].abs_diff(PROPHOTO_IN_SRGB[3][c]) > 6) {
        failures.push(format!(
            "prophoto.png パッチ 3: 実際 {:?} / 期待 {:?}（±6）",
            actual[3], PROPHOTO_IN_SRGB[3]
        ));
    }
    // 色域外の 3 色は、moxcms と LCMS で張り付き方が違いうるので、R/G/B の大小関係だけを正解と比べる
    let order = |p: [u8; 3]| [p[0].cmp(&p[1]), p[1].cmp(&p[2]), p[0].cmp(&p[2])];
    for i in 0..3 {
        if order(actual[i]) != order(PROPHOTO_IN_SRGB[i]) {
            failures.push(format!(
                "prophoto.png パッチ {i}: 実際 {:?} の大小関係が正解 {:?} と違う",
                actual[i], PROPHOTO_IN_SRGB[i]
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// JPEG から APP2 の ICC_PROFILE セグメントを全部取り除いたバイト列
fn strip_icc(jpeg: &[u8]) -> Vec<u8> {
    let mut out = jpeg[..2].to_vec();
    let mut pos = 2;
    loop {
        let marker = jpeg[pos + 1];
        if marker == 0xDA {
            out.extend_from_slice(&jpeg[pos..]);
            return out;
        }
        let len = u16::from_be_bytes([jpeg[pos + 2], jpeg[pos + 3]]) as usize;
        let segment = &jpeg[pos..pos + 2 + len];
        if !(marker == 0xE2 && segment[4..].starts_with(b"ICC_PROFILE\0")) {
            out.extend_from_slice(segment);
        }
        pos += 2 + len;
    }
}

/// APP2 の ICC_PROFILE を連番どおりに連結した ICC 本体。無ければ `None`
fn embedded_icc(jpeg: &[u8]) -> Option<Vec<u8>> {
    let mut chunks: Vec<(u8, Vec<u8>)> = Vec::new();
    let mut pos = 2;
    while jpeg[pos + 1] != 0xDA {
        let len = u16::from_be_bytes([jpeg[pos + 2], jpeg[pos + 3]]) as usize;
        let body = &jpeg[pos + 4..pos + 2 + len];
        if jpeg[pos + 1] == 0xE2 && body.starts_with(b"ICC_PROFILE\0") {
            chunks.push((body[12], body[14..].to_vec()));
        }
        pos += 2 + len;
    }
    chunks.sort_by_key(|(seq, _)| *seq);
    (!chunks.is_empty()).then(|| chunks.into_iter().flat_map(|(_, c)| c).collect())
}

#[test]
fn srgb_input_is_not_converted() {
    let original = std::fs::read(icc_fixture("srgb_lrc.jpg")).unwrap();
    let stripped = strip_icc(&original);
    // 前提: 元の入力には ICC があり、取り除いた入力には無い
    assert!(
        embedded_icc(&original).is_some(),
        "前提が崩れている: 入力に ICC が無い"
    );
    assert!(
        embedded_icc(&stripped).is_none(),
        "前提が崩れている: ICC を取り除けていない"
    );
    let dir = tempfile::tempdir().unwrap();
    let stripped_path = dir.path().join("stripped.jpg");
    std::fs::write(&stripped_path, &stripped).unwrap();

    let with_icc = image::load_from_memory(&export_file(&icc_fixture("srgb_lrc.jpg")).1)
        .unwrap()
        .to_rgb8();
    let without_icc = image::load_from_memory(&export_file(&stripped_path).1)
        .unwrap()
        .to_rgb8();
    assert!(
        with_icc == without_icc,
        "sRGB の ICC を持つ入力の出力が、ICC の無い同じ画素の入力の出力と一致しない"
    );
}

/// sRGB の原色（ICC v4 仕様 Annex F の D50 に順応した値。`rXYZ` / `gXYZ` / `bXYZ`）
const SRGB_D50_COLORANTS: [[f64; 3]; 3] = [
    [0.4360, 0.2225, 0.0139],
    [0.3851, 0.7169, 0.0971],
    [0.1431, 0.0606, 0.7141],
];

#[test]
fn output_carries_an_srgb_icc_profile() {
    let (_, out) = export_file(&icc_fixture("adobe.png"));
    let icc = embedded_icc(&out).expect("出力に ICC_PROFILE の APP2 が無い");
    let profile = moxcms::ColorProfile::new_from_slice(&icc).expect("埋め込まれた ICC が読めない");
    assert_eq!(profile.color_space, moxcms::DataColorSpace::Rgb);
    let actual = [
        profile.red_colorant,
        profile.green_colorant,
        profile.blue_colorant,
    ];
    for (a, e) in actual.iter().zip(SRGB_D50_COLORANTS) {
        let a = [a.x, a.y, a.z];
        assert!(
            (0..3).all(|c| (a[c] - e[c]).abs() <= 0.002),
            "原色が sRGB と違う: {a:?} / 期待 {e:?}"
        );
    }
}

#[test]
fn unreadable_icc_is_ignored_with_a_warning() {
    let (result, out) = export_file(&icc_fixture("broken_icc.png"));
    assert_eq!(result.warnings.len(), 1, "{:?}", result.warnings);
    assert!(
        result.warnings[0].starts_with("Ignoring unreadable ICC profile"),
        "{:?}",
        result.warnings
    );
    let failures = patch_mismatches(patch_centers(&out), PATCHES, 6, "broken_icc.png");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn icc_that_does_not_match_the_pixels_is_ignored_with_a_warning() {
    // RGB のパッチ（adobe.png の画素）に、Gray の ICC を付けた PNG をテスト内で作る
    let patch = image::open(icc_fixture("adobe.png")).unwrap().to_rgb8();
    let gray_icc = moxcms::ColorProfile::new_gray_with_gamma(1.0)
        .encode()
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("rgb_with_gray_icc.png");
    let mut encoder = image::codecs::png::PngEncoder::new(std::io::BufWriter::new(
        std::fs::File::create(&input).unwrap(),
    ));
    use image::ImageEncoder as _;
    encoder.set_icc_profile(gray_icc).unwrap();
    encoder
        .write_image(patch.as_raw(), 32, 8, image::ExtendedColorType::Rgb8)
        .unwrap();

    let (result, out) = export_file(&input);
    assert_eq!(result.warnings.len(), 1, "{:?}", result.warnings);
    assert!(
        result.warnings[0].starts_with("Ignoring ICC profile that cannot be converted"),
        "{:?}",
        result.warnings
    );
    let failures = patch_mismatches(patch_centers(&out), PATCHES, 6, "Gray の ICC 付き RGB");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn png_whose_output_exceeds_the_memory_limit_is_rejected() {
    let input = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/over_limit.png");
    // 前提: 正当な PNG で、寸法は 12000×12000（RGBA8 で 576,000,000 バイト > 512MiB）
    assert_eq!(image::image_dimensions(&input).unwrap(), (12000, 12000));
    let out = tempfile::tempdir().unwrap();
    let config = ProcessingConfig {
        mode: ConversionMode::Quality,
        bg_color: BackgroundColor::White,
        quality: 90,
        max_size_mb: 8,
        delete_originals: false,
        max_width: None,
    };
    assert!(process_image(&input, out.path(), &config, None, None).is_err());
}
