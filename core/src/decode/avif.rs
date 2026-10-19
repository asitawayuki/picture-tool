//! AVIF の、avif-parse / avif-decode が扱わない部分
//!
//! - 主画像の寸法（`ispe`）と向き（`irot` / `imir`）。どちらのクレートもこれらを読まない
//! - アイテムの AV1 データのシーケンスヘッダの数（av1-avif §2.1「exactly one」）と、その宣言寸法

use super::isobmff;
use anyhow::{Context, Result, bail, ensure};
use image::DynamicImage;
use std::io::{Read, Seek};

/// 主画像の、向きを適用する前の寸法と、適用すべき変換（ファイルに書かれた順）
pub(super) struct Layout {
    pub width: u32,
    pub height: u32,
    pub transforms: Vec<Transform>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Transform {
    /// `irot`: 反時計回りに 90 度 × n
    Rotate(u8),
    /// `imir`: 0 は上下の入れ替え、1 は左右の入れ替え（ISO/IEC 23008-12:2022 の mode。
    /// libavif も同じ意味で書く）
    Mirror(u8),
}

impl Layout {
    /// 向きを適用した後の (幅, 高さ)
    pub(super) fn oriented_dimensions(&self) -> (u32, u32) {
        let quarter_turns: u32 = self
            .transforms
            .iter()
            .map(|t| match t {
                Transform::Rotate(n) => u32::from(*n),
                Transform::Mirror(_) => 0,
            })
            .sum();
        if quarter_turns % 2 == 1 {
            (self.height, self.width)
        } else {
            (self.width, self.height)
        }
    }

    pub(super) fn apply(&self, mut img: DynamicImage) -> DynamicImage {
        for t in &self.transforms {
            img = match t {
                Transform::Rotate(1) => img.rotate270(),
                Transform::Rotate(2) => img.rotate180(),
                Transform::Rotate(3) => img.rotate90(),
                Transform::Rotate(_) => img,
                Transform::Mirror(0) => img.flipv(),
                Transform::Mirror(_) => img.fliph(),
            };
        }
        img
    }
}

/// `meta` ボックスだけを読んで主画像の [`Layout`] を得る（画像データは読まない）
pub(super) fn read_layout(mut r: impl Read + Seek) -> Result<Layout> {
    let meta = loop {
        let header = isobmff::read_header(&mut r)?.context("AVIF has no meta box")?;
        if &header.kind == b"meta" {
            break isobmff::read_body(&mut r, header.body_len)?;
        }
        isobmff::skip(&mut r, header.body_len.context("AVIF has no meta box")?)?;
    };
    // meta は FullBox。version と flags を飛ばす
    let meta = isobmff::children(meta.get(4..).context("meta box is too short")?)?;

    let pitm = find(&meta, b"pitm").context("AVIF has no primary item")?;
    let primary = match pitm.first() {
        Some(0) => u32::from(u16::from_be_bytes(field(pitm, 4)?)),
        _ => u32::from_be_bytes(field(pitm, 4)?),
    };

    let iprp = isobmff::children(find(&meta, b"iprp").context("AVIF has no item properties")?)?;
    let ipco = isobmff::children(find(&iprp, b"ipco").context("AVIF has no item properties")?)?;

    let mut size = None;
    let mut transforms = Vec::new();
    for index in property_indices(&iprp, primary)? {
        // 番号は 1 始まり。0 は「なし」
        let Some((kind, body)) = index.checked_sub(1).and_then(|i| ipco.get(i)) else {
            continue;
        };
        match kind {
            b"ispe" => {
                size = Some((
                    u32::from_be_bytes(field(body, 4)?),
                    u32::from_be_bytes(field(body, 8)?),
                ))
            }
            b"irot" => transforms.push(Transform::Rotate(field::<1>(body, 0)?[0] & 0b11)),
            b"imir" => transforms.push(Transform::Mirror(field::<1>(body, 0)?[0] & 0b1)),
            _ => {}
        }
    }
    let (width, height) = size.context("AVIF primary item has no size (ispe)")?;
    Ok(Layout {
        width,
        height,
        transforms,
    })
}

fn find<'a>(boxes: &[([u8; 4], &'a [u8])], kind: &[u8; 4]) -> Option<&'a [u8]> {
    boxes.iter().find(|(k, _)| k == kind).map(|(_, body)| *body)
}

fn field<const N: usize>(data: &[u8], at: usize) -> Result<[u8; N]> {
    data.get(at..)
        .and_then(|d| d.first_chunk::<N>())
        .copied()
        .context("AVIF box is truncated")
}

/// `ipma` から、ある項目に結びつく性質の番号を書かれた順に集める
fn property_indices(iprp: &[([u8; 4], &[u8])], item: u32) -> Result<Vec<usize>> {
    let mut indices = Vec::new();
    for (_, ipma) in iprp.iter().filter(|(k, _)| k == b"ipma") {
        let version = field::<1>(ipma, 0)?[0];
        let wide_index = field::<1>(ipma, 3)?[0] & 1 == 1;
        let mut at = 8;
        for _ in 0..u32::from_be_bytes(field(ipma, 4)?) {
            let id = if version < 1 {
                at += 2;
                u32::from(u16::from_be_bytes(field(ipma, at - 2)?))
            } else {
                at += 4;
                u32::from_be_bytes(field(ipma, at - 4)?)
            };
            let count = field::<1>(ipma, at)?[0];
            at += 1;
            for _ in 0..count {
                // 最上位ビットは essential フラグ
                let index = if wide_index {
                    at += 2;
                    usize::from(u16::from_be_bytes(field(ipma, at - 2)?) & 0x7FFF)
                } else {
                    at += 1;
                    usize::from(field::<1>(ipma, at - 1)?[0] & 0x7F)
                };
                if id == item {
                    indices.push(index);
                }
            }
        }
    }
    Ok(indices)
}

/// アイテムの AV1 データが宣言するフレームの寸法とビット深度（シーケンスヘッダの値）
#[derive(Debug)]
pub(super) struct Declared {
    /// `max_frame_width × max_frame_height`
    pub pixels: u64,
    pub bit_depth: u8,
}

/// シーケンスヘッダの宣言を読む。シーケンスヘッダがちょうど 1 個でなければ `Err`
///
/// av1-avif §2.1 はアイテムのシーケンスヘッダを「exactly one」と定める。数を 1 に限るのは、
/// 宣言の最大寸法を上限の根拠にするため: avif-parse が読むのは最初のヘッダだけで、
/// 後続のヘッダで寸法を大きく差し替えられると、宣言を見て通した判定が意味を失う。
/// 1 個であれば、avif-parse が読む「最初のヘッダ」は唯一のヘッダと一致する。
pub(super) fn declared(av1: &[u8]) -> Result<Declared> {
    let count = sequence_header_count(av1)?;
    ensure!(
        count == 1,
        "AVIF item must have exactly one sequence header (found {count})"
    );
    let meta = avif_parse::AV1Metadata::parse_av1_bitstream(av1)?;
    Ok(Declared {
        pixels: u64::from(meta.max_frame_width.get()) * u64::from(meta.max_frame_height.get()),
        bit_depth: meta.bit_depth,
    })
}

/// OBU を先頭から歩いて、シーケンスヘッダ OBU の数を数える。切れた OBU は `Err`
fn sequence_header_count(mut obus: &[u8]) -> Result<usize> {
    const OBU_SEQUENCE_HEADER: u8 = 1;
    let mut count = 0;
    while let Some((&header, rest)) = obus.split_first() {
        let has_extension = header & 0b100 != 0;
        let has_size = header & 0b10 != 0;
        let mut rest = rest
            .get(usize::from(has_extension)..)
            .context("OBU is truncated")?;
        let len = if has_size {
            leb128(&mut rest)?
        } else {
            rest.len()
        };
        let (_, next) = rest.split_at_checked(len).context("OBU is truncated")?;
        if (header >> 3) & 0b1111 == OBU_SEQUENCE_HEADER {
            count += 1;
        }
        obus = next;
    }
    Ok(count)
}

fn leb128(data: &mut &[u8]) -> Result<usize> {
    let mut value = 0u64;
    for i in 0..8 {
        let (&byte, rest) = data.split_first().context("OBU size is truncated")?;
        *data = rest;
        value |= u64::from(byte & 0x7F) << (i * 7);
        if byte & 0x80 == 0 {
            return Ok(usize::try_from(value)?);
        }
    }
    bail!("OBU size is too long")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// MSB から詰めるビット列（AV1 仕様の f(n)）
    #[derive(Default)]
    struct Writer {
        bits: Vec<bool>,
    }

    impl Writer {
        fn put(&mut self, value: u64, n: u32) -> &mut Self {
            for i in (0..n).rev() {
                self.bits.push((value >> i) & 1 == 1);
            }
            self
        }

        fn bytes(&self) -> Vec<u8> {
            self.bits
                .chunks(8)
                .map(|c| (0..8).fold(0, |b, i| (b << 1) | u8::from(c.get(i) == Some(&true))))
                .collect()
        }
    }

    /// 型 `kind` の OBU（サイズ欄あり）
    fn obu(kind: u8, payload: &[u8]) -> Vec<u8> {
        assert!(payload.len() < 128);
        [&[(kind << 3) | 0b10, payload.len() as u8][..], payload].concat()
    }

    /// reduced_still_picture_header のシーケンスヘッダ
    fn reduced(width_bits: u32, height_bits: u32, width: u64, height: u64) -> Vec<u8> {
        let mut w = Writer::default();
        w.put(0, 3).put(1, 1).put(1, 1).put(8, 5); // profile, still, reduced, level
        w.put(u64::from(width_bits - 1), 4)
            .put(u64::from(height_bits - 1), 4);
        w.put(width - 1, width_bits).put(height - 1, height_bits);
        obu(1, &w.bytes())
    }

    fn boxed(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        [&(8 + body.len() as u32).to_be_bytes()[..], kind, body].concat()
    }

    fn full_box(kind: &[u8; 4], version: u8, flags: u8, body: &[u8]) -> Vec<u8> {
        boxed(kind, &[&[version, 0, 0, flags][..], body].concat())
    }

    fn ispe(width: u32, height: u32) -> Vec<u8> {
        full_box(
            b"ispe",
            0,
            0,
            &[width.to_be_bytes(), height.to_be_bytes()].concat(),
        )
    }

    /// ftyp と、主画像 `primary` の meta だけのファイル
    fn file(pitm: Vec<u8>, ipco: &[Vec<u8>], ipmas: &[Vec<u8>]) -> Vec<u8> {
        let iprp = boxed(
            b"iprp",
            &[boxed(b"ipco", &ipco.concat()), ipmas.concat()].concat(),
        );
        let meta = full_box(b"meta", 0, 0, &[pitm, iprp].concat());
        [boxed(b"ftyp", b"avif\0\0\0\0"), meta, boxed(b"mdat", b"")].concat()
    }

    fn layout(data: Vec<u8>) -> Layout {
        read_layout(std::io::Cursor::new(data)).unwrap()
    }

    #[test]
    fn narrow_ipma_associates_properties_with_the_primary_item_only() {
        // 性質: 1=ispe(64x32) 2=irot(1) 3=imir(1) 4=ispe(8x8, 別の項目用)
        let ipco = [
            ispe(64, 32),
            boxed(b"irot", &[1]),
            boxed(b"imir", &[1]),
            ispe(8, 8),
        ];
        // version 0（16 ビットの項目番号）・flags 0（7 ビットの性質番号、最上位は essential）
        let ipma = full_box(
            b"ipma",
            0,
            0,
            &[
                &2u32.to_be_bytes()[..],
                &[0, 1, 3, 0x81, 0x82, 0x83], // 項目 1: ispe, irot, imir
                &[0, 2, 2, 0x84, 0x00],       // 項目 2: ispe(8x8), 番号 0（なし）
            ]
            .concat(),
        );
        let l = layout(file(full_box(b"pitm", 0, 0, &[0, 1]), &ipco, &[ipma]));
        assert_eq!((l.width, l.height), (64, 32));
        assert_eq!(l.transforms, [Transform::Rotate(1), Transform::Mirror(1)]);
        assert_eq!(l.oriented_dimensions(), (32, 64));
    }

    #[test]
    fn wide_ipma_with_32_bit_item_ids_across_several_ipma_boxes() {
        let ipco = [boxed(b"imir", &[0]), ispe(10, 20), boxed(b"irot", &[2])];
        // version 1（32 ビットの項目番号）・flags 1（15 ビットの性質番号）
        let wide = |item: u32, indices: &[u16]| {
            let mut body = [&1u32.to_be_bytes()[..], &item.to_be_bytes()].concat();
            body.push(indices.len() as u8);
            for i in indices {
                body.extend((i | 0x8000).to_be_bytes());
            }
            full_box(b"ipma", 1, 1, &body)
        };
        let item: u32 = 0x0001_0000;
        let l = layout(file(
            full_box(b"pitm", 1, 0, &item.to_be_bytes()),
            &ipco,
            &[wide(item, &[2, 3]), wide(7, &[1]), wide(item, &[1])],
        ));
        assert_eq!((l.width, l.height), (10, 20));
        // 書かれた順（ipma の並び順）に適用する
        assert_eq!(l.transforms, [Transform::Rotate(2), Transform::Mirror(0)]);
        assert_eq!(l.oriented_dimensions(), (10, 20));
    }

    #[test]
    fn missing_size_or_meta_is_an_error() {
        let ipma = full_box(b"ipma", 0, 0, &[0, 0, 0, 1, 0, 1, 1, 1]);
        let no_ispe = file(
            full_box(b"pitm", 0, 0, &[0, 1]),
            &[boxed(b"irot", &[1])],
            &[ipma],
        );
        assert!(read_layout(std::io::Cursor::new(no_ispe)).is_err());
        let no_meta = [boxed(b"ftyp", b"avif\0\0\0\0"), boxed(b"mdat", b"")].concat();
        assert!(read_layout(std::io::Cursor::new(no_meta)).is_err());
    }

    const TEMPORAL_DELIMITER: &[u8] = &[0x12, 0x00];

    #[test]
    fn exactly_one_sequence_header_is_required() {
        assert_eq!(sequence_header_count(&reduced(6, 5, 64, 32)).unwrap(), 1);
        let two = [
            reduced(6, 5, 64, 32),
            obu(6, &[0; 4]),
            reduced(6, 5, 64, 32),
        ]
        .concat();
        assert_eq!(sequence_header_count(&two).unwrap(), 2);
        assert_eq!(sequence_header_count(TEMPORAL_DELIMITER).unwrap(), 0);
        let err = declared(&two).unwrap_err();
        assert!(
            format!("{err:#}").contains("exactly one sequence header (found 2)"),
            "{err:#}"
        );
    }

    #[test]
    fn truncated_obu_is_an_error() {
        let mut truncated = reduced(13, 12, 6000, 4000);
        truncated.truncate(4);
        assert!(sequence_header_count(&truncated).is_err());
        let mut short_payload = reduced(13, 12, 6000, 4000);
        short_payload[1] = 2; // サイズ欄は正しいが中身が足りない
        short_payload.truncate(4);
        // OBU としては切れていないので数は 1。中身が足りないのは寸法を読む側が弾く
        assert_eq!(sequence_header_count(&short_payload).unwrap(), 1);
        assert!(declared(&short_payload).is_err());
    }

    #[test]
    fn declared_size_is_the_sequence_header_maximum() {
        // 実ファイルの AV1 データ（64x32）
        let data = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/orient_1.avif"
        ))
        .unwrap();
        let parsed = avif_parse::read_avif(&mut data.as_slice()).unwrap();
        let d = declared(&parsed.primary_item).unwrap();
        assert_eq!((d.pixels, d.bit_depth), (64 * 32, 8));
    }
}
