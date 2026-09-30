//! 書き込みで**既存のメタデータが失われていないか**を確かめる
//! （spec `docs/superpowers/specs/2026-09-29-metadata-write-verification-design.md`）。
//!
//! 入力は書き込み前後の **SOS より前のバイト列**だけで、ファイルには触らない。
//! 画素データの一致は `atomic::replace_verified` の冒頭が受け持つ。
//!
//! | 検査 | 守るもの |
//! |---|---|
//! | 1 | Exif の全フィールド（編集対象 5 タグと、位置に依存するだけの値を除く） |
//! | 2 | IFD1 サムネイル本体のバイト列 |
//! | 3 | Exif・XMP 以外のセグメント（ICC・IPTC・拡張 XMP・JFIF・量子化表など）と相対順序 |
//!
//! **Exif は kamadak-exif の厳格読み（`continue_on_error` は既定の `false`）で読む。**
//! 書き込み側の `little_exif` とは独立した実装で見るので、「little_exif で書いて
//! little_exif で読めた」の同語反復にならない。読めなければ比べる基準が無いので拒否する。

use anyhow::{Result, anyhow, bail};
use exif::{Exif, Value};

const APP0: u8 = 0xE0;
const APP1: u8 = 0xE1;
const EXIF_ID: &[u8] = b"Exif\0\0";

/// SOS より前の 1 セグメント。`contents` は長さフィールドの後ろ（識別子を含む）
#[derive(Debug, PartialEq)]
struct Segment<'a> {
    marker: u8,
    contents: &'a [u8],
}

/// `head`（SOI から SOS の直前まで）をセグメントに分ける。
///
/// 走査の規則は `atomic::scan_offset` と同じ（0xFF の fill byte を読み飛ばす）。
/// 入力は webview 由来のパスから読んだバイト列なので、添字は長さで押さえてから触る。
fn segments(head: &[u8]) -> Result<Vec<Segment<'_>>> {
    let mut found = Vec::new();
    let mut cursor = 2; // SOI をとばす
    while cursor < head.len() {
        if head[cursor] != 0xFF {
            bail!("JPEG のセグメント境界が壊れています（offset {}）", cursor);
        }
        let mut marker_at = cursor + 1;
        while marker_at < head.len() && head[marker_at] == 0xFF {
            marker_at += 1;
        }
        if marker_at + 2 >= head.len() {
            bail!("JPEG のセグメントが途中で切れています（offset {}）", cursor);
        }
        let length = u16::from_be_bytes([head[marker_at + 1], head[marker_at + 2]]) as usize;
        let end = marker_at + 1 + length;
        if length < 2 || end > head.len() {
            bail!("JPEG のセグメント長が壊れています（offset {}）", cursor);
        }
        found.push(Segment {
            marker: head[marker_at],
            contents: &head[marker_at + 3..end],
        });
        cursor = end;
    }
    Ok(found)
}

/// `prefix` で始まる**最初の** APP1 の位置。
/// `has_exif_segment`・`xmp::extract`・kamadak-exif が見るのも最初の 1 つだけ
fn first_app1(segments: &[Segment], prefix: &[u8]) -> Option<usize> {
    segments
        .iter()
        .position(|s| s.marker == APP1 && s.contents.starts_with(prefix))
}

/// 最初の Exif APP1 を kamadak-exif の厳格読みで読む。Exif APP1 が無ければ `None`。
///
/// `read_from_container` は使わない ── 有無の判定が `has_exif_segment` と 2 系統に
/// 分かれ、食い違えば検査 1・2 が空振りで通るため（spec「読めない場合の扱い」）
fn read_exif(segments: &[Segment]) -> Result<Option<Exif>> {
    let Some(at) = first_app1(segments, EXIF_ID) else {
        return Ok(None);
    };
    let tiff = segments[at].contents[EXIF_ID.len()..].to_vec();
    exif::Reader::new()
        .read_raw(tiff)
        .map(Some)
        .map_err(|_| anyhow!("既存の Exif を検証できないため中止しました"))
}

/// kamadak-exif 0.6 の `Value` は `PartialEq` を持たないので、spec の規則で比べる。
///
/// - バリアントが違えば不一致（SHORT → LONG の書き戻しも「変わった」）
/// - `Undefined` の第 2 要素と `Unknown` の第 3 要素は **Exif 内の位置**なので無視する
/// - 浮動小数はビット列で比べる（`NaN` が自分自身と不一致になる誤拒否を避ける）
/// - 有理数は約分せず分子・分母の組で比べる（`Rational` も `PartialEq` を持たない）
fn same_value(a: &Value, b: &Value) -> bool {
    use Value::*;
    match (a, b) {
        (Byte(x), Byte(y)) => x == y,
        (Ascii(x), Ascii(y)) => x == y,
        (Short(x), Short(y)) => x == y,
        (Long(x), Long(y)) => x == y,
        (SByte(x), SByte(y)) => x == y,
        (SShort(x), SShort(y)) => x == y,
        (SLong(x), SLong(y)) => x == y,
        (Rational(x), Rational(y)) => {
            x.len() == y.len()
                && x.iter()
                    .zip(y)
                    .all(|(p, q)| (p.num, p.denom) == (q.num, q.denom))
        }
        (SRational(x), SRational(y)) => {
            x.len() == y.len()
                && x.iter()
                    .zip(y)
                    .all(|(p, q)| (p.num, p.denom) == (q.num, q.denom))
        }
        (Float(x), Float(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| p.to_bits() == q.to_bits())
        }
        (Double(x), Double(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| p.to_bits() == q.to_bits())
        }
        (Undefined(x, _), Undefined(y, _)) => x == y,
        // 未知の型コードは中身を返さないので、中身の変化は検知できない（spec の残る制約）
        (Unknown(tx, cx, _), Unknown(ty, cy, _)) => (tx, cx) == (ty, cy),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use exif::{In, Tag};
    use std::path::PathBuf;

    // --- 入力の組み立て ------------------------------------------------------

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures")
                .join(name),
        )
        .unwrap()
    }

    /// SOS より前だけを切り出す（`check` への入力の形）
    fn head_of(bytes: &[u8]) -> Vec<u8> {
        bytes[..super::super::atomic::scan_offset(bytes).unwrap()].to_vec()
    }

    /// SOI ＋ 与えたセグメント列（SOS より前の形）
    fn head_with(segments: &[(u8, &[u8])]) -> Vec<u8> {
        let mut head = vec![0xFF, 0xD8];
        for (marker, contents) in segments {
            head.extend([0xFF, *marker]);
            head.extend(((contents.len() + 2) as u16).to_be_bytes());
            head.extend(*contents);
        }
        head
    }

    // --- セグメントの走査 ----------------------------------------------------

    #[test]
    fn segments_splits_a_head_into_markers_and_contents() {
        let head = head_with(&[(APP0, b"JFIF\0abc"), (0xDB, b"\x00\x01")]);
        assert_eq!(
            segments(&head).unwrap(),
            vec![
                Segment {
                    marker: APP0,
                    contents: b"JFIF\0abc"
                },
                Segment {
                    marker: 0xDB,
                    contents: b"\x00\x01"
                },
            ]
        );
    }

    /// 実データの SOS より前が、exiv2 で見た構造どおりに分かれる
    /// （APP0・Exif APP1・XMP APP1・SOF0・DQT×2・DHT×4）
    #[test]
    fn segments_reads_the_fixture_structure() {
        let head = head_of(&fixture("lightroom-xmp.jpg"));
        let markers: Vec<u8> = segments(&head).unwrap().iter().map(|s| s.marker).collect();
        assert_eq!(
            markers,
            vec![APP0, APP1, APP1, 0xC0, 0xDB, 0xDB, 0xC4, 0xC4, 0xC4, 0xC4]
        );
    }

    /// 壊れた入力で panic せず Err を返す
    #[test]
    fn segments_never_panics_on_malformed_input() {
        let cases: Vec<Vec<u8>> = vec![
            vec![0xFF, 0xD8, 0xFF],
            vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00],
            // 長さが末尾を飛び越える
            vec![0xFF, 0xD8, 0xFF, 0xE0, 0xFF, 0xFF, 0x11],
            // 長さが 2 未満
            vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x01, 0x11],
            // 境界が 0xFF で始まらない
            vec![0xFF, 0xD8, 0x00, 0x00, 0x00],
        ];
        for head in cases {
            assert!(
                segments(&head).is_err(),
                "壊れた入力を Ok にした: {:?}",
                head
            );
        }
    }

    // --- Exif の読み方 -------------------------------------------------------

    #[test]
    fn read_exif_reads_the_first_exif_app1_of_the_fixture() {
        let head = head_of(&fixture("exif-only.jpg"));
        let exif = read_exif(&segments(&head).unwrap())
            .unwrap()
            .expect("Exif が無い");
        assert!(exif.get_field(Tag::Make, In::PRIMARY).is_some());
    }

    #[test]
    fn read_exif_returns_none_without_an_exif_app1() {
        let head = head_with(&[(APP0, b"JFIF\0abc")]);
        assert!(read_exif(&segments(&head).unwrap()).unwrap().is_none());
    }

    /// Exif APP1 はあるが TIFF として読めない → 「検証できない」
    #[test]
    fn read_exif_refuses_an_exif_app1_that_kamadak_cannot_read() {
        let head = head_with(&[(APP1, b"Exif\0\0\xAA\xAA\xAA\xAA\xAA\xAA\xAA\xAA")]);
        // `Exif` は Debug を持たないので unwrap_err は使えない
        let Err(error) = read_exif(&segments(&head).unwrap()) else {
            panic!("読めない Exif を読めたことにした");
        };
        assert!(
            error.to_string().contains("既存の Exif を検証できない"),
            "{error:#}"
        );
    }

    // --- 値の比較規則 --------------------------------------------------------

    #[test]
    fn same_value_ignores_the_position_of_undefined_and_unknown() {
        assert!(same_value(
            &Value::Undefined(b"0232".to_vec(), 100),
            &Value::Undefined(b"0232".to_vec(), 250)
        ));
        assert!(same_value(
            &Value::Unknown(99, 4, 10),
            &Value::Unknown(99, 4, 30)
        ));
    }

    #[test]
    fn same_value_detects_changed_bytes_and_changed_unknown_shape() {
        assert!(!same_value(
            &Value::Undefined(b"0232".to_vec(), 100),
            &Value::Undefined(b"0231".to_vec(), 100)
        ));
        assert!(!same_value(
            &Value::Unknown(99, 4, 10),
            &Value::Unknown(99, 5, 10)
        ));
        assert!(!same_value(
            &Value::Unknown(99, 4, 10),
            &Value::Unknown(98, 4, 10)
        ));
    }

    /// 同じ数値でも型が違えば「変わった」
    #[test]
    fn same_value_treats_a_type_change_as_a_change() {
        assert!(!same_value(&Value::Short(vec![2]), &Value::Long(vec![2])));
    }

    /// NaN は自分自身と一致させる（`==` で比べると誤拒否になる）。+0.0 と -0.0 は別物
    #[test]
    fn same_value_compares_floats_by_bits() {
        assert!(same_value(
            &Value::Float(vec![f32::NAN]),
            &Value::Float(vec![f32::NAN])
        ));
        assert!(same_value(
            &Value::Double(vec![f64::NAN]),
            &Value::Double(vec![f64::NAN])
        ));
        assert!(!same_value(
            &Value::Float(vec![0.0]),
            &Value::Float(vec![-0.0])
        ));
    }

    /// 1/2 と 2/4 は別物（約分しない）
    #[test]
    fn same_value_does_not_reduce_rationals() {
        let r = |num, denom| exif::Rational { num, denom };
        assert!(same_value(
            &Value::Rational(vec![r(1, 2)]),
            &Value::Rational(vec![r(1, 2)])
        ));
        assert!(!same_value(
            &Value::Rational(vec![r(1, 2)]),
            &Value::Rational(vec![r(2, 4)])
        ));
        let s = |num, denom| exif::SRational { num, denom };
        assert!(!same_value(
            &Value::SRational(vec![s(-1, 2)]),
            &Value::SRational(vec![s(-2, 4)])
        ));
    }

    #[test]
    fn same_value_compares_element_sequences() {
        assert!(same_value(
            &Value::Ascii(vec![b"SONY".to_vec()]),
            &Value::Ascii(vec![b"SONY".to_vec()])
        ));
        assert!(!same_value(
            &Value::Ascii(vec![b"SONY".to_vec()]),
            &Value::Ascii(vec![b"Sony".to_vec()])
        ));
        assert!(!same_value(
            &Value::Short(vec![1, 2]),
            &Value::Short(vec![1])
        ));
    }
}
