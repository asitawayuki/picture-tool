//! SOS より前の JPEG セグメントを 1 つ読む規則。
//!
//! `atomic::scan_offset`（SOS の位置を探す）と `preserve::segments`（SOS より前を
//! 切り分ける）が同じ規則で歩けるよう、境界の判定をここに 1 つだけ置く。
//! 規則が 2 箇所で食い違うと、片方が通した入力をもう片方が別の位置で読むことになる。
//! `header.rs` は I/O を逐次読みするので、バイト列を歩くこの規則とは別に持っている。
//!
//! **入力は webview 由来のパスから読んだバイト列で、壊れていることも切り詰められている
//! こともある。** 添字は必ず長さで押さえてから触り、境界外で panic しない。

use anyhow::{Result, bail};

/// `cursor` から始まる 1 セグメントの位置。`length` はマーカーの後ろの 2 バイトの値（自身を含む）
pub(super) struct RawSegment {
    pub marker: u8,
    /// 長さフィールドの直後（識別子を含む中身の先頭）
    pub contents_start: usize,
    /// 次のセグメントの先頭。**`bytes.len()` を超えうる**ので、中身を切り出す側が確かめる
    pub end: usize,
    pub length: usize,
}

/// `cursor`（SOI の直後、または前のセグメントの `end`）にあるセグメントを読む。
///
/// 0xFF のパディング（fill byte）を読み飛ばし、マーカーと長さまで読めることを確かめる。
/// `cursor` は必ず 2 以上進む（無限ループにならない）。
pub(super) fn read_segment(bytes: &[u8], cursor: usize) -> Result<RawSegment> {
    match bytes.get(cursor) {
        Some(0xFF) => {}
        Some(_) => bail!("JPEG のセグメント境界が壊れています（offset {}）", cursor),
        None => bail!("JPEG が途中で切れています（offset {}）", cursor),
    }
    let mut marker_at = cursor + 1;
    while marker_at < bytes.len() && bytes[marker_at] == 0xFF {
        marker_at += 1;
    }
    // マーカー本体と、それに続く 2 バイトの長さまで読めること
    if marker_at + 2 >= bytes.len() {
        bail!("JPEG のセグメントが途中で切れています（offset {}）", cursor);
    }
    let length = u16::from_be_bytes([bytes[marker_at + 1], bytes[marker_at + 2]]) as usize;
    Ok(RawSegment {
        marker: bytes[marker_at],
        contents_start: marker_at + 3,
        end: marker_at + 1 + length,
        length,
    })
}
