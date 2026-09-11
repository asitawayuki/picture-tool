//! 実写真での round-trip 実測。
//!
//! spec 2026-08-18「未検証（実装時に必ず潰すこと）」への回答であり、
//! 将来クレートを上げたときの再検査にも使う。**通常のテスト実行では走らない**
//! （実写真をそのままリポジトリに入れないため）。
//!
//! MakerNote 付きの JPEG が手に入ったら、同じ変数へ渡して再検査する
//! （本ツールの編集対象は Lightroom 書き出しなので常用の経路ではないが、
//! `little_exif` の issue #93 が残っている以上、掴めるときに掴んでおく価値がある）。
//!
//! 実行:
//!   PICTURE_TOOL_SPIKE_JPEG=/path/to/DSC00123.JPG \
//!     cargo test -p picture-tool-core --test metadata_spike -- --ignored --nocapture

use little_exif::exif_tag::ExifTag;
use little_exif::filetype::FileExtension;
use little_exif::ifd::ExifTagGroup;
use little_exif::metadata::Metadata;

const MAKER_NOTE: u16 = 0x927c;
const GPS_IFD: u16 = 0x8825;
const XP_TITLE: u16 = 0x9c9b;

fn spike_source() -> std::path::PathBuf {
    let raw = std::env::var("PICTURE_TOOL_SPIKE_JPEG")
        .expect("PICTURE_TOOL_SPIKE_JPEG に検証用 JPEG の絶対パスを渡すこと");
    std::path::PathBuf::from(raw)
}

/// SOS（`FF DA`）の開始位置。ここから EOI までが実際の画素データ。
///
/// **同じ計算が Task 9 の `core/src/metadata/atomic.rs` にも要る。** ここに写しを置くのは、
/// この実測が本実装より先に走るため。本実装ができたらこちらは触らない
/// （クレート更新時の再検査を、本実装から独立させておく価値がある）。
fn scan_start(bytes: &[u8]) -> usize {
    let mut i = 2; // SOI をとばす
    loop {
        assert!(i + 1 < bytes.len(), "SOS が見つからない");
        if bytes[i] != 0xFF {
            panic!("セグメント境界が壊れている: offset {}", i);
        }
        // 0xFF のパディング（fill byte）を読み飛ばす
        let mut m = i + 1;
        while bytes[m] == 0xFF {
            m += 1;
        }
        let marker = bytes[m];
        if marker == 0xDA {
            return i;
        }
        let len = u16::from_be_bytes([bytes[m + 1], bytes[m + 2]]) as usize;
        i = m + 1 + len;
    }
}

fn baseline_fields_is_empty(bytes: &[u8]) -> bool {
    exif_fields(bytes).is_empty()
}

/// kamadak-exif で全フィールドを列挙する。**little_exif とは独立の実装**なので、
/// 「little_exif で書いて little_exif で読めた」の同語反復にならない。
///
/// `In(1)/JPEGInterchangeFormat`（IFD1 のサムネイル JPEG 本体への相対オフセット）だけは
/// 比較から除外する。IFD0 にタグが 1 つ増えれば Exif セグメント全体が伸び、その分だけ
/// 後方の IFD1 サムネイルは物理的に後ろへ移動するので、**このタグの値が変わるのは
/// little_exif の欠陥ではなく定義上正しい挙動**（実測: 1030 → 1056 も、サムネイル本体の
/// SHA256 が完全一致した状態でのシフトだった）。消失・切り詰めの検知はこの等値比較ではなく
/// `thumbnail_bytes()` によるサムネイル本体のバイト列比較で行う。対になる
/// `JPEGInterchangeFormatLength`（サムネイルの長さ）はここでは除外せず比較に残す
/// （長さが変われば切り詰められたということなので、それは検知したい）。
fn exif_fields(bytes: &[u8]) -> Vec<(String, String)> {
    let mut cursor = std::io::Cursor::new(bytes);
    let reader = exif::Reader::new()
        .read_from_container(&mut cursor)
        .expect("EXIF が読めない");
    let mut fields: Vec<(String, String)> = reader
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

/// IFD1 のサムネイル JPEG 本体を切り出す。サムネイルを持たない画像（Task 2 で作るフィクスチャ
/// を含む）では `None` を返す。
///
/// オフセット・長さはハードコードせず、渡された `bytes` 自身から kamadak-exif で都度読み直す
/// （書き込み後の実際の値を見ることで、little_exif がオフセットを正しく追従させているか
/// 自体を検査対象にするため）。オフセットは Exif セグメント内の `Exif\0\0` マーカー直後、
/// すなわち TIFF ヘッダの先頭からの相対値。
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
    // オフセット計算そのものが正しいことの検査。ずれていればここで SOI/EOI が見えないはず。
    assert!(
        thumb.starts_with(&[0xFF, 0xD8]) && thumb.ends_with(&[0xFF, 0xD9]),
        "切り出したサムネイルが JPEG として壊れている（offset={offset}, length={length}）"
    );
    Some(thumb.to_vec())
}

#[test]
#[ignore = "実写真が要る。PICTURE_TOOL_SPIKE_JPEG で渡す"]
fn a_real_photo_survives_two_write_cycles() {
    let source = spike_source();
    let original = std::fs::read(&source).expect("読めない");

    // --- この画像が何を持っているか（assert ではなく記録） ---
    let meta = Metadata::new_from_path(&source).expect("EXIF が読めない");
    let has_maker_note = meta.get_tag_by_hex(MAKER_NOTE, None).next().is_some();
    let has_gps = meta
        .get_tag_by_hex(GPS_IFD, Some(ExifTagGroup::GENERIC))
        .next()
        .is_some()
        || meta.get_ifd(ExifTagGroup::GPS, 0).is_some();
    // **MakerNote の有無は assert しない。** 本ツールの編集対象は Lightroom 書き出しで、
    // そこに MakerNote は入っていない。有無を出力に残し、実施メモへ書き写す
    // （MakerNote 付きで通った実績があるかどうかが、残リスクの評価そのものになる）
    println!("MakerNote: {} / GPS: {}", has_maker_note, has_gps);
    assert!(
        !baseline_fields_is_empty(&original),
        "Exif が 1 つも読めない。編集対象として不適切な画像"
    );

    let baseline_fields = exif_fields(&original);
    let baseline_scan = &original[scan_start(&original)..];
    let baseline_thumbnail = thumbnail_bytes(&original);
    println!("Exif フィールド数: {}", baseline_fields.len());
    println!("サムネイル本体: {}", baseline_thumbnail.is_some());

    // --- 1 回目 ---
    let mut buffer = original.clone();
    let mut meta = Metadata::new_from_vec(&buffer, FileExtension::JPEG).expect("読めない");
    meta.set_tag(ExifTag::UnknownINT8U(
        vec![0x41, 0x00, 0x00, 0x00], // "A" を UCS-2LE + NUL 終端で
        XP_TITLE,
        ExifTagGroup::GENERIC,
    ));
    meta.write_to_vec(&mut buffer, FileExtension::JPEG)
        .expect("1 回目の書き込みが失敗した（issue #93 の疑い）");

    assert_eq!(
        &buffer[scan_start(&buffer)..],
        baseline_scan,
        "1 回目でピクセルが変わった"
    );

    // --- 2 回目（自分が書いたものを読み直して書く） ---
    let mut second = buffer.clone();
    let mut meta = Metadata::new_from_vec(&second, FileExtension::JPEG).expect("読めない");
    meta.set_tag(ExifTag::UnknownINT8U(
        vec![0x42, 0x00, 0x00, 0x00],
        XP_TITLE,
        ExifTagGroup::GENERIC,
    ));
    meta.write_to_vec(&mut second, FileExtension::JPEG)
        .expect("2 回目の書き込みが失敗した（issue #93 の疑い）");

    assert_eq!(
        &second[scan_start(&second)..],
        baseline_scan,
        "2 回目でピクセルが変わった"
    );

    // --- 既存タグが消えていないか（追加した XPTitle 以外に差分が無いこと） ---
    let after_fields = exif_fields(&second);
    let lost: Vec<&(String, String)> = baseline_fields
        .iter()
        .filter(|f| !after_fields.contains(f))
        .collect();
    assert!(lost.is_empty(), "消えた・変わったタグがある: {:#?}", lost);

    println!(
        "タグ数 {} → {}（増分は XPTitle のみのはず）",
        baseline_fields.len(),
        after_fields.len()
    );

    // --- サムネイル本体のバイト列一致（オフセット等値比較より強い検査） ---
    // `exif_fields` の比較からは JPEGInterchangeFormat（オフセット）を除外したので、代わりに
    // ここでサムネイル本体そのものが書き込み前後で 1 バイトも変わっていないことを確認する。
    // `None == None`（サムネイルを持たない画像）は許容するが、`Some` から `None`
    // （サムネイル消失）は assert_eq! の不一致として必ず検知される。
    let after_thumbnail = thumbnail_bytes(&second);
    assert!(
        baseline_thumbnail.is_some(),
        "この写真には埋め込みサムネイルが必要（比較が空振りしていないことの前提条件）"
    );
    assert_eq!(
        baseline_thumbnail, after_thumbnail,
        "サムネイル本体が書き込み前後で変わった（消失または内容の変化）"
    );
    println!("サムネイル本体一致: {}", baseline_thumbnail.is_some());
}

use img_parts::Bytes;
use img_parts::jpeg::{Jpeg, markers};

fn fixture_path(name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// 64x80 の合成 JPEG。画素は何でもよい ── 守りたいのは移植する EXIF の中身。
fn tiny_jpeg() -> Vec<u8> {
    let img = image::RgbImage::from_fn(64, 80, |x, y| {
        image::Rgb([(x * 4) as u8, (y * 3) as u8, 128])
    });
    let mut buffer = Vec::new();
    img.write_with_encoder(image::codecs::jpeg::JpegEncoder::new_with_quality(
        &mut buffer,
        92,
    ))
    .unwrap();
    buffer
}

#[test]
#[ignore = "フィクスチャの生成。PICTURE_TOOL_SPIKE_JPEG で実写真を渡す"]
fn generate_exif_only_fixture() {
    let source = spike_source();
    let src = Jpeg::from_bytes(Bytes::from(std::fs::read(&source).unwrap())).unwrap();

    let exif_app1 = src
        .segments_by_marker(markers::APP1)
        .find(|s| s.contents().starts_with(b"Exif\0\0"))
        .expect("EXIF の APP1 が無い")
        .clone();

    let mut out = Jpeg::from_bytes(Bytes::from(tiny_jpeg())).unwrap();
    // APP0(JFIF) の直後に置く。**末尾に push すると EOI の後ろに行き、
    // 再パース時に消える**（spec「img-parts の落とし穴」）
    out.segments_mut().insert(1, exif_app1);

    let bytes = out.encoder().bytes();
    std::fs::write(fixture_path("exif-only.jpg"), &bytes).unwrap();
    println!("exif-only.jpg: {} bytes", bytes.len());

    // 移植できたことを、独立の実装（kamadak-exif）で確かめる
    let fields = exif_fields(&bytes);
    println!("フィールド数: {}", fields.len());
    for (name, value) in &fields {
        println!("  {} = {}", name, value);
    }
}

#[test]
#[ignore = "フィクスチャの生成。PICTURE_TOOL_LIGHTROOM_JPEG で Lightroom 書き出しを渡す"]
fn generate_lightroom_xmp_fixture() {
    let raw = std::env::var("PICTURE_TOOL_LIGHTROOM_JPEG")
        .expect("PICTURE_TOOL_LIGHTROOM_JPEG に Lightroom 書き出し JPEG のパスを渡すこと");
    let src = Jpeg::from_bytes(Bytes::from(std::fs::read(&raw).unwrap())).unwrap();

    let app1: Vec<_> = src
        .segments_by_marker(markers::APP1)
        .filter(|s| {
            s.contents().starts_with(b"Exif\0\0")
                || s.contents().starts_with(b"http://ns.adobe.com/xap/1.0/\0")
        })
        .cloned()
        .collect();
    assert_eq!(app1.len(), 2, "EXIF と XMP の APP1 が両方あること");

    let mut out = Jpeg::from_bytes(Bytes::from(tiny_jpeg())).unwrap();
    for (i, seg) in app1.into_iter().enumerate() {
        out.segments_mut().insert(1 + i, seg);
    }
    let bytes = out.encoder().bytes();
    std::fs::write(fixture_path("lightroom-xmp.jpg"), &bytes).unwrap();
    println!("lightroom-xmp.jpg: {} bytes", bytes.len());
}

#[test]
#[ignore = "実測。PICTURE_TOOL_SPIKE_FOLDER に写真フォルダーを渡す"]
fn measure_read_metadata_cost() {
    let folder = std::env::var("PICTURE_TOOL_SPIKE_FOLDER")
        .expect("PICTURE_TOOL_SPIKE_FOLDER に写真フォルダーのパスを渡すこと");
    let files: Vec<_> = std::fs::read_dir(&folder)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("jpg") || e.eq_ignore_ascii_case("jpeg"))
        })
        .take(60)
        .collect();
    assert!(!files.is_empty(), "JPEG が 1 枚も無い: {}", folder);

    let started = std::time::Instant::now();
    for file in &files {
        let _ = picture_tool_core::metadata::read_metadata(file);
    }
    let elapsed = started.elapsed();
    println!(
        "{} 枚 / {:?} → 1 枚あたり {:?}",
        files.len(),
        elapsed,
        elapsed / files.len() as u32
    );
}
