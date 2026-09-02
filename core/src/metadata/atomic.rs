//! 書き込みの安全性。**どの段階で失敗しても元写真は無傷**であること。
//!
//! 1. 同一ディレクトリに一時ファイルとして書く（`rename` が原子的であるために
//!    同じファイルシステム上にいる必要がある）
//! 2. **読み戻して検証する** ── 書いた値が正しく読めるか、既存 Exif が壊れていないか、
//!    画素が 1 バイトも変わっていないか
//! 3. 検証を通ったときだけ `rename` で置き換える
//!
//! 手順 2 を挟むのは、書いた結果が壊れていた場合に元ファイルを置き換えないため。
//! 失敗しても残るのは一時ファイルだけで、それも [`TempFile`] の `Drop` が片付ける。

use super::EditableMetadata;
use anyhow::{anyhow, bail, Context, Result};
use std::path::{Path, PathBuf};

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
    let temp = TempFile(dir.join(format!(".{}.{}.picture-tool.tmp", name, std::process::id())));

    std::fs::write(&temp.0, &patched)
        .with_context(|| format!("一時ファイルを書けません: {}", temp.0.display()))?;
    // 元の権限を引き継ぐ。落とすと「作った人しか読めない」写真になる
    if let Ok(meta) = std::fs::metadata(path) {
        let _ = std::fs::set_permissions(&temp.0, meta.permissions());
    }
    verify(&temp.0, path, expected)?;

    // 同一ディレクトリなので rename は原子的
    std::fs::rename(&temp.0, path).with_context(|| {
        format!(
            "置き換えに失敗しました: {} -> {}",
            temp.0.display(),
            path.display()
        )
    })?;
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
    let written = super::read_metadata(temp)?.metadata;
    if written != *expected {
        bail!(
            "書き込んだ値を読み戻せませんでした（期待 {:?} / 実際 {:?}）",
            expected,
            written
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
    use img_parts::jpeg::{markers, Jpeg};
    use img_parts::{Bytes, ImageEXIF};

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
    /// 独立実装（kamadak-exif）で撮影情報を突き合わせて初めて見える
    #[test]
    fn replace_verified_refuses_a_buffer_that_lost_the_existing_exif() {
        let (dir, path, original) = scratch();
        // 前提条件: フィクスチャは撮影情報を持っている
        assert!(crate::read_exif_info(&path).unwrap().camera_make.is_some());

        let mut jpeg = Jpeg::from_bytes(Bytes::from(original.clone())).unwrap();
        jpeg.set_exif(None);
        // 前提条件: Exif の APP1 だけが消え、画素は動いていない
        assert!(!jpeg
            .segments_by_marker(markers::APP1)
            .any(|s| s.contents().starts_with(b"Exif\0\0")));
        let stripped = jpeg.encoder().bytes().to_vec();
        assert_eq!(
            stripped[scan_offset(&stripped).unwrap()..],
            original[scan_offset(&original).unwrap()..]
        );

        let error =
            replace_verified(&path, &original, stripped, &EditableMetadata::default()).unwrap_err();
        assert!(
            error.to_string().contains("既存の Exif が壊れました"),
            "{error:#}"
        );
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
