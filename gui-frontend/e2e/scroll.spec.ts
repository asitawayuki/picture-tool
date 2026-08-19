import { expect, test } from "@playwright/test";
import baseline from "./scroll-baseline.json" with { type: "json" };
import { installTauriStub } from "./stub";
import { measureScroll, median } from "./scrollPerf";
// 純粋モジュール側から引く。runes を持つ .svelte.ts を e2e から import すると
// Node 側で $state が解決できずに落ちる
import { CACHE_BYTE_LIMIT } from "../src/lib/browser/thumbnailCache";

/**
 * spec §7-2「スクロール検査」。
 *
 * 指標は rAF 間隔の p95 と 32ms 超のフレーム割合。各条件 3 回の中央値で比べる。
 * 平均は詰まりを均し、最大は 1 回の外れ値で決まるので使わない。
 *
 * **スクロールする箱は `role="listbox"` の要素ではない**（PhotoGrid.svelte の
 * コメント。仮想化の余白は listbox 側の padding にある）。`.scroller` を測る。
 */
test.setTimeout(180_000);

const RUNS = 3;
/** 仮想スクロールの実体。listbox ではないので role では引けない */
const SCROLLER = ".scroller";

/**
 * 比較の有効桁。`rAF` 間隔は 60Hz なら 16.7ms 付近に張り付き、
 * 実装差ではなく浮動小数の末尾（16.700000000000728）で大小が決まってしまう。
 * 0.1ms 未満の差はこの測り方では意味を持たない。
 */
const round1 = (v: number) => Math.round(v * 10) / 10;

async function sampleThrice(page: import("@playwright/test").Page) {
  const p95: number[] = [];
  const jank: number[] = [];
  for (let i = 0; i < RUNS; i++) {
    const s = await measureScroll(page, SCROLLER);
    p95.push(s.p95);
    jank.push(s.jankRatio);
  }
  return { p95: median(p95), jankRatio: median(jank) };
}

async function openPhotos(page: import("@playwright/test").Page, imageCount: number) {
  await installTauriStub(page, { imageCount });
  await page.goto("/");
  await page.getByRole("button", { name: "photos", exact: false }).first().click();
  await expect(page.getByRole("option").first()).toBeVisible();
}

test.describe("スクロール性能（spec §7-2）", () => {
  test("新実装 / 50 枚 はベースラインを上回らない", async ({ page }) => {
    await openPhotos(page, 50);

    const result = await sampleThrice(page);

    // 前提条件: そもそもスクロールできる高さがあること。
    // 1 画面に収まっていたら「悪化しなかった」は自明に成立する
    const scrollable = await page
      .locator(SCROLLER)
      .evaluate((el) => el.scrollHeight - el.clientHeight);
    expect(scrollable).toBeGreaterThan(100);

    console.log(JSON.stringify({ scale: 50, ...result }, null, 2));
    expect(round1(result.p95)).toBeLessThanOrEqual(baseline.p95);
    expect(result.jankRatio).toBeLessThanOrEqual(baseline.jankRatio);
  });

  test("新実装 / 3,000 枚 の絶対値とキャッシュ実サイズを記録する", async ({ page }) => {
    await openPhotos(page, 3000);

    const result = await sampleThrice(page);
    const stats = await page.evaluate(() =>
      (
        window as unknown as {
          __thumbnailStats: () => { bytes: number; entries: number };
        }
      ).__thumbnailStats()
    );

    // 判定はしない。値を出力して spec に転記する（spec §7-2 / §8）
    console.log(
      JSON.stringify(
        {
          scale: 3000,
          p95: result.p95,
          jankRatio: result.jankRatio,
          cacheBytes: stats.bytes,
          cacheEntries: stats.entries,
          bytesPerThumbnail: stats.entries > 0 ? stats.bytes / stats.entries : 0,
        },
        null,
        2
      )
    );
    expect(stats.entries).toBeGreaterThan(0);
  });

  test("新実装 / 3,000 枚・フィルムストリップを開いた状態", async ({ page }) => {
    // Task 14 でストリップの仮想化を見送った判断の裏付け（要素数は写真の枚数分）。
    // グリッドのスクロールはプレビューの裏でも動くので、同じ指標で比べられる
    await openPhotos(page, 3000);
    await page.getByRole("option").first().dblclick();
    await expect(page.getByRole("dialog", { name: "画像プレビュー" })).toBeVisible();
    // 前提条件: 枚数分の枠が実際に DOM に出ていること（仮想化していない証拠）
    const frames = await page.locator(".strip button").count();
    expect(frames).toBe(3000);

    const result = await sampleThrice(page);
    console.log(JSON.stringify({ scale: "3000+strip", ...result }, null, 2));
  });

  test("解像度を変えて往復してもキャッシュが上限を超えない（spec §4-2）", async ({
    page,
  }) => {
    await openPhotos(page, 3000);
    const stats = () =>
      page.evaluate(() =>
        (
          window as unknown as {
            __thumbnailStats: () => { bytes: number; entries: number };
          }
        ).__thumbnailStats()
      );

    // 1 解像度で通しにスクロールして溜める
    await measureScroll(page, SCROLLER, 4000);
    const single = await stats();
    // 前提条件: 台帳に載っていること。0 件なら以降の比較は何も言っていない
    expect(single.entries).toBeGreaterThan(0);

    // サイズを変えると要求解像度が変わる。キーは path:maxDimension なので
    // 別エントリとして積み上がる（path だけをキーにすると増えない）
    for (const size of ["512", "96"]) {
      await page.getByLabel("サイズ", { exact: true }).fill(size);
      await measureScroll(page, SCROLLER, 4000);
    }
    const multi = await stats();

    console.log(JSON.stringify({ single, multi }, null, 2));
    expect(multi.entries).toBeGreaterThan(single.entries);
    // 1 件だけ上限を超える項目は保持する仕様なので、わずかな超過は許す
    expect(multi.bytes).toBeLessThanOrEqual(CACHE_BYTE_LIMIT * 1.05);
  });
});