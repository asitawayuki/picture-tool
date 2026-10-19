//! 1 枚の画像を `open_image_oriented` で開き、結果を 1 行で表示する（AVIF デコーダの検証用）
//!
//! 使い方: `cargo run -p picture-tool-core --example avif_probe -- <path> [--dump <out>]`
//!
//! - 標準出力: `Ok <w>x<h> <色型>` か `Err <理由>`。終了コードは Ok=0、Err=1
//! - `--dump <out>`: `<色型>\n<w> <h>\n` の後に生のサンプル列を書く（16bit は各サンプルを LE 2 バイト）
//!
//! CLI はフォルダー単位で JPEG を書くので、1 枚の結果と生の画素を取るためにこれを使う。
//! 差し替え前後の出力の照合と、abort の回帰の確認に使う。

use image::DynamicImage;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);
    let usage = "usage: avif_probe <path> [--dump <out>]";
    let Some(path) = args.next().map(PathBuf::from) else {
        eprintln!("{usage}");
        return ExitCode::from(2);
    };
    let dump = match (args.next(), args.next(), args.next()) {
        (None, ..) => None,
        (Some(flag), Some(out), None) if flag == "--dump" => Some(PathBuf::from(out)),
        _ => {
            eprintln!("{usage}");
            return ExitCode::from(2);
        }
    };

    let img = match picture_tool_core::open_image_oriented(&path) {
        Ok(img) => img,
        Err(err) => {
            println!("Err {err:#}");
            return ExitCode::from(1);
        }
    };
    let color = format!("{:?}", img.color());
    println!("Ok {}x{} {color}", img.width(), img.height());

    if let Some(out) = dump {
        let mut bytes = format!("{color}\n{} {}\n", img.width(), img.height()).into_bytes();
        bytes.extend(samples_le(&img));
        if let Err(err) = std::fs::write(&out, bytes) {
            eprintln!("failed to write {}: {err}", out.display());
            return ExitCode::from(2);
        }
    }
    ExitCode::SUCCESS
}

/// サンプル列。16bit は各サンプルを LE 2 バイトにする（`as_bytes` はネイティブ順なので使わない）
fn samples_le(img: &DynamicImage) -> Vec<u8> {
    let le16 = |s: &[u16]| s.iter().flat_map(|v| v.to_le_bytes()).collect();
    match img {
        DynamicImage::ImageLuma16(i) => le16(i.as_raw()),
        DynamicImage::ImageLumaA16(i) => le16(i.as_raw()),
        DynamicImage::ImageRgb16(i) => le16(i.as_raw()),
        DynamicImage::ImageRgba16(i) => le16(i.as_raw()),
        DynamicImage::ImageLuma8(_)
        | DynamicImage::ImageLumaA8(_)
        | DynamicImage::ImageRgb8(_)
        | DynamicImage::ImageRgba8(_) => img.as_bytes().to_vec(),
        // AVIF のデコードは f32 を返さない。来たら照合の前提が崩れているので気づけるようにする
        other => panic!("unexpected color type {:?}", other.color()),
    }
}
