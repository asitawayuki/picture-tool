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

/// 読めた文字列を正規形に寄せる。
///
/// **前後の空白は落とす。** XMP は要素テキストが整形の改行・インデントを含みうるので
/// XMP 側は必ず trim することになり、EXIF 側だけ残すと同じ値が「食い違い」に見える。
/// 書き込み側も `EditableMetadata::normalized` で trim するので、
/// 保存 → 読み戻しの検証（Task 9）が一致する。
/// 空白詰めの `ImageDescription` を書くカメラも救われる。
fn cleaned(text: &str) -> Option<String> {
    let trimmed = text.trim_matches(|c: char| c == '\0' || c.is_whitespace());
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// UCS-2LE（実質 UTF-16LE）＋ NUL 終端を読む
pub(super) fn from_ucs2le_nul(bytes: &[u8]) -> Option<String> {
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
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
            .chunks_exact(2)
            .map(|pair| match endian {
                Endian::Little => u16::from_le_bytes([pair[0], pair[1]]),
                Endian::Big => u16::from_be_bytes([pair[0], pair[1]]),
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

#[cfg(test)]
mod tests {
    use super::*;

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
