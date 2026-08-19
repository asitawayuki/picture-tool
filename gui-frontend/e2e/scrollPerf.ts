import type { Page } from "@playwright/test";

export interface ScrollSample {
  /** rAF 間隔の 95 パーセンタイル (ms) */
  p95: number;
  /** 32ms を超えたフレームの割合 (0-1) */
  jankRatio: number;
  frames: number;
}

/**
 * 一定速度で最上部から最下部までスクロールしながら rAF 間隔を測る。
 *
 * スクロールは rAF ごとに scrollTop を等量ずつ進める。wheel イベントだと
 * OS とブラウザのスムーススクロールが挟まり、条件を揃えられない。
 */
export async function measureScroll(
  page: Page,
  selector: string,
  durationMs = 6000
): Promise<ScrollSample> {
  return page.evaluate(
    async ({ selector, durationMs }) => {
      const el = document.querySelector<HTMLElement>(selector)!;
      el.scrollTop = 0;
      await new Promise((r) => requestAnimationFrame(() => r(null)));

      const distance = el.scrollHeight - el.clientHeight;
      const intervals: number[] = [];
      let last = performance.now();
      const start = last;

      await new Promise<void>((resolve) => {
        function tick(now: number) {
          intervals.push(now - last);
          last = now;
          const elapsed = now - start;
          if (elapsed >= durationMs) {
            resolve();
            return;
          }
          el.scrollTop = (distance * elapsed) / durationMs;
          requestAnimationFrame(tick);
        }
        requestAnimationFrame(tick);
      });

      // 最初の 1 フレームは計測開始のオーバーヘッドを含むので捨てる
      const samples = intervals.slice(1).sort((a, b) => a - b);
      const p95 = samples[Math.floor(samples.length * 0.95)] ?? 0;
      const janky = samples.filter((v) => v > 32).length;
      return { p95, jankRatio: janky / samples.length, frames: samples.length };
    },
    { selector, durationMs }
  );
}

export function median(values: number[]): number {
  const sorted = [...values].sort((a, b) => a - b);
  const mid = Math.floor(sorted.length / 2);
  return sorted.length % 2 === 1 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2;
}
