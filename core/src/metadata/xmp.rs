//! XMP パケット（APP1 セグメント）の読み書き。
//!
//! **既存のパケットを削除して作り直してはならない。** Lightroom は現像設定を `crs:`
//! に、他のツールも独自の名前空間に大量のデータを書き込む。本ツールが扱う 4 プロパティ
//! だけを差し替え、他はすべて元のまま残す（spec の必須条件。ピクセル不変と同格）。
//!
//! 判定は**接頭辞ではなく名前空間 URI**で行う。`xmp:` `dc:` は慣習であって規約ではなく、
//! 書き手が別の接頭辞を使っていても同じプロパティである。

use anyhow::{anyhow, bail, Result};
use img_parts::jpeg::{markers, Jpeg, JpegSegment};
use img_parts::Bytes;
use quick_xml::events::{BytesStart, BytesText, Event};
use quick_xml::name::{Namespace, ResolveResult};
use quick_xml::{NsReader, Writer};

use super::{cleaned, stars_to_percent, EditableMetadata};

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

/// JPEG から XMP パケットの XML を取り出す。
///
/// **Extended XMP（65503 バイトを超えるパケットが複数の APP1 に分割される形式）は
/// 意図的に扱わない。** その識別子は `http://ns.adobe.com/xmp/extension/\0`（GUID 付き）で
/// `XMP_ID` とは一致しないため、`strip_prefix` に弾かれて誤って本文に混入することはない
/// （安全側に倒れている）。本ツールが扱う4プロパティ（`dc:title` / `dc:description` /
/// `xmp:Rating` / `MicrosoftPhoto:Rating`）だけでこの上限を超えることは実運用で
/// 極めて考えにくく、対応する必要性が薄いための判断
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

/// APP1 セグメントの内容の上限。長さフィールドは 2 バイトで自身を含む
const MAX_APP1_CONTENTS: usize = 65_533;

/// 既存パケットのうち、本ツールが扱う 4 プロパティ**だけ**を差し替える。
///
/// 他名前空間のデータには一切触らない。`rdf:Description` が 1 つも無い（＝
/// 差し込み先が無い）壊れたパケットのときだけ、新規パケットに置き換える。
pub(super) fn patch(packet: &str, values: &EditableMetadata) -> Result<String> {
    let mut reader = NsReader::from_str(packet);
    let mut writer = Writer::new(Vec::new());
    let mut injected = false;

    loop {
        let (ns, event) = reader.read_resolved_event()?;
        match event {
            Event::Eof => break,

            Event::Start(start) => {
                if is_owned_name(&ns, start.local_name().as_ref()) {
                    // 中身ごと捨てる（あとで作り直す）
                    let end = start.to_end().into_owned();
                    reader.read_to_end(end.name())?;
                    continue;
                }
                if matches(&ns, start.local_name().as_ref(), NS_RDF, "Description") {
                    // **落とす属性が無いなら元の start タグをそのまま流す。**
                    // 組み直すと quick-xml の push_attribute が区切りを単一スペースに
                    // 揃えるため、Lightroom が複数行に分けて書いた crs: の属性群は
                    // 値は保たれても整形が変わる。触らなくてよいものは触らない
                    match strip_owned_attributes(&reader, &start)? {
                        Some(cleaned) => writer.write_event(Event::Start(cleaned))?,
                        None => writer.write_event(Event::Start(start.clone()))?,
                    }
                    if !injected {
                        write_properties(&mut writer, values)?;
                        injected = true;
                    }
                    continue;
                }
                writer.write_event(Event::Start(start))?;
            }

            Event::Empty(start) => {
                if is_owned_name(&ns, start.local_name().as_ref()) {
                    continue;
                }
                if matches(&ns, start.local_name().as_ref(), NS_RDF, "Description") {
                    let tag =
                        strip_owned_attributes(&reader, &start)?.unwrap_or_else(|| start.clone());
                    if injected {
                        writer.write_event(Event::Empty(tag))?;
                    } else {
                        // 空要素のままでは子を持てないので Start + 子 + End に開く
                        let end = tag.to_end().into_owned();
                        writer.write_event(Event::Start(tag))?;
                        write_properties(&mut writer, values)?;
                        writer.write_event(Event::End(end))?;
                        injected = true;
                    }
                    continue;
                }
                writer.write_event(Event::Empty(start))?;
            }

            other => writer.write_event(other)?,
        }
    }

    if !injected {
        // 差し込み先が無い＝ rdf:Description を持たない壊れたパケット。
        // ここだけは作り直す（他名前空間のデータもそもそも入らない形なので失うものが無い）
        return Ok(new_packet(values));
    }
    Ok(String::from_utf8(writer.into_inner())?)
}

/// `rdf:Description` の属性から、本ツールの 4 プロパティだけを落とす。
/// 要素形式だけ差し替えて属性形式を残すと、読み手によって新旧どちらも出うる。
///
/// **落とすものが無ければ `None` を返す。** 呼び出し側は元の start タグをそのまま
/// 書き戻す ── 組み直すと属性の区切りが単一スペースへ正規化されるため、
/// 触る必要が無いときは触らない。
fn strip_owned_attributes(
    reader: &NsReader<&[u8]>,
    start: &BytesStart<'_>,
) -> Result<Option<BytesStart<'static>>> {
    let mut kept = Vec::new();
    let mut dropped = false;
    for attribute in start.attributes() {
        let attribute = attribute?;
        let (ns, local) = reader.resolve_attribute(attribute.key);
        if is_owned_name(&ns, local.as_ref()) {
            dropped = true;
        } else {
            kept.push(attribute);
        }
    }
    if !dropped {
        return Ok(None);
    }
    let name = String::from_utf8(start.name().as_ref().to_vec())?;
    let mut cleaned = BytesStart::new(name);
    for attribute in kept {
        cleaned.push_attribute(attribute);
    }
    Ok(Some(cleaned))
}

fn write_properties<W: std::io::Write>(
    writer: &mut Writer<W>,
    values: &EditableMetadata,
) -> Result<()> {
    let xml = properties_xml(values);
    if xml.is_empty() {
        return Ok(());
    }
    // 組み立て済みの断片をそのまま流す。中の利用者テキストは properties_xml が
    // escape 済みなので、ここで再エスケープさせない
    writer.write_event(Event::Text(BytesText::from_escaped(xml)))?;
    Ok(())
}

fn properties_xml(values: &EditableMetadata) -> String {
    let mut out = String::new();
    if let Some(stars @ 1..=5) = values.rating {
        out.push_str(&format!(
            r#"<xmp:Rating xmlns:xmp="{NS_XMP}">{stars}</xmp:Rating>"#
        ));
        out.push_str(&format!(
            r#"<MicrosoftPhoto:Rating xmlns:MicrosoftPhoto="{NS_MS_PHOTO}">{}</MicrosoftPhoto:Rating>"#,
            stars_to_percent(stars)
        ));
    }
    if let Some(title) = values.title.as_deref().filter(|t| !t.is_empty()) {
        out.push_str(&lang_alt("dc:title", title));
    }
    if let Some(comment) = values.comment.as_deref().filter(|c| !c.is_empty()) {
        out.push_str(&lang_alt("dc:description", comment));
    }
    out
}

/// `dc:title` / `dc:description` は Lang Alt（`rdf:Alt` + `rdf:li`）。
///
/// **名前空間の宣言を要素自身に付ける。** `rdf:Description` 側に足すと、
/// 同じ接頭辞が別の URI に束縛されていた場合に兄弟のプロパティを壊しうる
fn lang_alt(name: &str, text: &str) -> String {
    let escaped = quick_xml::escape::escape(text);
    format!(
        r#"<{name} xmlns:dc="{NS_DC}" xmlns:rdf="{NS_RDF}"><rdf:Alt><rdf:li xml:lang="x-default">{escaped}</rdf:li></rdf:Alt></{name}>"#
    )
}

/// パケットが存在しない場合にだけ使う。**既存パケットの置き換えには使わない**
pub(super) fn new_packet(values: &EditableMetadata) -> String {
    format!(
        "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\
<x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\
<rdf:RDF xmlns:rdf=\"{NS_RDF}\">\
<rdf:Description rdf:about=\"\">{}</rdf:Description>\
</rdf:RDF></x:xmpmeta><?xpacket end=\"w\"?>",
        properties_xml(values)
    )
}

/// XMP パケットを APP1 として据え付ける。既存の XMP セグメントは置き換える
pub(super) fn install(jpeg: &mut Jpeg, packet: &str) -> Result<()> {
    let mut contents = Vec::with_capacity(XMP_ID.len() + packet.len());
    contents.extend_from_slice(XMP_ID);
    contents.extend_from_slice(packet.as_bytes());
    if contents.len() > MAX_APP1_CONTENTS {
        bail!(
            "XMP が APP1 の上限（{} バイト）を超えました。既存のデータを削らずには収まりません",
            MAX_APP1_CONTENTS
        );
    }

    // EXIF の APP1 は残す。落とすのは XMP の APP1 だけ
    jpeg.segments_mut().retain(|segment| {
        !(segment.marker() == markers::APP1 && segment.contents().starts_with(XMP_ID))
    });

    // **push ではなく insert。** push すると EOI の後ろに置かれ、再パース時に消える。
    // 置き場所は Exif の APP1 の直後 ── XMP 仕様が推奨する位置であり、
    // 読み手を選ばない。Exif が無ければ SOS の直前に落とす（規格上はどちらも合法）
    let scan = jpeg
        .segments()
        .iter()
        .position(|segment| segment.marker() == markers::SOS)
        .ok_or_else(|| anyhow!("SOS セグメントが見つかりません"))?;
    let at = jpeg
        .segments()
        .iter()
        .position(|segment| {
            segment.marker() == markers::APP1 && segment.contents().starts_with(b"Exif\0\0")
        })
        .map_or(scan, |exif| exif + 1);
    jpeg.segments_mut().insert(
        at,
        JpegSegment::new_with_contents(markers::APP1, Bytes::from(contents)),
    );
    Ok(())
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
    use crate::metadata::EditableMetadata;

    /// 名前空間の宣言（`xmlns:*`）そのものの名前空間
    const NS_XMLNS: &str = "http://www.w3.org/2000/xmlns/";

    /// 名前空間つきで、本ツールの 4 プロパティ**以外**をすべて列挙する。
    /// 「他名前空間が変わらない」の検査対象を、読み取りとは別の経路で取り出す。
    ///
    /// 比較は**意味論**で行う（利用者判断 U4）。要素・属性はすべて解決済みの
    /// 名前空間 URI 付きで並べるので、宣言が 1 つでも失われれば、それを使っていた
    /// 要素・属性の URI が変わって差分に出る。
    ///
    /// **本ツールが差し込む 4 プロパティ要素に付けた宣言だけは対象外にする。**
    /// 宣言はプロパティでも値でもなく、`rdf:Description` 側ではなく要素自身に
    /// 付けるのが設計（同じ接頭辞が別の URI に束縛されていても兄弟を壊さない）。
    /// 他名前空間の要素に付いた宣言は今までどおり全部比較する
    fn foreign_content(packet: &str) -> Vec<String> {
        let mut reader = NsReader::from_str(packet);
        let mut out = Vec::new();
        loop {
            let (ns, event) = reader.read_resolved_event().unwrap();
            match event {
                Event::Eof => break,
                Event::Start(ref s) | Event::Empty(ref s) => {
                    let uri = uri_of(&ns).map(|u| String::from_utf8_lossy(u).into_owned());
                    let local = String::from_utf8_lossy(s.local_name().as_ref()).into_owned();
                    let element_is_owned = OWNED.iter().any(|(o_uri, o_local)| {
                        uri.as_deref() == Some(*o_uri) && local == *o_local
                    });
                    if !element_is_owned {
                        out.push(format!("<{:?}:{}>", uri, local));
                    }
                    for attribute in s.attributes() {
                        let attribute = attribute.unwrap();
                        let (a_ns, a_local) = reader.resolve_attribute(attribute.key);
                        let a_uri = uri_of(&a_ns).map(|u| String::from_utf8_lossy(u).into_owned());
                        let a_local = String::from_utf8_lossy(a_local.as_ref()).into_owned();
                        if element_is_owned && a_uri.as_deref() == Some(NS_XMLNS) {
                            continue;
                        }
                        let is_owned = OWNED.iter().any(|(o_uri, o_local)| {
                            a_uri.as_deref() == Some(*o_uri) && a_local == *o_local
                        });
                        if !is_owned {
                            out.push(format!(
                                "@{:?}:{}={}",
                                a_uri,
                                a_local,
                                attribute.unescape_value().unwrap()
                            ));
                        }
                    }
                }
                _ => {}
            }
        }
        out.sort();
        out
    }

    #[test]
    fn patching_leaves_every_other_namespace_untouched() {
        let before = foreign_content(ELEMENT_FORM);
        // 前提条件: 検査対象（crs:）が本当に入っていること。
        // ここが空だと「壊れていない」は自明に通ってしまう
        assert!(
            before
                .iter()
                .any(|item| item.contains("camera-raw-settings")),
            "フィクスチャに他名前空間が無い: {:?}",
            before
        );

        let patched = patch(
            ELEMENT_FORM,
            &EditableMetadata {
                title: Some("新しい題".into()),
                comment: Some("新しい注".into()),
                rating: Some(2),
            },
        )
        .unwrap();

        assert_eq!(foreign_content(&patched), before);
    }

    /// 落とす属性が無い `rdf:Description` は、**タグごとバイト列で保たれる**こと。
    /// 組み直すと属性の区切りが単一スペースに正規化される
    #[test]
    fn a_description_without_owned_attributes_is_copied_byte_for_byte() {
        let patched = patch(
            ELEMENT_FORM,
            &EditableMetadata {
                rating: Some(1),
                ..Default::default()
            },
        )
        .unwrap();
        let original_tag = &ELEMENT_FORM[ELEMENT_FORM.find("<rdf:Description").unwrap()
            ..ELEMENT_FORM.find("crs:Exposure2012=\"+0.35\">").unwrap()
                + "crs:Exposure2012=\"+0.35\">".len()];
        assert!(
            patched.contains(original_tag),
            "改行込みの属性の並びが変わった:\n{}",
            patched
        );
    }

    #[test]
    fn patching_replaces_the_four_owned_properties() {
        let patched = patch(
            ELEMENT_FORM,
            &EditableMetadata {
                title: Some("新しい題".into()),
                comment: Some("新しい注".into()),
                rating: Some(2),
            },
        )
        .unwrap();

        let side = read(&patched).unwrap();
        assert_eq!(side.title.as_deref(), Some("新しい題"));
        assert_eq!(side.description.as_deref(), Some("新しい注"));
        assert_eq!(side.rating, Some(2));
        assert_eq!(side.ms_rating_percent, Some(25));
    }

    /// 圧縮形式で書かれていた古い値が**属性として残らない**こと。
    /// 要素だけ差し替えて属性を残すと、読み手によって新旧どちらも出うる
    #[test]
    fn patching_drops_owned_values_written_as_attributes() {
        let patched = patch(
            ATTRIBUTE_FORM,
            &EditableMetadata {
                rating: Some(5),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(
            !patched.contains("xmp:Rating=\""),
            "属性形式の古い値が残っている: {}",
            patched
        );
        assert_eq!(read(&patched).unwrap().rating, Some(5));
    }

    /// 空欄で保存したらプロパティごと消える
    #[test]
    fn patching_with_empty_values_removes_the_properties() {
        let patched = patch(ELEMENT_FORM, &EditableMetadata::default()).unwrap();
        let side = read(&patched).unwrap();
        assert_eq!(side.title, None);
        assert_eq!(side.description, None);
        assert_eq!(side.rating, None);
        // 他名前空間は残っている
        assert!(patched.contains("camera-raw-settings"));
    }

    #[test]
    fn a_new_packet_is_readable_by_our_own_reader() {
        let packet = new_packet(&EditableMetadata {
            title: Some("港".into()),
            comment: Some("順光".into()),
            rating: Some(3),
        });
        let side = read(&packet).unwrap();
        assert_eq!(side.title.as_deref(), Some("港"));
        assert_eq!(side.description.as_deref(), Some("順光"));
        assert_eq!(side.rating, Some(3));
        assert_eq!(side.ms_rating_percent, Some(50));
    }

    /// XML の特殊文字を含むテキストが往復すること
    #[test]
    fn special_characters_survive_the_round_trip() {
        let text = r#"a < b & c > d "e" 'f'"#;
        let packet = new_packet(&EditableMetadata {
            title: Some(text.into()),
            ..Default::default()
        });
        assert_eq!(read(&packet).unwrap().title.as_deref(), Some(text));
    }

    /// **push すると EOI の後ろに置かれ、再パース時に消える**（spec の落とし穴）
    #[test]
    fn install_puts_the_packet_before_the_scan() {
        use img_parts::jpeg::Jpeg;
        use img_parts::Bytes;
        let bytes = std::fs::read(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/exif-only.jpg"),
        )
        .unwrap();
        let mut jpeg = Jpeg::from_bytes(Bytes::from(bytes)).unwrap();
        // 前提条件: いまは XMP を持っていない
        assert_eq!(extract(&jpeg), None);

        let packet = new_packet(&EditableMetadata {
            rating: Some(1),
            ..Default::default()
        });
        install(&mut jpeg, &packet).unwrap();

        // 書き出して読み直しても残っていること（＝ SOS より前に入った）
        let encoded = jpeg.encoder().bytes();
        let reparsed = Jpeg::from_bytes(encoded).unwrap();
        assert_eq!(read(&extract(&reparsed).unwrap()).unwrap().rating, Some(1));
    }

    /// 2 回書いてもセグメントが増えない
    #[test]
    fn install_replaces_the_existing_packet_instead_of_adding_one() {
        use img_parts::jpeg::{markers, Jpeg};
        use img_parts::Bytes;
        let bytes = std::fs::read(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/lightroom-xmp.jpg"),
        )
        .unwrap();
        let mut jpeg = Jpeg::from_bytes(Bytes::from(bytes)).unwrap();
        let before = jpeg.segments_by_marker(markers::APP1).count();
        // 前提条件: すでに XMP を持っている
        assert!(extract(&jpeg).is_some());

        for stars in [1u8, 5] {
            let packet = patch(
                &extract(&jpeg).unwrap(),
                &EditableMetadata {
                    rating: Some(stars),
                    ..Default::default()
                },
            )
            .unwrap();
            install(&mut jpeg, &packet).unwrap();
        }

        assert_eq!(jpeg.segments_by_marker(markers::APP1).count(), before);
        assert_eq!(read(&extract(&jpeg).unwrap()).unwrap().rating, Some(5));
    }
}
