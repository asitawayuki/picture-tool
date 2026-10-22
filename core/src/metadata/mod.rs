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

mod atomic;
mod exif;
mod header;
mod preserve;
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
use img_parts::Bytes;
use img_parts::jpeg::Jpeg;
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
    read_metadata_bytes(raw, path)
}

/// 読み込み済みのバイト列から同じ 3 項目を読む。`path` はエラーメッセージのためだけに使う。
///
/// **`read_metadata`（全体読み）と `read_metadata_header`（先頭だけ読む）の共通の下地。**
/// 解析の実装がここ 1 箇所にあるので、「読む量」を変えても Exif と XMP の
/// 優先順位・警告・trim 規則は変わらない。
///
/// バイト列が JPEG として妥当であることは呼び出し側の責任なので、
/// 外向けの入口にはしない（`read_metadata` か `read_metadata_fast` を使う）。
#[doc(hidden)]
pub fn read_metadata_bytes(raw: Vec<u8>, path: &Path) -> Result<MetadataRead> {
    let mut warnings = Vec::new();

    // 解析は先に試すが、**警告にするかどうかは JPEG の構造を見てから決める**
    // （`raw` を `Jpeg` へ渡すと所有権が移るので、順序はこの形になる）。
    let exif_parse = Metadata::new_from_vec(&raw, FileExtension::JPEG);

    let jpeg = Jpeg::from_bytes(Bytes::from(raw))
        .with_context(|| format!("JPEG として読めません: {}", path.display()))?;

    // Exif が無い画像でも読み取り自体は成立させる（全項目 None になるだけ）。
    // ただし**解析に失敗したことは黙らない** ── そのまま保存すると Exif を
    // 作り直すことになるので、利用者に伝わっている必要がある
    // （書き込み側の `build` は同じ状況で Err にする）。
    //
    // **「Exif が無い」と「Exif が壊れている」を混ぜない。** `little_exif` は
    // APP1 に出会わないまま EOI へ到達しても `Err("No EXIF data found!")` を返すので、
    // 素直に警告へ積むと **Exif APP1 を持たない JPEG すべて**に生の英語エラーが出る。
    // 本ツールが変換して出力した JPEG は Exif を持たないため、`_processed` フォルダーを
    // 情報モードで開くと全写真がその状態になる。判定は `build` 側と同じ
    // `has_exif_segment` に揃える。
    let exif_meta = match exif_parse {
        Ok(meta) => meta,
        Err(error) => {
            if has_exif_segment(&jpeg) {
                warnings.push(format!("Exif を解析できませんでした: {}", error));
            }
            Metadata::new()
        }
    };
    let exif_side = exif::read(&exif_meta);

    // XMP も Exif と同じく、読めなくても読み取り自体は成立させて警告に積む
    // （書き込み側の `build` は同じ状況で Err にする）
    let xmp_side =
        match xmp::extract(&jpeg).and_then(|packet| packet.map(|p| xmp::read(&p)).transpose()) {
            Ok(side) => side.unwrap_or_default(),
            Err(error) => {
                warnings.push(format!("XMP を解析できませんでした: {error:#}"));
                xmp::XmpSide::default()
            }
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

/// 拡張子だけで JPEG かどうかを見る。**中身は見ない** ──
/// ここは「読みに行くかどうか」の分岐であって、正しさの判定ではない
/// （中身の検査は `header_bytes` の SOI 検査と `read_metadata` がやる）。
/// 条件は `gui/src/security.rs` の `readable_jpeg` と同じ。
fn has_jpeg_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("jpg") || ext.eq_ignore_ascii_case("jpeg"))
}

/// JPEG の先頭（SOI〜SOS）だけから、人が編集する 3 項目を読む。
///
/// **解析の実装は一切変えない。** `read_metadata` と同じ [`read_metadata_bytes`] に、
/// APP1 だけを写したバイト列を渡す。Exif と XMP の優先順位・警告・trim 規則は
/// `docs/superpowers/specs/2026-08-18-metadata-editing-design.md` のまま。
///
/// 手書きのセグメント走査なので、歪んだ JPEG では `Err` になる。
/// **直接呼ばず [`read_metadata_fast`] を使うこと**（フォールバックがそこにある）。
pub fn read_metadata_header(path: &Path) -> Result<MetadataRead> {
    let raw = header::header_bytes(path)
        .with_context(|| format!("JPEG の先頭を読めません: {}", path.display()))?;
    read_metadata_bytes(raw, path)
}

/// **読み取りの唯一の入口。★を読むところは全部これを使う。**
///
/// 呼び出し元は 3 つ ── `list_images`（グリッドの★）、`read_image_metadata`
/// （情報パネル）、`write_image_metadata` の読み戻し。**1 つでも外すと、
/// 「保存した直後だけ正しい★／フォルダーを開き直すと違う★」という、
/// 再現条件が「保存を挟んだかどうか」になる差が出る。**
///
/// | 段 | 動き |
/// |---|---|
/// | 1 | 拡張子が JPEG でなければ `Ok`（全項目 `None`） |
/// | 2 | [`read_metadata_header`] で先頭だけ読む。`Ok` ならそれを返す |
/// | 3 | `Err` なら [`read_metadata`]（全体読み）へフォールバック |
///
/// **段 3 が要る理由**: 手書きの走査は「境界が 0xFF で始まっていない」等で `Err` を
/// 返すが、`read_metadata` は `img_parts` / `little_exif` にバイト列ごと渡すので
/// 多少歪んでいても APP1 に辿り着けば読める。フォールバックが無いと、
/// **いままで情報パネルで開けていた写真がエラートーストになる**（編集して保存する
/// 導線ごと塞がる）。落ちるのは歪んだファイルだけなので、性能は現行以上に悪くならない。
///
/// **段 1 は意図的に `read_metadata` と非対称**（`read_metadata` は非 JPEG で `Err`）。
/// ★の読み取りは一覧表示の副作用なので、PNG が混ざっているだけで一覧が失敗したり
/// 警告が出たりしてはいけない。段 1 に到達するのは `list_images` からの呼び出しだけで、
/// 情報パネル側は `security::readable_jpeg` が先に非 JPEG を弾く。
pub fn read_metadata_fast(path: &Path) -> Result<MetadataRead> {
    if !has_jpeg_extension(path) {
        return Ok(MetadataRead::default());
    }
    match read_metadata_header(path) {
        Ok(read) => Ok(read),
        Err(_) => read_metadata(path),
    }
}

impl EditableMetadata {
    /// 保存の前に、**書き込み側が実際に落とすものを先に落とす。**
    /// 前後の空白と NUL、XML 1.0 が許さない文字、空文字、★0。
    ///
    /// ここを素の `trim()` にしてはならない。読み取り側（`exif::read` / `xmp::read`）は
    /// [`cleaned`] を通り、書き込み側（`exif::apply` / `xmp` の `properties_xml`）は
    /// [`without_xml_forbidden_chars`] を通る。両方を同じ順序で通しておかないと、
    /// `atomic::verify` に渡る期待値が「実際に保存されて読み戻される値」と食い違い、
    /// **制御文字や NUL を含む入力の保存が必ず失敗する。**
    ///
    /// 順序も逆にできない。`cleaned` を先に通すと `"\u{01}"` のような入力が
    /// `Some("")` として残り、「空欄ならタグごと消す」規則が XMP 側で破れる
    /// （`exif::apply` は「フィルタ → 空判定」の順）。
    ///
    /// trim が要るのは、XMP の要素テキストが整形の改行・インデントを含むため。
    /// コメント欄は複数行入力なので、末尾改行は例外ではなく常用入力である。
    ///
    /// 空文字と★0 を `None` に寄せるのは、どちらも書き込みがタグ削除として扱い、
    /// 読み戻すと `None` が返るため。
    pub fn normalized(&self) -> EditableMetadata {
        fn tidy(text: &Option<String>) -> Option<String> {
            cleaned(&without_xml_forbidden_chars(text.as_deref()?))
        }
        EditableMetadata {
            title: tidy(&self.title),
            comment: tidy(&self.comment),
            rating: self.rating.filter(|stars| (1..=5).contains(stars)),
        }
    }
}

/// 画像のメタデータセグメントだけを差し替える。**画素は再エンコードしない。**
///
/// `little_exif` は `unwrap()` が多数残っており、壊れた Exif で panic しうる既知の
/// issue（#77）がある。core が panic するとアプリごと落ちるため `catch_unwind` で包む。
///
/// **既定の panic hook が stderr にメッセージを出す点だけは「core は `eprintln!` しない」
/// から外れる。** hook を差し替えるとプロセス全体（rayon のワーカーを含む）に効いてしまい、
/// 本物のバグの手掛かりを消す。panic は routine な事象ではないので、そちらを避けた。
///
/// **どの失敗経路でも元ファイルは 1 バイトも変わらない**（置き換えは検証を通ったあとの
/// `rename` だけで起き、それが最後の操作である）。その事実を利用者に伝えるのは
/// ここの責務 ── 下位の `bail!`（`xmp::patch` の「書き込み先が無い」等）は
/// 原因しか語らないので、文脈をここで 1 度だけ足す。
///
/// **`Ok` の中身は保存を止めない警告**（MakerNote の位置の変化。`preserve` 参照）。
/// 出すか捨てるかは呼び出し側（境界）が決める。
pub fn write_metadata(path: &Path, values: &EditableMetadata) -> Result<Vec<String>> {
    let values = values.normalized();
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| write_inner(path, &values)))
        .unwrap_or_else(|_| Err(anyhow::anyhow!("書き込み処理が異常終了しました")))
        .with_context(|| {
            format!(
                "メタデータを保存できませんでした（元ファイルは変更していません）: {}",
                path.display()
            )
        })
}

fn write_inner(path: &Path, values: &EditableMetadata) -> Result<Vec<String>> {
    let original =
        std::fs::read(path).with_context(|| format!("ファイルを読めません: {}", path.display()))?;
    let patched = build(&original, values)?;
    atomic::replace_verified(path, &original, patched, values)
}

/// メタデータを差し替えたバイト列を作る。**ファイルには触らない。**
///
/// XMP パケットは **`little_exif` を通す前の原本から**取り出す。順序を逆にすると、
/// EXIF の書き込みが XMP セグメントに何をするかに依存してしまう。
fn build(original: &[u8], values: &EditableMetadata) -> Result<Vec<u8>> {
    // 写真 1 枚が数十 MB になりうるので、原本の複製は 2 つに抑える
    // （`buffer` は Exif の書き込み先を兼ねる）
    let mut buffer = original.to_vec();
    let jpeg = Jpeg::from_bytes(Bytes::from(original.to_vec())).context("JPEG として読めません")?;
    let packet = match xmp::extract(&jpeg)
        .context("既存の XMP を読めないため書き込みを中止しました")?
    {
        Some(existing) => xmp::patch(&existing, values)?,
        None => xmp::new_packet(values),
    };

    // **既存 Exif の解析に失敗したら、そこで止める。**
    // 空の Metadata から書き直すと little_exif は既存の Exif APP1 を丸ごと落とし、
    // MakerNote も GPS もサムネイルも消えた「本ツールの 3 タグだけの Exif」になる。
    // `atomic::verify` の `preserve::check` もこの結果を拒否する（書き込み前の全フィールドが
    // 失われるか、書き直した Exif を kamadak-exif が読めない）が、ここで止めた方が
    // 理由（「解析できない」）が正確に伝わる。**二重にあるのは意図的**:
    // ここは little_exif が読めない場合、`preserve` は kamadak-exif で比べた結果を受け持つ。
    let mut meta = match Metadata::new_from_vec(&buffer, FileExtension::JPEG) {
        Ok(meta) => meta,
        Err(error) if has_exif_segment(&jpeg) => {
            return Err(anyhow::anyhow!(
                "既存の Exif を解析できないため書き込みを中止しました: {}",
                error
            ));
        }
        // Exif をそもそも持たない画像。新しく作ってよい
        Err(_) => Metadata::new(),
    };
    exif::apply(&mut meta, values);
    meta.write_to_vec(&mut buffer, FileExtension::JPEG)
        .context("Exif を書き込めません")?;

    let mut written =
        Jpeg::from_bytes(Bytes::from(buffer)).context("書き込み後の JPEG を読めません")?;
    xmp::install(&mut written, &packet)?;
    Ok(written.encoder().bytes().to_vec())
}

/// Exif の APP1 を持っているか。「Exif が無い」と「Exif を解析できない」を分ける
fn has_exif_segment(jpeg: &Jpeg) -> bool {
    jpeg.segments_by_marker(img_parts::jpeg::markers::APP1)
        .any(|segment| segment.contents().starts_with(b"Exif\0\0"))
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

    // --- Task 9: 書き込み ---------------------------------------------------

    /// EXIF 側・XMP 側の生の読み取り結果。**`read_metadata` は優先順位で 1 つに畳むので、
    /// 「両方に書けているか」の前提条件はこちらで確かめる**
    fn sides(path: &std::path::Path) -> (exif::ExifSide, xmp::XmpSide) {
        let raw = std::fs::read(path).unwrap();
        let meta = Metadata::new_from_vec(&raw, FileExtension::JPEG).unwrap();
        let jpeg = Jpeg::from_bytes(Bytes::from(raw)).unwrap();
        let packet = xmp::extract(&jpeg)
            .unwrap()
            .expect("XMP パケットが書かれていない");
        (exif::read(&meta), xmp::read(&packet).unwrap())
    }

    /// **末尾改行つきのコメントが保存できること。**
    /// 読み取り側が trim するので、書き込み側も trim しないと読み戻し検証が必ず落ちる
    #[test]
    fn a_comment_with_a_trailing_newline_survives_a_save() {
        let (_dir, path) = scratch("exif-only.jpg");
        write_metadata(
            &path,
            &EditableMetadata {
                comment: Some("順光。もう少し寄れた\n".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let read = read_metadata(&path).unwrap();
        assert_eq!(
            read.metadata.comment.as_deref(),
            Some("順光。もう少し寄れた")
        );

        // 前提条件: コメントが EXIF 側と XMP 側の**両方**に載っていること。
        // 片側にしか無ければ候補が 1 つしか無く、食い違い警告は出ようが無い
        let (exif_side, xmp_side) = sides(&path);
        assert_eq!(
            exif_side.comment_user.as_deref(),
            Some("順光。もう少し寄れた")
        );
        assert_eq!(
            exif_side.comment_xp.as_deref(),
            Some("順光。もう少し寄れた")
        );
        assert_eq!(
            xmp_side.description.as_deref(),
            Some("順光。もう少し寄れた")
        );

        assert!(
            read.warnings.is_empty(),
            "trim の非対称で食い違い警告が出てはいけない: {:?}",
            read.warnings
        );
    }

    #[test]
    fn normalizing_trims_and_folds_empty_strings_and_zero_stars_into_none() {
        let normalized = EditableMetadata {
            title: Some("   ".into()),
            comment: Some(" 残る\n".into()),
            rating: Some(0),
        }
        .normalized();
        assert_eq!(normalized.title, None, "空白だけの欄はタグごと消す");
        assert_eq!(normalized.comment.as_deref(), Some("残る"));
        assert_eq!(normalized.rating, None);
    }

    /// **`normalized()` は書き込み側と同じ文字の落とし方をすること。**
    /// ここが素の `trim()` だと、読み戻し検証に渡す期待値が実際に保存される値と
    /// 食い違い、制御文字や NUL を含む入力の保存が必ず失敗する
    #[test]
    fn normalizing_drops_the_characters_that_the_writers_drop() {
        let normalized = EditableMetadata {
            title: Some("夕暮れ\u{0b}の港\u{0}".into()),
            comment: Some("\u{01}".into()),
            rating: None,
        }
        .normalized();
        assert_eq!(normalized.title.as_deref(), Some("夕暮れの港"));
        assert_eq!(
            normalized.comment, None,
            "落とした結果が空なら欄ごと消す（EXIF 側の「フィルタ → 空判定」と同じ順序）"
        );
    }

    /// XML が許さない文字や NUL を含む入力でも保存できること。
    /// **NUL は `to_ucs2le_nul` がそのまま埋め込み `from_ucs2le_nul` がそこで切り詰めるため、
    /// 落とさずに書くと読み戻しが必ず短くなる**
    #[test]
    fn a_value_with_forbidden_characters_still_saves() {
        let (_dir, path) = scratch("exif-only.jpg");
        write_metadata(
            &path,
            &EditableMetadata {
                title: Some("夕暮れ\u{0b}の港".into()),
                comment: Some("順光\u{0}。もう少し寄れた".into()),
                rating: Some(4),
            },
        )
        .unwrap();
        let read = read_metadata(&path).unwrap();
        assert_eq!(read.metadata.title.as_deref(), Some("夕暮れの港"));
        assert_eq!(
            read.metadata.comment.as_deref(),
            Some("順光。もう少し寄れた")
        );
    }

    /// 優先順位は XMP が先頭。業界標準であり MWG も digiKam も Lightroom もそこを見る
    #[test]
    fn xmp_wins_over_exif_and_the_discarded_value_becomes_a_warning() {
        let (_dir, path) = scratch("exif-only.jpg");

        // EXIF だけに古い値を入れる
        write_metadata(
            &path,
            &EditableMetadata {
                title: Some("old".into()),
                comment: Some("old comment".into()),
                rating: Some(2),
            },
        )
        .unwrap();
        // 前提条件: いま食い違いは無い
        assert!(read_metadata(&path).unwrap().warnings.is_empty());

        // XMP 側だけを別の値に書き換える（他アプリが編集した状況を作る）
        overwrite_xmp_only(
            &path,
            &EditableMetadata {
                title: Some("new".into()),
                comment: Some("new comment".into()),
                rating: Some(5),
            },
        );

        let read = read_metadata(&path).unwrap();
        assert_eq!(read.metadata.title.as_deref(), Some("new"));
        assert_eq!(read.metadata.comment.as_deref(), Some("new comment"));
        assert_eq!(read.metadata.rating, Some(5));
        assert_eq!(
            read.warnings.len(),
            3,
            "3 項目それぞれに警告が出る: {:?}",
            read.warnings
        );
        assert!(
            read.warnings.iter().any(|w| w.contains("old")),
            "捨てた値を文面に含める: {:?}",
            read.warnings
        );
    }

    /// 値が一致していれば警告は出ない。**保存すれば全箇所が揃うので警告は自然に消える**
    #[test]
    fn matching_values_produce_no_warning() {
        let (_dir, path) = scratch("exif-only.jpg");
        write_metadata(
            &path,
            &EditableMetadata {
                title: Some("同じ".into()),
                comment: Some("同じ".into()),
                rating: Some(3),
            },
        )
        .unwrap();

        // 前提条件: EXIF 側と XMP 側の**両方**に値が入っていること。片側にしか
        // 書けていなければ `pick` の候補が 1 つだけになり、警告が出ないのは当たり前になる
        let (exif_side, xmp_side) = sides(&path);
        assert_eq!(exif_side.title_xp.as_deref(), Some("同じ"));
        assert_eq!(exif_side.comment_user.as_deref(), Some("同じ"));
        assert_eq!(exif_side.rating_percent, Some(stars_to_percent(3)));
        assert_eq!(xmp_side.title.as_deref(), Some("同じ"));
        assert_eq!(xmp_side.description.as_deref(), Some("同じ"));
        assert_eq!(xmp_side.rating, Some(3));

        assert!(read_metadata(&path).unwrap().warnings.is_empty());
    }

    /// **★の全5段階で、往復（`stars_to_percent` → `percent_to_stars`）が誤発火を
    /// 起こさないこと。** `matching_values_produce_no_warning` は★3しか見ておらず、
    /// 特定の段階（例: 端数が生じる percent）でだけ丸めが非対称になる回帰は見逃す。
    /// `atomic::verify` に足した食い違い警告 gate（K2）は、この非対称を実際の書き込みで
    /// 誤検知する形で顕在化するので、5段階すべてを実際に保存して確かめる
    #[test]
    fn every_star_rating_round_trips_without_warnings() {
        for stars in 1..=5u8 {
            let (_dir, path) = scratch("exif-only.jpg");
            write_metadata(
                &path,
                &EditableMetadata {
                    rating: Some(stars),
                    ..Default::default()
                },
            )
            .unwrap();

            // 前提条件: EXIF 側（MicrosoftPhoto:Rating の percent）と XMP 側
            // （xmp:Rating の★そのもの）の両方に実際に値が入っていること
            let (exif_side, xmp_side) = sides(&path);
            assert_eq!(
                exif_side.rating_percent,
                Some(stars_to_percent(stars)),
                "★{stars}"
            );
            assert_eq!(xmp_side.rating, Some(stars), "★{stars}");

            let read = read_metadata(&path).unwrap();
            assert_eq!(read.metadata.rating, Some(stars), "★{stars}");
            assert!(
                read.warnings.is_empty(),
                "★{stars}: 誤発火した警告 {:?}",
                read.warnings
            );
        }
    }

    /// XMP 側だけを差し替えるテスト用ヘルパー。
    /// `write_metadata` は両方に書くので、食い違いは作れない
    fn overwrite_xmp_only(path: &std::path::Path, values: &EditableMetadata) {
        let bytes = std::fs::read(path).unwrap();
        let mut jpeg = Jpeg::from_bytes(Bytes::from(bytes)).unwrap();
        let packet = match xmp::extract(&jpeg).unwrap() {
            Some(existing) => xmp::patch(&existing, values).unwrap(),
            None => xmp::new_packet(values),
        };
        xmp::install(&mut jpeg, &packet).unwrap();
        std::fs::write(path, jpeg.encoder().bytes()).unwrap();
    }

    /// **ピクセル不変は設計の必須条件。** 回帰したら即座に落ちること
    #[test]
    fn writing_metadata_does_not_touch_a_single_pixel_byte() {
        let (_dir, path) = scratch("exif-only.jpg");
        let before = std::fs::read(&path).unwrap();
        let before_scan = before[atomic::scan_offset(&before).unwrap()..].to_vec();

        write_metadata(
            &path,
            &EditableMetadata {
                title: Some("夕暮れの港".into()),
                comment: Some("順光。もう少し寄れた".into()),
                rating: Some(4),
            },
        )
        .unwrap();

        let after = std::fs::read(&path).unwrap();
        // 前提条件: 書き込みでファイル自体は変わっていること（無変更なら不変は自明）
        assert_ne!(after, before, "メタデータが書かれていない");
        assert_eq!(
            after[atomic::scan_offset(&after).unwrap()..],
            before_scan[..]
        );
    }

    #[test]
    fn writing_metadata_round_trips_japanese() {
        let (_dir, path) = scratch("exif-only.jpg");
        let values = EditableMetadata {
            title: Some("夕暮れの港".into()),
            comment: Some("順光。もう少し寄れた".into()),
            rating: Some(4),
        };
        write_metadata(&path, &values).unwrap();
        assert_eq!(read_metadata(&path).unwrap().metadata, values);
    }

    /// 既存の XMP を持つ画像でも、**本ツールの 4 プロパティ以外は 1 文字も変えない**。
    /// 削除して作り直す実装なら Lightroom の現像設定が消える
    #[test]
    fn writing_metadata_keeps_other_namespaces_and_pixels_in_a_lightroom_file() {
        let (_dir, path) = scratch("lightroom-xmp.jpg");
        let before = std::fs::read(&path).unwrap();
        let before_scan = before[atomic::scan_offset(&before).unwrap()..].to_vec();
        // 前提条件: このフィクスチャは実際に crs: のデータを持っている
        let before_packet = xmp::extract(&Jpeg::from_bytes(Bytes::from(before.clone())).unwrap())
            .unwrap()
            .unwrap();
        assert!(
            before_packet.contains("crs:Exposure2012"),
            "前提条件が崩れている"
        );

        write_metadata(
            &path,
            &EditableMetadata {
                title: Some("夕暮れの港".into()),
                rating: Some(5),
                ..Default::default()
            },
        )
        .unwrap();

        let after = std::fs::read(&path).unwrap();
        assert_eq!(
            after[atomic::scan_offset(&after).unwrap()..],
            before_scan[..]
        );
        let after_packet = xmp::extract(&Jpeg::from_bytes(Bytes::from(after)).unwrap())
            .unwrap()
            .unwrap();
        assert!(
            after_packet.contains("crs:Exposure2012"),
            "現像設定が消えた"
        );
        assert!(after_packet.contains(r#"crs:RawFileName="DSC05240.ARW""#));
        let read = read_metadata(&path).unwrap();
        assert_eq!(read.metadata.title.as_deref(), Some("夕暮れの港"));
        assert_eq!(read.metadata.rating, Some(5));
    }

    /// 既存の XMP APP1 の `</x:xmpmeta>` の直前に `insert` を差し込む
    fn corrupt_xmp(path: &std::path::Path, insert: &[u8]) {
        let bytes = std::fs::read(path).unwrap();
        let mut jpeg = Jpeg::from_bytes(Bytes::from(bytes)).unwrap();
        let segment = jpeg
            .segments_mut()
            .iter_mut()
            .find(|s| {
                s.marker() == img_parts::jpeg::markers::APP1
                    && s.contents().starts_with(xmp::XMP_ID)
            })
            .expect("前提条件が崩れている: XMP APP1 が無い");
        let mut contents = segment.contents().to_vec();
        let at = contents
            .windows(b"</x:xmpmeta>".len())
            .position(|w| w == b"</x:xmpmeta>")
            .expect("前提条件が崩れている: </x:xmpmeta> が無い");
        contents.splice(at..at, insert.iter().copied());
        *segment = img_parts::jpeg::JpegSegment::new_with_contents(
            img_parts::jpeg::markers::APP1,
            Bytes::from(contents),
        );
        std::fs::write(path, jpeg.encoder().bytes()).unwrap();
    }

    /// **読めない XMP を「XMP が無い」と取り違えて作り直してはならない。**
    /// 作り直すと Lightroom の現像設定（`crs:`）が警告も無く全部消える。
    /// Latin-1 の `é`（0xE9）を 1 バイト含むだけで UTF-8 として読めなくなる
    #[test]
    fn writing_over_an_undecodable_xmp_fails_and_leaves_the_file_untouched() {
        let (_dir, path) = scratch("lightroom-xmp.jpg");
        corrupt_xmp(&path, b"<!-- caf\xe9 -->");
        let before = std::fs::read(&path).unwrap();
        // 前提条件: 失われうるデータが実際にある
        assert!(
            before
                .windows(b"crs:Exposure2012".len())
                .any(|w| w == b"crs:Exposure2012"),
            "前提条件が崩れている"
        );

        let result = write_metadata(
            &path,
            &EditableMetadata {
                title: Some("夕暮れの港".into()),
                ..Default::default()
            },
        );

        assert!(result.is_err(), "読めない XMP の上に保存できてしまった");
        assert_eq!(std::fs::read(&path).unwrap(), before, "ファイルが変わった");
    }

    /// 読めない XMP があっても**読み取りは成立させ、黙らない**（Exif が壊れている時と同じ扱い）。
    /// Exif 側の値はそのまま読める
    #[test]
    fn reading_an_unreadable_xmp_warns_and_keeps_the_exif_side() {
        for (label, insert) in [
            ("UTF-8 として不正", &b"<!-- caf\xe9 -->"[..]),
            ("XML として非整形式", &b"<broken>"[..]),
        ] {
            let (_dir, path) = scratch("lightroom-xmp.jpg");
            write_metadata(
                &path,
                &EditableMetadata {
                    title: Some("夕暮れの港".into()),
                    rating: Some(3),
                    ..Default::default()
                },
            )
            .unwrap();
            corrupt_xmp(&path, insert);

            let read = read_metadata(&path)
                .unwrap_or_else(|e| panic!("{label}: 読み取り全体が失敗した: {e:#}"));

            assert_eq!(
                read.metadata.title.as_deref(),
                Some("夕暮れの港"),
                "{label}"
            );
            assert_eq!(read.metadata.rating, Some(3), "{label}");
            assert!(
                read.warnings.iter().any(|w| w.contains("XMP")),
                "{label}: 警告が出ない {:?}",
                read.warnings
            );
        }
    }

    /// 一時ファイルが残らないこと。残ると次回の書き込みで衝突しうる
    #[test]
    fn writing_metadata_leaves_no_temporary_file_behind() {
        let (dir, path) = scratch("exif-only.jpg");
        write_metadata(
            &path,
            &EditableMetadata {
                rating: Some(1),
                ..Default::default()
            },
        )
        .unwrap();
        // 前提条件: 書き込み自体は成立している（何もしていないなら残骸が無いのは自明）
        assert_eq!(read_metadata(&path).unwrap().metadata.rating, Some(1));

        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|name| name.contains("picture-tool.tmp"))
            .collect();
        assert!(leftovers.is_empty(), "残骸: {:?}", leftovers);
        // 名前の付け方を変えても検知できるよう、フォルダーの中身そのものを見る
        assert_eq!(entries(dir.path()), vec!["exif-only.jpg".to_string()]);
    }

    /// 書き込めない対象では**元ファイルを 1 バイトも変えずに** Err で戻ること
    #[test]
    fn a_non_jpeg_fails_without_modifying_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("broken.jpg");
        std::fs::write(&path, b"not a jpeg at all").unwrap();

        let error = write_metadata(
            &path,
            &EditableMetadata {
                rating: Some(3),
                ..Default::default()
            },
        )
        .unwrap_err();
        // 前提条件: JPEG として読めなかったことが失敗の理由である
        // （別の理由で早々に落ちていると「無変更」は自明に通ってしまう）
        assert!(
            format!("{error:#}").contains("JPEG として読めません"),
            "{error:#}"
        );
        assert_eq!(std::fs::read(&path).unwrap(), b"not a jpeg at all");
        assert_eq!(entries(dir.path()), vec!["broken.jpg".to_string()]);
    }

    /// `xmp::patch` が「書き込み先の `rdf:Description` が無い」で失敗したとき、
    /// **元ファイルが無傷であることが利用者に伝わる**こと
    #[test]
    fn a_packet_without_a_description_fails_and_says_the_original_is_untouched() {
        let (dir, path) = scratch("exif-only.jpg");
        let bytes = std::fs::read(&path).unwrap();
        let mut jpeg = Jpeg::from_bytes(Bytes::from(bytes)).unwrap();
        xmp::install(
            &mut jpeg,
            r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><nothing-to-patch/></x:xmpmeta>"#,
        )
        .unwrap();
        std::fs::write(&path, jpeg.encoder().bytes()).unwrap();
        let before = std::fs::read(&path).unwrap();
        // 前提条件: 差し込み先の無いパケットが実際に据え付けられている
        assert!(
            xmp::extract(&Jpeg::from_bytes(Bytes::from(before.clone())).unwrap())
                .unwrap()
                .is_some(),
            "前提条件が崩れている: XMP パケットが入っていない"
        );

        let error = write_metadata(
            &path,
            &EditableMetadata {
                rating: Some(3),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(
            format!("{error:#}").contains("元ファイルは変更していません"),
            "{error:#}"
        );
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(entries(dir.path()), vec!["exif-only.jpg".to_string()]);
    }

    /// フォルダーの中身を名前順に並べて返す
    fn entries(dir: &std::path::Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    /// **JPEG については `read_metadata_fast` と `read_metadata` の結果は常に一致する。**
    ///
    /// 先頭経路が成功すれば同じ `read_metadata_bytes` に同じ APP1 を渡しており、
    /// 失敗すれば `read_metadata` そのものを呼ぶので、構成上そうなる。
    #[test]
    fn fast_and_full_reads_agree_on_jpeg() {
        let (_dir, path) = scratch("lightroom-xmp.jpg");
        write_metadata(
            &path,
            &EditableMetadata {
                title: Some("夕暮れの港".to_string()),
                comment: Some("テスト".to_string()),
                rating: Some(4),
            },
        )
        .unwrap();

        let full = read_metadata(&path).unwrap();
        // 前提条件: 比較対象が default でないこと。両方 default なら
        // `read_metadata_fast` が中身を読まずに Ok(None) を返しても一致してしまう
        assert_eq!(full.metadata.rating, Some(4), "前提条件が崩れている");
        assert_eq!(full.metadata.title.as_deref(), Some("夕暮れの港"));

        assert_eq!(read_metadata_fast(&path).unwrap().metadata, full.metadata);
    }

    /// 既存 fixture 2 つでも一致する（片方は Exif のみ、片方は XMP 付き）
    #[test]
    fn fast_and_full_reads_agree_on_the_fixtures() {
        for name in ["exif-only.jpg", "lightroom-xmp.jpg"] {
            let path = fixture(name);
            assert_eq!(
                read_metadata_fast(&path).unwrap().metadata,
                read_metadata(&path).unwrap().metadata,
                "{name} で食い違った"
            );
        }
    }

    /// **フォールバックが効くこと。**
    ///
    /// 前提条件として `read_metadata_header` 単体が `Err` になることを先に assert する ──
    /// 入力が実は歪んでいなければ、フォールバックを消しても緑のままになる。
    #[test]
    fn a_jpeg_the_header_scan_cannot_parse_falls_back_to_the_full_read() {
        let (_dir, path) = scratch("lightroom-xmp.jpg");
        // **読める中身を先に入れる。** 空の写真で比べると、フォールバックを消して
        // Err を握り潰す実装でも「両方 default」で一致してしまう
        write_metadata(
            &path,
            &EditableMetadata {
                rating: Some(4),
                ..Default::default()
            },
        )
        .unwrap();

        // SOI の直後にゴミを 2 バイト差し込み、セグメント境界を壊す。
        // `img_parts` は APP1 に辿り着けるが、手書きの走査は 0xFF を期待して落ちる
        // （この非対称は実測で確認済み。計画の Task 2 の実験ログ参照）
        let mut raw = std::fs::read(&path).unwrap();
        raw.splice(2..2, [0x00, 0x00]);
        std::fs::write(&path, &raw).unwrap();

        // 前提条件 1: 先頭だけの走査は失敗する
        assert!(
            read_metadata_header(&path).is_err(),
            "前提条件が崩れている: 壊したはずの JPEG を先頭走査が読めてしまった"
        );
        // 前提条件 2: 全体読みは成功し、★も生きている（＝フォールバック先が生きている）
        let full = read_metadata(&path).unwrap();
        assert_eq!(full.metadata.rating, Some(4), "前提条件が崩れている");

        assert_eq!(read_metadata_fast(&path).unwrap().metadata, full.metadata);
    }

    /// **非 JPEG は意図的に非対称にする。**
    ///
    /// `list_images` は PNG / WebP / JPEG XL / AVIF も並べる。★の読み取りは一覧表示の副作用なので、
    /// PNG が混ざっているだけで一覧が失敗したり警告が出たりしてはいけない。
    #[test]
    fn non_jpeg_is_deliberately_asymmetric() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("photo.png");
        std::fs::write(&path, [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]).unwrap();

        // 全体読みは「JPEG として読めません」で Err（現行のまま）
        assert!(read_metadata(&path).is_err());
        // 入口は Ok かつ全項目 None
        let read = read_metadata_fast(&path).unwrap();
        assert_eq!(read.metadata, EditableMetadata::default());
        assert!(read.warnings.is_empty());
    }

    /// **APP1 を持たない JPEG は、フォールバックへ落ちずに `Ok` かつ全項目 `None`。**
    ///
    /// これを `read_metadata_fast` で見てはならない ── 先頭走査が組み立てた
    /// バイト列を `read_metadata_bytes` が受け取れなくても、フォールバックが
    /// 全体読みで拾ってしまい**テストは緑のまま通る**。落ちていないことを
    /// 確かめたいので `read_metadata_header` 単体を見る。
    /// （本ツールが変換して出力した JPEG がこの形。`_processed` フォルダーを
    /// 開くと全写真がここを通る）
    #[test]
    fn a_jpeg_without_app1_reads_as_none_without_falling_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("no-app1.jpg");
        // SOI ＋ SOS ヘッダ ＋ 画素 ＋ EOI。APP1 も APP0 も持たない
        let mut raw = vec![0xFF, 0xD8, 0xFF, 0xDA, 0x00, 0x0C];
        raw.extend_from_slice(b"sos-header");
        raw.extend_from_slice(&[0x11; 256]);
        raw.extend_from_slice(&[0xFF, 0xD9]);
        std::fs::write(&path, &raw).unwrap();

        let read = read_metadata_header(&path).unwrap();
        assert_eq!(read.metadata, EditableMetadata::default());
        // **警告も出さない。** 「Exif が無い」と「Exif が壊れている」は別である
        assert!(read.warnings.is_empty(), "警告が出た: {:?}", read.warnings);
    }

    /// 拡張子が `.jpg` なのに中身が JPEG ですらないファイル。
    /// **先頭も全体も読めないので `Err`。panic はしない**
    #[test]
    fn a_file_that_is_not_a_jpeg_at_all_errors_without_panicking() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("liar.jpg");
        std::fs::write(&path, b"this is not a jpeg at all").unwrap();

        assert!(read_metadata_header(&path).is_err()); // 前提条件
        assert!(read_metadata_fast(&path).is_err());
    }

    /// **保存を止めない警告は `write_metadata` の戻り値で返る**（core は `eprintln!` しない）。
    /// MakerNote を持たない写真では空であることを対照に置く
    #[test]
    fn a_moved_maker_note_is_returned_as_a_warning() {
        let (_dir, plain) = scratch("exif-only.jpg");
        let values = EditableMetadata {
            title: Some("Harbor at dusk".into()),
            ..Default::default()
        };
        assert_eq!(
            write_metadata(&plain, &values).unwrap(),
            Vec::<String>::new(),
            "MakerNote が無ければ警告も無い"
        );

        let (_dir, path) = scratch("exif-only.jpg");
        let mut bytes = std::fs::read(&path).unwrap();
        let mut meta = Metadata::new_from_vec(&bytes, FileExtension::JPEG).unwrap();
        meta.set_tag(little_exif::exif_tag::ExifTag::MakerNote(
            b"SONY DSC \0\0\0maker-note-body".to_vec(),
        ));
        meta.write_to_vec(&mut bytes, FileExtension::JPEG).unwrap();
        std::fs::write(&path, &bytes).unwrap();

        assert_eq!(
            write_metadata(&path, &values).unwrap(),
            vec![preserve::MAKER_NOTE_MOVED.to_string()]
        );
        assert_eq!(
            read_metadata(&path).unwrap().metadata,
            values,
            "保存はされている"
        );
    }
}
