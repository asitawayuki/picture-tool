//! EXIF 側（`little_exif`）の読み書き。
//!
//! **`XPTitle` / `XPComment` と `UserComment` は別物である。**
//! 前者は EXIF 上の型が BYTE 配列で、中身は UCS-2LE ＋ 2 バイトの NUL 終端。
//! 後者は `undef` 型だが構造が違い、**先頭 8 バイトが文字コード識別子**
//! （`UNICODE\0` / `ASCII\0\0\0` 等）で本文はその後ろに続く。混同すると
//! 先頭 8 バイトが化けた文字列を利用者に見せることになる。

use little_exif::endian::Endian;
use little_exif::exif_tag::ExifTag;
use little_exif::ifd::ExifTagGroup;
use little_exif::metadata::Metadata;

use crate::metadata::{cleaned, stars_to_percent, without_xml_forbidden_chars, EditableMetadata};

/// IFD0 に置くタグ。`little_exif` に専用の enum バリアントが無いので生の ID で扱う
pub(super) const XP_TITLE: u16 = 0x9c9b;
pub(super) const XP_COMMENT: u16 = 0x9c9c;
pub(super) const RATING_PERCENT: u16 = 0x4749;
pub(super) const IMAGE_DESCRIPTION: u16 = 0x010e;
/// ExifIFD に置かれる。グループが IFD0 ではないので削除時に注意
pub(super) const USER_COMMENT: u16 = 0x9286;

const ID_UNICODE: &[u8; 8] = b"UNICODE\0";
const ID_ASCII: &[u8; 8] = b"ASCII\0\0\0";

/// EXIF 側から読めた生の値。**優先順位の判断はここではしない**（`mod.rs` の責務）
#[derive(Debug, Default)]
pub(super) struct ExifSide {
    pub title_xp: Option<String>,
    pub title_description: Option<String>,
    pub comment_xp: Option<String>,
    pub comment_user: Option<String>,
    pub rating_percent: Option<u16>,
}

pub(super) fn read(meta: &Metadata) -> ExifSide {
    let endian = meta.get_endian();

    let xp = |hex: u16| -> Option<String> {
        let tag = meta
            .get_tag_by_hex(hex, Some(ExifTagGroup::GENERIC))
            .next()?;
        from_ucs2le_nul(&tag.value_as_u8_vec(&endian))
    };

    let title_description = meta
        .get_tag_by_hex(IMAGE_DESCRIPTION, Some(ExifTagGroup::GENERIC))
        .next()
        .and_then(|tag| match tag {
            ExifTag::ImageDescription(text) => Some(text.clone()),
            _ => None,
        })
        .and_then(|text| cleaned(&text));

    let comment_user = meta
        .get_tag_by_hex(USER_COMMENT, Some(ExifTagGroup::EXIF))
        .next()
        .and_then(|tag| match tag {
            ExifTag::UserComment(bytes) => decode_user_comment(bytes, &endian),
            _ => None,
        });

    let rating_percent = meta
        .get_tag_by_hex(RATING_PERCENT, Some(ExifTagGroup::GENERIC))
        .next()
        .and_then(|tag| match tag {
            ExifTag::UnknownINT16U(values, _, _) => values.first().copied(),
            _ => None,
        });

    ExifSide {
        title_xp: xp(XP_TITLE),
        title_description,
        comment_xp: xp(XP_COMMENT),
        comment_user,
        rating_percent,
    }
}

/// UCS-2LE（実質 UTF-16LE）＋ NUL 終端を読む
pub(super) fn from_ucs2le_nul(bytes: &[u8]) -> Option<String> {
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&pair| u16::from_le_bytes(pair))
        .collect();
    let end = units.iter().position(|&u| u == 0).unwrap_or(units.len());
    cleaned(&String::from_utf16(&units[..end]).ok()?)
}

/// `UserComment` の本文を取り出す。先頭 8 バイトの識別子で本文の解釈が変わる
pub(super) fn decode_user_comment(bytes: &[u8], endian: &Endian) -> Option<String> {
    if bytes.len() < 8 {
        return None;
    }
    let (id, body) = bytes.split_at(8);
    let text = if id == ID_UNICODE {
        let units: Vec<u16> = body
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&pair| match endian {
                Endian::Little => u16::from_le_bytes(pair),
                Endian::Big => u16::from_be_bytes(pair),
            })
            .collect();
        String::from_utf16(&units).ok()?
    } else if id == ID_ASCII {
        String::from_utf8(body.to_vec()).ok()?
    } else {
        // 識別子が無い / 未知。読めなかったものとして扱う
        return None;
    };
    cleaned(&text)
}

/// UCS-2LE（実質 UTF-16LE）＋ 2 バイトの NUL 終端に直す
pub(super) fn to_ucs2le_nul(text: &str) -> Vec<u8> {
    let mut bytes: Vec<u8> = text
        .encode_utf16()
        .flat_map(|unit| unit.to_le_bytes())
        .collect();
    bytes.extend_from_slice(&[0, 0]);
    bytes
}

/// `UserComment` の本文を組み立てる。**先頭 8 バイトは文字コード識別子**で、
/// 本文のバイト順は TIFF ヘッダの endian に従う
fn encode_user_comment(text: &str, endian: &Endian) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + text.len() * 2);
    out.extend_from_slice(ID_UNICODE);
    for unit in text.encode_utf16() {
        match endian {
            Endian::Little => out.extend_from_slice(&unit.to_le_bytes()),
            Endian::Big => out.extend_from_slice(&unit.to_be_bytes()),
        }
    }
    out
}

/// EXIF 側の 5 箇所を、`values` の内容で置き換える。
///
/// `None` の項目は**タグごと削除する**（空文字のタグを残さない）。
pub(super) fn apply(meta: &mut Metadata, values: &EditableMetadata) {
    let endian = meta.get_endian();

    // XMP 側（XML）が許さない制御文字をここでも落とす。**両側で同じ文字列を
    // 書かないと、Task 9 の「保存 → 読み戻して一致検証」が食い違う**
    let title = values.title.as_deref().map(without_xml_forbidden_chars);
    let comment = values.comment.as_deref().map(without_xml_forbidden_chars);

    // --- タイトル ---
    match title.as_deref() {
        Some(title) if !title.is_empty() => {
            meta.set_tag(ExifTag::UnknownINT8U(
                to_ucs2le_nul(title),
                XP_TITLE,
                ExifTagGroup::GENERIC,
            ));
            if title.is_ascii() {
                meta.set_tag(ExifTag::ImageDescription(title.to_string()));
            } else {
                // ASCII 型に日本語は入らない。古い ASCII 値が残ると
                // Windows でそちらが出るので消す（正しい値は XPTitle と dc:title にある）
                meta.remove_tag_by_hex_group(IMAGE_DESCRIPTION, ExifTagGroup::GENERIC);
            }
        }
        _ => {
            meta.remove_tag_by_hex_group(XP_TITLE, ExifTagGroup::GENERIC);
            meta.remove_tag_by_hex_group(IMAGE_DESCRIPTION, ExifTagGroup::GENERIC);
        }
    }

    // --- コメント（ImageDescription には一切触らない） ---
    match comment.as_deref() {
        Some(comment) if !comment.is_empty() => {
            meta.set_tag(ExifTag::UnknownINT8U(
                to_ucs2le_nul(comment),
                XP_COMMENT,
                ExifTagGroup::GENERIC,
            ));
            meta.set_tag(ExifTag::UserComment(encode_user_comment(comment, &endian)));
        }
        _ => {
            meta.remove_tag_by_hex_group(XP_COMMENT, ExifTagGroup::GENERIC);
            meta.remove_tag_by_hex_group(USER_COMMENT, ExifTagGroup::EXIF);
        }
    }

    // --- レーティング ---
    match values.rating {
        Some(stars @ 1..=5) => {
            meta.set_tag(ExifTag::UnknownINT16U(
                vec![stars_to_percent(stars)],
                RATING_PERCENT,
                ExifTagGroup::GENERIC,
            ));
        }
        // Some(0) も None も「未評価」。0 を書くと Windows 側で
        // 「評価なし」と区別が付かなくなるうえ、タグだけが増える
        _ => {
            meta.remove_tag_by_hex_group(RATING_PERCENT, ExifTagGroup::GENERIC);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::EditableMetadata;

    fn applied(values: EditableMetadata) -> ExifSide {
        let mut meta = Metadata::new();
        apply(&mut meta, &values);
        read(&meta)
    }

    #[test]
    fn ucs2le_round_trips_japanese() {
        assert_eq!(to_ucs2le_nul("港"), vec![0x2F, 0x6E, 0x00, 0x00]);
        assert_eq!(
            from_ucs2le_nul(&to_ucs2le_nul("夕暮れの港")).as_deref(),
            Some("夕暮れの港")
        );
    }

    #[test]
    fn a_title_lands_in_every_exif_slot_when_it_is_ascii() {
        let side = applied(EditableMetadata {
            title: Some("Harbour".into()),
            ..Default::default()
        });
        assert_eq!(side.title_xp.as_deref(), Some("Harbour"));
        assert_eq!(side.title_description.as_deref(), Some("Harbour"));
    }

    /// **日本語のタイトルでは `ImageDescription` を書かない。**
    /// ASCII 型なので化ける。Windows がどちらを優先しても正しい値が出るよう、
    /// 矛盾する値を残さないのが狙い
    #[test]
    fn a_japanese_title_skips_image_description() {
        let side = applied(EditableMetadata {
            title: Some("夕暮れの港".into()),
            ..Default::default()
        });
        assert_eq!(side.title_xp.as_deref(), Some("夕暮れの港"));
        assert_eq!(side.title_description, None);
    }

    /// 古い ASCII の `ImageDescription` が残ると、Windows でそちらが出る事故になる。
    /// **上書きではなく削除**まで踏み込む
    #[test]
    fn a_japanese_title_deletes_an_existing_image_description() {
        let mut meta = Metadata::new();
        apply(
            &mut meta,
            &EditableMetadata {
                title: Some("Harbour".into()),
                ..Default::default()
            },
        );
        // 前提条件: いったん書けていること。ここが空なら次の assert は自明に通る
        assert_eq!(read(&meta).title_description.as_deref(), Some("Harbour"));

        apply(
            &mut meta,
            &EditableMetadata {
                title: Some("夕暮れの港".into()),
                ..Default::default()
            },
        );
        assert_eq!(read(&meta).title_description, None);
    }

    #[test]
    fn a_comment_lands_in_both_exif_slots() {
        let side = applied(EditableMetadata {
            comment: Some("順光。もう少し寄れた".into()),
            ..Default::default()
        });
        assert_eq!(side.comment_xp.as_deref(), Some("順光。もう少し寄れた"));
        assert_eq!(side.comment_user.as_deref(), Some("順光。もう少し寄れた"));
    }

    /// **コメントは `ImageDescription` を一切読み書きしない**（タイトル用途と衝突するため）。
    ///
    /// 前提条件: 同じ `Metadata` にタイトルを書いてあること。`title: None` で apply すると
    /// `apply` 自身が `ImageDescription` を削除するため、その分岐に頼ると
    /// コメント側の実装を壊しても検査が空振りしたまま緑になる（実測は task-5-report.md）
    #[test]
    fn a_comment_never_touches_image_description() {
        let mut meta = Metadata::new();
        apply(
            &mut meta,
            &EditableMetadata {
                title: Some("Harbour".into()),
                ..Default::default()
            },
        );
        // 前提条件: コメントを書く前に ImageDescription が存在していること
        assert_eq!(read(&meta).title_description.as_deref(), Some("Harbour"));

        apply(
            &mut meta,
            &EditableMetadata {
                title: Some("Harbour".into()),
                comment: Some("hello".into()),
                ..Default::default()
            },
        );
        assert_eq!(read(&meta).title_description.as_deref(), Some("Harbour"));
    }

    #[test]
    fn a_rating_is_written_as_the_official_percent() {
        let side = applied(EditableMetadata {
            rating: Some(3),
            ..Default::default()
        });
        assert_eq!(side.rating_percent, Some(50));
    }

    /// `Some(0)` は「未評価」であって★0 ではない。**書かない**
    #[test]
    fn zero_stars_removes_the_rating_instead_of_writing_zero() {
        let mut meta = Metadata::new();
        apply(
            &mut meta,
            &EditableMetadata {
                rating: Some(4),
                ..Default::default()
            },
        );
        assert_eq!(read(&meta).rating_percent, Some(75)); // 前提条件

        apply(
            &mut meta,
            &EditableMetadata {
                rating: Some(0),
                ..Default::default()
            },
        );
        assert_eq!(read(&meta).rating_percent, None);
    }

    /// 空欄で保存したらタグごと消す。空文字のタグが残ると、他アプリで
    /// 「空のコメントがある」状態になる
    #[test]
    fn clearing_a_field_removes_the_tags() {
        let mut meta = Metadata::new();
        apply(
            &mut meta,
            &EditableMetadata {
                title: Some("Harbour".into()),
                comment: Some("hello".into()),
                rating: Some(5),
            },
        );
        let before = read(&meta); // 前提条件
        assert!(
            before.title_xp.is_some()
                && before.comment_xp.is_some()
                && before.rating_percent.is_some()
        );

        apply(&mut meta, &EditableMetadata::default());
        let after = read(&meta);
        assert!(after.title_xp.is_none());
        assert!(after.title_description.is_none());
        assert!(after.comment_xp.is_none());
        assert!(after.comment_user.is_none());
        assert!(after.rating_percent.is_none());
    }

    #[test]
    fn ucs2le_decodes_japanese_and_stops_at_the_nul_terminator() {
        // "港" = U+6E2F → 2F 6E（LE）、続いて NUL 終端
        let bytes = [0x2F, 0x6E, 0x00, 0x00];
        assert_eq!(from_ucs2le_nul(&bytes).as_deref(), Some("港"));
    }

    #[test]
    fn ucs2le_handles_a_missing_terminator() {
        let bytes = [0x2F, 0x6E];
        assert_eq!(from_ucs2le_nul(&bytes).as_deref(), Some("港"));
    }

    #[test]
    fn ucs2le_survives_surrogate_pairs() {
        // "𠮟" = U+20B9F → サロゲートペア D842 DF9F
        let bytes = [0x42, 0xD8, 0x9F, 0xDF, 0x00, 0x00];
        assert_eq!(from_ucs2le_nul(&bytes).as_deref(), Some("𠮟"));
    }

    /// **`UserComment` は `XPComment` と構造が違う。**
    /// 先頭 8 バイトが文字コード識別子で、本文はその後ろ
    #[test]
    fn user_comment_reads_the_unicode_identifier() {
        let mut bytes = b"UNICODE\0".to_vec();
        bytes.extend_from_slice(&[0x2F, 0x6E]);
        assert_eq!(
            decode_user_comment(&bytes, &Endian::Little).as_deref(),
            Some("港")
        );
    }

    #[test]
    fn user_comment_reads_the_ascii_identifier() {
        let mut bytes = b"ASCII\0\0\0".to_vec();
        bytes.extend_from_slice(b"hello");
        assert_eq!(
            decode_user_comment(&bytes, &Endian::Little).as_deref(),
            Some("hello")
        );
    }

    /// 識別子が無い / 未知のものは「読めなかった」にする。
    /// 8 バイトを本文と誤読すると先頭が化けた文字列を利用者に見せることになる
    #[test]
    fn user_comment_rejects_an_unknown_identifier() {
        let bytes = b"\0\0\0\0\0\0\0\0hello".to_vec();
        assert_eq!(decode_user_comment(&bytes, &Endian::Little), None);
    }

    /// **読み取りは前後の空白を落とす。**
    /// XMP 側は整形の改行を必ず含むので、EXIF だけ残すと同じ値が食い違いに見える。
    /// 空白詰めの ImageDescription を書くカメラもここで救われる
    #[test]
    fn reading_trims_surrounding_whitespace_and_nul_padding() {
        assert_eq!(
            from_ucs2le_nul(&[0x20, 0x00, 0x2F, 0x6E, 0x20, 0x00, 0x00, 0x00]).as_deref(),
            Some("港")
        );
        assert_eq!(
            from_ucs2le_nul(&[0x20, 0x00, 0x00, 0x00]),
            None,
            "空白だけならタグ無し扱い"
        );
    }

    /// 前提条件つきの否定形。空の `Metadata` で「全部 None」になること
    #[test]
    fn an_empty_metadata_yields_no_values() {
        let side = read(&Metadata::new());
        assert!(side.title_xp.is_none());
        assert!(side.title_description.is_none());
        assert!(side.comment_xp.is_none());
        assert!(side.comment_user.is_none());
        assert!(side.rating_percent.is_none());
    }
}
