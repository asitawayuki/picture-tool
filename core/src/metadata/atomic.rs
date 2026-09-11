//! 書き込みの安全性。**どの段階で失敗しても元写真は無傷**であること。
//!
//! 1. 同一ディレクトリに一時ファイルとして書き、`sync_all()` でディスクへ永続化する
//!    （同じファイルシステム上にいる必要があるのは、後段の `rename` が原子的であるため）
//! 2. **読み戻して検証する** ── 書いた値が正しく読めるか、EXIF 側と XMP 側が食い違って
//!    いないか、既存 Exif が壊れていないか、画素が 1 バイトも変わっていないか
//! 3. 検証を通ったときだけ `rename` で置き換える
//!
//! 手順 2 を挟むのは、書いた結果が壊れていた場合に元ファイルを置き換えないため。
//! 失敗しても残るのは一時ファイルだけで、それも [`TempFile`] の `Drop` が片付ける。
//!
//! **`rename` が原子的なのは、同時に読む側から見て「古い内容」か「新しい内容」の
//! どちらかしか観測できないという意味であって、クラッシュ・電源断に対する耐性では
//! ない。** 後者は一時ファイル自体の `sync_all()` が担う ── これが無いと、
//! 遅延割り当て下では rename によるディレクトリエントリの更新だけが先に永続化され、
//! 長さ 0 または途中までしか書かれていない内容で元写真が置き換わりうる。

use super::EditableMetadata;
use anyhow::{Context, Result, anyhow, bail};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// SOS（`FF DA`）の開始位置。ここから EOI までが実際の画素データ。
///
/// 本ツールは APP セグメントしか触らないので、ここから後ろは**必ず**不変のはずである。
/// それを機械的に確かめるための位置。
///
/// **入力は webview 由来のパスから読んだバイト列で、壊れていることも切り詰められている
/// こともある。** 添字は必ず長さで押さえてから触り、境界外で panic しない
/// （spike 版の `scan_start` は `assert!` と生の添字で書いてあり、ここでは踏襲しない）。
pub(super) fn scan_offset(bytes: &[u8]) -> Result<usize> {
    let mut cursor = 2; // SOI をとばす
    loop {
        if cursor + 1 >= bytes.len() {
            bail!("SOS が見つかりません（JPEG が途中で切れている可能性があります）");
        }
        if bytes[cursor] != 0xFF {
            bail!("JPEG のセグメント境界が壊れています（offset {}）", cursor);
        }
        // 0xFF のパディング（fill byte）を読み飛ばす
        let mut marker_at = cursor + 1;
        while marker_at < bytes.len() && bytes[marker_at] == 0xFF {
            marker_at += 1;
        }
        // マーカー本体と、それに続く 2 バイトの長さまで読めることを確かめる
        if marker_at + 2 >= bytes.len() {
            bail!("SOS が見つかりません（JPEG が途中で切れている可能性があります）");
        }
        if bytes[marker_at] == 0xDA {
            return Ok(cursor);
        }
        let length = u16::from_be_bytes([bytes[marker_at + 1], bytes[marker_at + 2]]) as usize;
        // `marker_at >= cursor + 1` なので cursor は必ず 2 以上進む（無限ループにならない）
        cursor = marker_at + 1 + length;
    }
}

/// プロセス内で一時ファイル名を一意にするためのカウンタ。
///
/// PID だけでは**他プロセスとの**衝突は防げても、**同一プロセス内**の並行書き込み
/// （`write_image_metadata` が `spawn_blocking` 上で走ることで起きうる、同じ写真への
/// 同時書き込み）は防げない。同じ temp パスを 2 スレッドが使うと、書き込み → verify →
/// rename の間に別スレッドが同じ temp を書き直し、検証済みでないバイト列が元写真の
/// 上に rename されうる（TOCTOU）。このカウンタが両方の軸で一意性を作る。
static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

/// 一時ファイルのパスを組み立てる。`dir` と `name` は保ったまま、
/// プレフィックス `.` とサフィックス `.picture-tool.tmp` の間に PID とカウンタを挟む。
///
/// **呼ぶたびに異なるパスを返す**（同一プロセス内でも）。
fn temp_path(dir: &Path, name: &str) -> PathBuf {
    let seq = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
    dir.join(format!(
        ".{}.{}.{}.picture-tool.tmp",
        name,
        std::process::id(),
        seq
    ))
}

/// 検証付きの原子的置換
pub(super) fn replace_verified(
    path: &Path,
    original: &[u8],
    patched: Vec<u8>,
    expected: &EditableMetadata,
) -> Result<()> {
    // --- ピクセル不変（設計の必須条件） ---
    if original[scan_offset(original)?..] != patched[scan_offset(&patched)?..] {
        bail!("画素データが変化したため書き込みを中止しました");
    }

    let dir = path
        .parent()
        .ok_or_else(|| anyhow!("親フォルダーがありません: {}", path.display()))?;
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| anyhow!("ファイル名を解釈できません: {}", path.display()))?;
    let temp = TempFile(temp_path(dir, name));

    // **`fs::write` ではなく `File::create` + `write_all` + `sync_all`。**
    // `verify` の読み戻しはページキャッシュ経由なので、それだけではディスクへの
    // 永続化を保証しない。`sync_all` を欠くと、この直後にクラッシュ・電源断が
    // 起きたとき rename 後の写真が長さ 0 または途中までの内容になりうる
    // （モジュール冒頭のコメント参照）。
    {
        let mut file = std::fs::File::create(&temp.0)
            .with_context(|| format!("一時ファイルを作れません: {}", temp.0.display()))?;
        file.write_all(&patched)
            .with_context(|| format!("一時ファイルを書けません: {}", temp.0.display()))?;
        // 元の権限を引き継ぐ。落とすと「作った人しか読めない」写真になる。
        // **`sync_all` より前に置く** ── `sync_all` は内容と inode の両方を
        // 永続化するので、後ろに置くと権限の変更だけがディスクに届かないまま
        // rename されうる（内容は無事だが、クラッシュ直後に権限が落ちる）
        if let Ok(meta) = std::fs::metadata(path) {
            let _ = std::fs::set_permissions(&temp.0, meta.permissions());
        }
        file.sync_all()
            .with_context(|| format!("一時ファイルを永続化できません: {}", temp.0.display()))?;
    }
    verify(&temp.0, path, expected)?;

    // 同一ディレクトリなので rename は同時に読む側に対しては原子的
    // （古い内容と新しい内容が混ざった状態を外から観測できない）。
    // クラッシュ耐性はここではなく、直前の `sync_all` が担っている。
    std::fs::rename(&temp.0, path).with_context(|| {
        format!(
            "置き換えに失敗しました: {} -> {}",
            temp.0.display(),
            path.display()
        )
    })?;

    // ディレクトリエントリ（rename によるリンクの張り替え）自体の永続化は
    // best-effort にとどめる。**理由**: Windows はディレクトリを `File` として
    // 開けず、`File::open(dir)` の時点で失敗する。ここは「握りつぶしていい」
    // という判断であり、`let _` で失敗そのものが見えなくなる点は認識した上で
    // 受け入れている（`set_permissions` の `let _` のような「理由の説明が無い
    // 握りつぶし」とは区別する）。ファイル本体は既に `sync_all` 済みなので、
    // 最悪ケースでも「rename の反映がまだ見えない」だけで元写真が壊れることはない。
    if let Ok(dir_handle) = std::fs::File::open(dir) {
        let _ = dir_handle.sync_all();
    }

    Ok(())
}

/// 一時ファイルの後始末。
///
/// **`?` による早期脱出でも、`little_exif` の panic による巻き戻しでも消える。**
/// 手で `remove_file` を呼ぶ形にすると、経路が増えたときに 1 箇所落として
/// 残骸が出る ── 残骸は次回の書き込みと衝突しうる。
struct TempFile(PathBuf);

impl Drop for TempFile {
    /// 置き換えに成功していれば `rename` でこのパスは消えているので、
    /// ここでの削除は失敗する。**それで正しい**（成功時に消すものは無い）
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// 書いた結果を読み戻して確かめる
fn verify(temp: &Path, original: &Path, expected: &EditableMetadata) -> Result<()> {
    let read = super::read_metadata(temp)?;
    if read.metadata != *expected {
        bail!(
            "書き込んだ値を読み戻せませんでした（期待 {:?} / 実際 {:?}）",
            expected,
            read.metadata
        );
    }

    // **`write_metadata` は EXIF 側と XMP 側へ同じ値を書く契約なので、自分が書いた
    // 直後のファイルで食い違い警告が出ることは定義上あってはならない。** EXIF 側は
    // little_exif、XMP 側は自前の文字列組み立てという独立した2経路で書いているため、
    // 上の `read.metadata != *expected` は `pick`（食い違えば XMP を優先）が期待値を
    // 返す限り通ってしまい、片側だけの書き込み失敗や古い値の残存を見逃す。
    // ここで拾う。EXIF 実装と XMP 実装の相互照合になるので、同一パーサで自分の
    // 出力を読む同語反復ではない。
    if !read.warnings.is_empty() {
        bail!(
            "書き込み直後の読み戻しで食い違い警告が出ました（EXIF 側と XMP 側が揃っていません）: {:?}",
            read.warnings
        );
    }

    // 撮影情報が壊れていないこと。**独立の実装（kamadak-exif）で見る**ので、
    // 「little_exif で書いて little_exif で読めた」の同語反復にならない
    let before = crate::read_exif_info(original)?;
    let after = crate::read_exif_info(temp)?;
    if before != after {
        bail!(
            "既存の Exif が壊れました（期待 {:?} / 実際 {:?}）",
            before,
            after
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use img_parts::Bytes;
    use img_parts::jpeg::Jpeg;
    use little_exif::filetype::FileExtension;
    use little_exif::metadata::Metadata;

    /// **同一プロセス内で 2 回呼ぶと必ず異なるパスになる。**
    ///
    /// `write_image_metadata` が `spawn_blocking` 上で走ると、同じ写真への
    /// `write_metadata` が同一プロセス内で並行しうる。PID だけの名前だと
    /// 2 スレッドが同じ temp パスを使い、検証済みでないバイト列が rename されうる
    /// （TOCTOU）。`dir` と `name`、プレフィックス `.` とサフィックス
    /// `.picture-tool.tmp` は保たれることも見る。
    ///
    /// この検査は逐次 2 回の呼び出しで足りる。`AtomicU64::fetch_add` は並行に
    /// 呼んでも同じ値を 2 度返さない（呼び出しごとに異なる値を返すことが
    /// 保証されている）ため、並行呼び出しを足しても検証力はほぼ増えない。
    #[test]
    fn temp_path_is_unique_within_the_same_process() {
        let dir = PathBuf::from("/tmp/picture-tool-test-dir");
        let name = "photo.jpg";

        let first = temp_path(&dir, name);
        let second = temp_path(&dir, name);

        assert_ne!(first, second, "同一プロセス内で同じパスが出た");
        for path in [&first, &second] {
            assert_eq!(path.parent(), Some(dir.as_path()), "dir が保たれていない");
            let file_name = path.file_name().unwrap().to_str().unwrap();
            assert!(
                file_name.starts_with(&format!(".{}.", name)),
                "プレフィックスが無い: {}",
                file_name
            );
            assert!(
                file_name.ends_with(".picture-tool.tmp"),
                "サフィックスが無い: {}",
                file_name
            );
        }
    }

    /// テンポラリに複製したフィクスチャ。元のフィクスチャは書き換えない
    fn scratch() -> (tempfile::TempDir, PathBuf, Vec<u8>) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("exif-only.jpg");
        std::fs::copy(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/exif-only.jpg"),
            &path,
        )
        .unwrap();
        let original = std::fs::read(&path).unwrap();
        (dir, path, original)
    }

    /// 関門が拒んだあと、**元ファイルが 1 バイトも変わっておらず、一時ファイルも
    /// 残っていない**こと。3 つの関門で同じことを確かめる
    fn assert_untouched(dir: &tempfile::TempDir, path: &Path, original: &[u8]) {
        assert_eq!(
            std::fs::read(path).unwrap(),
            original,
            "元ファイルが変わった"
        );
        let leftovers: Vec<String> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name != "exif-only.jpg")
            .collect();
        assert!(leftovers.is_empty(), "残骸: {:?}", leftovers);
    }

    /// **ピクセル不変の関門そのものを動かす。**
    ///
    /// `write_metadata` 経由のピクセル不変テストは「実装が実際には画素を触らない」
    /// ことしか見ておらず、この `if` を丸ごと外しても緑のままになる（実測済み）。
    /// 関門は他所が壊れたときにだけ火を噴くので、直接ここで火を点ける
    #[test]
    fn replace_verified_refuses_a_buffer_whose_scan_changed() {
        let (dir, path, original) = scratch();
        let scan = scan_offset(&original).unwrap();
        let mut tampered = original.clone();
        // 前提条件: 変えるのは SOS 以降（＝画素データ）であること
        let at = scan + 8;
        assert!(at > scan && at < tampered.len());
        tampered[at] ^= 0xFF;
        assert_eq!(
            scan_offset(&tampered).unwrap(),
            scan,
            "前提条件: SOS の位置は動いていない"
        );

        let error =
            replace_verified(&path, &original, tampered, &EditableMetadata::default()).unwrap_err();
        assert!(
            error.to_string().contains("画素データが変化した"),
            "{error:#}"
        );
        assert_untouched(&dir, &path, &original);
    }

    /// **読み戻し検証の関門そのものを動かす。**
    /// 書いたはずの値が読み戻せないバイト列は、置き換えずに Err で返る
    #[test]
    fn replace_verified_refuses_a_buffer_whose_values_do_not_read_back() {
        let (dir, path, original) = scratch();
        // 前提条件: このフィクスチャは★を持っていない（＝期待値と食い違う）
        assert_eq!(
            super::super::read_metadata(&path).unwrap().metadata.rating,
            None
        );

        let expected = EditableMetadata {
            rating: Some(5),
            ..Default::default()
        };
        let error = replace_verified(&path, &original, original.clone(), &expected).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("書き込んだ値を読み戻せませんでした"),
            "{error:#}"
        );
        assert_untouched(&dir, &path, &original);
    }

    /// **既存 Exif が消えたら置き換えない。**
    /// little_exif が Exif APP1 を丸ごと落とす事故（spec の issue #93）は、
    /// 「本ツールの 3 タグは読み戻せる」ので値の一致だけでは捕まらない。
    /// 独立実装（kamadak-exif）で撮影情報を突き合わせて初めて見える。
    ///
    /// **空の `Metadata` で書き直す（issue #93 そのものの手口）ことで、この buffer は
    /// little_exif 自身は問題なく読める（＝ K2 の食い違い警告 gate では捕まらない）**
    /// ようにしてある。`jpeg.set_exif(None)`（APP1 ごと削除）では little_exif の
    /// パースそのものが失敗して K2 が先に捕まえてしまい、この gate（kamadak-exif との
    /// 突き合わせ）を単独で動かせなくなる（実測済み）
    #[test]
    fn replace_verified_refuses_a_buffer_that_lost_the_existing_exif() {
        let (dir, path, original) = scratch();
        // 前提条件: フィクスチャは撮影情報を持っている
        assert!(crate::read_exif_info(&path).unwrap().camera_make.is_some());

        let mut buffer = original.clone();
        Metadata::new()
            .write_to_vec(&mut buffer, FileExtension::JPEG)
            .unwrap();
        // 前提条件: little_exif 自身はこの buffer を問題なく読める
        // （＝ K2 の食い違い警告は出ない。exif-only.jpg は XMP を持たないので
        // 候補は EXIF 側だけになり、空の Metadata でも食い違いようが無い）
        let probe_dir = tempfile::tempdir().unwrap();
        let probe_path = probe_dir.path().join("probe.jpg");
        std::fs::write(&probe_path, &buffer).unwrap();
        let probe = super::super::read_metadata(&probe_path).unwrap();
        assert!(
            probe.warnings.is_empty(),
            "前提条件が崩れている: {:?}",
            probe.warnings
        );
        // 前提条件: 画素は動いていない
        assert_eq!(
            buffer[scan_offset(&buffer).unwrap()..],
            original[scan_offset(&original).unwrap()..]
        );

        let error =
            replace_verified(&path, &original, buffer, &EditableMetadata::default()).unwrap_err();
        assert!(
            error.to_string().contains("既存の Exif が壊れました"),
            "{error:#}"
        );
        assert_untouched(&dir, &path, &original);
    }

    /// **K2: 読み戻し直後の食い違い警告も関門になる。**
    ///
    /// EXIF 側と XMP 側は独立した2経路で書かれるため、`build` にバグがあって片側だけ
    /// 古い値が残っても、`pick`（食い違えば XMP を優先）が期待値さえ返せば値一致
    /// チェック（gate 2 の前半）は素通りする。ここは gate 2 の後半 ── その素通りを
    /// 捕まえられることを、`build` を経由せず直接組み立てた buffer で確かめる
    #[test]
    fn replace_verified_refuses_a_buffer_whose_exif_and_xmp_disagree() {
        let (dir, path, original) = scratch();

        let stale = EditableMetadata {
            title: Some("old".into()),
            ..Default::default()
        };
        let fresh = EditableMetadata {
            title: Some("new".into()),
            ..Default::default()
        };

        // EXIF 側だけ古い値のまま、XMP 側だけ最新値、という「片側だけ書き損じた」
        // buffer を `build` を経由せず直接組み立てる
        let mut buffer = original.clone();
        let mut meta = Metadata::new_from_vec(&buffer, FileExtension::JPEG).unwrap();
        super::super::exif::apply(&mut meta, &stale);
        meta.write_to_vec(&mut buffer, FileExtension::JPEG).unwrap();
        let mut jpeg = Jpeg::from_bytes(Bytes::from(buffer)).unwrap();
        let packet = super::super::xmp::new_packet(&fresh);
        super::super::xmp::install(&mut jpeg, &packet).unwrap();
        let patched = jpeg.encoder().bytes().to_vec();

        // 前提条件: 値だけ見れば期待どおり（XMP 優先で新値）だが、食い違い警告が
        // 出る ── これが K2 の対象そのものであること
        let probe_dir = tempfile::tempdir().unwrap();
        let probe_path = probe_dir.path().join("probe.jpg");
        std::fs::write(&probe_path, &patched).unwrap();
        let probe = super::super::read_metadata(&probe_path).unwrap();
        assert_eq!(
            probe.metadata.title.as_deref(),
            Some("new"),
            "前提条件が崩れている"
        );
        assert!(
            !probe.warnings.is_empty(),
            "前提条件が崩れている: 食い違い警告が出ていない"
        );

        let error = replace_verified(&path, &original, patched, &fresh).unwrap_err();
        assert!(error.to_string().contains("食い違い警告"), "{error:#}");
        assert_untouched(&dir, &path, &original);
    }

    /// SOI(2) + APP0 長さ 4 + SOS
    #[test]
    fn scan_offset_finds_the_start_of_the_scan() {
        let bytes = [
            0xFF, 0xD8, // SOI
            0xFF, 0xE0, 0x00, 0x04, 0x11, 0x22, // APP0（長さ 4 = 自身 2 + 中身 2）
            0xFF, 0xDA, 0x00, 0x02, // SOS
            0x99, 0x99,
        ];
        assert_eq!(scan_offset(&bytes).unwrap(), 8);
    }

    /// 0xFF のパディング（fill byte）が挟まっても迷子にならない
    #[test]
    fn scan_offset_skips_fill_bytes() {
        let bytes = [
            0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0x11, 0x22, 0xFF, 0xFF, 0xFF, 0xDA, 0x00, 0x02,
        ];
        assert_eq!(scan_offset(&bytes).unwrap(), 8);
    }

    #[test]
    fn scan_offset_reports_a_truncated_file_instead_of_panicking() {
        assert!(scan_offset(&[0xFF, 0xD8, 0xFF, 0xE0]).is_err());
    }

    /// **壊れた入力で境界外に触らないこと。** ここは webview から渡されたパスの
    /// 中身を読む場所で、切り詰められた JPEG も破損したセグメントも普通に来る
    #[test]
    fn scan_offset_never_panics_on_malformed_input() {
        let cases: Vec<Vec<u8>> = vec![
            vec![],
            vec![0xFF],
            vec![0xFF, 0xD8],
            // 長さが 0 のセグメントが続いても止まる（無限ループにならない）
            vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x00, 0xFF, 0xE1, 0x00, 0x00],
            // 長さがファイル末尾を飛び越える
            vec![0xFF, 0xD8, 0xFF, 0xE0, 0xFF, 0xFF, 0x11],
            // 0xFF が延々と続く（fill byte のループが末尾まで走る）
            vec![0xFF; 32],
            // セグメント境界が 0xFF で始まらない
            vec![0xFF, 0xD8, 0x00, 0x00, 0x00],
        ];
        for bytes in cases {
            assert!(
                scan_offset(&bytes).is_err(),
                "壊れた入力を Ok にした: {:?}",
                bytes
            );
        }
    }

    /// 実データのあらゆる切り詰めで panic しないこと。
    /// **SOS より手前で切れていれば必ず Err、SOS まで届いていれば必ず同じ位置を返す**
    #[test]
    fn scan_offset_survives_every_truncation_of_a_real_jpeg() {
        let bytes = std::fs::read(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/exif-only.jpg"),
        )
        .unwrap();
        let whole = scan_offset(&bytes).unwrap();
        // 前提条件: フィクスチャの SOS はファイル末尾ではなく途中にある
        assert!(
            whole > 2 && whole < bytes.len() - 4,
            "SOS の位置: {}",
            whole
        );

        for cut in 0..bytes.len() {
            match scan_offset(&bytes[..cut]) {
                Ok(found) => assert_eq!(found, whole, "{} バイトで切ったとき", cut),
                Err(_) => assert!(cut <= whole + 3, "{} バイトで切ったとき", cut),
            }
        }
    }
}
