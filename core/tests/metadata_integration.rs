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

/// **既存タグの保存** — 追加した項目以外のタグ集合に差分が無いこと
#[test]
fn writing_metadata_keeps_every_existing_exif_tag() {
    let (_dir, path) = scratch("exif-only.jpg");
    let before = exif_fields(&path);
    // 前提条件: 実写真由来の Exif が本当に入っていること。
    // ここが空だと「タグが消えていない」は自明に通ってしまう
    assert!(
        before.len() > 10 && before.iter().any(|(name, _)| name.contains("Model")),
        "フィクスチャの Exif が薄すぎる。Task 2 を見直すこと: {:?}",
        before
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
    assert!(
        format!("{error:#}").contains("Exif"),
        "理由が伝わる文面であること: {error:#}"
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
/// （Exif APP1 を持たない画像には新しい Exif を作ってよい）を通す。
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
