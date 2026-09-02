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

use super::{cleaned, stars_to_percent, without_forbidden_controls, EditableMetadata};

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

/// **読み取りも `patch` と同じ深さの規則に従う。** 4 プロパティとみなすのは
/// トップレベル `rdf:Description`（`rdf:RDF` 直下）の直接の子だけ。
/// `xmpMM:DerivedFrom` の中の `dc:title` は「元画像の題」であって
/// 「この画像の題」ではないので拾わない。
///
/// **書き手と読み手で規則が食い違うと、保存 → 読み戻しの一致検証（Task 9）が
/// 壊れる**（`patch` が入れ子を残す一方 `read` がそれを拾い、保存できているのに
/// 失敗と報告される）
pub(super) fn read(packet: &str) -> Result<XmpSide> {
    let mut reader = NsReader::from_str(packet);
    let mut side = XmpSide::default();
    let mut stack: Vec<Frame> = Vec::new();

    loop {
        let (ns, event) = reader.read_resolved_event()?;
        match event {
            Event::Eof => break,
            Event::End(_) => {
                stack.pop();
            }
            Event::Empty(ref start) => {
                if frame_of(&ns, start.local_name().as_ref(), stack.last()) == Frame::TopDescription
                {
                    read_compact_attributes(&reader, start, &mut side)?;
                }
            }
            Event::Start(ref start) => {
                let local = start.local_name().as_ref().to_vec();
                let frame = frame_of(&ns, &local, stack.last());
                let owned_here =
                    stack.last() == Some(&Frame::TopDescription) && is_owned_name(&ns, &local);
                if frame == Frame::TopDescription {
                    read_compact_attributes(&reader, start, &mut side)?;
                    stack.push(frame);
                    continue;
                }
                // **本ツールの 4 プロパティ以外では中身を読み進めない。**
                // ここで無条件に read_first_text を呼ぶと、x:xmpmeta や rdf:RDF の
                // 中身まで丸ごと飲み込んで、以降が一切パースされなくなる
                if !owned_here {
                    stack.push(frame);
                    continue;
                }
                // URI を写して ns の借用を終わらせてから &mut reader を使う。
                // この先は read_first_text が部分木ごと消費するので stack は積まない
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

/// `patch` と検査器が「XML のどこにいるか」を見るための枠。
///
/// **本ツールの 4 プロパティとみなすのは「トップレベル `rdf:Description`
/// （`rdf:RDF` の直下）の直接の子」だけ。** 名前だけで深さ無制限に判定すると、
/// `xmpMM:DerivedFrom` の中の `dc:title` のような**他名前空間の構造化値の一部**まで
/// 巻き添えで消してしまう（Premiere / After Effects の `xmpMM:Pantry` や
/// `xmpMM:Ingredients` で現実に踏む）。それは「この画像のタイトル」ではない
#[derive(Debug, PartialEq, Eq)]
enum Frame {
    /// パケットの最外殻。`x:xmpmeta` のようなラッパを名前で決め打ちしないための枠
    /// （古い `x:xapmeta` や、ラッパを持たないパケットも同じ規則で扱える）
    PacketRoot,
    /// **パケットの最外殻か、その直下**にある `rdf:RDF`。
    /// 深さを見ないと、`xmpMM:Pantry` が抱える**入れ子の `rdf:RDF`** まで
    /// 「本体」とみなしてしまう（その中の `dc:title` は派生元素材の題であって
    /// この画像の題ではない）
    RdfRoot,
    /// `RdfRoot` 直下の `rdf:Description`。本ツールのプロパティが載る唯一の場所
    TopDescription,
    Other,
}

fn frame_of(ns: &ResolveResult<'_>, local: &[u8], parent: Option<&Frame>) -> Frame {
    let at_packet_top = matches!(parent, None | Some(Frame::PacketRoot));
    if at_packet_top && matches(ns, local, NS_RDF, "RDF") {
        Frame::RdfRoot
    } else if parent.is_none() {
        Frame::PacketRoot
    } else if parent == Some(&Frame::RdfRoot) && matches(ns, local, NS_RDF, "Description") {
        Frame::TopDescription
    } else {
        Frame::Other
    }
}

/// 既存パケットのうち、本ツールが扱う 4 プロパティ**だけ**を差し替える。
///
/// 他名前空間のデータには一切触らない。**書き込み先の `rdf:Description` が
/// 見つからないときは作り直さず `Err` を返す**（下の理由を参照）。
pub(super) fn patch(packet: &str, values: &EditableMetadata) -> Result<String> {
    if packet.trim().is_empty() {
        // 中身が無いのだから、作り直しても失うものが無い唯一のケース
        return Ok(new_packet(values));
    }

    let mut reader = NsReader::from_str(packet);
    let mut writer = Writer::new(Vec::new());
    let mut stack: Vec<Frame> = Vec::new();
    let mut injected = false;

    loop {
        let (ns, event) = reader.read_resolved_event()?;
        match event {
            Event::Eof => break,

            Event::Start(start) => {
                // ns の借用はここで使い切る（このあと reader を借り直すため）
                let frame = frame_of(&ns, start.local_name().as_ref(), stack.last());
                let owned_here = stack.last() == Some(&Frame::TopDescription)
                    && is_owned_name(&ns, start.local_name().as_ref());

                if owned_here {
                    // 中身ごと捨てる（あとで作り直す）
                    let end = start.to_end().into_owned();
                    reader.read_to_end(end.name())?;
                    continue;
                }
                if frame == Frame::TopDescription {
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
                    stack.push(frame);
                    continue;
                }
                writer.write_event(Event::Start(start))?;
                stack.push(frame);
            }

            Event::Empty(start) => {
                let frame = frame_of(&ns, start.local_name().as_ref(), stack.last());
                let owned_here = stack.last() == Some(&Frame::TopDescription)
                    && is_owned_name(&ns, start.local_name().as_ref());

                if owned_here {
                    continue;
                }
                if frame == Frame::TopDescription {
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

            Event::End(end) => {
                stack.pop();
                writer.write_event(Event::End(end))?;
            }

            other => writer.write_event(other)?,
        }
    }

    if !injected {
        // **ここで作り直してはならない。** 「`rdf:Description` が無い＝他名前空間の
        // データも入らない形なので失うものが無い」は**偽**である ── 実際にここへ来る
        // のは「RDF の名前空間 URI に解決できる `rdf:Description` が 1 つも無い」場合で、
        // 宣言の URI が 1 文字違う・`xmlns:rdf` を書き忘れた、といったパケットでも
        // 起きる。その中の `crs:` などのデータは健在でありうる。
        // 黙って全消しするより、保存を失敗させて元ファイルを無傷で残すほうが一貫している
        bail!("XMP パケットに書き込み先の rdf:Description が見つかりません（壊れている可能性があります）");
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
        // **生バイトのまま push してはならない。** `Attribute::value` は引用符の
        // 内側の生バイトで、元が**単一引用符**で囲まれていれば `"` を含みうる
        // （XML として合法）。`push_attribute` は必ず `"` で囲むので、そのまま出すと
        // 非整形式になり、**そのパケットが全リーダーから読めなくなる**。
        // `(&str, &str)` 版は値をエスケープするので、表現は変わっても値は不変。
        // 不正な実体参照はここで `Err` になる ── 壊れたものを黙って書くより、
        // 保存を失敗させて元ファイルを無傷で残す（Task 9 の置換は原子的）
        let key = String::from_utf8(attribute.key.as_ref().to_vec())?;
        let value = attribute.unescape_value()?.into_owned();
        cleaned.push_attribute((key.as_str(), value.as_str()));
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
    // **フィルタしてから空判定する。** 逆にすると `"\u{0b}"` のような入力で
    // EXIF 側（`exif::apply` は フィルタ → 空判定）はタグを削除し、XMP 側は
    // 空の `dc:title` 要素を書く ── 「空欄ならプロパティごと消す」規則が片側で破れる
    let title = values.title.as_deref().map(without_forbidden_controls);
    let comment = values.comment.as_deref().map(without_forbidden_controls);
    if let Some(title) = title.as_deref().filter(|t| !t.is_empty()) {
        out.push_str(&lang_alt("dc:title", title));
    }
    if let Some(comment) = comment.as_deref().filter(|c| !c.is_empty()) {
        out.push_str(&lang_alt("dc:description", comment));
    }
    out
}

/// `dc:title` / `dc:description` は Lang Alt（`rdf:Alt` + `rdf:li`）。
///
/// **名前空間の宣言を要素自身に付ける。** `rdf:Description` 側に足すと、
/// 同じ接頭辞が別の URI に束縛されていた場合に兄弟のプロパティを壊しうる
///
/// **多言語の Lang Alt は保たない。** 既存の `dc:title` に `en-US` / `fr-FR` の
/// `rdf:li` が並んでいても、差し替え後は `x-default` の 1 つだけになる。
/// この 4 プロパティは本ツールが所有する（UI が 1 言語しか編集できない）ため
/// 意図した挙動だが、他言語を持つ利用者にとっては損失である
fn lang_alt(name: &str, text: &str) -> String {
    // `text` は呼び出し側で `without_forbidden_controls` を通してある
    // （XML 1.0 が許さない文字はエスケープしても入れられないため）
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

    // EXIF の APP1 は残す。落とすのは XMP の APP1 だけ。
    // **Extended XMP（`http://ns.adobe.com/xmp/extension/` 前置き）のセグメントは
    // そのまま残る。** 本ツールは Extended XMP を読まない（`extract` の注記）ので、
    // 中身を判断できないものを消さない ── 安全側だが、他ツールが分割して書いた
    // 巨大なパケットが同居しうることは意識しておくこと
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

    /// 圧縮形式で、**単一引用符**に囲まれた `"` 入りの他名前空間属性が同居する形。
    /// XML として完全に合法で、`rdf:Description` の組み直しが必要になる
    const SINGLE_QUOTED_FORM: &str = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about=""
    xmlns:xmp="http://ns.adobe.com/xap/1.0/"
    xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/"
    xmp:Rating="2" crs:Note='say "hi" now'/>
 </rdf:RDF>
</x:xmpmeta>"#;

    /// 他名前空間の**構造化値の中**に本ツールと同名のプロパティが居る形。
    /// `xmpMM:DerivedFrom` の `dc:title` は「元画像の題」であって
    /// 「この画像の題」ではない（Premiere / After Effects が現実に書く）
    const NESTED_FOREIGN_STRUCTURE: &str = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about=""
    xmlns:dc="http://purl.org/dc/elements/1.1/"
    xmlns:xmpMM="http://ns.adobe.com/xap/1.0/mm/">
   <dc:title><rdf:Alt><rdf:li xml:lang="x-default">この画像の題</rdf:li></rdf:Alt></dc:title>
   <xmpMM:DerivedFrom rdf:parseType="Resource">
    <dc:title>元画像の題</dc:title>
    <xmpMM:instanceID>uuid:1234</xmpMM:instanceID>
   </xmpMM:DerivedFrom>
  </rdf:Description>
 </rdf:RDF>
</x:xmpmeta>"#;

    /// 他名前空間の構造化値の中に**入れ子の `rdf:RDF`** がある形。
    /// `xmpMM:Pantry` は派生元素材のメタデータを丸ごと抱えるので現実に起きる。
    /// その中の `dc:title` は**素材の題**であって、この画像の題ではない
    const NESTED_RDF_ROOT: &str = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about=""
    xmlns:dc="http://purl.org/dc/elements/1.1/"
    xmlns:xmpMM="http://ns.adobe.com/xap/1.0/mm/">
   <dc:title><rdf:Alt><rdf:li xml:lang="x-default">この画像の題</rdf:li></rdf:Alt></dc:title>
   <xmpMM:Pantry>
    <rdf:Bag>
     <rdf:li>
      <rdf:RDF>
       <rdf:Description rdf:about="">
        <dc:title><rdf:Alt><rdf:li xml:lang="x-default">素材の題</rdf:li></rdf:Alt></dc:title>
       </rdf:Description>
      </rdf:RDF>
     </rdf:li>
    </rdf:Bag>
   </xmpMM:Pantry>
  </rdf:Description>
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

    /// 検査器（オラクル）だけが使う定数。**実装の `OWNED` / `NS_*` をあえて参照しない。**
    ///
    /// 検査器が実装と同じ判定を共有すると、**規則そのものの誤りは before / after の
    /// 両方で同じように skip され、原理的に差分に出ない**（`frame_of` が入れ子の
    /// `rdf:RDF` を本体と誤認していたときが、まさにそれで検出できなかった）。
    /// ここでの重複は意図的である ── 「重複だから」と実装側へ寄せないこと
    const ORACLE_RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
    const ORACLE_OWNED: [(&str, &str); 4] = [
        ("http://ns.adobe.com/xap/1.0/", "Rating"),
        ("http://ns.microsoft.com/photo/1.0/", "Rating"),
        ("http://purl.org/dc/elements/1.1/", "title"),
        ("http://purl.org/dc/elements/1.1/", "description"),
    ];

    /// 解決済みの (名前空間 URI, ローカル名)
    type Name = (Option<String>, String);

    fn show(name: &Name) -> String {
        format!("{:?}:{}", name.0, name.1)
    }

    /// パスの末尾が「本ツールのプロパティが載る唯一の場所」＝
    /// トップレベルの `rdf:Description` か。
    ///
    /// **`frame_of` を使わず、親のパスをリテラルに綴る**（上の定数のコメントを参照）。
    /// 認めるのは `<rdf:RDF><rdf:Description>` と
    /// `<ラッパ><rdf:RDF><rdf:Description>` の 2 形だけで、
    /// `xmpMM:Pantry` が抱える入れ子の `rdf:RDF` の下は当てはまらない
    fn ends_at_top_description(path: &[Name]) -> bool {
        let rdf =
            |name: &Name, local: &str| name.0.as_deref() == Some(ORACLE_RDF) && name.1 == local;
        matches!(path.len(), 2 | 3)
            && rdf(&path[path.len() - 1], "Description")
            && rdf(&path[path.len() - 2], "RDF")
    }

    /// 名前空間つきで、本ツールの 4 プロパティ**以外**をすべて列挙する。
    /// 検査対象を、読み取り（`read`）とは別の経路で取り出すための道具。
    ///
    /// - 要素・属性は**解決済みの名前空間 URI 付き**で並べる。宣言が 1 つでも
    ///   失われたり別の URI に束縛し直されたりすれば、それを使っていた要素・属性の
    ///   URI が変わって差分に出る
    /// - **要素形式の値（テキスト）も親の名前付きで拾う。** これが無いと
    ///   `<crs:UUID>…</crs:UUID>` の中身の書き換えを検出できない
    /// - 本ツールの 4 プロパティは**部分木ごと**対象外にする。位置の判定は
    ///   `ends_at_top_description`（実装から独立）で行うので、入れ子の
    ///   `rdf:Description` や入れ子の `rdf:RDF` の下にある同名プロパティは
    ///   **他名前空間の値として検査対象に残る**
    fn foreign_content(packet: &str) -> Vec<String> {
        let mut reader = NsReader::from_str(packet);
        let mut out = Vec::new();
        // いま開いている要素のパス（テキストを親の名前とともに記録するのにも使う）
        let mut path: Vec<Name> = Vec::new();
        // 本ツールのプロパティの部分木を飛ばしている間、その開始深さを持つ
        let mut skip_at: Option<usize> = None;

        loop {
            let (ns, event) = reader.read_resolved_event().unwrap();
            let is_start = matches!(event, Event::Start(_));
            match event {
                Event::Eof => break,
                Event::Start(ref s) | Event::Empty(ref s) => {
                    path.push((
                        uri_of(&ns).map(|u| String::from_utf8_lossy(u).into_owned()),
                        String::from_utf8_lossy(s.local_name().as_ref()).into_owned(),
                    ));
                    let self_is_top = ends_at_top_description(&path);
                    let parent_is_top = ends_at_top_description(&path[..path.len() - 1]);
                    let name = &path[path.len() - 1];
                    let owned_here = parent_is_top
                        && ORACLE_OWNED.iter().any(|(uri, local)| {
                            name.0.as_deref() == Some(*uri) && name.1 == *local
                        });

                    if skip_at.is_none() && !owned_here {
                        out.push(format!("<{}>", show(name)));
                        for attribute in s.attributes() {
                            let attribute = attribute.unwrap();
                            let (a_ns, a_local) = reader.resolve_attribute(attribute.key);
                            let a_name: Name = (
                                uri_of(&a_ns).map(|u| String::from_utf8_lossy(u).into_owned()),
                                String::from_utf8_lossy(a_local.as_ref()).into_owned(),
                            );
                            // 圧縮形式（トップレベル `rdf:Description` の属性）に
                            // 書かれた本ツールのプロパティだけを対象外にする。
                            // 入れ子の `rdf:Description` の同名属性は他名前空間の値
                            let owned_attribute = self_is_top
                                && ORACLE_OWNED.iter().any(|(uri, local)| {
                                    a_name.0.as_deref() == Some(*uri) && a_name.1 == *local
                                });
                            if owned_attribute {
                                continue;
                            }
                            out.push(format!(
                                "@{}={}",
                                show(&a_name),
                                attribute.unescape_value().unwrap()
                            ));
                        }
                    }
                    if is_start {
                        if skip_at.is_none() && owned_here {
                            skip_at = Some(path.len() - 1);
                        }
                    } else {
                        path.pop();
                    }
                }
                Event::End(_) => {
                    path.pop();
                    if skip_at == Some(path.len()) {
                        skip_at = None;
                    }
                }
                Event::Text(ref text) => {
                    if skip_at.is_some() {
                        continue;
                    }
                    let text = text.unescape().unwrap().trim().to_string();
                    if text.is_empty() {
                        continue;
                    }
                    let parent = path.last().map_or(String::new(), show);
                    out.push(format!("#{parent}={text}"));
                }
                _ => {}
            }
        }
        out.sort();
        out
    }

    /// `before` にあって `after` から失われた項目（重複も数える）。
    ///
    /// 等値比較にしないのは、`dc:title` を**新しく足す**ケースでは足場
    /// （`rdf:Alt` / `rdf:li`）が増えて等値が成立しないため。検査したいのは
    /// 「**他名前空間のデータが減っていないこと**」である
    fn missing(before: &[String], after: &[String]) -> Vec<String> {
        let mut remaining: std::collections::HashMap<&str, usize> =
            std::collections::HashMap::new();
        for item in after {
            *remaining.entry(item.as_str()).or_default() += 1;
        }
        before
            .iter()
            .filter(|item| match remaining.get_mut(item.as_str()) {
                Some(count) if *count > 0 => {
                    *count -= 1;
                    false
                }
                _ => true,
            })
            .cloned()
            .collect()
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

        let lost = missing(&before, &foreign_content(&patched));
        assert!(lost.is_empty(), "他名前空間のデータが失われた: {:?}", lost);
        // ELEMENT_FORM は 4 プロパティを最初から持っているので、増える側も無い
        let added = missing(&foreign_content(&patched), &before);
        assert!(
            added.is_empty(),
            "他名前空間に余計なものが増えた: {:?}",
            added
        );
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
        // 前提条件: そもそも属性形式の古い値が入っていること。
        // これが無いと「残っていない」は自明に通る
        assert!(ATTRIBUTE_FORM.contains("xmp:Rating=\""));

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
        // 前提条件: 消える対象と、残るべき他名前空間が両方入っていること
        assert_eq!(read(ELEMENT_FORM).unwrap().rating, Some(4));
        assert!(ELEMENT_FORM.contains("camera-raw-settings"));

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
    /// **属性を組み直すときに値を再エスケープすること。**
    /// `Attribute::value` は引用符の内側の生バイトで、元が単一引用符なら `"` を
    /// 含みうる。生のまま `"` で囲み直すと非整形式になり、**そのパケットが
    /// 全リーダーから読めなくなる**（他名前空間のデータが全損する）
    #[test]
    fn rebuilding_a_description_keeps_single_quoted_values_well_formed() {
        // 前提条件: 「落とす属性がある」＋「単一引用符で " を含む他名前空間属性がある」
        // が揃っていること。片方でも欠けると組み直し自体が起きず、検査は空振りする
        assert!(SINGLE_QUOTED_FORM.contains("xmp:Rating=\""));
        assert!(SINGLE_QUOTED_FORM.contains(r#"crs:Note='say "hi" now'"#));

        let before = foreign_content(SINGLE_QUOTED_FORM);
        let patched = patch(
            SINGLE_QUOTED_FORM,
            &EditableMetadata {
                rating: Some(5),
                ..Default::default()
            },
        )
        .unwrap();

        // 読み戻せる＝整形式である
        assert_eq!(
            read(&patched).unwrap().rating,
            Some(5),
            "組み直しで壊れた: {}",
            patched
        );
        // 値そのものは変わっていない（表現が `&quot;` になるのは可）
        let after = foreign_content(&patched);
        let lost = missing(&before, &after);
        assert!(lost.is_empty(), "他名前空間の値が失われた: {:?}", lost);
        let added = missing(&after, &before);
        assert!(
            added.is_empty(),
            "他名前空間に余計なものが増えた: {:?}",
            added
        );
    }

    /// **書き込み先が見つからないパケットは作り直さず `Err`。**
    /// 「`rdf:Description` が無い＝失うものが無い」は偽で、rdf の URI が
    /// 1 文字違うだけでもここに来る一方、中の他名前空間のデータは健在でありうる
    #[test]
    fn a_packet_without_a_top_level_description_is_an_error_not_a_rewrite() {
        // rdf の名前空間 URI から末尾の # が欠けているだけ
        let packet = r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns">
  <rdf:Description rdf:about="" xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/"
   crs:Exposure2012="+0.35"/>
 </rdf:RDF>"#;
        // 前提条件: 消えては困るデータが実際に入っていること
        assert!(packet.contains("crs:Exposure2012"));

        assert!(patch(
            packet,
            &EditableMetadata {
                rating: Some(3),
                ..Default::default()
            }
        )
        .is_err());
    }

    /// 中身が空のときだけは作り直してよい（失うものが無い）
    #[test]
    fn an_empty_packet_falls_back_to_a_new_one() {
        let packet = patch(
            "   \n ",
            &EditableMetadata {
                rating: Some(3),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(read(&packet).unwrap().rating, Some(3));
    }

    /// **他名前空間の構造化値の中にある同名プロパティには触らない。**
    /// 名前だけで深さ無制限に消すと、`xmpMM:DerivedFrom` の中の `dc:title`
    /// （＝元画像の題）まで巻き添えになる
    #[test]
    fn owned_names_nested_in_another_namespaces_value_are_left_alone() {
        let before = foreign_content(NESTED_FOREIGN_STRUCTURE);
        // 前提条件: 入れ子側の値が検査対象に入っていること。
        // ここが漏れていると「壊れていない」は自明に通る
        assert!(
            before.iter().any(|item| item.contains("元画像の題")),
            "入れ子の他名前空間の値が検査対象に入っていない: {:?}",
            before
        );

        let patched = patch(
            NESTED_FOREIGN_STRUCTURE,
            &EditableMetadata {
                title: Some("新しい題".into()),
                ..Default::default()
            },
        )
        .unwrap();

        let after = foreign_content(&patched);
        let lost = missing(&before, &after);
        assert!(lost.is_empty(), "他名前空間のデータが失われた: {:?}", lost);
        let added = missing(&after, &before);
        assert!(
            added.is_empty(),
            "他名前空間に余計なものが増えた: {:?}",
            added
        );
        assert!(
            patched.contains("元画像の題"),
            "入れ子の値が消えた: {}",
            patched
        );
        // トップレベルの dc:title は差し替わっている
        assert!(!patched.contains("この画像の題"));
        assert_eq!(read(&patched).unwrap().title.as_deref(), Some("新しい題"));
    }

    /// **`Some(0)` は未評価。** プロパティを 1 つも書かない
    /// （0 を書くと未評価と区別が付かず、パーセント系にも表現できない）
    #[test]
    fn a_zero_rating_writes_no_rating_property() {
        // 前提条件: 元のパケットには★が入っている（消えたことを見るため）
        assert_eq!(read(ELEMENT_FORM).unwrap().rating, Some(4));

        let patched = patch(
            ELEMENT_FORM,
            &EditableMetadata {
                rating: Some(0),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!patched.contains("Rating"), "★0 で書かれた: {}", patched);
        assert_eq!(read(&patched).unwrap().rating, None);
    }

    /// **XML 1.0 が許さない制御文字はパケットに入れない。**
    /// エスケープしても入れられず、残すと Adobe/expat 系のリーダーが
    /// パケットごと捨てる。自前の `read()` は読み戻せてしまうので、
    /// 「保存 → 読み戻して一致検証」ではこの破損を捕まえられない
    #[test]
    fn forbidden_control_characters_never_reach_the_packet() {
        let packet = new_packet(&EditableMetadata {
            title: Some("港\u{0b}の\u{0}朝".into()),
            comment: Some("順光\u{7}".into()),
            rating: None,
        });
        assert!(
            !packet
                .chars()
                .any(|c| c < '\u{20}' && !matches!(c, '\t' | '\n' | '\r')),
            "制御文字が残った: {:?}",
            packet
        );
        let side = read(&packet).unwrap();
        assert_eq!(side.title.as_deref(), Some("港の朝"));
        assert_eq!(side.description.as_deref(), Some("順光"));
    }
    /// **入れ子の `rdf:RDF` は「本体」ではない。**
    /// `xmpMM:Pantry` が抱える派生元素材の `dc:title` を、この画像の題として
    /// 読んだり消したりしないこと
    #[test]
    fn an_rdf_root_nested_in_another_namespaces_value_is_not_the_top_level_one() {
        let before = foreign_content(NESTED_RDF_ROOT);
        // 前提条件: 入れ子側の題が検査対象に入っていること。
        // ここが漏れていると「壊れていない」は自明に通る
        assert!(
            before.iter().any(|item| item.contains("素材の題")),
            "入れ子の rdf:RDF の中の値が検査対象に入っていない: {:?}",
            before
        );
        // 前提条件: 読み取りもトップレベル側を返している
        assert_eq!(
            read(NESTED_RDF_ROOT).unwrap().title.as_deref(),
            Some("この画像の題")
        );

        let patched = patch(
            NESTED_RDF_ROOT,
            &EditableMetadata {
                title: Some("新しい題".into()),
                ..Default::default()
            },
        )
        .unwrap();

        let after = foreign_content(&patched);
        let lost = missing(&before, &after);
        assert!(lost.is_empty(), "他名前空間のデータが失われた: {:?}", lost);
        let added = missing(&after, &before);
        assert!(
            added.is_empty(),
            "他名前空間に余計なものが増えた: {:?}",
            added
        );
        assert!(
            patched.contains("素材の題"),
            "素材の題が消えた: {}",
            patched
        );
        assert!(!patched.contains("この画像の題"));
        assert_eq!(read(&patched).unwrap().title.as_deref(), Some("新しい題"));
    }

    /// **U+FFFE / U+FFFF も XML 1.0 の文字ではない。**
    /// C0 制御文字と同じ失敗モード（全リーダーから読めないパケットが、
    /// 自前の読み戻し検証だけをすり抜ける）
    #[test]
    fn noncharacters_never_reach_the_packet() {
        let packet = new_packet(&EditableMetadata {
            title: Some("a\u{ffff}b\u{fffe}c".into()),
            comment: None,
            rating: None,
        });
        assert!(
            !packet.contains(['\u{fffe}', '\u{ffff}']),
            "非文字が残った: {:?}",
            packet
        );
        assert_eq!(read(&packet).unwrap().title.as_deref(), Some("abc"));
    }

    /// 制御文字だけのタイトルは**空欄と同じ**。プロパティごと消える
    /// （EXIF 側と順序を揃えていないと、空の `dc:title` が書かれる）
    #[test]
    fn a_title_made_only_of_forbidden_characters_removes_the_property() {
        let patched = patch(
            ELEMENT_FORM,
            &EditableMetadata {
                title: Some("\u{0b}".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(
            !patched.contains("dc:title"),
            "空の題が書かれた: {}",
            patched
        );
        assert_eq!(read(&patched).unwrap().title, None);
    }
}
