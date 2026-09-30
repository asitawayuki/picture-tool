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
use exif::{Context, Exif, Field, In, Tag, Value};

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

/// 比較から外すフィールドか（spec 検査 1「比較から除外するキー」）。
///
/// 編集対象の 5 タグ（値の正しさは `atomic::verify` の読み戻し検査が受け持つ）と、
/// サムネイルへのオフセット（サムネイル本体は検査 2 が受け持つ）。
/// **context を取り違えると除外が効かず、全保存が拒否される。**
/// kamadak-exif には XPTitle 等の名前付き定数が無いので番号で組み立てる
fn excluded(field: &Field) -> bool {
    const EDITED: [Tag; 5] = [
        Tag::ImageDescription,
        Tag(Context::Tiff, 0x9c9b), // XPTitle
        Tag(Context::Tiff, 0x9c9c), // XPComment
        Tag(Context::Tiff, 0x4749), // RatingPercent
        Tag::UserComment,
    ];
    match field.ifd_num {
        In::PRIMARY => EDITED.contains(&field.tag),
        In::THUMBNAIL => field.tag == Tag::JPEGInterchangeFormat,
        _ => false,
    }
}

/// 書き込み前にあったのに、書き込み後に**同じ値で**残っていないフィールドのキー。
///
/// 多重集合の包含「書き込み前 ⊆ 書き込み後」を見る。書き込み後にだけあるものは許す
/// （編集対象を初めて書けば増える）。書き込み後に Exif が無ければ空集合とみなす。
/// 返すキーは `(IFD 番号, タグ番号)` 順で重複なし
fn lost_fields(before: &Exif, after: Option<&Exif>) -> Vec<(In, Tag)> {
    let mut remaining: Vec<&Field> = after
        .map(|exif| exif.fields().filter(|f| !excluded(f)).collect())
        .unwrap_or_default();
    let mut lost = Vec::new();
    for field in before.fields().filter(|f| !excluded(f)) {
        let kept = remaining.iter().position(|candidate| {
            candidate.ifd_num == field.ifd_num
                && candidate.tag == field.tag
                && same_value(&candidate.value, &field.value)
        });
        match kept {
            Some(at) => {
                remaining.swap_remove(at);
            }
            None => lost.push((field.ifd_num, field.tag)),
        }
    }
    lost.sort_by_key(|(ifd, tag)| (ifd.index(), tag.number(), tag.context()));
    lost.dedup();
    lost
}

/// 利用者に見せるキーの表記。kamadak-exif の `Tag` の `Display` は名前を知らないタグで
/// `Tag(Tiff, 18249)` に落ちるので、そのときは 16 進の番号にする
fn key_label((ifd, tag): (In, Tag)) -> String {
    let name = if tag.description().is_some() {
        tag.to_string()
    } else {
        format!("0x{:04x}", tag.number())
    };
    if ifd == In::THUMBNAIL {
        format!("{name}（サムネイル）")
    } else {
        name
    }
}

/// 検査 1 の拒否の文面。最大 3 件を並べ、残りは件数だけ
fn lost_message(lost: &[(In, Tag)]) -> String {
    let shown: Vec<String> = lost.iter().take(3).map(|key| key_label(*key)).collect();
    let mut message = format!(
        "既存の Exif が保存で失われるか変わるため中止しました: {}",
        shown.join(", ")
    );
    if lost.len() > 3 {
        message.push_str(&format!(" ほか {} 件", lost.len() - 3));
    }
    message
}

/// IFD1 サムネイル本体。オフセットは TIFF 先頭（`Exif\0\0` の直後）基準で、
/// 検査 1 で読んだのと同じ Exif APP1 の中身から切り出す。
///
/// `JPEGInterchangeFormat` と `JPEGInterchangeFormatLength` の**両方**があり、
/// 範囲が溢れず TIFF 本体に収まるときだけ `Some`。範囲外で panic しない
/// （panic は `catch_unwind` で捕まるが、文面が「異常終了しました」に化ける）
fn thumbnail(exif: &Exif) -> Option<&[u8]> {
    let uint = |tag| exif.get_field(tag, In::THUMBNAIL)?.value.get_uint(0);
    let offset = uint(Tag::JPEGInterchangeFormat)?;
    let length = uint(Tag::JPEGInterchangeFormatLength)?;
    let end = offset.checked_add(length)?;
    exif.buf().get(offset as usize..end as usize)
}

/// 書き込み前後の SOS より前を比べ、既存のメタデータが失われていれば `Err`。
///
/// `Ok` の中身は保存を止めない警告。
/// 拒否の文面は `write_metadata` の文脈「メタデータを保存できませんでした…」の
/// **後ろに `: ` で連結されて**利用者に届くので、単独で意味が通るように書く
pub(super) fn check(original_head: &[u8], written_head: &[u8]) -> Result<Vec<String>> {
    let before_segments = segments(original_head)?;
    let after_segments = segments(written_head)?;
    let warnings = Vec::new();

    if let Some(before) = read_exif(&before_segments)? {
        let after = read_exif(&after_segments)?;

        // 検査 1: Exif 全フィールドの保全
        let lost = lost_fields(&before, after.as_ref());
        if !lost.is_empty() {
            bail!(lost_message(&lost));
        }

        // 書き込み前に 1 つでも比べるフィールドがあれば、検査 1 を通った時点で after は
        // 必ず Some。フィールドが 1 つも無い Exif から Exif が消えた場合だけ None で残る
        if let Some(after) = &after {
            // 検査 2: サムネイル本体（元写真のサムネイルが壊れているか無ければ比べない）
            if let Some(original_thumbnail) = thumbnail(&before)
                && thumbnail(after) != Some(original_thumbnail)
            {
                bail!("Exif のサムネイルが保存で変わるため中止しました");
            }
        }
    }

    Ok(warnings)
}

#[cfg(test)]
mod tests {
    use super::*;
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

    // --- TIFF 本体の書き換え（検査 1・2 の入力づくり） -----------------------
    //
    // 書き込み後のバイト列は、フィクスチャのコピーの TIFF 本体をその場で書き換えて作る
    // （spec「テスト」）。オフセットをずらさないので、変えたもの以外は前後で同じになる。
    // フィクスチャは 2 枚ともリトルエンディアン（`II`）。

    fn u16_le(tiff: &[u8], at: usize) -> u16 {
        u16::from_le_bytes([tiff[at], tiff[at + 1]])
    }

    fn u32_le(tiff: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(tiff[at..at + 4].try_into().unwrap())
    }

    fn ifd0(tiff: &[u8]) -> usize {
        assert_eq!(&tiff[..2], b"II", "前提条件: リトルエンディアンの TIFF");
        u32_le(tiff, 4) as usize
    }

    /// IFD0 の直後の「次の IFD」が IFD1（サムネイル側）
    fn ifd1(tiff: &[u8]) -> usize {
        let ifd0 = ifd0(tiff);
        let count = u16_le(tiff, ifd0) as usize;
        u32_le(tiff, ifd0 + 2 + 12 * count) as usize
    }

    /// IFD 内で `tag` を持つエントリの位置（TIFF 先頭基準）
    fn entry(tiff: &[u8], ifd: usize, tag: u16) -> usize {
        let count = u16_le(tiff, ifd) as usize;
        (0..count)
            .map(|i| ifd + 2 + 12 * i)
            .find(|&at| u16_le(tiff, at) == tag)
            .unwrap_or_else(|| panic!("前提条件: タグ 0x{tag:04x} が IFD（{ifd}）に無い"))
    }

    /// エントリのタグ番号を、どのタグとも衝突しない番号へ付け替える。
    /// kamadak-exif から見ると「元のタグが消え、未知のタグが増えた」になる
    fn rename_tag(tiff: &mut [u8], ifd: usize, tag: u16, to: u16) {
        let at = entry(tiff, ifd, tag);
        tiff[at..at + 2].copy_from_slice(&to.to_le_bytes());
    }

    /// head の最初の Exif APP1 の TIFF 本体だけを `edit` で書き換えた head を返す
    fn edit_tiff(head: &[u8], edit: impl FnOnce(&mut Vec<u8>)) -> Vec<u8> {
        let segments = segments(head).unwrap();
        let exif_at = first_app1(&segments, EXIF_ID).expect("前提条件: Exif APP1 が無い");
        let mut tiff = segments[exif_at].contents[EXIF_ID.len()..].to_vec();
        edit(&mut tiff);
        let exif_contents = [EXIF_ID, &tiff[..]].concat();
        let rebuilt: Vec<(u8, &[u8])> = segments
            .iter()
            .enumerate()
            .map(|(at, s)| {
                if at == exif_at {
                    (s.marker, &exif_contents[..])
                } else {
                    (s.marker, s.contents)
                }
            })
            .collect();
        head_with(&rebuilt)
    }

    fn exif_of(head: &[u8]) -> Exif {
        read_exif(&segments(head).unwrap())
            .unwrap()
            .expect("Exif が無い")
    }

    fn refusal(before: &[u8], after: &[u8]) -> String {
        match check(before, after) {
            Ok(warnings) => panic!("拒否されなかった（警告 {warnings:?}）"),
            Err(error) => error.to_string(),
        }
    }

    // --- 検査 1: Exif 全フィールドの保全 --------------------------------------

    /// 同じ head を比べれば通る。以降の拒否テストが「何でも拒否する」実装で
    /// 緑にならないための対照
    #[test]
    fn identical_heads_pass_without_warnings() {
        for name in ["exif-only.jpg", "lightroom-xmp.jpg"] {
            let head = head_of(&fixture(name));
            assert_eq!(check(&head, &head).unwrap(), Vec::<String>::new(), "{name}");
        }
    }

    #[test]
    fn a_lost_tag_is_refused_by_name() {
        let before = head_of(&fixture("exif-only.jpg"));
        assert!(
            exif_of(&before).get_field(Tag::Make, In::PRIMARY).is_some(),
            "前提条件: Make がある"
        );
        let after = edit_tiff(&before, |tiff| {
            let ifd0 = ifd0(tiff);
            rename_tag(tiff, ifd0, 0x010f, 0xfff0);
        });
        assert!(exif_of(&after).get_field(Tag::Make, In::PRIMARY).is_none());

        assert_eq!(
            refusal(&before, &after),
            "既存の Exif が保存で失われるか変わるため中止しました: Make"
        );
    }

    #[test]
    fn a_changed_value_is_refused_by_name() {
        let before = head_of(&fixture("exif-only.jpg"));
        let after = edit_tiff(&before, |tiff| {
            // Make は 5 バイト（"SONY\0"）なので値は外に置かれ、エントリはその位置を持つ
            let at = u32_le(tiff, entry(tiff, ifd0(tiff), 0x010f) + 8) as usize;
            assert_eq!(&tiff[at..at + 4], b"SONY", "前提条件: Make の値の位置");
            tiff[at] = b'X';
        });

        assert_eq!(
            refusal(&before, &after),
            "既存の Exif が保存で失われるか変わるため中止しました: Make"
        );
    }

    /// 同じ数値 2 のまま SHORT → LONG。リトルエンディアンの埋め込み値は
    /// どちらも `02 00 00 00` なので、型コードだけが変わる
    #[test]
    fn a_type_change_with_the_same_number_is_refused() {
        let before = head_of(&fixture("exif-only.jpg"));
        let after = edit_tiff(&before, |tiff| {
            let at = entry(tiff, ifd0(tiff), 0x0128) + 2;
            assert_eq!(u16_le(tiff, at), 3, "前提条件: ResolutionUnit は SHORT");
            tiff[at..at + 2].copy_from_slice(&4u16.to_le_bytes());
        });
        let unit = |head: &[u8]| {
            exif_of(head)
                .get_field(Tag::ResolutionUnit, In::PRIMARY)
                .unwrap()
                .value
                .clone()
        };
        assert!(matches!(unit(&before), Value::Short(ref v) if v == &[2]));
        assert!(matches!(unit(&after), Value::Long(ref v) if v == &[2]));

        assert_eq!(
            refusal(&before, &after),
            "既存の Exif が保存で失われるか変わるため中止しました: ResolutionUnit"
        );
    }

    /// 3 件まで名前を並べ、残りは件数だけ。並びは (IFD, タグ番号) 順
    #[test]
    fn more_than_three_lost_tags_are_summarized() {
        let before = head_of(&fixture("exif-only.jpg"));
        let after = edit_tiff(&before, |tiff| {
            let ifd0 = ifd0(tiff);
            // Copyright(0x8298)・Artist(0x013b)・Software(0x0131)・Model(0x0110)・Make(0x010f)
            // （並べる順を番号順と逆にして、文面の並びが実装の整列によることを確かめる）
            for (tag, to) in [
                (0x8298, 0xfff4),
                (0x013b, 0xfff3),
                (0x0131, 0xfff2),
                (0x0110, 0xfff1),
                (0x010f, 0xfff0),
            ] {
                rename_tag(tiff, ifd0, tag, to);
            }
        });

        assert_eq!(
            refusal(&before, &after),
            "既存の Exif が保存で失われるか変わるため中止しました: Make, Model, Software ほか 2 件"
        );
    }

    /// kamadak-exif が名前を知らないタグは 16 進、IFD1 のタグには「（サムネイル）」
    #[test]
    fn keys_are_labeled_by_name_hex_and_thumbnail_suffix() {
        assert_eq!(key_label((In::PRIMARY, Tag::Make)), "Make");
        assert_eq!(
            key_label((In::PRIMARY, Tag(Context::Tiff, 0x9c9b))),
            "0x9c9b"
        );
        assert_eq!(
            key_label((In::THUMBNAIL, Tag::XResolution)),
            "XResolution（サムネイル）"
        );
    }

    /// Exif APP1 ごと消えれば、書き込み前の全フィールドが失われたことになる（計画 A-1）
    #[test]
    fn losing_the_whole_exif_app1_is_refused() {
        let before = head_of(&fixture("exif-only.jpg"));
        let without_exif: Vec<(u8, &[u8])> = segments(&before)
            .unwrap()
            .iter()
            .filter(|s| !(s.marker == APP1 && s.contents.starts_with(EXIF_ID)))
            .map(|s| (s.marker, s.contents))
            .collect();
        let after = head_with(&without_exif);

        assert!(
            refusal(&before, &after)
                .starts_with("既存の Exif が保存で失われるか変わるため中止しました: Make, Model"),
        );
    }

    /// 1 エントリだけ値の位置が範囲外 → 厳格読みでは Err → 拒否
    #[test]
    fn an_exif_with_one_broken_entry_is_refused_as_unverifiable() {
        let head = edit_tiff(&head_of(&fixture("exif-only.jpg")), |tiff| {
            let at = entry(tiff, ifd0(tiff), 0x010f) + 8;
            tiff[at..at + 4].copy_from_slice(&0xFFFF_FF00u32.to_le_bytes());
        });
        // 前提条件: 壊れているのは 1 エントリだけ（部分読みなら残りは読める）
        let list = segments(&head).unwrap();
        let tiff = list[first_app1(&list, EXIF_ID).unwrap()].contents[EXIF_ID.len()..].to_vec();
        match exif::Reader::new().continue_on_error(true).read_raw(tiff) {
            Err(exif::Error::PartialResult(partial)) => {
                let (exif, errors) = partial.into_inner();
                assert_eq!(errors.len(), 1, "前提条件: 壊れたのは 1 エントリ");
                assert!(exif.get_field(Tag::Model, In::PRIMARY).is_some());
            }
            _ => panic!("前提条件: 部分結果にならなかった"),
        }

        assert_eq!(
            refusal(&head, &head),
            "既存の Exif を検証できないため中止しました"
        );
    }

    #[test]
    fn an_unreadable_written_exif_is_refused_as_unverifiable() {
        let before = head_of(&fixture("exif-only.jpg"));
        let after = edit_tiff(&before, |tiff| tiff[..8].copy_from_slice(&[0xAA; 8]));
        assert_eq!(
            refusal(&before, &after),
            "既存の Exif を検証できないため中止しました"
        );
    }

    #[test]
    fn a_head_without_exif_passes() {
        let head = head_with(&[(APP0, b"JFIF\0abc")]);
        assert_eq!(check(&head, &head).unwrap(), Vec::<String>::new());
    }

    // --- 実際の書き込み（build）を通した入力 ----------------------------------

    use crate::metadata::EditableMetadata;

    fn built(original: &[u8], values: &EditableMetadata) -> Vec<u8> {
        super::super::build(original, values).unwrap()
    }

    /// タイトルは ASCII にする。`exif::apply` は ASCII のタイトルだけを
    /// ImageDescription にも書くので、日本語だと 5 タグのうち 1 つが現れない
    fn all_three() -> EditableMetadata {
        EditableMetadata {
            title: Some("Harbor at dusk".into()),
            comment: Some("順光".into()),
            rating: Some(4),
        }
    }

    fn edited_value(head: &[u8], tag: Tag) -> Option<Value> {
        exif_of(head)
            .get_field(tag, In::PRIMARY)
            .map(|f| f.value.clone())
    }

    /// 編集対象の 5 タグ。除外リスト（本体の `excluded`）とは**独立に**書く ──
    /// 同じ定数を参照すると、context の取り違えが両側で一致して見逃される
    const EDITED_TAGS: [Tag; 5] = [
        Tag::ImageDescription,
        Tag(Context::Tiff, 0x9c9b),
        Tag(Context::Tiff, 0x9c9c),
        Tag(Context::Tiff, 0x4749),
        Tag::UserComment,
    ];

    /// 編集対象の 5 タグが増えても、変わっても、消えても通す（Review Focus 2）
    #[test]
    fn edited_tags_may_appear_change_and_disappear() {
        let plain = fixture("exif-only.jpg");
        let first = built(&plain, &all_three());
        let second = built(
            &first,
            &EditableMetadata {
                title: Some("Another title".into()),
                comment: Some("別のコメント".into()),
                rating: Some(2),
            },
        );
        let cleared = built(&first, &EditableMetadata::default());
        // 日本語のタイトルに変えると ImageDescription だけが消える
        let japanese = built(
            &first,
            &EditableMetadata {
                title: Some("夕暮れの港".into()),
                ..all_three()
            },
        );
        for tag in EDITED_TAGS {
            // 前提条件: 5 タグとも 1 回目の書き込みで現れ、2 回目で値が変わり、空欄で消える
            // （＝除外が効いていなければ拒否される入力になっている）
            let a = edited_value(&head_of(&first), tag)
                .unwrap_or_else(|| panic!("前提条件: {tag:?} が書かれていない"));
            let b = edited_value(&head_of(&second), tag).unwrap();
            assert!(
                !same_value(&a, &b),
                "前提条件: {tag:?} の値が変わっていない"
            );
            assert!(
                edited_value(&head_of(&cleared), tag).is_none(),
                "前提条件: {tag:?} が消えていない"
            );
        }
        assert!(
            edited_value(&head_of(&japanese), Tag::ImageDescription).is_none(),
            "前提条件: 日本語タイトルで ImageDescription が消えていない"
        );

        for (before, after) in [
            (&plain, &first),
            (&first, &second),
            (&first, &cleared),
            (&first, &japanese),
        ] {
            assert_eq!(
                check(&head_of(before), &head_of(after)).unwrap(),
                Vec::<String>::new()
            );
        }
    }

    // --- 検査 2: サムネイル ---------------------------------------------------

    #[test]
    fn a_changed_thumbnail_is_refused() {
        let before = head_of(&fixture("exif-only.jpg"));
        let original = exif_of(&before);
        let thumb = thumbnail(&original)
            .expect("前提条件: サムネイルがある")
            .to_vec();
        let offset = original
            .get_field(Tag::JPEGInterchangeFormat, In::THUMBNAIL)
            .unwrap()
            .value
            .get_uint(0)
            .unwrap() as usize;
        let after = edit_tiff(&before, |tiff| {
            assert_eq!(&tiff[offset..offset + thumb.len()], &thumb[..]);
            tiff[offset + thumb.len() / 2] ^= 0xFF;
        });

        assert_eq!(
            refusal(&before, &after),
            "Exif のサムネイルが保存で変わるため中止しました"
        );
    }

    /// `JPEGInterchangeFormat` は検査 1 の除外キーなので、ここを消しても検査 1 は通る。
    /// 検査 2 が捕まえる
    #[test]
    fn a_lost_thumbnail_pointer_is_refused() {
        let before = head_of(&fixture("exif-only.jpg"));
        assert!(
            thumbnail(&exif_of(&before)).is_some(),
            "前提条件: サムネイルがある"
        );
        let after = edit_tiff(&before, |tiff| {
            let ifd1 = ifd1(tiff);
            rename_tag(tiff, ifd1, 0x0201, 0xfff0);
        });
        assert!(
            thumbnail(&exif_of(&after)).is_none(),
            "前提条件: サムネイルが引けない"
        );

        assert_eq!(
            refusal(&before, &after),
            "Exif のサムネイルが保存で変わるため中止しました"
        );
    }

    /// サムネイルが後ろへ動いても、本体が同じなら通す（IFD0 が伸びたときの正常な変化。
    /// Review Focus 5）
    #[test]
    fn a_moved_thumbnail_with_the_same_bytes_passes() {
        let before = head_of(&fixture("exif-only.jpg"));
        let thumb = thumbnail(&exif_of(&before)).unwrap().to_vec();
        let after = edit_tiff(&before, |tiff| {
            let moved_to = tiff.len() as u32;
            tiff.extend(&thumb);
            let at = entry(tiff, ifd1(tiff), 0x0201) + 8;
            tiff[at..at + 4].copy_from_slice(&moved_to.to_le_bytes());
        });
        let pointer = |head: &[u8]| {
            exif_of(head)
                .get_field(Tag::JPEGInterchangeFormat, In::THUMBNAIL)
                .unwrap()
                .value
                .get_uint(0)
        };
        assert_ne!(
            pointer(&before),
            pointer(&after),
            "前提条件: オフセットが変わっている"
        );

        assert_eq!(check(&before, &after).unwrap(), Vec::<String>::new());
    }

    /// 範囲外・溢れるサムネイルで panic しない。元写真のサムネイルが壊れていれば比べない
    #[test]
    fn a_broken_thumbnail_range_neither_panics_nor_is_compared() {
        let head = head_of(&fixture("exif-only.jpg"));
        for (offset, length) in [(u32::MAX, 16), (16, u32::MAX), (0xFFFF_0000, 0x1_0000)] {
            let broken = edit_tiff(&head, |tiff| {
                let ifd1 = ifd1(tiff);
                let at = entry(tiff, ifd1, 0x0201) + 8;
                tiff[at..at + 4].copy_from_slice(&offset.to_le_bytes());
                let at = entry(tiff, ifd1, 0x0202) + 8;
                tiff[at..at + 4].copy_from_slice(&length.to_le_bytes());
            });
            assert!(
                thumbnail(&exif_of(&broken)).is_none(),
                "{offset:#x}+{length:#x}"
            );
            assert_eq!(check(&broken, &broken).unwrap(), Vec::<String>::new());
        }
    }
}
