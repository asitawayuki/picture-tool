//! ISOBMFF の箱の読み出し（JPEG XL のコンテナと AVIF で共用）
//!
//! 箱の長さはファイルに書かれた値なので信用しない。長さぶん先に確保せず、
//! 読めたぶんだけを持つ（`Read::take` + `read_to_end`）。

use anyhow::{Context, Result, bail};
use std::io::{self, Read, Seek, SeekFrom};

pub(super) struct BoxHeader {
    pub kind: [u8; 4],
    /// 中身の長さ。`None` はファイル末尾まで（size 0）
    pub body_len: Option<u64>,
}

/// 次の箱のヘッダを読む。ちょうどファイル末尾なら `None`
pub(super) fn read_header(r: &mut impl Read) -> Result<Option<BoxHeader>> {
    let mut head = [0u8; 8];
    let mut filled = 0;
    while filled < head.len() {
        match r.read(&mut head[filled..])? {
            0 if filled == 0 => return Ok(None),
            0 => bail!("box header is truncated"),
            n => filled += n,
        }
    }
    let kind = [head[4], head[5], head[6], head[7]];
    let (size, header_len) = match u32::from_be_bytes([head[0], head[1], head[2], head[3]]) {
        0 => {
            return Ok(Some(BoxHeader {
                kind,
                body_len: None,
            }));
        }
        1 => {
            let mut large = [0u8; 8];
            r.read_exact(&mut large)?;
            (u64::from_be_bytes(large), 16)
        }
        n => (u64::from(n), 8),
    };
    let body_len = size
        .checked_sub(header_len)
        .context("box size is smaller than its header")?;
    Ok(Some(BoxHeader {
        kind,
        body_len: Some(body_len),
    }))
}

/// 箱の中身を読み飛ばす
pub(super) fn skip(r: &mut impl Seek, body_len: u64) -> io::Result<()> {
    let offset = i64::try_from(body_len).map_err(io::Error::other)?;
    r.seek(SeekFrom::Current(offset)).map(drop)
}

/// 箱の中身を読む。途中で切れていればエラー
pub(super) fn read_body(r: &mut impl Read, body_len: Option<u64>) -> Result<Vec<u8>> {
    let mut body = Vec::new();
    r.take(body_len.unwrap_or(u64::MAX))
        .read_to_end(&mut body)?;
    if body_len.is_some_and(|len| body.len() as u64 != len) {
        bail!("box is truncated");
    }
    Ok(body)
}

/// メモリ上に並んだ子の箱を `(種類, 中身)` で列挙する
pub(super) fn children(mut data: &[u8]) -> Result<Vec<([u8; 4], &[u8])>> {
    let mut boxes = Vec::new();
    while !data.is_empty() {
        let Some(header) = read_header(&mut data)? else {
            break;
        };
        let len = match header.body_len {
            Some(len) => usize::try_from(len)?,
            None => data.len(),
        };
        let Some((body, rest)) = data.split_at_checked(len) else {
            bail!("box is truncated");
        };
        boxes.push((header.kind, body));
        data = rest;
    }
    Ok(boxes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_sizes_follow_iso_14496_12() {
        // 通常: size は 8 バイトのヘッダを含む
        let h = read_header(&mut &b"\0\0\0\x0Cabcd...."[..])
            .unwrap()
            .unwrap();
        assert_eq!((&h.kind, h.body_len), (b"abcd", Some(4)));
        // size 1: 64 ビットの largesize が続き、ヘッダは 16 バイト
        let h = read_header(&mut &b"\0\0\0\x01abcd\0\0\0\0\0\0\0\x14"[..])
            .unwrap()
            .unwrap();
        assert_eq!(h.body_len, Some(4));
        // size 0: ファイル末尾まで
        let h = read_header(&mut &b"\0\0\0\0abcd"[..]).unwrap().unwrap();
        assert_eq!(h.body_len, None);
    }

    #[test]
    fn end_of_data_and_malformed_headers() {
        assert!(read_header(&mut &b""[..]).unwrap().is_none());
        assert!(read_header(&mut &b"\0\0\0\x0Cab"[..]).is_err()); // 途中で切れている
        assert!(read_header(&mut &b"\0\0\0\x07abcd"[..]).is_err()); // ヘッダより小さい
        assert!(read_header(&mut &b"\0\0\0\x01abcd\0\0\0\0\0\0\0\x0F"[..]).is_err());
    }

    #[test]
    fn children_are_split_by_size_and_truncation_is_an_error() {
        let data = b"\0\0\0\x09aaaa1\0\0\0\x0Abbbb22";
        let boxes = children(data).unwrap();
        assert_eq!(boxes, [(*b"aaaa", &b"1"[..]), (*b"bbbb", &b"22"[..])]);
        assert!(children(&data[..data.len() - 1]).is_err());
        assert!(read_body(&mut &b"12"[..], Some(3)).is_err());
    }
}
