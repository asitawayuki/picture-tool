//! XMP パケット（APP1 セグメント）の読み書き。
//!
//! **既存のパケットを削除して作り直してはならない。** Lightroom は現像設定を `crs:`
//! に、他のツールも独自の名前空間に大量のデータを書き込む。本ツールが扱う 4 プロパティ
//! だけを差し替え、他はすべて元のまま残す（spec の必須条件。ピクセル不変と同格）。
//!
//! 判定は**接頭辞ではなく名前空間 URI**で行う。`xmp:` `dc:` は慣習であって規約ではなく、
//! 書き手が別の接頭辞を使っていても同じプロパティである。

use anyhow::Result;
use img_parts::jpeg::{markers, Jpeg};
use quick_xml::events::{BytesStart, Event};
use quick_xml::name::{Namespace, ResolveResult};
use quick_xml::NsReader;

/// APP1 の XMP パケットに前置きされる識別子（末尾の NUL 込み）
pub(super) const XMP_ID: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";

pub(super) const NS_RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
pub(super) const NS_XMP: &str = "http://ns.adobe.com/xap/1.0/";
pub(super) const NS_DC: &str = "http://purl.org/dc/elements/1.1/";
/// **末尾のスラッシュに注意。** 落とすと Windows 側から読めない
pub(super) const NS_MS_PHOTO: &str = "http://ns.microsoft.com/photo/1.0/";

/// 本ツールが差し替える 4 プロパティ。(名前空間 URI, ローカル名)
pub(super) const OWNED: [(&str, &str); 4] = [
    (NS_XMP, "Rating"),
    (NS_MS_PHOTO, "Rating"),
    (NS_DC, "title"),
    (NS_DC, "description"),
];

/// XMP 側から読めた生の値。優先順位の判断はここではしない（`mod.rs` の責務）
#[derive(Debug, Default)]
pub(super) struct XmpSide {
    pub title: Option<String>,
    pub description: Option<String>,
    /// `xmp:Rating`（0〜5）。0 と -1（rejected）は `None` に落とす
    pub rating: Option<u8>,
    pub ms_rating_percent: Option<u16>,
}

/// JPEG から XMP パケットの XML を取り出す
pub(super) fn extract(jpeg: &Jpeg) -> Option<String> {
    jpeg.segments_by_marker(markers::APP1).find_map(|segment| {
        let xml = segment.contents().strip_prefix(XMP_ID)?;
        std::str::from_utf8(xml).ok().map(str::to_owned)
    })
}

fn is_owned_name(ns: &ResolveResult<'_>, local: &[u8]) -> bool {
    OWNED
        .iter()
        .any(|(uri, name)| matches(ns, local, uri, name))
}

fn uri_of<'ns>(ns: &ResolveResult<'ns>) -> Option<&'ns [u8]> {
    match ns {
        ResolveResult::Bound(Namespace(uri)) => Some(*uri),
        _ => None,
    }
}

fn matches(ns: &ResolveResult<'_>, local: &[u8], uri: &str, name: &str) -> bool {
    uri_of(ns) == Some(uri.as_bytes()) && local == name.as_bytes()
}

/// 読めた文字列を正規形に寄せる。**前後の空白は落とし、trim 後が空なら `None`。**
///
/// EXIF 側（`exif::cleaned`）と同じ扱いに揃えてある。XMP は要素テキストが整形の
/// 改行・インデントを含みうるので、ここで trim しないと同じ値が「食い違い」に
/// 見えたり、末尾に改行の付いたコメントの保存が失敗したりする（C1 対処）。
fn cleaned(text: &str) -> Option<String> {
    let trimmed = text.trim_matches(|c: char| c == '\0' || c.is_whitespace());
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// `xmp:Rating` の文字列を★にする。
/// 0（未評価）と -1（rejected）は `None`。★0 はパーセント系に表現できないため
fn parse_stars(text: &str) -> Option<u8> {
    let value: f64 = text.trim().parse().ok()?;
    let rounded = value.round();
    (1.0..=5.0).contains(&rounded).then_some(rounded as u8)
}

pub(super) fn read(packet: &str) -> Result<XmpSide> {
    let mut reader = NsReader::from_str(packet);
    let mut side = XmpSide::default();

    loop {
        let (ns, event) = reader.read_resolved_event()?;
        match event {
            Event::Eof => break,
            Event::Empty(ref start) => {
                if matches(&ns, start.local_name().as_ref(), NS_RDF, "Description") {
                    read_compact_attributes(&reader, start, &mut side)?;
                }
            }
            Event::Start(ref start) => {
                let local = start.local_name().as_ref().to_vec();
                if matches(&ns, &local, NS_RDF, "Description") {
                    read_compact_attributes(&reader, start, &mut side)?;
                    continue;
                }
                // **本ツールの 4 プロパティ以外では中身を読み進めない。**
                // ここで無条件に read_first_text を呼ぶと、x:xmpmeta や rdf:RDF の
                // 中身まで丸ごと飲み込んで、以降が一切パースされなくなる
                if !is_owned_name(&ns, &local) {
                    continue;
                }
                // URI を写して ns の借用を終わらせてから &mut reader を使う
                let Some(uri) = uri_of(&ns).map(<[u8]>::to_vec) else {
                    continue;
                };
                let end = start.to_end().into_owned();
                let text = read_first_text(&mut reader, &end)?;
                let Some(text) = text else { continue };
                match (uri.as_slice(), local.as_slice()) {
                    (u, b"Rating") if u == NS_XMP.as_bytes() => side.rating = parse_stars(&text),
                    (u, b"Rating") if u == NS_MS_PHOTO.as_bytes() => {
                        side.ms_rating_percent = text.trim().parse().ok()
                    }
                    (u, b"title") if u == NS_DC.as_bytes() => side.title = cleaned(&text),
                    (u, b"description") if u == NS_DC.as_bytes() => {
                        side.description = cleaned(&text)
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    Ok(side)
}

/// 圧縮形式（`rdf:Description` の属性として書かれた単純値）を拾う
fn read_compact_attributes(
    reader: &NsReader<&[u8]>,
    start: &BytesStart<'_>,
    side: &mut XmpSide,
) -> Result<()> {
    for attribute in start.attributes() {
        let attribute = attribute?;
        let (ns, local) = reader.resolve_attribute(attribute.key);
        let Some(uri) = uri_of(&ns) else { continue };
        let value = attribute.unescape_value()?.into_owned();
        match (uri, local.as_ref()) {
            (u, b"Rating") if u == NS_XMP.as_bytes() => side.rating = parse_stars(&value),
            (u, b"Rating") if u == NS_MS_PHOTO.as_bytes() => {
                side.ms_rating_percent = value.trim().parse().ok()
            }
            (u, b"title") if u == NS_DC.as_bytes() => side.title = cleaned(&value),
            (u, b"description") if u == NS_DC.as_bytes() => side.description = cleaned(&value),
            _ => {}
        }
    }
    Ok(())
}

/// 要素の中から最初の非空テキストを取る。`rdf:Alt` があれば
/// `xml:lang="x-default"` の `rdf:li` を優先する
fn read_first_text(
    reader: &mut NsReader<&[u8]>,
    end: &quick_xml::events::BytesEnd<'_>,
) -> Result<Option<String>> {
    let mut preferred: Option<String> = None;
    let mut first: Option<String> = None;
    let mut in_default = false;

    loop {
        let (ns, event) = reader.read_resolved_event()?;
        match event {
            Event::Eof => break,
            Event::Start(ref start) if matches(&ns, start.local_name().as_ref(), NS_RDF, "li") => {
                in_default = start.attributes().flatten().any(|attribute| {
                    attribute.key.as_ref() == b"xml:lang"
                        && attribute.value.as_ref() == b"x-default"
                });
            }
            Event::End(ref e) if e.name() == end.name() => break,
            Event::End(ref e) if matches(&ns, e.local_name().as_ref(), NS_RDF, "li") => {
                in_default = false;
            }
            Event::Text(text) => {
                let text = text.unescape()?.trim().to_string();
                if text.is_empty() {
                    continue;
                }
                if in_default && preferred.is_none() {
                    preferred = Some(text.clone());
                }
                if first.is_none() {
                    first = Some(text);
                }
            }
            _ => {}
        }
    }
    Ok(preferred.or(first))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 要素形式（Lightroom が `dc:title` / `dc:description` を書く形）
    const ELEMENT_FORM: &str = r#"<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about=""
    xmlns:dc="http://purl.org/dc/elements/1.1/"
    xmlns:xmp="http://ns.adobe.com/xap/1.0/"
    xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/"
    crs:Exposure2012="+0.35">
   <xmp:Rating>4</xmp:Rating>
   <dc:title><rdf:Alt><rdf:li xml:lang="x-default">夕暮れの港</rdf:li></rdf:Alt></dc:title>
   <dc:description><rdf:Alt><rdf:li xml:lang="x-default">順光</rdf:li></rdf:Alt></dc:description>
  </rdf:Description>
 </rdf:RDF>
</x:xmpmeta><?xpacket end="w"?>"#;

    /// 圧縮形式（単純値を属性で書く形）
    const ATTRIBUTE_FORM: &str = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about=""
    xmlns:xmp="http://ns.adobe.com/xap/1.0/"
    xmlns:MicrosoftPhoto="http://ns.microsoft.com/photo/1.0/"
    xmp:Rating="2" MicrosoftPhoto:Rating="25"/>
 </rdf:RDF>
</x:xmpmeta>"#;

    #[test]
    fn reads_properties_written_as_elements() {
        let side = read(ELEMENT_FORM).unwrap();
        assert_eq!(side.rating, Some(4));
        assert_eq!(side.title.as_deref(), Some("夕暮れの港"));
        assert_eq!(side.description.as_deref(), Some("順光"));
    }

    #[test]
    fn reads_properties_written_as_attributes() {
        let side = read(ATTRIBUTE_FORM).unwrap();
        assert_eq!(side.rating, Some(2));
        assert_eq!(side.ms_rating_percent, Some(25));
    }

    /// **接頭辞ではなく名前空間 URI で判定する。**
    /// `xmp:` `dc:` は慣習であって規約ではない
    #[test]
    fn resolves_by_namespace_uri_not_by_prefix() {
        let packet = r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about="" xmlns:zz="http://ns.adobe.com/xap/1.0/" zz:Rating="5"/>
 </rdf:RDF>"#;
        assert_eq!(read(packet).unwrap().rating, Some(5));
    }

    /// 他アプリが書いた 0（未評価）と -1（rejected）は★にしない
    #[test]
    fn zero_and_rejected_ratings_read_as_unrated() {
        for value in ["0", "-1"] {
            let packet = format!(
                r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about="" xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmp:Rating="{}"/>
 </rdf:RDF>"#,
                value
            );
            assert_eq!(read(&packet).unwrap().rating, None, "xmp:Rating={}", value);
        }
    }

    /// `xml:lang` の付いていない `rdf:li` しかない場合も拾う
    #[test]
    fn falls_back_to_the_first_alternative_without_x_default() {
        let packet = r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about="" xmlns:dc="http://purl.org/dc/elements/1.1/">
   <dc:title><rdf:Alt><rdf:li xml:lang="ja-JP">港</rdf:li></rdf:Alt></dc:title>
  </rdf:Description>
 </rdf:RDF>"#;
        assert_eq!(read(packet).unwrap().title.as_deref(), Some("港"));
    }

    /// 前提条件つきの否定形。XMP を持たない JPEG から取り出そうとしても落ちない
    #[test]
    fn extract_returns_none_when_there_is_no_xmp_segment() {
        use img_parts::jpeg::Jpeg;
        use img_parts::Bytes;
        let bytes = std::fs::read(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/exif-only.jpg"),
        )
        .unwrap();
        let jpeg = Jpeg::from_bytes(Bytes::from(bytes)).unwrap();
        // 前提条件: EXIF の APP1 は在る（＝「APP1 が無いから None」ではない）
        assert!(jpeg
            .segments_by_marker(img_parts::jpeg::markers::APP1)
            .any(|s| s.contents().starts_with(b"Exif\0\0")));
        assert_eq!(extract(&jpeg), None);
    }
}
