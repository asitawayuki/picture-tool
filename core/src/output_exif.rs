//! 出力 JPEG に載せる Exif を、元の Exif から組み立て直す（spec 2026-10-20「Exif」）。
//!
//! 位置情報（GPS）と、撮影者や機材個体を特定しうるタグを消し、画素と食い違うタグを書き換え、
//! それ以外の本体 IFD・Exif IFD・Interop IFD のタグを残す。元のバイト列は切り貼りせず、
//! kamadak-exif の `experimental::Writer` で新しい TIFF を作る（`little_exif` は SHORT を LONG に
//! 正規化することがあるので使わない）。
//!
//! 読めない・組み立てられない・大きすぎる Exif は、呼び出し側（`process_image`）が警告にして
//! Exif なしで書く。ここでは `Err` / `None` を返すだけで、警告の文言は持たない。

use std::fs::File;
use std::io::{BufReader, Cursor};
use std::path::Path;

use anyhow::Result;
use exif::experimental::Writer;
use exif::{Context, Exif, Field, In, Reader, Tag, Value};

use crate::decode;

/// APP1 に入る TIFF 本体の上限。`jpeg-encoder` 0.7.1 の `add_app_segment` は
/// `"Exif\0\0"` の 6 バイトを足して 65,533 以下を受け付ける（doc の 65,528 とは 1 ずれている）
pub(crate) const MAX_EXIF_TIFF_LEN: usize = 65_527;

/// 撮影者や機材個体を特定しうるタグ（利用者の判断。`Copyright` は権利表示なので残す）
const IDENTIFYING: [Tag; 5] = [
    Tag::BodySerialNumber,      // 0xA431
    Tag::LensSerialNumber,      // 0xA435
    Tag::CameraOwnerName,       // 0xA430
    Tag::Artist,                // 0x013B
    Tag(Context::Tiff, 0x9C9D), // XPAuthor
];

/// 値がファイル内のオフセットであるタグ（組み立て直した TIFF の中ではどこも指さない）。
/// 本体 IFD にも Exif IFD にも置かれうるので、番号で照合する
const OFFSET_TAG_NUMBERS: [u16; 11] = [
    0x0111, // StripOffsets
    0x0117, // StripByteCounts
    0x0120, // FreeOffsets
    0x0121, // FreeByteCounts
    0x0144, // TileOffsets
    0x0145, // TileByteCounts
    0x014A, // SubIFDs
    0x0201, // JPEGInterchangeFormat
    0x0202, // JPEGInterchangeFormatLength
    0xC634, // DNGPrivateData
    0xEA1D, // OffsetSchema
];

/// TIFF のタグに入った XMP・IPTC（spec「XMP と IPTC は写さない」。LrC の XMP は位置情報を持ちうる）
const XMP_AND_IPTC: [Tag; 3] = [
    Tag(Context::Tiff, 0x02BC), // XMP
    Tag(Context::Tiff, 0x83BB), // IPTC-NAA
    Tag(Context::Tiff, 0x8649), // Photoshop の画像リソース（IPTC を含む）
];

/// これしか残らなければ Exif を書かない（spec「Exif を書かない場合」の列挙を逐語で）。
/// 書き換えたタグと、構造や形式だけを表すタグ。Interop IFD の中身もこれに数える。
/// ポインタの 2 つは kamadak-exif がフィールドとして返さないので実際には現れないが、列挙に合わせて置く
const STRUCTURAL: [Tag; 13] = [
    // 書き換えたタグ
    Tag::Orientation,
    Tag::ColorSpace,
    Tag::PixelXDimension,
    Tag::PixelYDimension,
    // 構造や形式だけを表すタグ
    Tag::ExifVersion,
    Tag::FlashpixVersion,
    Tag::XResolution,
    Tag::YResolution,
    Tag::ResolutionUnit,
    Tag::YCbCrPositioning,
    Tag::ComponentsConfiguration,
    Tag::ExifIFDPointer,
    Tag::InteropIFDPointer,
];

/// 入力の Exif を読む。無ければ `Ok(None)`、壊れていれば `Err`
///
/// JPEG XL は kamadak-exif のコンテナ判定が知らないので、Exif ボックスを自前で取り出す。
/// JPEG / PNG / WebP / AVIF（HEIF として）は `read_from_container` で読める
pub(crate) fn load(path: &Path, format: decode::Format) -> Result<Option<Exif>> {
    let parsed = if format == decode::Format::Jxl {
        match decode::jxl_exif(path)? {
            Some(tiff) => Reader::new().read_raw(tiff),
            None => return Ok(None),
        }
    } else {
        Reader::new().read_from_container(&mut BufReader::new(File::open(path)?))
    };
    match parsed {
        Ok(exif) => Ok(Some(exif)),
        Err(exif::Error::NotFound(_)) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// 出力用の Exif（TIFF 本体）を組み立てる。`width`/`height` は出力の寸法。
/// 消して書き換えた後に構造のタグしか残らなければ `None`
pub(crate) fn build(src: &Exif, width: u32, height: u32) -> Result<Option<Vec<u8>>> {
    let fields: Vec<Field> = src
        .fields()
        .filter_map(|f| rewrite(f, width, height))
        .collect();
    if fields
        .iter()
        .all(|f| f.tag.context() == Context::Interop || STRUCTURAL.contains(&f.tag))
    {
        return Ok(None);
    }
    let mut writer = Writer::new();
    for f in &fields {
        writer.push_field(f);
    }
    let mut out = Cursor::new(Vec::new());
    writer.write(&mut out, src.little_endian())?;
    Ok(Some(out.into_inner()))
}

/// 1 つのタグの扱い（spec「Exif」の表を上から順に）。消すなら `None`
fn rewrite(f: &Field, width: u32, height: u32) -> Option<Field> {
    let tag = f.tag;
    // IFD へのポインタ（ExifIFDPointer・GPSInfoIFDPointer 等）は kamadak-exif がフィールドとして
    // 返さず、Writer が中身から作り直すので、GPS は Context で消せば足りる
    let drop = tag.context() == Context::Gps
        || f.ifd_num != In::PRIMARY
        || tag == Tag::MakerNote
        || IDENTIFYING.contains(&tag)
        || (matches!(tag.context(), Context::Tiff | Context::Exif)
            && OFFSET_TAG_NUMBERS.contains(&tag.number()))
        || XMP_AND_IPTC.contains(&tag)
        // Writer は Unknown を書けない（spec「付随処理の失敗」: そのタグを落として組み直す）
        || matches!(f.value, Value::Unknown(..));
    if drop {
        return None;
    }
    let value = match tag {
        // 画素は回転済み。残すと二重に回る
        Tag::Orientation => Value::Short(vec![1]),
        // 出力は常に sRGB
        Tag::ColorSpace => Value::Short(vec![1]),
        Tag::PixelXDimension => dimension(&f.value, width),
        Tag::PixelYDimension => dimension(&f.value, height),
        _ => return Some(f.clone()),
    };
    Some(Field {
        tag,
        ifd_num: f.ifd_num,
        value,
    })
}

/// 元の型が SHORT で収まるなら SHORT のまま、そうでなければ LONG（Exif はどちらも許す）
fn dimension(original: &Value, v: u32) -> Value {
    match (original, u16::try_from(v)) {
        (Value::Short(_), Ok(s)) => Value::Short(vec![s]),
        _ => Value::Long(vec![v]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `fields` を `Writer` で書き、`Reader::read_raw` で読んだ `Exif`
    /// （`Exif` には公開のコンストラクタが無い）
    fn exif_of(fields: &[Field], little_endian: bool) -> Exif {
        let mut writer = Writer::new();
        for f in fields {
            writer.push_field(f);
        }
        let mut out = Cursor::new(Vec::new());
        writer.write(&mut out, little_endian).unwrap();
        Reader::new().read_raw(out.into_inner()).unwrap()
    }

    fn field(tag: Tag, value: Value) -> Field {
        Field {
            tag,
            ifd_num: In::PRIMARY,
            value,
        }
    }

    fn rational(num: u32) -> Value {
        Value::Rational(vec![exif::Rational { num, denom: 1 }])
    }

    /// spec の構造タグだけ（Interop IFD の中身を含む）
    fn structural_only() -> Vec<Field> {
        vec![
            field(Tag::Orientation, Value::Short(vec![6])),
            field(Tag::ColorSpace, Value::Short(vec![65535])),
            field(Tag::PixelXDimension, Value::Long(vec![6000])),
            field(Tag::PixelYDimension, Value::Long(vec![4000])),
            field(Tag::ExifVersion, Value::Undefined(b"0232".to_vec(), 0)),
            field(Tag::FlashpixVersion, Value::Undefined(b"0100".to_vec(), 0)),
            field(Tag::XResolution, rational(350)),
            field(Tag::YResolution, rational(350)),
            field(Tag::ResolutionUnit, Value::Short(vec![2])),
            field(Tag::YCbCrPositioning, Value::Short(vec![2])),
            field(
                Tag::ComponentsConfiguration,
                Value::Undefined(vec![1, 2, 3, 0], 0),
            ),
            field(
                Tag::InteroperabilityIndex,
                Value::Ascii(vec![b"R98".to_vec()]),
            ),
        ]
    }

    fn read(tiff: &[u8]) -> Exif {
        Reader::new().read_raw(tiff.to_vec()).unwrap()
    }

    #[test]
    fn only_structural_tags_left_means_no_exif() {
        let src = exif_of(&structural_only(), false);
        // 前提: Exif IFD と Interop IFD の中身も読み込まれている
        assert!(src.fields().any(|f| f.tag.context() == Context::Exif));
        assert!(src.fields().any(|f| f.tag.context() == Context::Interop));
        assert!(build(&src, 1080, 1350).unwrap().is_none());
    }

    #[test]
    fn one_descriptive_tag_is_enough_to_write() {
        let mut fields = structural_only();
        fields.push(field(Tag::Software, Value::Ascii(vec![b"Tool".to_vec()])));
        let out = build(&exif_of(&fields, false), 1080, 1350).unwrap();
        let out = read(&out.expect("Software が残るのに Exif が書かれない"));
        assert!(out.get_field(Tag::Software, In::PRIMARY).is_some());
    }

    /// 消すタグが残る全部なら、それも「書かない」に数える（GPS だけの入力を空の Exif にしない）
    #[test]
    fn removed_tags_do_not_count_as_something_to_write() {
        let mut fields = structural_only();
        fields.push(field(
            Tag::GPSLatitudeRef,
            Value::Ascii(vec![b"N".to_vec()]),
        ));
        fields.push(field(Tag::Artist, Value::Ascii(vec![b"Me".to_vec()])));
        let src = exif_of(&fields, false);
        assert!(
            src.fields().any(|f| f.tag.context() == Context::Gps),
            "前提"
        );
        assert!(build(&src, 1080, 1350).unwrap().is_none());
    }

    #[test]
    fn offset_tags_are_removed() {
        let offsets = [
            Tag(Context::Tiff, 0x014A), // SubIFDs
            Tag(Context::Tiff, 0xC634), // DNGPrivateData
            Tag(Context::Exif, 0xEA1D), // OffsetSchema
        ];
        let mut fields = vec![field(Tag::Make, Value::Ascii(vec![b"M".to_vec()]))];
        fields.extend(offsets.map(|t| field(t, Value::Long(vec![1234]))));
        let src = exif_of(&fields, false);
        for t in offsets {
            assert!(
                src.get_field(t, In::PRIMARY).is_some(),
                "前提: 入力に {t} がある"
            );
        }
        let out = read(&build(&src, 10, 10).unwrap().unwrap());
        for t in offsets {
            assert!(out.get_field(t, In::PRIMARY).is_none(), "{t} が残っている");
        }
        assert!(out.get_field(Tag::Make, In::PRIMARY).is_some());
    }

    #[test]
    fn xmp_and_iptc_in_tiff_tags_are_removed() {
        let mut fields = vec![field(Tag::Make, Value::Ascii(vec![b"M".to_vec()]))];
        fields.extend(XMP_AND_IPTC.map(|t| field(t, Value::Byte(b"<x:xmpmeta/>".to_vec()))));
        let src = exif_of(&fields, false);
        for t in XMP_AND_IPTC {
            assert!(
                src.get_field(t, In::PRIMARY).is_some(),
                "前提: 入力に {t} がある"
            );
        }
        let out = read(&build(&src, 10, 10).unwrap().unwrap());
        for t in XMP_AND_IPTC {
            assert!(out.get_field(t, In::PRIMARY).is_none(), "{t} が残っている");
        }
    }

    /// spec: Orientation・ColorSpace・PixelX/YDimension は「元に無ければ足さない」
    #[test]
    fn rewritten_tags_are_not_added_when_absent() {
        let src = exif_of(
            &[field(Tag::Make, Value::Ascii(vec![b"M".to_vec()]))],
            false,
        );
        let out = read(&build(&src, 1080, 1350).unwrap().unwrap());
        for t in [
            Tag::Orientation,
            Tag::ColorSpace,
            Tag::PixelXDimension,
            Tag::PixelYDimension,
        ] {
            assert!(out.get_field(t, In::PRIMARY).is_none(), "{t} が足された");
        }
    }

    #[test]
    fn color_space_becomes_srgb() {
        let src = exif_of(
            &[
                field(Tag::Make, Value::Ascii(vec![b"M".to_vec()])),
                // 0xFFFF = Uncalibrated（AdobeRGB の写真が持つ）
                field(Tag::ColorSpace, Value::Short(vec![0xFFFF])),
            ],
            false,
        );
        let out = read(&build(&src, 10, 10).unwrap().unwrap());
        let cs = out.get_field(Tag::ColorSpace, In::PRIMARY).unwrap();
        assert_eq!(cs.value.get_uint(0), Some(1));
    }

    #[test]
    fn byte_order_follows_the_source() {
        let fields = [field(Tag::Make, Value::Ascii(vec![b"M".to_vec()]))];
        for (little_endian, sig) in [(true, b"II"), (false, b"MM")] {
            let out = build(&exif_of(&fields, little_endian), 10, 10)
                .unwrap()
                .unwrap();
            assert_eq!(&out[..2], sig);
        }
    }

    /// 上限は jpeg-encoder 0.7.1 の実際の判定と一致する（ちょうどなら入り、1 バイト超えると拒まれる）
    #[test]
    fn max_exif_tiff_len_is_the_encoder_limit() {
        let rgb = image::RgbImage::new(8, 8);
        let encode = |len: usize| {
            let tiff = vec![0u8; len];
            crate::encode::encode_jpeg(
                &rgb,
                90,
                &crate::encode::Embed {
                    icc: None,
                    exif: Some(&tiff),
                },
            )
        };
        assert!(encode(MAX_EXIF_TIFF_LEN).is_ok());
        assert!(encode(MAX_EXIF_TIFF_LEN + 1).is_err());
    }
}
