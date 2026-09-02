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
