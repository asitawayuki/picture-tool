/**
 * spec §4-1「密度と操作」/ §4-2「仮想スクロール」。
 *
 * 列数の決定を JS の単一のソースに寄せる、というのが設計の要点。
 * auto-fill に任せると仮想スクロールの行位置と 1px でもずれてスクロールが飛ぶ。
 * ここはその「単一のソース」を、UI を起動せずに検査する。
 *
 * 数値は spec §3-1 の実測表から取っている。
 */
import { describe, expect, test } from "bun:test";
import {
  GRID_GAP,
  GRID_PADDING,
  LABEL_HEIGHT,
  MAX_THUMB_SIZE,
  MIN_THUMB_SIZE,
  OVERSCAN_ROWS,
  computeGridMetrics,
  computeVisibleRange,
} from "./gridMetrics";

describe("computeGridMetrics", () => {
  // spec §3-1 の「打ち消し後の実測値」の表と一致すること。
  // ここが合わないなら spec の列数の議論ごと成り立たない。
  // タイル幅も表に載っている（「約 253px」等）ので併せて固定値で見る ──
  // 式を書き写すと、式そのものを間違えていても一致してしまう
  const CASES: [width: number, target: number, columns: number, tileWidth: number][] = [
    [800, 200, 3, 253.33], // 新既定 1440・変換（右 320）
    [760, 200, 3, 240], // 新既定 1440・メタデータ（右 360）
    [1120, 200, 5, 212.8], // 新既定 1440・右パネル折りたたみ
    [460, 200, 2, 214], // minWidth 1100・変換（右 320）
    [560, 200, 2, 264], // 打ち消し前の 1200
  ];
  for (const [width, target, columns, tileWidth] of CASES) {
    test(`幅 ${width} / N=${target} で ${columns} 列・タイル幅 ${tileWidth}px`, () => {
      const m = computeGridMetrics(width, target, 200);
      expect(m.columns).toBe(columns);
      expect(m.tileWidth).toBeCloseTo(tileWidth, 1);
    });
  }

  test("列数は cols = floor((内側 + gap) / (N + gap))", () => {
    const inner = 800 - GRID_PADDING * 2;
    const expected = Math.floor((inner + GRID_GAP) / (200 + GRID_GAP));
    expect(computeGridMetrics(800, 200, 200).columns).toBe(expected);
  });

  test("タイル幅は gap を差し引いた等分", () => {
    const m = computeGridMetrics(800, 200, 200);
    const inner = 800 - GRID_PADDING * 2;
    expect(m.tileWidth).toBeCloseTo((inner - GRID_GAP * (m.columns - 1)) / m.columns, 5);
  });

  test("極端に狭くても 1 列を下回らない", () => {
    expect(computeGridMetrics(50, 200, 10).columns).toBe(1);
    expect(computeGridMetrics(0, 200, 10).columns).toBe(1);
  });

  test("要求解像度は 64px 刻みに丸め、96〜512 に収める", () => {
    // 生の列幅をそのままキャッシュキーにすると 1px の差で別エントリになる
    const m = computeGridMetrics(800, 200, 200);
    expect(m.thumbnailSize % 64).toBe(0);
    expect(m.thumbnailSize).toBeGreaterThanOrEqual(MIN_THUMB_SIZE);
    expect(m.thumbnailSize).toBeLessThanOrEqual(MAX_THUMB_SIZE);
    expect(computeGridMetrics(50, 200, 10).thumbnailSize).toBe(MIN_THUMB_SIZE);
    expect(computeGridMetrics(4000, 2000, 10).thumbnailSize).toBe(MAX_THUMB_SIZE);
  });

  test("要求解像度はタイル幅を下回らない（切り上げであって四捨五入ではない）", () => {
    // 切り捨て／四捨五入だと、タイル幅 253px に対して 192px を要求して
    // 引き伸ばされた絵が出る。上へ丸めることが仕様
    const m = computeGridMetrics(800, 200, 200);
    expect(m.thumbnailSize).toBeGreaterThanOrEqual(m.tileWidth);
  });

  test("行数は列数から出る", () => {
    const m = computeGridMetrics(800, 200, 10); // 3 列
    expect(m.columns).toBe(3);
    expect(m.totalRows).toBe(4);
    expect(computeGridMetrics(800, 200, 0).totalRows).toBe(0);
  });

  test("行高は 4:5 のタイル ＋ ファイル名 ＋ gap", () => {
    const m = computeGridMetrics(800, 200, 200);
    expect(m.rowHeight).toBeGreaterThan((m.tileWidth * 5) / 4);
    // 行高がタイル分しか無いと、ファイル名の行だけ毎行はみ出て
    // 仮想化の行位置が下へずれていく
    expect(m.rowHeight - (m.tileWidth * 5) / 4).toBeCloseTo(LABEL_HEIGHT + GRID_GAP, 5);
  });
});

describe("computeVisibleRange", () => {
  const metrics = computeGridMetrics(800, 200, 3000); // 3 列

  test("先頭では前方の余白が 0 で、後方に残り全部が積まれる", () => {
    const r = computeVisibleRange(metrics, 0, 800, 3000);
    expect(r.firstRow).toBe(0);
    expect(r.startIndex).toBe(0);
    expect(r.paddingTop).toBe(0);
    expect(r.paddingBottom).toBeGreaterThan(0);
  });

  test("前後 OVERSCAN_ROWS 行分を余分に描く", () => {
    const r = computeVisibleRange(metrics, metrics.rowHeight * 10, 800, 3000);
    expect(r.firstRow).toBe(10 - OVERSCAN_ROWS);
  });

  test("前後の余白の合計 ＋ 描画分の高さが総高と一致する", () => {
    // ここがずれるとスクロールバーの長さが動き、スクロールが飛ぶ
    for (const scrollTop of [0, 500, 5000, 50_000]) {
      const r = computeVisibleRange(metrics, scrollTop, 800, 3000);
      const rendered = (r.lastRow - r.firstRow + 1) * metrics.rowHeight;
      expect(r.paddingTop + rendered + r.paddingBottom).toBeCloseTo(
        metrics.totalRows * metrics.rowHeight,
        5
      );
    }
  });

  test("描く範囲は必ず可視部分を覆う", () => {
    // overscan の符号を間違えると「見えている行を描いていない」まま
    // 上の高さ一致の検査は通ってしまう
    for (const scrollTop of [0, 500, 5000, 50_000]) {
      const r = computeVisibleRange(metrics, scrollTop, 800, 3000);
      const visibleTop = Math.min(scrollTop, (metrics.totalRows - 1) * metrics.rowHeight);
      expect(r.firstRow * metrics.rowHeight).toBeLessThanOrEqual(visibleTop);
      expect((r.lastRow + 1) * metrics.rowHeight).toBeGreaterThanOrEqual(
        Math.min(visibleTop + 800, metrics.totalRows * metrics.rowHeight)
      );
    }
  });

  test("最終行を超えてスクロールしても範囲が要素数を超えない", () => {
    const r = computeVisibleRange(metrics, 10_000_000, 800, 3000);
    expect(r.lastRow).toBe(metrics.totalRows - 1);
    expect(r.endIndex).toBe(2999);
    expect(r.paddingBottom).toBe(0);
  });

  test("末尾を超えた位置でも前方の余白が総高を超えない", () => {
    // 列数を増やすと総高が縮む。要素側が clamp されるまでの 1 フレーム、
    // 古い scrollTop で描くことになる。ここで抑えないと画面が飛ぶ
    const r = computeVisibleRange(metrics, 10_000_000, 800, 3000);
    expect(r.paddingTop).toBeLessThanOrEqual(metrics.totalRows * metrics.rowHeight);
    expect(r.firstRow).toBeLessThanOrEqual(r.lastRow);
  });

  test("要素が 0 件なら空の範囲を返す", () => {
    const empty = computeGridMetrics(800, 200, 0);
    const r = computeVisibleRange(empty, 0, 800, 0);
    expect(r.startIndex).toBe(0);
    expect(r.endIndex).toBe(-1);
    expect(r.paddingTop).toBe(0);
    expect(r.paddingBottom).toBe(0);
  });

  test("行高が 0 になっても無限ループや NaN を出さない", () => {
    // 初回描画で clientWidth が 0 の瞬間に通る経路
    const zero = computeGridMetrics(0, 0, 100);
    const r = computeVisibleRange(zero, 0, 0, 100);
    expect(Number.isFinite(r.startIndex)).toBe(true);
    expect(Number.isFinite(r.endIndex)).toBe(true);
    expect(r.endIndex).toBeGreaterThanOrEqual(-1);
  });
});
