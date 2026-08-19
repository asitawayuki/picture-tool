/**
 * spec §7-1 のコントラスト検査。
 *
 * 全ペアを検査する方式は成立しない。outline-variant や scrim は意図的に
 * 低コントラストで、M3 のトーン設計上 AA を満たさないため必ず赤になる。
 * 検査対象は「対になるロール」に限定する。
 *
 * color-tokens.css を直接読む。生成スクリプトの戻り値ではなく出荷される
 * ファイルを見ることで、手で編集された場合もここで落ちる。
 */
import { describe, expect, test } from "bun:test";

const SURFACES = [
  "surface",
  "surface-container-lowest",
  "surface-container-low",
  "surface-container",
  "surface-container-high",
  "surface-container-highest",
] as const;

interface Pair {
  fg: string;
  bg: string;
  /** WCAG の基準。本文は AA 4.5:1、境界線などの非テキストは 3:1 */
  min: number;
}

function pairsUnderTest(): Pair[] {
  const pairs: Pair[] = [];
  for (const bg of SURFACES) {
    pairs.push({ fg: "on-surface", bg, min: 4.5 });
    pairs.push({ fg: "on-surface-variant", bg, min: 4.5 });
  }
  pairs.push({ fg: "on-primary", bg: "primary", min: 4.5 });
  pairs.push({ fg: "on-primary-container", bg: "primary-container", min: 4.5 });
  pairs.push({ fg: "on-error", bg: "error", min: 4.5 });
  pairs.push({ fg: "on-error-container", bg: "error-container", min: 4.5 });
  pairs.push({ fg: "inverse-on-surface", bg: "inverse-surface", min: 4.5 });
  pairs.push({ fg: "outline", bg: "surface", min: 3 });
  return pairs;
}

/** WCAG 2.x の相対輝度。3 チャンネルすべてに同じガンマ補正を掛けること。 */
function channelLuminance(value8bit: number): number {
  const s = value8bit / 255;
  return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
}

function relativeLuminance(hex: string): number {
  const n = Number.parseInt(hex.slice(1), 16);
  return (
    0.2126 * channelLuminance((n >> 16) & 0xff) +
    0.7152 * channelLuminance((n >> 8) & 0xff) +
    0.0722 * channelLuminance(n & 0xff)
  );
}

function contrastRatio(a: string, b: string): number {
  const [hi, lo] = [relativeLuminance(a), relativeLuminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

const source = await Bun.file(new URL("./color-tokens.css", import.meta.url)).text();

const values: Record<"light" | "dark", Record<string, string>> = { light: {}, dark: {} };
for (const m of source.matchAll(/--_(light|dark)-([a-z-]+):\s*(#[0-9a-f]{6});/g)) {
  values[m[1] as "light" | "dark"][m[2]] = m[3];
}

for (const scheme of ["light", "dark"] as const) describe(`${scheme} スキーム`, () => {
  // 前提条件: そもそも値が読めていないと、以下の検査は「対象が無い」だけで
  // 素通りしうる。21 ロールが揃っていることを先に確かめる。
  test("spec §1-1 の 21 ロールがすべて定義されている", () => {
    expect(Object.keys(values[scheme]).sort()).toEqual(
      [
        "error", "error-container", "inverse-on-surface", "inverse-surface",
        "on-error", "on-error-container", "on-primary", "on-primary-container",
        "on-surface", "on-surface-variant", "outline", "outline-variant",
        "primary", "primary-container", "scrim", "surface",
        "surface-container", "surface-container-high", "surface-container-highest",
        "surface-container-low", "surface-container-lowest",
      ].sort()
    );
  });

  for (const { fg, bg, min } of pairsUnderTest()) {
    test(`${fg} / ${bg} が ${min}:1 以上`, () => {
      const ratio = contrastRatio(values[scheme][fg], values[scheme][bg]);
      expect(ratio).toBeGreaterThanOrEqual(min);
    });
  }
});

describe("面は無彩色である（spec §1-2）", () => {
  // 「背景の色被りが写真の色判断を狂わせる」を防ぐのが目的なので、
  // surface 系は R=G=B であることまで要求する。
  for (const role of SURFACES) {
    test(`${role} は R=G=B`, () => {
      for (const scheme of ["light", "dark"] as const) {
        const hex = values[scheme][role];
        expect(hex.slice(1, 3)).toBe(hex.slice(3, 5));
        expect(hex.slice(3, 5)).toBe(hex.slice(5, 7));
      }
    });
  }
});
