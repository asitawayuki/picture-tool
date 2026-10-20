//! 出力 JPEG のヘッダー（DQT・SOF）を読む、統合テスト共通の道具。

/// SOI の後ろのマーカーセグメントを SOS の手前まで辿り、`(マーカー, ペイロード)` を返す。
fn segments(jpeg: &[u8]) -> Vec<(u8, &[u8])> {
    assert_eq!(&jpeg[..2], &[0xFF, 0xD8], "SOI で始まっていない");
    let mut out = Vec::new();
    let mut pos = 2;
    while pos + 4 <= jpeg.len() {
        assert_eq!(jpeg[pos], 0xFF, "マーカーが 0xFF で始まっていない");
        let marker = jpeg[pos + 1];
        if marker == 0xFF {
            pos += 1;
            continue;
        }
        let len = u16::from_be_bytes([jpeg[pos + 2], jpeg[pos + 3]]) as usize;
        out.push((marker, &jpeg[pos + 4..pos + 2 + len]));
        if marker == 0xDA {
            break;
        }
        pos += 2 + len;
    }
    out
}

/// DQT の `(テーブル番号, 値)`。値は**ファイル内の順＝zigzag 順**。
pub fn dqt(jpeg: &[u8]) -> Vec<(u8, [u16; 64])> {
    let mut tables = Vec::new();
    for (marker, mut body) in segments(jpeg) {
        if marker != 0xDB {
            continue;
        }
        while !body.is_empty() {
            let (pq, tq) = (body[0] >> 4, body[0] & 0x0F);
            let width = if pq == 0 { 1 } else { 2 };
            let mut values = [0u16; 64];
            for (i, v) in values.iter_mut().enumerate() {
                *v = if width == 1 {
                    body[1 + i] as u16
                } else {
                    u16::from_be_bytes([body[1 + 2 * i], body[2 + 2 * i]])
                };
            }
            tables.push((tq, values));
            body = &body[1 + 64 * width..];
        }
    }
    tables
}

/// SOF の成分ごとの `(水平, 垂直)` サンプリング係数（成分の宣言順）。
pub fn sof_sampling(jpeg: &[u8]) -> Vec<(u8, u8)> {
    for (marker, body) in segments(jpeg) {
        // SOF0〜SOF15。ただし DHT(C4)・JPG(C8)・DAC(CC) は SOF ではない
        if (0xC0..=0xCF).contains(&marker) && ![0xC4, 0xC8, 0xCC].contains(&marker) {
            let n = body[5] as usize;
            return (0..n)
                .map(|i| {
                    let hv = body[7 + 3 * i];
                    (hv >> 4, hv & 0x0F)
                })
                .collect();
        }
    }
    panic!("SOF が無い");
}

/// SOS までに現れたマーカーの並び（SOI は含まない）。
pub fn markers(jpeg: &[u8]) -> Vec<u8> {
    segments(jpeg).into_iter().map(|(m, _)| m).collect()
}

/// DHT の `(クラス, テーブル番号, BITS)`。クラスは 0 が DC、1 が AC。
/// BITS は符号長 1〜16 ごとの符号の数（T.81 B.2.4.2）。
pub fn dht(jpeg: &[u8]) -> Vec<(u8, u8, [u8; 16])> {
    let mut tables = Vec::new();
    for (marker, mut body) in segments(jpeg) {
        if marker != 0xC4 {
            continue;
        }
        while !body.is_empty() {
            let (tc, th) = (body[0] >> 4, body[0] & 0x0F);
            let bits: [u8; 16] = body[1..17].try_into().unwrap();
            let n: usize = bits.iter().map(|&b| b as usize).sum();
            tables.push((tc, th, bits));
            body = &body[17 + n..];
        }
    }
    tables
}
