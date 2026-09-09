//! JPEG の先頭（SOI〜SOS）だけを読み、**APP1 だけを写した最小の JPEG** を組み立てる。
//!
//! Exif も XMP も SOS（画素データの開始）より前の APP1 セグメントにある。
//! ★1 つを知るためにファイル全体（5MB）を読む必要は無い ── 実測で
//! 1,000 枚あたり 1,118ms → 68ms、ピーク RSS 201MB → 7.4MB
//! （`core/tests/rating_scan_spike.rs` / spec の「実測」節）。
//!
//! **ここは解析をしない。** 組み立てたバイト列は
//! [`super::read_metadata_bytes`] がそのまま食える形にしてある ──
//! 解析の実装を変えずに「読む量」だけを変えるのが設計の核心で、
//! 実測でも解析コストは差に現れなかった（この機能は I/O 律速である）。

use std::fs::File;
use std::io::{BufReader, Read, Seek};
use std::path::Path;

fn broken(reason: &'static str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, reason)
}

/// 先頭だけを読んだ最小の JPEG を返す。
pub(super) fn header_bytes(path: &Path) -> std::io::Result<Vec<u8>> {
    scan(&mut BufReader::with_capacity(64 * 1024, File::open(path)?))
}

fn scan<R: Read + Seek>(file: &mut BufReader<R>) -> std::io::Result<Vec<u8>> {
    let mut out = Vec::with_capacity(64 * 1024);

    let mut soi = [0u8; 2];
    file.read_exact(&mut soi)?;
    if soi != [0xFF, 0xD8] {
        return Err(broken("SOI ではない"));
    }
    out.extend_from_slice(&soi);

    let mut byte = [0u8; 1];
    loop {
        file.read_exact(&mut byte)?;
        if byte[0] != 0xFF {
            return Err(broken("セグメント境界が 0xFF で始まっていない"));
        }
        // 0xFF のパディング（fill byte）を読み飛ばす
        let mut marker = 0xFF;
        while marker == 0xFF {
            file.read_exact(&mut byte)?;
            marker = byte[0];
        }
        match marker {
            // 長さを持たないマーカー
            0xD9 => {
                // APP1 も SOS も無いまま EOI へ着いた。**Err にしない** ──
                // 画素を持たない退化した JPEG でも「全項目 None」で成立する
                out.extend_from_slice(&[0xFF, 0xD9]);
                break;
            }
            0x01 | 0xD0..=0xD7 => continue,
            _ => {}
        }

        let mut len_bytes = [0u8; 2];
        file.read_exact(&mut len_bytes)?;
        let len = u16::from_be_bytes(len_bytes) as usize;
        if len < 2 {
            return Err(broken("セグメント長が短すぎる"));
        }
        let payload = len - 2;

        if marker == 0xDA {
            // SOS。ヘッダだけ写し、画素は 1 バイトも読まずに EOI で閉じる
            let mut body = vec![0u8; payload];
            file.read_exact(&mut body)?;
            out.extend_from_slice(&[0xFF, 0xDA]);
            out.extend_from_slice(&len_bytes);
            out.extend_from_slice(&body);
            out.extend_from_slice(&[0xFF, 0xD9]);
            break;
        }

        if marker == 0xE1 {
            // APP1 = Exif と XMP。**これだけが要る**
            let mut body = vec![0u8; payload];
            file.read_exact(&mut body)?;
            out.extend_from_slice(&[0xFF, marker]);
            out.extend_from_slice(&len_bytes);
            out.extend_from_slice(&body);
        } else {
            // **`Seek::seek` ではなく `seek_relative`。** 前者は BufReader の
            // バッファを無条件に捨てるので、飛ばした先を読むために同じ 64KB を
            // 読み直すことになる（spike の実測で 2 倍遅くなった原因。
            // spec の「計測の罠」3）
            file.seek_relative(payload as i64)?;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// 長さ付きセグメント 1 つ分のバイト列。`len` は自分自身の 2 バイトを含む
    fn segment(marker: u8, payload: &[u8]) -> Vec<u8> {
        let len = u16::try_from(payload.len() + 2).unwrap();
        let mut out = vec![0xFF, marker];
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(payload);
        out
    }

    fn run(bytes: Vec<u8>) -> std::io::Result<Vec<u8>> {
        scan(&mut BufReader::new(Cursor::new(bytes)))
    }

    #[test]
    fn app1_だけを写し_画素は一切読まない() {
        let app1 = segment(0xE1, b"Exif\0\0dummy-exif-payload");
        let mut input = vec![0xFF, 0xD8];
        input.extend_from_slice(&segment(
            0xE0,
            b"JFIF\0\xE6\x8D\xA8\xE3\x81\xA6\xE3\x82\x89\xE3\x82\x8C\xE3\x82\x8B",
        )); // APP0（中身は「捨てられる」の UTF-8 バイト列。b"..." は ASCII のみのため \xHH で表現）
        input.extend_from_slice(&app1);
        input.extend_from_slice(&segment(0xDB, b"quantization")); // DQT
        input.extend_from_slice(&segment(0xDA, b"sos-header")); // SOS
        input.extend_from_slice(&[0x12; 4096]); // 画素データ
        input.extend_from_slice(&[0xFF, 0xD9]); // EOI

        let mut expected = vec![0xFF, 0xD8];
        expected.extend_from_slice(&app1);
        expected.extend_from_slice(&segment(0xDA, b"sos-header"));
        expected.extend_from_slice(&[0xFF, 0xD9]);

        assert_eq!(run(input).unwrap(), expected);
    }

    #[test]
    fn app1_を持たない_jpeg_でも最小の_jpeg_が返る() {
        let mut input = vec![0xFF, 0xD8];
        input.extend_from_slice(&segment(0xE0, b"JFIF\0"));
        input.extend_from_slice(&segment(0xDA, b"sos"));
        input.extend_from_slice(&[0x34; 512]);
        input.extend_from_slice(&[0xFF, 0xD9]);

        let mut expected = vec![0xFF, 0xD8];
        expected.extend_from_slice(&segment(0xDA, b"sos"));
        expected.extend_from_slice(&[0xFF, 0xD9]);

        assert_eq!(run(input).unwrap(), expected);
    }

    #[test]
    fn sos_の前に_eoi_が来ても_eoi_で閉じて返す() {
        // Exif も画素も持たない退化した JPEG。ここで Err にすると
        // Task 2 のフォールバックが無駄に全体読みへ落ちる
        let input = vec![0xFF, 0xD8, 0xFF, 0xD9];
        assert_eq!(run(input).unwrap(), vec![0xFF, 0xD8, 0xFF, 0xD9]);
    }

    #[test]
    fn マーカー前の_0xff_パディングを読み飛ばす() {
        let app1 = segment(0xE1, b"Exif\0\0x");
        let mut input = vec![0xFF, 0xD8, 0xFF, 0xFF, 0xFF]; // fill byte が 2 つ
        input.extend_from_slice(&app1[1..]); // 0xFF は上で出したので marker から
        input.extend_from_slice(&segment(0xDA, b"s"));
        input.extend_from_slice(&[0xFF, 0xD9]);

        let mut expected = vec![0xFF, 0xD8];
        expected.extend_from_slice(&app1);
        expected.extend_from_slice(&segment(0xDA, b"s"));
        expected.extend_from_slice(&[0xFF, 0xD9]);

        assert_eq!(run(input).unwrap(), expected);
    }

    #[test]
    fn soi_で始まらなければ_err() {
        // PNG のヘッダ。Task 2 の入口が拡張子で弾くので通常は到達しないが、
        // 拡張子を偽った .jpg はここへ来る
        let input = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        assert!(run(input).is_err());
    }

    #[test]
    fn セグメント境界が_0xff_で始まらなければ_err() {
        let input = vec![0xFF, 0xD8, 0x00, 0xE1, 0x00, 0x04, 0x00, 0x00];
        assert!(run(input).is_err());
    }

    #[test]
    fn セグメント長が_2_未満なら_err() {
        // len = 1。payload = -1 バイトになるので、素直に引き算すると panic する
        let input = vec![0xFF, 0xD8, 0xFF, 0xE1, 0x00, 0x01];
        assert!(run(input).is_err());
    }

    #[test]
    fn 途中で切れたファイルは_err_になり_panic_しない() {
        // APP1 の長さは 1000 バイトだと言っているが、実体は 4 バイトしかない
        let input = vec![0xFF, 0xD8, 0xFF, 0xE1, 0x03, 0xE8, 1, 2, 3, 4];
        assert!(run(input).is_err());
    }
}
