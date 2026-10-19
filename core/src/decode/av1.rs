//! AV1 のバイト列（AVIF の 1 アイテム）を libdav1d でデコードして 1 枚の [`Picture`] にする
//!
//! 色変換は知らない（`yuv` モジュール）。設定するのは `n_threads`・`max_frame_delay`・
//! `apply_grain`・`frame_size_limit` の 4 つだけで、他は既定のまま（差し替え前の avif-decode が
//! rav1d に設定していたのも前の 3 つと既定値。画素を一致させるため）。
//!
//! # 最後の 1 枚を返す
//!
//! レイヤー付き（プログレッシブ）のアイテムは 1 レイヤーごとに 1 枚出て、最後の 1 枚が完全な
//! 画像になる（後のレイヤーが前のものを詳細化し、フィルムグレインも最後のレイヤーにだけ掛かる）。
//! dav1d の `all_layers` は既定で真なので全レイヤーが出てくる。avif-decode と同じく、送り切った後に
//! `get_picture` が `Again` を返すまで取り続けて最後の 1 枚を取る。
//!
//! # 周回数で打ち切る
//!
//! **1 周 = `get_picture` を 1 回呼び、送り残りがあれば `send_pending_data` を 1 回呼ぶこと。**
//! 送り残りのバイト数は dav1d クレートの公開 API で観測できない（`Decoder` の private な
//! `pending_data` にあり、`send_pending_data` が `Again` を返しても何バイト消費されたかは分からない）。
//! そのため「進まなくなったこと」を直接は検知できず、周回数に上限を置く。正常に進む周回は
//! 「1 バイト以上消費される」か「1 枚出る」のどちらかで、どちらもバイト数以下の回数しか起きないので、
//! 上限 `2 × バイト数 + 16` は正常な入力を打ち切らず、止まったデコーダでも必ず終わる。

use anyhow::{Context, Result, anyhow};
use dav1d::{Decoder, Picture, Settings};
use std::cell::RefCell;

/// AV1 データを 1 枚にデコードする。`frame_size_limit` を超える画素数のフレームはデコーダが拒否する
///
/// `frame_size_limit` の 0 は無制限なので、呼び出し側は 0 を渡さない（`frame_size_limits` が保証する）。
pub(super) fn decode(av1: &[u8], frame_size_limit: u32) -> Result<Picture> {
    let mut settings = Settings::new();
    settings.set_n_threads(1);
    settings.set_max_frame_delay(1);
    settings.set_apply_grain(true);
    settings.set_frame_size_limit(frame_size_limit);
    let mut decoder = Decoder::with_settings(&settings).context(FAILED)?;
    // `send_data` は `T: 'static` が要るので複製する（AVIF の圧縮データは小さい）
    let pending = match decoder.send_data(av1.to_vec(), None, None, None) {
        Ok(()) => false,
        Err(dav1d::Error::Again) => true,
        Err(e) => return Err(e).context(FAILED),
    };
    // `get_picture` も `send_pending_data` も `&mut self` なので、2 つのクロージャで共有する
    let decoder = RefCell::new(decoder);
    drain(
        av1.len(),
        pending,
        || decoder.borrow_mut().get_picture(),
        || decoder.borrow_mut().send_pending_data(),
    )
}

const FAILED: &str = "AV1 decoding failed";

/// 送り切るまで `send_pending` し、`get` が `Again` を返すまで取り続けて最後の 1 枚を返す
///
/// dav1d の `Decoder` に依らない形にしてあるのは、止まったデコーダを偽物で再現してテストするため。
fn drain<P>(
    len: usize,
    mut pending: bool,
    mut get: impl FnMut() -> Result<P, dav1d::Error>,
    mut send_pending: impl FnMut() -> Result<(), dav1d::Error>,
) -> Result<P> {
    let mut last = None;
    for _ in 0..len.saturating_mul(2).saturating_add(16) {
        match get() {
            Ok(picture) => last = Some(picture),
            Err(dav1d::Error::Again) if !pending => {
                return last.ok_or_else(|| anyhow!("no frame decoded"));
            }
            Err(dav1d::Error::Again) => {}
            Err(e) => return Err(e).context(FAILED),
        }
        if pending {
            match send_pending() {
                Ok(()) => pending = false,
                Err(dav1d::Error::Again) => {}
                Err(e) => return Err(e).context(FAILED),
            }
        }
    }
    Err(anyhow!("AV1 decoder made no progress"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use dav1d::Error as E;

    #[test]
    fn the_last_picture_is_returned() {
        // レイヤー付きのアイテムは 1 レイヤーごとに 1 枚出て、最後の 1 枚が完全な画像
        let mut out = vec![Ok(2), Ok(1)]; // pop で 1, 2 の順に出る
        let p = drain(10, false, || out.pop().unwrap_or(Err(E::Again)), || Ok(())).unwrap();
        assert_eq!(p, 2);
    }

    #[test]
    fn pending_data_is_sent_until_accepted() {
        // 3 回目の send_pending で受け付けられ、その後に 1 枚だけ出る
        let sends = std::cell::Cell::new(0);
        let mut emitted = false;
        let p = drain(
            10,
            true,
            || {
                if sends.get() >= 3 && !emitted {
                    emitted = true;
                    Ok(7)
                } else {
                    Err(E::Again)
                }
            },
            || {
                sends.set(sends.get() + 1);
                if sends.get() < 3 {
                    Err(E::Again)
                } else {
                    Ok(())
                }
            },
        )
        .unwrap();
        assert_eq!((p, sends.get()), (7, 3));
    }

    #[test]
    fn nothing_decoded_is_an_error() {
        let err = drain::<u8>(10, false, || Err(E::Again), || Ok(())).unwrap_err();
        assert!(format!("{err:#}").contains("no frame decoded"), "{err:#}");
    }

    #[test]
    fn a_stalled_decoder_is_cut_off() {
        // データを消費もせずフレームも出さないデコーダでも、有限回で Err になる
        let mut calls = 0;
        let err = drain::<u8>(
            100,
            true,
            || {
                calls += 1;
                Err(E::Again)
            },
            || Err(E::Again),
        )
        .unwrap_err();
        assert!(
            format!("{err:#}").contains("AV1 decoder made no progress"),
            "{err:#}"
        );
        assert_eq!(calls, 2 * 100 + 16);
    }

    #[test]
    fn decoder_errors_are_returned() {
        let from_get = drain::<u8>(10, false, || Err(E::InvalidArgument), || Ok(())).unwrap_err();
        let from_send =
            drain::<u8>(10, true, || Err(E::Again), || Err(E::UnsupportedBitstream)).unwrap_err();
        for (err, reason) in [
            (from_get, "Invalid argument"),
            (from_send, "Unsupported bitstream"),
        ] {
            let text = format!("{err:#}");
            assert!(text.contains("AV1 decoding failed"), "{text}");
            assert!(text.contains(reason), "{text}");
        }
    }

    #[test]
    fn frame_size_limit_is_enforced_by_the_decoder() {
        let data = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/orient_1.avif"
        ))
        .unwrap();
        let parsed = avif_parse::read_avif(&mut data.as_slice()).unwrap();
        // 64x32 = 2048 画素。上限は「以下なら通す」
        let ok = decode(&parsed.primary_item, 2048).unwrap();
        assert_eq!((ok.width(), ok.height()), (64, 32));
        let err = decode(&parsed.primary_item, 2047).unwrap_err();
        assert!(
            format!("{err:#}").contains("AV1 decoding failed"),
            "{err:#}"
        );
    }
}
