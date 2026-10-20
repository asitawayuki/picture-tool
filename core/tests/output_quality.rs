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
