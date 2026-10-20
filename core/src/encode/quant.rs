//! 品質（1〜100）から量子化テーブルとサンプリングを決める「目盛り」。
//!
//! 基準は LrC 20% の書き出しのテーブル（spec 2026-10-20「品質の目盛り」）。絶対条件は
//! 品質 20 = LrC 20% と完全一致、品質 100 = 全要素 1、品質を下げるとどの要素も増えるか変わらない、
//! 全要素 1〜255（256 以上は DQT に下位 8bit だけが書かれ、壊れた JPEG がエラーなしで出る）。
//!
//! `B` を LrC 20% の値、`K` を ITU-T T.81 Annex K の標準テーブル、`S` を倍率（輝度 [`LUMA_SCALE`]・
//! 色差 [`CHROMA_SCALE`]）、`A` を [`ANCHOR_QUALITY`] として、`k(q) = min(B, K·S·(100 − q)/50)` と置き
//!
//! - q ≥ A: `v = k(q)`（libjpeg の品質 50 以上の倍率 `(200 − 2q)/100` を Annex K に掛け、LrC 20% で上から抑えたもの）
//! - 20 ≤ q < A: `v = B^(1 − w)·k(A)^w`、`w = (q − 20)/(A − 20)`（要素ごとに `B` と `k(A)` を対数で補間する）
//! - q < 20: `v = min(B·(20 / q)^BETA, max(B, CAP))`
//!
//! を丸めて 1〜255 に収める。`k(A) ≤ B` なので q = 20 で `B`、q = 100 で 0 → 1、各区間で q について単調で、
//! 区間の境目でも値が続く。20〜A を `k(q)` のままにしないのは、`B` が Annex K よりずっと細かい要素
//! （低周波）が品質 60 前後まで `B` に張り付き、品質 20〜50 のテーブルがほとんど変わらなくなるため。
//!
//! 性質 1・3 から、品質 20 以上のどの要素も LrC 20% 以下（高周波は 12 以下）に限られる。
//! この制約の下で「既定値 90 が現行（Annex K の品質 90）より悪くならない」に最も近いのは、
//! 現行の品質 90 を LrC 20% で抑えた形だった。LrC 20% の値を指数で 1 へ近づける形は、
//! 定数をどう選んでも細部の多い写真のサイズと滑らかな写真の PSNR が両立しなかった
//! （計画 2026-10-20-output-quality の実施メモ「Task 3」）。

use jpeg_encoder::SamplingFactor;

// --- 目盛りの定数（調整するのはここだけ） ---

/// q ≥ 20 で輝度の Annex K に掛ける倍率。大きいほど同じ品質で粗くなる。
/// 品質 90 の実写真 4 枚で、現行よりサイズ +1% 以内・PSNR −0.1dB 以内に収まるよう選んだ
const LUMA_SCALE: f64 = 1.05;
/// q ≥ 20 で色差の Annex K に掛ける倍率（[`LUMA_SCALE`] と同じ選び方）
const CHROMA_SCALE: f64 = 1.0;
/// この品質以上は Annex K の形（を LrC 20% で抑えたもの）そのもの。既定値 90 をここに合わせる
const ANCHOR_QUALITY: f64 = 90.0;
/// 品質 20 未満で、品質に反比例して粗くする度合い
const BETA: f64 = 1.0;
/// 品質 20 未満での刻みの上限。LrC 20% がすでにこれより粗い要素はそのまま
const CAP: f64 = 30.0;

/// 4:2:0 にする品質の上限（これ未満なら 4:2:0、以上なら 4:4:4）
const SUBSAMPLE_BELOW: u8 = 50;

/// zigzag の k 番目の係数が自然順（行優先）のどこにあるか（ITU-T T.81 Figure A.6）
#[rustfmt::skip]
pub(crate) const ZIGZAG: [usize; 64] = [
     0,  1,  8, 16,  9,  2,  3, 10,
    17, 24, 32, 25, 18, 11,  4,  5,
    12, 19, 26, 33, 40, 48, 41, 34,
    27, 20, 13,  6,  7, 14, 21, 28,
    35, 42, 49, 56, 57, 50, 43, 36,
    29, 22, 15, 23, 30, 37, 44, 51,
    58, 59, 52, 45, 38, 31, 39, 46,
    53, 60, 61, 54, 47, 55, 62, 63,
];

/// LrC 20% の輝度テーブル。**DQT のままの zigzag 順**で、spec の値を逐語で写したもの
#[rustfmt::skip]
pub(crate) const LRC_20_LUMA_ZIGZAG: [u16; 64] = [
    12, 8, 8,13, 9,13,21,12,12,21,26,20,16,20,26,32,27,26,26,27,32,34,23,23,23,23,23,34,17,12,12,12,
    12,12,12,17,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,
];

/// LrC 20% の色差テーブル。**DQT のままの zigzag 順**で、spec の値を逐語で写したもの
#[rustfmt::skip]
pub(crate) const LRC_20_CHROMA_ZIGZAG: [u16; 64] = [
    13,13,13,17,14,17,27,17,17,27,20,14,14,14,20,20,14,14,14,14,20,17,12,12,12,12,12,17,17,12,12,12,
    12,12,12,17,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,
];

/// ITU-T T.81 Annex K の輝度テーブル（Table K.1）。**自然順（行優先）**
#[rustfmt::skip]
const ANNEX_K_LUMA: [u16; 64] = [
    16, 11, 10, 16,  24,  40,  51,  61,
    12, 12, 14, 19,  26,  58,  60,  55,
    14, 13, 16, 24,  40,  57,  69,  56,
    14, 17, 22, 29,  51,  87,  80,  62,
    18, 22, 37, 56,  68, 109, 103,  77,
    24, 35, 55, 64,  81, 104, 113,  92,
    49, 64, 78, 87, 103, 121, 120, 101,
    72, 92, 95, 98, 112, 100, 103,  99,
];

/// ITU-T T.81 Annex K の色差テーブル（Table K.2）。**自然順（行優先）**
#[rustfmt::skip]
const ANNEX_K_CHROMA: [u16; 64] = [
    17, 18, 24, 47, 99, 99, 99, 99,
    18, 21, 26, 66, 99, 99, 99, 99,
    24, 26, 56, 99, 99, 99, 99, 99,
    47, 66, 99, 99, 99, 99, 99, 99,
    99, 99, 99, 99, 99, 99, 99, 99,
    99, 99, 99, 99, 99, 99, 99, 99,
    99, 99, 99, 99, 99, 99, 99, 99,
    99, 99, 99, 99, 99, 99, 99, 99,
];

/// 輝度・色差の量子化テーブル。**自然順（行優先）**。`jpeg-encoder` の `Custom` にそのまま渡せる
pub(crate) struct QuantTables {
    pub luma: [u16; 64],
    pub chroma: [u16; 64],
}

/// 品質（1..=100）の量子化テーブル。範囲外の品質は呼び出し側が弾く
pub(crate) fn quant_tables(quality: u8) -> QuantTables {
    debug_assert!((1..=100).contains(&quality), "品質 {quality} は範囲外");
    QuantTables {
        luma: scale(&LRC_20_LUMA_ZIGZAG, &ANNEX_K_LUMA, LUMA_SCALE, quality),
        chroma: scale(
            &LRC_20_CHROMA_ZIGZAG,
            &ANNEX_K_CHROMA,
            CHROMA_SCALE,
            quality,
        ),
    }
}

/// 品質のサンプリング。低品質では色差を間引いてその分を輝度に回す（LrC 20% も 4:2:0）
pub(crate) fn sampling(quality: u8) -> SamplingFactor {
    if quality < SUBSAMPLE_BELOW {
        SamplingFactor::F_2_2
    } else {
        SamplingFactor::F_1_1
    }
}

fn scale(base_zigzag: &[u16; 64], annex_k: &[u16; 64], k_scale: f64, quality: u8) -> [u16; 64] {
    let mut base = [0u16; 64];
    for (k, &v) in base_zigzag.iter().enumerate() {
        base[ZIGZAG[k]] = v;
    }

    let q = f64::from(quality);
    std::array::from_fn(|i| {
        let b = f64::from(base[i]);
        let annex = |q: f64| (f64::from(annex_k[i]) * k_scale * (100.0 - q) / 50.0).min(b);
        let v = if q >= ANCHOR_QUALITY {
            annex(q)
        } else if q >= 20.0 {
            let w = (q - 20.0) / (ANCHOR_QUALITY - 20.0);
            b.powf(1.0 - w) * annex(ANCHOR_QUALITY).powf(w)
        } else {
            (b * (20.0 / q).powf(BETA)).min(b.max(CAP))
        };
        v.round().clamp(1.0, 255.0) as u16
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use jpeg_encoder::SamplingFactor;

    fn in_zigzag_order(natural: &[u16; 64]) -> [u16; 64] {
        std::array::from_fn(|k| natural[ZIGZAG[k]])
    }

    #[test]
    fn zigzag_is_a_permutation_of_0_to_63() {
        let mut seen = [false; 64];
        for &i in &ZIGZAG {
            assert!(i < 64, "範囲外の位置 {i}");
            assert!(!seen[i], "位置 {i} が 2 度出てくる");
            seen[i] = true;
        }
    }

    #[test]
    fn zigzag_follows_t81_figure_a6() {
        // zigzag の 1 番目は (行 0, 列 1)、2 番目は (行 1, 列 0)、最後は (行 7, 列 7)
        assert_eq!(ZIGZAG[1], 1);
        assert_eq!(ZIGZAG[2], 8);
        assert_eq!(ZIGZAG[63], 63);
    }

    #[test]
    fn quality_20_is_lrc_20_percent() {
        let t = quant_tables(20);
        assert_eq!(in_zigzag_order(&t.luma), LRC_20_LUMA_ZIGZAG);
        assert_eq!(in_zigzag_order(&t.chroma), LRC_20_CHROMA_ZIGZAG);
    }

    #[test]
    fn quality_100_is_all_ones() {
        let t = quant_tables(100);
        assert_eq!(t.luma, [1; 64]);
        assert_eq!(t.chroma, [1; 64]);
    }

    #[test]
    fn every_element_is_a_valid_8bit_step() {
        for q in 1..=100 {
            let t = quant_tables(q);
            for (i, &v) in t.luma.iter().chain(&t.chroma).enumerate() {
                assert!((1..=255).contains(&v), "品質 {q} の要素 {i} が {v}");
            }
        }
    }

    #[test]
    fn lowering_quality_never_makes_a_step_finer() {
        for q in 2..=100 {
            let (lower, higher) = (quant_tables(q - 1), quant_tables(q));
            let lower = lower.luma.iter().chain(&lower.chroma);
            let higher = higher.luma.iter().chain(&higher.chroma);
            for (i, (&l, &h)) in lower.zip(higher).enumerate() {
                assert!(l >= h, "要素 {i}: 品質 {} が {l}、品質 {q} が {h}", q - 1);
            }
        }
    }

    #[test]
    fn sampling_switches_at_50() {
        assert_eq!(sampling(49), SamplingFactor::F_2_2);
        assert_eq!(sampling(50), SamplingFactor::F_1_1);
        assert_eq!(sampling(20), SamplingFactor::F_2_2);
        assert_eq!(sampling(90), SamplingFactor::F_1_1);
    }
}
