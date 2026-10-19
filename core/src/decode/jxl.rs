//! JPEG XL のコンテナ（ISOBMFF）の扱い
//!
//! **jxl-oxide にはコードストリームだけを渡す。** jxl-oxide はコンテナを受け取ると、
//! Brotli 圧縮された補助ボックス（`brob`）を上限なしで展開する。数 KB の爆弾で
//! メモリを使い切ると abort し、`catch_unwind` では止まらない。画素と寸法と向きは
//! すべてコードストリームにあるので、コンテナはこちらで剥がし、Exif だけを
//! 上限つきで自前で取り出す。剥がした中身がまたコンテナなら jxl-oxide は内側を
//! コンテナとして扱うので、呼び出し側（`super::jxl_decoder`）で素のコードストリームの
//! 署名を確かめる。

use super::isobmff;
use anyhow::{Result, bail};
use std::io::{self, Read, Seek, SeekFrom};

const CONTAINER_SIGNATURE: &[u8] = b"\0\0\0\x0CJXL \r\n\x87\n";

/// Exif（TIFF）の長さの上限
///
/// JPEG の APP1 に入る Exif は 64KB までで、JPEG から作った JPEG XL もこれに収まる。
/// 1MiB を超えるものは壊れているか細工されたものとみなし、読まない（`brob` の展開爆弾対策）。
const MAX_EXIF_LEN: u64 = 1 << 20;

/// コンテナを剥がしてコードストリーム（`jxlc` / `jxlp` の中身）だけを返す `Read`。
/// 素のコードストリームはそのまま返す
pub(super) struct Codestream<R> {
    inner: R,
    state: State,
}

enum State {
    /// 素のコードストリーム、またはファイル末尾まで続く箱の中
    ToEnd,
    /// 箱の中。残りのバイト数
    InBox(u64),
    BetweenBoxes,
}

impl<R: Read + Seek> Codestream<R> {
    pub(super) fn new(mut inner: R) -> io::Result<Self> {
        let state = if starts_with_container_signature(&mut inner)? {
            State::BetweenBoxes
        } else {
            inner.seek(SeekFrom::Start(0))?;
            State::ToEnd
        };
        Ok(Self { inner, state })
    }
}

impl<R: Read + Seek> Read for Codestream<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            match self.state {
                State::ToEnd => return self.inner.read(buf),
                State::InBox(0) => self.state = State::BetweenBoxes,
                State::InBox(left) => {
                    let n = self.inner.by_ref().take(left).read(buf)?;
                    if n == 0 && !buf.is_empty() {
                        return Err(io::ErrorKind::UnexpectedEof.into());
                    }
                    self.state = State::InBox(left - n as u64);
                    return Ok(n);
                }
                State::BetweenBoxes => {
                    let Some(header) =
                        isobmff::read_header(&mut self.inner).map_err(io::Error::other)?
                    else {
                        return Ok(0);
                    };
                    match (&header.kind, header.body_len) {
                        (b"jxlc", None) => self.state = State::ToEnd,
                        (b"jxlc", Some(len)) => self.state = State::InBox(len),
                        // 分割されたコードストリーム。先頭 4 バイトは通し番号
                        (b"jxlp", len) => {
                            let mut index = [0u8; 4];
                            self.inner.read_exact(&mut index)?;
                            self.state = match len {
                                None => State::ToEnd,
                                Some(len) => State::InBox(
                                    len.checked_sub(4)
                                        .ok_or_else(|| io::Error::other("jxlp box is too short"))?,
                                ),
                            };
                        }
                        (_, None) => return Ok(0),
                        (_, Some(len)) => isobmff::skip(&mut self.inner, len)?,
                    }
                }
            }
        }
    }
}

fn starts_with_container_signature(r: &mut impl Read) -> io::Result<bool> {
    let mut head = Vec::with_capacity(CONTAINER_SIGNATURE.len());
    r.take(CONTAINER_SIGNATURE.len() as u64)
        .read_to_end(&mut head)?;
    Ok(head == CONTAINER_SIGNATURE)
}

/// Exif ボックス（`Exif`、または中身が Exif の `brob`）を `exif` クレートで読める
/// 生の TIFF として取り出す。無ければ `None`
pub(super) fn raw_exif(mut r: impl Read + Seek) -> Result<Option<Vec<u8>>> {
    if !starts_with_container_signature(&mut r)? {
        return Ok(None);
    }
    while let Some(header) = isobmff::read_header(&mut r)? {
        let len = header.body_len.unwrap_or(u64::MAX);
        let payload = match &header.kind {
            b"Exif" => read_capped(r.by_ref().take(len))?,
            b"brob" => {
                // 本体が種類（4 バイト）より短い brob は壊れている。種類を読むと次の箱の
                // ヘッダを食うので、読まずに丸ごと飛ばす（種類が Exif でない brob と同じ扱い）
                let Some(len) = len.checked_sub(4) else {
                    isobmff::skip(&mut r, len)?;
                    continue;
                };
                let mut inner_kind = [0u8; 4];
                r.read_exact(&mut inner_kind)?;
                if &inner_kind != b"Exif" {
                    isobmff::skip(&mut r, len)?;
                    continue;
                }
                let compressed = r.by_ref().take(len);
                read_capped(brotli_decompressor::Decompressor::new(compressed, 4096))?
            }
            _ => {
                match header.body_len {
                    Some(len) => isobmff::skip(&mut r, len)?,
                    None => break,
                }
                continue;
            }
        };
        let Some(payload) = payload else {
            return Ok(None);
        };
        // 先頭 4 バイトは tiff_header_offset。その後のオフセットぶんは TIFF の前置き
        let Some((offset, rest)) = payload.split_first_chunk::<4>() else {
            bail!("Exif box is too short");
        };
        let offset = usize::try_from(u32::from_be_bytes(*offset))?;
        return Ok(rest.get(offset..).map(<[u8]>::to_vec));
    }
    Ok(None)
}

/// [`MAX_EXIF_LEN`] まで読む。超えたら `None`
fn read_capped(r: impl Read) -> io::Result<Option<Vec<u8>>> {
    let mut payload = Vec::new();
    r.take(MAX_EXIF_LEN + 1).read_to_end(&mut payload)?;
    Ok((payload.len() as u64 <= MAX_EXIF_LEN).then_some(payload))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn boxed(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        [&(8 + body.len() as u32).to_be_bytes()[..], kind, body].concat()
    }

    fn read_all(data: Vec<u8>) -> io::Result<Vec<u8>> {
        let mut out = Vec::new();
        Codestream::new(Cursor::new(data))?.read_to_end(&mut out)?;
        Ok(out)
    }

    #[test]
    fn bare_codestream_passes_through() {
        assert_eq!(
            read_all(vec![0xFF, 0x0A, 1, 2]).unwrap(),
            [0xFF, 0x0A, 1, 2]
        );
    }

    #[test]
    fn container_yields_only_codestream_parts_in_order() {
        let data = [
            CONTAINER_SIGNATURE,
            &boxed(b"ftyp", b"jxl \0\0\0\0jxl "),
            &boxed(b"jxlp", &[0, 0, 0, 0, 0xFF, 0x0A]),
            &boxed(b"Exif", b"ignored"),
            &boxed(b"jxlp", &[0x80, 0, 0, 1, 7, 8]),
        ]
        .concat();
        assert_eq!(read_all(data).unwrap(), [0xFF, 0x0A, 7, 8]);
    }

    #[test]
    fn codestream_box_running_to_end_of_file() {
        let data = [CONTAINER_SIGNATURE, b"\0\0\0\0jxlc", &[0xFF, 0x0A, 9]].concat();
        assert_eq!(read_all(data).unwrap(), [0xFF, 0x0A, 9]);
    }

    #[test]
    fn malformed_codestream_boxes_are_errors() {
        // jxlp は 4 バイトの通し番号を持つ
        let short = [CONTAINER_SIGNATURE, &boxed(b"jxlp", &[0, 0])].concat();
        assert!(read_all(short).is_err());
        // 宣言より短い jxlc
        let mut truncated = [CONTAINER_SIGNATURE, &boxed(b"jxlc", &[0xFF, 0x0A, 1, 2])].concat();
        truncated.pop();
        assert!(read_all(truncated).is_err());
    }

    #[test]
    fn exif_is_read_after_its_tiff_header_offset() {
        let data = [
            CONTAINER_SIGNATURE,
            &boxed(b"Exif", b"\0\0\0\x02xxII*\0"),
            &boxed(b"jxlc", &[0xFF, 0x0A]),
        ]
        .concat();
        assert_eq!(
            raw_exif(Cursor::new(data)).unwrap().unwrap(),
            b"II*\0".to_vec()
        );
        // 素のコードストリームには Exif を入れる場所がない
        assert_eq!(raw_exif(Cursor::new(vec![0xFF, 0x0A])).unwrap(), None);
    }

    #[test]
    fn exif_is_read_after_a_brob_shorter_than_its_kind_field() {
        // brob の本体は先頭 4 バイトが元の箱の種類。それより短い brob は壊れているが、
        // 箱の長さは信用できるので、飛ばして後ろの箱を読めなければならない
        let exif = boxed(b"Exif", b"\0\0\0\x02xxII*\0");
        let without_brob = [CONTAINER_SIGNATURE, &exif].concat();
        let with_short_brob = [CONTAINER_SIGNATURE, &boxed(b"brob", b"ab"), &exif].concat();
        // 前提: 短い brob が無ければ読める（テストの Exif 箱自体は正しい）
        assert_eq!(
            raw_exif(Cursor::new(without_brob)).unwrap().unwrap(),
            b"II*\0".to_vec()
        );
        assert_eq!(
            raw_exif(Cursor::new(with_short_brob)).unwrap().unwrap(),
            b"II*\0".to_vec()
        );
    }
}
