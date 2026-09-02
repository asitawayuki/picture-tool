//! 人が編集するメタデータ（タイトル・コメント・レーティング）の読み書き。
//!
//! 既存の [`crate::read_exif_info`] が「**カメラが書いた撮影情報**を読む」のに対し、
//! こちらは「**人が書くもの**を読み書きする」。目的が違うのでモジュールを分けてある。
//!
//! 書くときは Windows 系（`XPTitle` / `XPComment` / `RatingPercent` /
//! `MicrosoftPhoto:Rating`）と MWG 系（`dc:title` / `dc:description` / `xmp:Rating`）の
//! **両方**へ同じ値を書く。両者は根拠タグが完全に分離しており、片方だけでは
//! もう一方のアプリから「未設定」に見えるため。詳細は
//! `docs/superpowers/specs/2026-08-18-metadata-editing-design.md`。

mod exif;
mod xmp;

/// 読めた文字列を正規形に寄せる。**前後の空白（と NUL）は落とし、trim 後が空なら `None`。**
///
/// EXIF と XMP の両方から使う共通の trim ルール。**この計画の C1 はまさにこの非対称から
/// 生まれた実在のバグだった**: XMP 側だけ trim して EXIF 側は trim しない、という状態だと
/// 末尾に改行の付いたコメントは保存が必ず失敗し、Task 8 の食い違い警告も誤発火する。
/// EXIF・XMP のどちらが読んでも同じ結果になるよう、trim のロジックをここ1箇所に集約し、
/// 各モジュールが個別に持たない（3箇所目ができる前に統合しておく）。
/// 書き込み側も `EditableMetadata::normalized` で trim するので、
/// 保存 → 読み戻しの検証（Task 9）が一致する。
pub(crate) fn cleaned(text: &str) -> Option<String> {
    let trimmed = text.trim_matches(|c: char| c == '\0' || c.is_whitespace());
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// **XML 1.0 が文字として許さないものを落とす。**
///
/// 対象は「TAB/LF/CR 以外の C0 制御文字」と **U+FFFE / U+FFFF**
/// （`Char` 生成規則が両方とも除外している）。**C1 制御文字（U+0080–U+009F）と
/// DEL（U+007F）は合法なので落とさない。** 不正サロゲートは Rust の `char` に
/// 入らないので対象外。
///
/// XMP パケットは XML なので、エスケープしてもこれらの文字は入れられない
/// （`&#11;` と書いても非整形式）。webview 由来の JSON は `\u{0b}` のような文字を
/// 普通に運べるため、素通しすると**パケット全体が Adobe/expat 系のリーダーから
/// 読めなくなる**。しかも本ツールの `read()` はそれを読み戻せてしまうので、
/// 「保存 → 読み戻して一致検証」（Task 9）をすり抜ける。
///
/// **EXIF 側と XMP 側の両方から呼ぶ。** 片側だけ落とすと同じ入力から
/// 違う文字列が保存され、読み戻しの一致検証が食い違う（`cleaned` を 1 箇所に
/// 統合したのとまったく同じ理由）。`EditableMetadata::normalized`（Task 9）も
/// このヘルパを使うこと。
pub(crate) fn without_xml_forbidden_chars(text: &str) -> std::borrow::Cow<'_, str> {
    fn forbidden(c: char) -> bool {
        (c < '\u{20}' && !matches!(c, '\t' | '\n' | '\r')) || matches!(c, '\u{fffe}' | '\u{ffff}')
    }
    if text.contains(forbidden) {
        std::borrow::Cow::Owned(text.chars().filter(|c| !forbidden(*c)).collect())
    } else {
        std::borrow::Cow::Borrowed(text)
    }
}

use serde::{Deserialize, Serialize};

/// 人が編集する対象のメタデータ。
///
/// `None` は「タグが無い」、`Some("")` は「空文字のタグがある」を意味し、**区別する**。
/// UI で欄を空にして保存した場合は `None`（＝タグを削除）として扱う。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditableMetadata {
    pub title: Option<String>,
    pub comment: Option<String>,
    /// `None` = 未評価。`Some(1..=5)`。
    /// **書き込み時に `Some(0)` は生成しない** ── `RatingPercent` では 0 が未評価を
    /// 意味するため、★0 という状態はパーセント系に表現できない。
    /// 他アプリが書いた `xmp:Rating = 0` は読み取り時のみ受け入れる
    pub rating: Option<u8>,
}

/// 読み取り結果。`warnings` には箇所ごとの値の食い違いが載る。
///
/// core は「どちらが新しいか」を判断できない（ファイル内にその情報が無い）。
/// 優先順位で 1 つ採ったうえで、**捨てた値があった事実だけを上へ渡す**。
#[derive(Debug, Clone, Default, Serialize)]
pub struct MetadataRead {
    pub metadata: EditableMetadata,
    pub warnings: Vec<String>,
}

/// ★ → パーセント（Microsoft 公式の変換表）
pub(crate) fn stars_to_percent(stars: u8) -> u16 {
    match stars {
        1 => 1,
        2 => 25,
        3 => 50,
        4 => 75,
        5 => 99,
        _ => 0,
    }
}

/// パーセント → ★。`None` は未評価
pub(crate) fn percent_to_stars(percent: u16) -> Option<u8> {
    match percent {
        0 => None,
        1..=12 => Some(1),
        13..=37 => Some(2),
        38..=62 => Some(3),
        63..=87 => Some(4),
        _ => Some(5),
    }
}

use anyhow::{Context, Result};
use img_parts::jpeg::Jpeg;
use img_parts::Bytes;
use little_exif::filetype::FileExtension;
use little_exif::metadata::Metadata;
use std::path::Path;

/// 1 箇所ぶんの候補。`&'static str` は利用者に見せる場所の名前
type Candidate = (&'static str, Option<String>);

/// 優先順位の高い順に並んだ候補から 1 つ採る。
/// **採らなかった側に別の値があれば警告に積む**（core は判断せず事実だけを上へ渡す）
fn pick(label: &str, candidates: Vec<Candidate>, warnings: &mut Vec<String>) -> Option<String> {
    let present: Vec<Candidate> = candidates
        .into_iter()
        .filter(|(_, v)| v.is_some())
        .collect();
    let (chosen_where, chosen) = present.first()?.clone();

    let others: Vec<String> = present
        .iter()
        .skip(1)
        .filter(|(_, value)| *value != chosen)
        .map(|(place, value)| format!("{} = 「{}」", place, value.as_deref().unwrap_or("")))
        .collect();

    if !others.is_empty() {
        warnings.push(format!(
            "{}に、他のアプリが書いた別の値があります（{} を採用。ほかに {}）。保存すると揃います",
            label,
            chosen_where,
            others.join(" / ")
        ));
    }
    chosen
}

/// 画像から、人が編集する 3 項目を読む。
///
/// 箇所ごとに値が食い違っていた場合は優先順位で 1 つ採り、
/// 捨てた値の存在を `warnings` に積む。
pub fn read_metadata(path: &Path) -> Result<MetadataRead> {
    let raw =
        std::fs::read(path).with_context(|| format!("ファイルを読めません: {}", path.display()))?;

    let mut warnings = Vec::new();

    // Exif が無い画像でも読み取り自体は成立させる（全項目 None になるだけ）。
    // ただし**解析に失敗したことは黙らない** ── そのまま保存すると Exif を
    // 作り直すことになるので、利用者に伝わっている必要がある
    // （書き込み側の `build` は同じ状況で Err にする）
    let exif_meta = match Metadata::new_from_vec(&raw, FileExtension::JPEG) {
        Ok(meta) => meta,
        Err(error) => {
            warnings.push(format!("Exif を解析できませんでした: {}", error));
            Metadata::new()
        }
    };
    let exif_side = exif::read(&exif_meta);

    let jpeg = Jpeg::from_bytes(Bytes::from(raw))
        .with_context(|| format!("JPEG として読めません: {}", path.display()))?;
    let xmp_side = match xmp::extract(&jpeg) {
        Some(packet) => xmp::read(&packet)?,
        None => xmp::XmpSide::default(),
    };

    let title = pick(
        "タイトル",
        vec![
            ("XMP dc:title", xmp_side.title.clone()),
            ("Exif XPTitle", exif_side.title_xp.clone()),
            ("Exif ImageDescription", exif_side.title_description.clone()),
        ],
        &mut warnings,
    );

    let comment = pick(
        "コメント",
        vec![
            ("XMP dc:description", xmp_side.description.clone()),
            ("Exif UserComment", exif_side.comment_user.clone()),
            ("Exif XPComment", exif_side.comment_xp.clone()),
        ],
        &mut warnings,
    );

    // ★は型が違うので、いったん文字列に寄せてから同じ仕組みに通す
    let rating = pick(
        "レーティング",
        vec![
            ("XMP xmp:Rating", xmp_side.rating.map(|s| s.to_string())),
            (
                "Exif RatingPercent",
                exif_side
                    .rating_percent
                    .and_then(percent_to_stars)
                    .map(|s| s.to_string()),
            ),
            (
                "XMP MicrosoftPhoto:Rating",
                xmp_side
                    .ms_rating_percent
                    .and_then(percent_to_stars)
                    .map(|s| s.to_string()),
            ),
        ],
        &mut warnings,
    )
    .and_then(|s| s.parse::<u8>().ok());

    Ok(MetadataRead {
        metadata: EditableMetadata {
            title,
            comment,
            rating,
        },
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name)
    }

    /// テンポラリに複製して返す。元のフィクスチャは書き換えない
    fn scratch(name: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(name);
        std::fs::copy(fixture(name), &path).unwrap();
        (dir, path)
    }

    #[test]
    fn reading_a_file_without_any_editable_metadata_yields_none() {
        let (_dir, path) = scratch("exif-only.jpg");

        // 前提条件: exif-only.jpg は実際に Exif を持ち、`Metadata::new_from_vec` は
        // 実データに対して解析を試みている。warnings が空なのは「解析対象が無いから」
        // ではないことを、独立実装（kamadak-exif ベースの `read_exif_info`）で裏付ける
        let exif_info = crate::read_exif_info(&path).unwrap();
        assert!(
            exif_info.camera_make.is_some() && exif_info.camera_model.is_some(),
            "前提条件が崩れている: exif-only.jpg から撮影情報が読めなかった"
        );

        let read = read_metadata(&path).unwrap();
        assert_eq!(read.metadata, EditableMetadata::default());
        assert!(read.warnings.is_empty());
    }

    /// Microsoft 公式の変換表（spec §レーティングの値変換）。
    /// 1 が 1% なのは 20% ではない ── ここを等間隔にすると Windows 側の★が 1 つずれる
    #[test]
    fn stars_map_to_the_microsoft_percent_table() {
        assert_eq!(stars_to_percent(1), 1);
        assert_eq!(stars_to_percent(2), 25);
        assert_eq!(stars_to_percent(3), 50);
        assert_eq!(stars_to_percent(4), 75);
        assert_eq!(stars_to_percent(5), 99);
    }

    #[test]
    fn percent_maps_back_through_the_official_ranges() {
        assert_eq!(percent_to_stars(0), None, "0 は未評価であって★0 ではない");
        for (percent, stars) in [
            (1, 1),
            (12, 1),
            (13, 2),
            (37, 2),
            (38, 3),
            (62, 3),
            (63, 4),
            (87, 4),
            (88, 5),
            (99, 5),
        ] {
            assert_eq!(percent_to_stars(percent), Some(stars), "{}%", percent);
        }
    }

    /// 他アプリが範囲外を書いていても落ちない
    #[test]
    fn out_of_range_percent_clamps_to_five_stars() {
        assert_eq!(percent_to_stars(100), Some(5));
        assert_eq!(percent_to_stars(65535), Some(5));
    }

    /// XML 1.0 が許さない制御文字は落ち、TAB/LF/CR と通常の文字は残る
    #[test]
    fn forbidden_control_characters_are_removed_but_tab_lf_cr_survive() {
        assert_eq!(
            without_xml_forbidden_chars("a\u{0b}b\u{0c}c\u{0}d\u{7}e"),
            "abcde"
        );
        // XML 1.0 の Char 生成規則は U+FFFE / U+FFFF も除外する
        assert_eq!(without_xml_forbidden_chars("a\u{fffe}b\u{ffff}c"), "abc");
        assert_eq!(without_xml_forbidden_chars("a\tb\nc\rd"), "a\tb\nc\rd");
        // C1 制御文字と DEL は XML 1.0 で合法。落とさない
        assert_eq!(
            without_xml_forbidden_chars("a\u{7f}b\u{85}c"),
            "a\u{7f}b\u{85}c"
        );
        // 落とすものが無ければ借用のまま（余計な確保をしない）
        assert!(matches!(
            without_xml_forbidden_chars("港"),
            std::borrow::Cow::Borrowed(_)
        ));
    }

    /// 往復して同じ★に戻ること。表を片方だけ直した事故を捕まえる
    #[test]
    fn stars_survive_a_round_trip_through_percent() {
        for stars in 1..=5u8 {
            assert_eq!(percent_to_stars(stars_to_percent(stars)), Some(stars));
        }
    }
}
