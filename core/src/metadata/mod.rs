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

#[cfg(test)]
mod tests {
    use super::*;

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

    /// 往復して同じ★に戻ること。表を片方だけ直した事故を捕まえる
    #[test]
    fn stars_survive_a_round_trip_through_percent() {
        for stars in 1..=5u8 {
            assert_eq!(percent_to_stars(stars_to_percent(stars)), Some(stars));
        }
    }
}
