//! 入力の Exif をテスト内で作る道具。「TIFF のバイト列を作る」と「コンテナに入れる」を分ける。

use std::io::Cursor;

use exif::experimental::Writer;
use exif::{Context, Field, In, Rational, Tag, Value};

/// `XPAuthor`（Windows の作者欄）。kamadak-exif に定数が無い
pub const XP_AUTHOR: Tag = Tag(Context::Tiff, 0x9C9D);

pub const MAKE: &str = "TestMake";
pub const MODEL: &str = "TestModel";
pub const DATE_TIME_ORIGINAL: &str = "2024:01:02 03:04:05";
pub const COPYRIGHT: &str = "(c) Test Owner";
pub const EXPOSURE_TIME: Rational = Rational { num: 1, denom: 250 };

fn ascii(tag: Tag, s: &str) -> Field {
    Field {
        tag,
        ifd_num: In::PRIMARY,
        value: Value::Ascii(vec![s.as_bytes().to_vec()]),
    }
}

fn short(tag: Tag, v: u16) -> Field {
    Field {
        tag,
        ifd_num: In::PRIMARY,
        value: Value::Short(vec![v]),
    }
}

fn long(tag: Tag, v: u32) -> Field {
    Field {
        tag,
        ifd_num: In::PRIMARY,
        value: Value::Long(vec![v]),
    }
}

/// 撮って出しの写真に近い一式: 撮影情報・位置情報・個体識別タグ・MakerNote・IFD1 のサムネイル。
/// `width`/`height` は `PixelXDimension`/`PixelYDimension` に入れる元の寸法
pub fn camera_fields(orientation: u16, width: u32, height: u32) -> Vec<Field> {
    vec![
        ascii(Tag::Make, MAKE),
        ascii(Tag::Model, MODEL),
        Field {
            tag: Tag::ExposureTime,
            ifd_num: In::PRIMARY,
            value: Value::Rational(vec![EXPOSURE_TIME]),
        },
        ascii(Tag::DateTimeOriginal, DATE_TIME_ORIGINAL),
        ascii(Tag::Copyright, COPYRIGHT),
        short(Tag::Orientation, orientation),
        short(Tag::ColorSpace, 1),
        long(Tag::PixelXDimension, width),
        long(Tag::PixelYDimension, height),
        ascii(Tag::GPSLatitudeRef, "N"),
        Field {
            tag: Tag::GPSLatitude,
            ifd_num: In::PRIMARY,
            value: Value::Rational(vec![
                Rational { num: 35, denom: 1 },
                Rational { num: 39, denom: 1 },
                Rational { num: 0, denom: 1 },
            ]),
        },
        ascii(Tag::BodySerialNumber, "BODY-1234"),
        ascii(Tag::LensSerialNumber, "LENS-5678"),
        ascii(Tag::CameraOwnerName, "Owner Name"),
        ascii(Tag::Artist, "Artist Name"),
        Field {
            tag: XP_AUTHOR,
            ifd_num: In::PRIMARY,
            // UCS-2LE の "Au" と終端
            value: Value::Byte(vec![b'A', 0, b'u', 0, 0, 0]),
        },
        Field {
            tag: Tag::MakerNote,
            ifd_num: In::PRIMARY,
            value: Value::Undefined(b"VENDOR-NOTE-0123456789".to_vec(), 0),
        },
        // IFD1 は少なくとも 1 つのエントリを持つ（サムネイルの位置は Writer が足す）
        Field {
            tag: Tag::Compression,
            ifd_num: In::THUMBNAIL,
            value: Value::Short(vec![6]),
        },
    ]
}

/// `fields`（と、あれば IFD1 のサムネイル JPEG）をビッグエンディアンの TIFF にする
pub fn tiff(fields: &[Field], thumbnail: Option<&[u8]>) -> Vec<u8> {
    let mut writer = Writer::new();
    for f in fields {
        writer.push_field(f);
    }
    if let Some(jpeg) = thumbnail {
        writer.set_jpeg(jpeg, In::THUMBNAIL);
    }
    let mut buf = Cursor::new(Vec::new());
    writer.write(&mut buf, false).unwrap();
    buf.into_inner()
}

/// [`camera_fields`] にサムネイルを付けた TIFF
pub fn camera_tiff(orientation: u16, width: u32, height: u32) -> Vec<u8> {
    tiff(
        &camera_fields(orientation, width, height),
        Some(&small_jpeg()),
    )
}

/// 8×8 の灰色の JPEG（サムネイル用）
pub fn small_jpeg() -> Vec<u8> {
    let img = image::RgbImage::from_pixel(8, 8, image::Rgb([128, 128, 128]));
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Jpeg).unwrap();
    out.into_inner()
}

/// ビッグエンディアンの TIFF の IFD0 と Exif IFD から `tag` のエントリを探し、型コードを
/// 未定義の 99 に書き換える。kamadak-exif は未知の型を `Value::Unknown` として読む
/// （`Writer` は `Unknown` を書けないので、こうして作る）
pub fn with_unknown_type(mut tiff: Vec<u8>, tag: u16) -> Vec<u8> {
    let u16_at = |b: &[u8], p: usize| u16::from_be_bytes([b[p], b[p + 1]]);
    let u32_at = |b: &[u8], p: usize| u32::from_be_bytes(b[p..p + 4].try_into().unwrap()) as usize;
    assert_eq!(
        &tiff[..4],
        b"MM\0\x2a",
        "ビッグエンディアンの TIFF ではない"
    );
    let mut ifds = vec![u32_at(&tiff, 4)];
    while let Some(ifd) = ifds.pop() {
        let count = u16_at(&tiff, ifd) as usize;
        for i in 0..count {
            let entry = ifd + 2 + 12 * i;
            match u16_at(&tiff, entry) {
                t if t == tag => {
                    tiff[entry + 2..entry + 4].copy_from_slice(&99u16.to_be_bytes());
                    return tiff;
                }
                0x8769 => ifds.push(u32_at(&tiff, entry + 8)),
                _ => {}
            }
        }
    }
    panic!("タグ {tag:#06X} が IFD0 にも Exif IFD にも無い");
}

/// `w`×`h` の JPEG（左半分が赤・右半分が青）の SOI の直後に Exif の APP1 を差し込む
pub fn jpeg_with_exif(w: u32, h: u32, tiff: &[u8]) -> Vec<u8> {
    let jpeg = encode(w, h, image::ImageFormat::Jpeg);
    let len =
        u16::try_from(2 + 6 + tiff.len()).expect("APP1 に入らない（TIFF は 65,527 バイトまで）");
    let mut out = jpeg[..2].to_vec();
    out.extend_from_slice(&[0xFF, 0xE1]);
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(b"Exif\0\0");
    out.extend_from_slice(tiff);
    out.extend_from_slice(&jpeg[2..]);
    out
}

/// `w`×`h` の PNG の IHDR の直後に `eXIf` チャンクを差し込む（65,527 バイトを超える Exif もこちらで作れる）
pub fn png_with_exif(w: u32, h: u32, tiff: &[u8]) -> Vec<u8> {
    let png = encode(w, h, image::ImageFormat::Png);
    // シグネチャ 8 バイト + IHDR（長さ 4 + 種類 4 + 本体 13 + CRC 4）
    let after_ihdr = 8 + 25;
    assert_eq!(&png[12..16], b"IHDR");
    let mut chunk = (tiff.len() as u32).to_be_bytes().to_vec();
    chunk.extend_from_slice(b"eXIf");
    chunk.extend_from_slice(tiff);
    chunk.extend_from_slice(&crc32(&chunk[4..]).to_be_bytes());
    [&png[..after_ihdr], &chunk, &png[after_ihdr..]].concat()
}

/// 画素だけの JPEG（Exif なし）
pub fn plain_jpeg(w: u32, h: u32) -> Vec<u8> {
    encode(w, h, image::ImageFormat::Jpeg)
}

fn encode(w: u32, h: u32, format: image::ImageFormat) -> Vec<u8> {
    let img = image::RgbImage::from_fn(w, h, |x, _| {
        if x < w / 2 {
            image::Rgb([255, 0, 0])
        } else {
            image::Rgb([0, 0, 255])
        }
    });
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, format).unwrap();
    out.into_inner()
}

/// PNG のチャンクの CRC-32（ISO 3309）
fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}
