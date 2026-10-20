//! 書き出し品質の実写真での計測（spike）。サイズ・PSNR・DQT・サンプリングを TSV に書く。
//!
//! JPEG エンコーダを差し替える前の数値を残し、差し替え後に同じ道具で比べるためのもの。
//! 差し替えた後は「現行」の数値を作り直せない。
//!
//! 通常のテスト実行では走らない（実写真をリポジトリに入れないため）。
//!
//! 実行:
//! ```text
//! PICTURE_TOOL_SPIKE_IMAGES=a.jpg:b.jxl \
//! PICTURE_TOOL_SPIKE_QUALITIES=1,5,10,20,40,60,90,100 \
//! PICTURE_TOOL_SPIKE_OUT=/path/to/result.tsv \
//!   cargo test -p picture-tool-core --release --test output_quality_spike -- --ignored measure
//! ```
//! 出力は要約されうるので、結果は `PICTURE_TOOL_SPIKE_OUT` のファイルを読む。
//! PSNR は 8bit RGB の全 3 チャンネルの MSE から `10·log10(255²/MSE)`。基準は入力を
//! `open_image_oriented` でデコードした RGB8。

mod common;

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use common::jpeg::{dqt, sof_sampling};
use picture_tool_core::{
    BackgroundColor, ConversionMode, ProcessingConfig, open_image_oriented, process_image,
};

fn psnr(a: &image::RgbImage, b: &image::RgbImage) -> f64 {
    assert_eq!(a.dimensions(), b.dimensions(), "寸法が違う");
    let sum: u64 = a
        .as_raw()
        .iter()
        .zip(b.as_raw())
        .map(|(&x, &y)| {
            let d = x as i64 - y as i64;
            (d * d) as u64
        })
        .sum();
    if sum == 0 {
        return f64::INFINITY;
    }
    let mse = sum as f64 / a.as_raw().len() as f64;
    10.0 * (255.0f64 * 255.0 / mse).log10()
}

#[test]
#[ignore = "実写真が要る。実行方法はモジュールコメント"]
fn measure() {
    let images = std::env::var("PICTURE_TOOL_SPIKE_IMAGES").expect("PICTURE_TOOL_SPIKE_IMAGES");
    let qualities: Vec<u8> = std::env::var("PICTURE_TOOL_SPIKE_QUALITIES")
        .unwrap_or_else(|_| "1,5,10,20,40,60,90,100".into())
        .split(',')
        .map(|q| q.trim().parse().expect("品質は整数"))
        .collect();
    let out_path = std::env::var("PICTURE_TOOL_SPIKE_OUT").expect("PICTURE_TOOL_SPIKE_OUT");

    let mut tsv = String::from("file\tquality\tbytes\tpsnr_db\tluma_max\tchroma_max\tsampling\n");
    for input in images
        .split(':')
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
    {
        let reference = open_image_oriented(&input)
            .expect("入力を開けない")
            .to_rgb8();
        for &quality in &qualities {
            let dir = tempfile::tempdir().unwrap();
            let config = ProcessingConfig {
                mode: ConversionMode::Quality,
                bg_color: BackgroundColor::White,
                quality,
                max_size_mb: 1024,
                delete_originals: false,
                max_width: None,
            };
            let result = process_image(&input, dir.path(), &config, None, None).expect("変換");
            let bytes = std::fs::read(Path::new(&result.output_path)).unwrap();
            let decoded = image::load_from_memory(&bytes).unwrap().to_rgb8();
            let tables = dqt(&bytes);
            let max_of = |id: u8| {
                tables
                    .iter()
                    .find(|(t, _)| *t == id)
                    .map(|(_, v)| *v.iter().max().unwrap())
                    .map_or("-".to_string(), |m| m.to_string())
            };
            let sampling = sof_sampling(&bytes)
                .iter()
                .map(|(h, v)| format!("{h}x{v}"))
                .collect::<Vec<_>>()
                .join("/");
            writeln!(
                tsv,
                "{}\t{}\t{}\t{:.2}\t{}\t{}\t{}",
                input.file_name().unwrap().to_string_lossy(),
                quality,
                bytes.len(),
                psnr(&reference, &decoded),
                max_of(0),
                max_of(1),
                sampling
            )
            .unwrap();
        }
    }
    std::fs::write(out_path, tsv).unwrap();
}
