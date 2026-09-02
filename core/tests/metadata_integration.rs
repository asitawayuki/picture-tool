//! メタデータ編集の統合テスト。
//!
//! 合成画像だけでは spec の「未検証リスク」を踏み抜く。**実写真由来の Exif** と、
//! **他名前空間を含む実物の XMP パケット**に対して検査する。

use picture_tool_core::metadata::{read_metadata, write_metadata, EditableMetadata};
use std::path::{Path, PathBuf};

fn scratch(name: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(name);
    std::fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name),
        &path,
    )
    .unwrap();
    (dir, path)
}

/// kamadak-exif で全フィールドを列挙する。little_exif とは独立の実装
///
/// `In(1)/JPEGInterchangeFormat`（IFD1 のサムネイル JPEG 本体への相対オフセット）だけは
/// 比較から除外する。IFD0 にタグが増えれば Exif セグメント全体が伸び、その分だけ後方の
/// IFD1 サムネイルは物理的に後ろへ移動するので、**このタグの値が変わるのは little_exif の
/// 欠陥ではなく定義上正しい挙動**（`core/tests/metadata_spike.rs` の同名ヘルパで実測済み:
/// 1030 → 1056 も、サムネイル本体の SHA256 が完全一致した状態でのシフトだった）。
/// 対になる `JPEGInterchangeFormatLength`（サムネイルの長さ）はここでは除外せず比較に残す
/// （長さが変われば切り詰められたということなので、それは検知したい）。
///
/// **この除外の対価は `thumbnail_bytes` による本体バイト列比較で払う。**
/// オフセットだけを比較から外して本体を見ないままにすると、「長さは同じだが本体が
/// 別のバイト列に化ける／古い位置を指す」欠陥をどこにも引っかからない穴にしてしまう
/// （`writing_metadata_keeps_every_existing_exif_tag` 参照）。
fn exif_fields(path: &Path) -> Vec<(String, String)> {
    let file = std::fs::File::open(path).unwrap();
    let mut reader = std::io::BufReader::new(file);
    let exif = exif::Reader::new()
        .read_from_container(&mut reader)
        .unwrap();
    let mut fields: Vec<(String, String)> = exif
        .fields()
        .filter(|f| {
            !(f.ifd_num == exif::In::THUMBNAIL && f.tag == exif::Tag::JPEGInterchangeFormat)
        })
        .map(|f| {
            (
                format!("{:?}/{}", f.ifd_num, f.tag),
                f.display_value().to_string(),
            )
        })
        .collect();
    fields.sort();
    fields
}

/// IFD1 のサムネイル JPEG 本体を切り出す。サムネイルを持たない画像では `None` を返す。
///
/// `core/tests/metadata_spike.rs` の同名ヘルパと同じ考え方 ── オフセット・長さは
/// ハードコードせず、渡された `bytes` 自身から kamadak-exif で都度読み直す
/// （書き込み後の実際の値を見ることで、little_exif がオフセットを正しく追従させているか
/// 自体を検査対象にするため）。オフセットは Exif セグメント内の `Exif\0\0` マーカー直後、
/// すなわち TIFF ヘッダの先頭からの相対値
fn thumbnail_bytes(bytes: &[u8]) -> Option<Vec<u8>> {
    let mut cursor = std::io::Cursor::new(bytes);
    let reader = exif::Reader::new().read_from_container(&mut cursor).ok()?;

    let offset_field = reader.get_field(exif::Tag::JPEGInterchangeFormat, exif::In::THUMBNAIL)?;
    let length_field =
        reader.get_field(exif::Tag::JPEGInterchangeFormatLength, exif::In::THUMBNAIL)?;
    let offset = offset_field
        .value
        .get_uint(0)
        .expect("JPEGInterchangeFormat が整数として読めない") as usize;
    let length = length_field
        .value
        .get_uint(0)
        .expect("JPEGInterchangeFormatLength が整数として読めない") as usize;

    const TIFF_HEADER_MARKER: &[u8] = b"Exif\0\0";
    let exif_pos = bytes
        .windows(TIFF_HEADER_MARKER.len())
        .position(|w| w == TIFF_HEADER_MARKER)
        .expect("Exif セグメントの `Exif\\0\\0` マーカーが見つからない");
    let tiff_start = exif_pos + TIFF_HEADER_MARKER.len();

    let start = tiff_start + offset;
    let end = start + length;
    assert!(
        end <= bytes.len(),
        "サムネイルの範囲がファイル長を超えている（start={start}, end={end}, file_len={}）",
        bytes.len()
    );
    let thumb = &bytes[start..end];
    // オフセット計算そのものが正しいことの検査。ずれていればここで SOI/EOI が見えないはず
    assert!(
        thumb.starts_with(&[0xFF, 0xD8]) && thumb.ends_with(&[0xFF, 0xD9]),
        "切り出したサムネイルが JPEG として壊れている（offset={offset}, length={length}）"
    );
    Some(thumb.to_vec())
}

/// **既存タグの保存** — 追加した項目以外のタグ集合に差分が無いこと。
///
/// **サムネイル本体のバイト列一致も検査する**（`exif_fields` が `JPEGInterchangeFormat`
/// を比較から除外している対価）。**画素不変**（`sos_offset` 以降が不変）も併せて検査する
/// ── 実写真＋実 Exif という最も本番に近い経路でピクセル不変を押さえておく
#[test]
fn writing_metadata_keeps_every_existing_exif_tag() {
    let (_dir, path) = scratch("exif-only.jpg");
    let before_bytes = std::fs::read(&path).unwrap();
    let before = exif_fields(&path);
    // 前提条件: 実写真由来の Exif が本当に入っていること。
    // ここが空だと「タグが消えていない」は自明に通ってしまう
    assert!(
        before.len() > 10 && before.iter().any(|(name, _)| name.contains("Model")),
        "フィクスチャの Exif が薄すぎる。Task 2 を見直すこと: {:?}",
        before
    );
    let before_thumbnail = thumbnail_bytes(&before_bytes);
    // 前提条件: このフィクスチャは本当にサムネイルを埋め込んでいること。
    // ここが `None` だと、下のサムネイル比較は `None == None` で自明に緑になってしまう
    assert!(
        before_thumbnail.is_some(),
        "この写真には埋め込みサムネイルが必要（比較が空振りしていないことの前提条件）"
    );

    write_metadata(
        &path,
        &EditableMetadata {
            title: Some("夕暮れの港".into()),
            comment: Some("順光".into()),
            rating: Some(4),
        },
    )
    .unwrap();

    let after = exif_fields(&path);
    let lost: Vec<&(String, String)> = before.iter().filter(|f| !after.contains(f)).collect();
    assert!(lost.is_empty(), "消えた・変わったタグ: {:#?}", lost);

    let after_bytes = std::fs::read(&path).unwrap();
    let after_thumbnail = thumbnail_bytes(&after_bytes);
    assert_eq!(
        before_thumbnail, after_thumbnail,
        "サムネイル本体が書き込み前後で変わった（消失または内容の変化）"
    );

    assert_eq!(
        after_bytes[sos_offset(&after_bytes)..],
        before_bytes[sos_offset(&before_bytes)..],
        "既存タグを保存する経路で画素が変化した"
    );
}

/// **再編集** — 2 回書いても壊れない（`little_exif` issue #93 の再現条件）
#[test]
fn writing_twice_keeps_the_file_readable() {
    let (_dir, path) = scratch("exif-only.jpg");
    for stars in [2u8, 5] {
        write_metadata(
            &path,
            &EditableMetadata {
                title: Some(format!("{} 回目", stars)),
                rating: Some(stars),
                ..Default::default()
            },
        )
        .unwrap();
    }
    let read = read_metadata(&path).unwrap();
    assert_eq!(read.metadata.rating, Some(5));
    assert_eq!(read.metadata.title.as_deref(), Some("5 回目"));
}

/// XMP パケットの XML だけを取り出す。`xmp::extract` は private なので自前で切る
fn xmp_packet(path: &Path) -> String {
    const ID: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";
    let bytes = std::fs::read(path).unwrap();
    let start = bytes
        .windows(ID.len())
        .position(|window| window == ID)
        .expect("XMP パケットが無い")
        + ID.len();
    let tail = &bytes[start..];
    let end = tail
        .windows(13)
        .position(|window| window == b"<?xpacket end")
        .unwrap_or(tail.len());
    String::from_utf8(tail[..end].to_vec()).expect("XMP は UTF-8")
}

/// `名前空間接頭辞:属性名="値"` を 1 つ丸ごと切り出す。
/// **固定長で切らない** ── `from_utf8_lossy` した任意位置は char 境界を割って panic する
fn first_attribute(packet: &str, prefix: &str) -> String {
    let at = packet.find(prefix).expect("接頭辞が見つからない");
    let rest = &packet[at..];
    let open = rest.find('"').expect("属性値の開き引用符が無い");
    let close = rest[open + 1..]
        .find('"')
        .expect("属性値の閉じ引用符が無い");
    rest[..open + 1 + close + 1].to_string()
}

/// **既存 XMP の保持** — 本ツールの 4 プロパティ以外の値が 1 つも変わらないこと
#[test]
fn writing_metadata_preserves_foreign_xmp_namespaces() {
    let (_dir, path) = scratch("lightroom-xmp.jpg");
    let before = xmp_packet(&path);

    // 前提条件: 他名前空間（crs:）が本当に入っていること。
    // ここが空だと「壊れていない」は自明に通ってしまう
    assert!(
        before.contains("camera-raw-settings"),
        "フィクスチャの XMP に crs: が無い。Task 2 の Step 4 を見直すこと"
    );
    let sample = first_attribute(&before, "crs:");

    write_metadata(
        &path,
        &EditableMetadata {
            title: Some("差し替えた題".into()),
            rating: Some(1),
            ..Default::default()
        },
    )
    .unwrap();

    let after = xmp_packet(&path);
    assert!(after.contains(&sample), "crs: の値が失われた: {:?}", sample);
    assert!(
        after.contains("camera-raw-settings"),
        "crs: の名前空間宣言が消えた"
    );
    assert_eq!(read_metadata(&path).unwrap().metadata.rating, Some(1));
}

/// **解析できない Exif を黙って作り直さないこと。**
///
/// 本ツールの編集対象は Lightroom 書き出しで MakerNote を持たないため、
/// `little_exif` の issue #93（MakerNote 絡みで解析に失敗しうる）は
/// **残リスクとして受け入れている**（Task 1 の注記）。その受け入れが成立する根拠が
/// この検査 ── 解析できない Exif に当たったら、作り直さずに失敗する。
/// **このテストを落とすと残リスクの前提が崩れる。**
#[test]
fn a_file_whose_exif_cannot_be_parsed_is_refused_without_being_modified() {
    let (_dir, path) = scratch("exif-only.jpg");
    let mut bytes = std::fs::read(&path).unwrap();
    // Exif APP1 の中の TIFF ヘッダだけを壊す。セグメント長は保つので
    // img-parts は読めるが、little_exif は解析に失敗する
    let at = bytes
        .windows(6)
        .position(|window| window == b"Exif\0\0")
        .expect("Exif APP1 が無い")
        + 6;
    bytes[at..at + 8].copy_from_slice(&[0xAA; 8]);
    std::fs::write(&path, &bytes).unwrap();

    let error = write_metadata(
        &path,
        &EditableMetadata {
            rating: Some(3),
            ..Default::default()
        },
    )
    .expect_err("解析できない Exif を黙って作り直してはいけない");
    // `anyhow::Error` の素の `Display`（`to_string()`）は最上位の文脈しか見せない
    // （`write_metadata` が足す「メタデータを保存できませんでした」だけ）。
    // "Exif" は 1 段階内側の `build` の文脈にあるので、チェーン全体を見る
    // `{:#}`（alternate）で確認する。既存の単体テスト（`mod.rs` の
    // `a_non_jpeg_fails_without_modifying_the_file` 等）と同じ確認方法
    let message = format!("{error:#}");
    assert!(
        message.contains("Exif"),
        "理由が伝わる文面であること: {message}"
    );
    // "Exif" だけの部分一致だと、`build` の別の失敗文脈（例:
    // `.context("Exif を書き込めません")`）でも通ってしまい、
    // `has_exif_segment` ガードを名指ししたことにならない。
    // ガード固有の文言（"解析できない"）まで確認する
    assert!(
        message.contains("解析できない"),
        "has_exif_segment ガードのメッセージであること（別の失敗経路と取り違えていないか）: {message}"
    );
    assert_eq!(
        std::fs::read(&path).unwrap(),
        bytes,
        "元ファイルは無傷であること"
    );
}

/// **原子性** — 書き込みに失敗しても元ファイルが無傷であること
#[test]
fn a_failed_write_leaves_the_original_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("truncated.jpg");
    // SOS まで届かずに終わる JPEG。`Jpeg::from_bytes` が Truncated で落ちる
    // （`scan_offset` まで到達しない）ので、一時ファイルすら作られない
    std::fs::write(&path, [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0x11, 0x22]).unwrap();
    let before = std::fs::read(&path).unwrap();

    assert!(write_metadata(
        &path,
        &EditableMetadata {
            rating: Some(3),
            ..Default::default()
        }
    )
    .is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

// --- Task 9 レビューからの申し送り（Task 10 ブリーフの5本には無い） ------------

/// SOS（`FF DA`）の開始位置。`metadata::atomic::scan_offset` と同じ考え方だが、
/// 統合テストからは private な実装に触れないので自前で持つ。
/// 入力は img-parts で再エンコードした直後のバイト列（fill byte が入る心配は無い）だが、
/// 念のため fill byte 対応まで揃えておく
fn sos_offset(bytes: &[u8]) -> usize {
    let mut cursor = 2; // SOI をとばす
    loop {
        assert_eq!(
            bytes[cursor], 0xFF,
            "セグメント境界が壊れている（offset {}）",
            cursor
        );
        let mut marker_at = cursor + 1;
        while bytes[marker_at] == 0xFF {
            marker_at += 1;
        }
        if bytes[marker_at] == 0xDA {
            return cursor;
        }
        let length = u16::from_be_bytes([bytes[marker_at + 1], bytes[marker_at + 2]]) as usize;
        cursor = marker_at + 1 + length;
    }
}

/// **「値を消す」往復** — ★と本文を設定して保存 → 空欄（`None`）で保存し直す →
/// 読み戻すと全部 `None` になること。
///
/// `exif::apply` の削除が効いていないと `atomic::verify` の読み戻し一致検査が
/// 永久に `Err` を返し、利用者は欄を空にして保存できなくなる。`lightroom-xmp.jpg` で
/// 行い、**他名前空間（`crs:`）が消去の往復後も残っていること**も確かめる
/// （4 プロパティ以外を触らない実装なら、消去の往復でも壊れないはず）
#[test]
fn clearing_previously_set_fields_removes_them_and_keeps_foreign_xmp_namespaces() {
    let (_dir, path) = scratch("lightroom-xmp.jpg");
    let original_packet = xmp_packet(&path);
    // 前提条件: 他名前空間（crs:）が本当に入っていること
    assert!(
        original_packet.contains("camera-raw-settings"),
        "フィクスチャの XMP に crs: が無い"
    );
    let sample = first_attribute(&original_packet, "crs:");

    write_metadata(
        &path,
        &EditableMetadata {
            title: Some("夕暮れの港".into()),
            comment: Some("順光".into()),
            rating: Some(4),
        },
    )
    .unwrap();
    // 前提条件: 消す前に、本当に3項目とも値が入っていること。
    // ここが揃っていないと「消えた」は自明に通ってしまう
    let set = read_metadata(&path).unwrap().metadata;
    assert_eq!(
        set,
        EditableMetadata {
            title: Some("夕暮れの港".into()),
            comment: Some("順光".into()),
            rating: Some(4),
        },
        "前提条件が崩れている: 消す前の値が保存できていない"
    );

    write_metadata(&path, &EditableMetadata::default()).unwrap();

    let cleared = read_metadata(&path).unwrap();
    assert_eq!(
        cleared.metadata,
        EditableMetadata::default(),
        "空欄で保存し直しても値が消えない"
    );
    assert!(
        cleared.warnings.is_empty(),
        "消去後に EXIF 側と XMP 側が食い違っている: {:?}",
        cleared.warnings
    );

    let after_packet = xmp_packet(&path);
    assert!(
        after_packet.contains(&sample),
        "crs: の値が消去の往復で失われた: {:?}",
        sample
    );
    assert!(
        after_packet.contains("camera-raw-settings"),
        "crs: の名前空間宣言が消去の往復で消えた"
    );
}

/// **Exif を1つも持たない JPEG への保存** — `build` の `Err(_) => Metadata::new()` 分岐
/// （Exif APP1 を持たない画像には新しい Exif を作ってよい）を通すことを**狙った**テスト。
///
/// 統合テストは公開 API（`read_metadata` / `write_metadata`）越しにしか観測できないため、
/// `little_exif::Metadata::new_from_vec` が実際に `Err` を返して `Err(_) => Metadata::new()`
/// を通ったのか、`Ok` を返す別の内部経路を通ったのかを直接は区別できない。
/// **前提条件として保証しているのは「Exif APP1 セグメントが無い」こと**
/// （下の assert）── これは `has_exif_segment(&jpeg)` が false になる状況そのものなので、
/// `Metadata::new_from_vec` がどちらを返しても `build` は失敗せず書き込みが完了するはず、
/// というのがこのテストの主張
///
/// `img-parts` で `exif-only.jpg` から Exif APP1 を取り除いたバイト列を用意し、
/// そこへ書き込めること・読み戻せること・画素が不変であることを確かめる
#[test]
fn writing_metadata_to_a_jpeg_without_any_exif_segment_creates_one() {
    let (_dir, path) = scratch("exif-only.jpg");
    let original = std::fs::read(&path).unwrap();

    let mut jpeg =
        img_parts::jpeg::Jpeg::from_bytes(img_parts::Bytes::from(original.clone())).unwrap();
    jpeg.segments_mut().retain(|segment| {
        !(segment.marker() == img_parts::jpeg::markers::APP1
            && segment.contents().starts_with(b"Exif\0\0"))
    });
    let stripped = jpeg.encoder().bytes().to_vec();
    // 前提条件: Exif APP1 を本当に取り除けていること。
    // ここが崩れていると `build` の Err(_) 分岐を通ったことにならない
    assert!(
        !stripped.windows(6).any(|window| window == b"Exif\0\0"),
        "前提条件が崩れている: Exif APP1 が残っている"
    );
    std::fs::write(&path, &stripped).unwrap();
    let before_scan = stripped[sos_offset(&stripped)..].to_vec();

    write_metadata(
        &path,
        &EditableMetadata {
            title: Some("夕暮れの港".into()),
            rating: Some(3),
            ..Default::default()
        },
    )
    .unwrap();

    let after = std::fs::read(&path).unwrap();
    assert_eq!(
        after[sos_offset(&after)..],
        before_scan[..],
        "Exif を新規作成した際に画素が変化した"
    );

    let read = read_metadata(&path).unwrap();
    assert_eq!(read.metadata.title.as_deref(), Some("夕暮れの港"));
    assert_eq!(read.metadata.rating, Some(3));
    assert!(
        read.warnings.is_empty(),
        "新規作成した Exif と XMP が食い違っている: {:?}",
        read.warnings
    );
}

/// **Exif APP1 を持たない JPEG を読んでも警告は出ないこと。**
///
/// `little_exif` の読み取りは、APP1 に出会わないまま EOI へ到達すると
/// `Err("No EXIF data found!")` を返す。これをそのまま警告へ積むと
/// **Exif を持たない JPEG すべて**に生の英語エラーが出る ── 本ツールの変換出力
/// （`JpegEncoder` は Exif を書かない）が丸ごとそれに当たるので、利用者が
/// `_processed` フォルダーを情報モードで開くと全写真に警告が並ぶ。
///
/// あわせて **XMP 側の読み取りが生きている**ことも見る。警告が空なだけなら
/// 「読み取りごと死んでいる」でも通ってしまうため。
#[test]
fn reading_a_jpeg_without_any_exif_segment_reports_no_warning() {
    let (_dir, path) = scratch("exif-only.jpg");

    // まず両方（Exif と XMP）に値を書いてから、Exif APP1 だけを取り除く。
    // XMP が残るので「Exif 不在でも XMP は読める」ことを検査できる
    write_metadata(
        &path,
        &EditableMetadata {
            title: Some("夕暮れの港".into()),
            rating: Some(3),
            ..Default::default()
        },
    )
    .unwrap();

    let written = std::fs::read(&path).unwrap();
    let mut jpeg =
        img_parts::jpeg::Jpeg::from_bytes(img_parts::Bytes::from(written.clone())).unwrap();
    jpeg.segments_mut().retain(|segment| {
        !(segment.marker() == img_parts::jpeg::markers::APP1
            && segment.contents().starts_with(b"Exif\0\0"))
    });
    let stripped = jpeg.encoder().bytes().to_vec();
    std::fs::write(&path, &stripped).unwrap();

    // 前提条件1: Exif APP1 が本当に無い（＝解析が必ず失敗する状況を作れている）
    assert!(
        !stripped.windows(6).any(|window| window == b"Exif\0\0"),
        "前提条件が崩れている: Exif APP1 が残っている"
    );
    // 前提条件2: XMP パケットは残っている（読めるものが何も無いなら、
    // 「XMP が読めた」という下の検査は成立しない）
    assert!(
        stripped
            .windows(XMP_NAMESPACE.len())
            .any(|window| window == XMP_NAMESPACE),
        "前提条件が崩れている: XMP パケットまで消えている"
    );

    let read = read_metadata(&path).unwrap();
    assert!(
        read.warnings.is_empty(),
        "Exif を持たない JPEG に警告を出してはいけない: {:?}",
        read.warnings
    );
    // XMP 側の読み取りは機能している
    assert_eq!(read.metadata.title.as_deref(), Some("夕暮れの港"));
    assert_eq!(read.metadata.rating, Some(3));
}

/// XMP パケットを見分ける APP1 の識別子
const XMP_NAMESPACE: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";
