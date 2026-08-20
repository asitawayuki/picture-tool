import { expect, test } from "@playwright/test";
import { clearStorageOnce, installTauriStub } from "./stub";

/**
 * spec §4-2「サムネイル取得」。
 *
 * 取得キューの内部規則（LIFO・priming・範囲外の破棄・LRU の追い出し順）は
 * `requestQueue.test.ts` と `thumbnailCache.test.ts` が純粋ロジックとして見ている。
 * ここで見るのは**それらを繋いだ結果として画面が壊れていないこと**:
 * サムネイルが実際に出ること、解像度が上がったときに取り直すこと、
 * 取得した分が LRU の台帳に載っていること（載っていなければ上限が効かない）。
 *
 * Task 12 Step 10 の実機確認の代わり（GUI ウィンドウを操作する手段が無いため）。
 */
test.describe("サムネイル取得", () => {
  test.beforeEach(async ({ page }) => {
    await installTauriStub(page, { imageCount: 12 });
    await clearStorageOnce(page);
    await page.goto("/");
    await page.getByRole("button", { name: "photos", exact: false }).first().click();
  });

  test("サムネイルが実データで描画される", async ({ page }) => {
    const first = page.getByRole("option", { name: "photo-0000.jpg" }).locator("img");
    await expect(first).toBeVisible();
    await expect(first).toHaveAttribute("src", /^data:image\/jpeg;base64,.+/);
  });

  test("タイルを大きくすると大きい解像度で取り直す", async ({ page }) => {
    await expect(page.getByRole("option", { name: "photo-0000.jpg" }).locator("img")).toBeVisible();

    const sizesBefore = await page.evaluate(() =>
      ((window as any).__thumbnailRequests as { maxDimension: number }[]).map(
        (r) => r.maxDimension
      )
    );
    // 前提条件: 1 度は取りに行っていること。0 件だと「増えた」が判定できない
    expect(sizesBefore.length).toBeGreaterThan(0);
    const maxBefore = Math.max(...sizesBefore);

    // タイルの目標幅を上げる＝1 枚あたりが広くなる（列は減る）。
    // 低解像度を引き伸ばしたままにしない
    await page.getByLabel("サイズ", { exact: true }).fill("512");

    await expect
      .poll(async () =>
        page.evaluate(
          () =>
            ((window as any).__thumbnailRequests as { maxDimension: number }[]).length
        )
      )
      .toBeGreaterThan(sizesBefore.length);

    const sizesAfter = await page.evaluate(() =>
      ((window as any).__thumbnailRequests as { maxDimension: number }[]).map(
        (r) => r.maxDimension
      )
    );
    expect(Math.max(...sizesAfter)).toBeGreaterThan(maxBefore);
  });

  test("取得した分が LRU の台帳に載る", async ({ page }) => {
    // 台帳に載らないとバイト上限が一生発火せず、eviction が無い現行と同じになる。
    // 画面上は正常に見えるので、ここを見ないと気付けない
    await expect(page.getByRole("option", { name: "photo-0000.jpg" }).locator("img")).toBeVisible();

    await expect
      .poll(async () =>
        page.evaluate(() => {
          const stats = (window as any).__thumbnailStats;
          return stats ? stats().entries : 0;
        })
      )
      .toBeGreaterThan(0);

    const stats = await page.evaluate(() => (window as any).__thumbnailStats());
    expect(stats.bytes).toBeGreaterThan(0);
  });

  test("フォルダーを変えてもサムネイルが出る", async ({ page }) => {
    // resetForFolder はキューを空にして priming を張り直す。
    // 張り直しを間違えると 2 つ目以降のフォルダーが永久に埋まらない
    await expect(page.getByRole("option", { name: "photo-0000.jpg" }).locator("img")).toBeVisible();

    await page.getByRole("button", { name: "archive", exact: false }).first().click();

    // 前提条件: 移動先は別パスなのでキャッシュが当たらず、実際に取りに行くこと
    await expect
      .poll(async () =>
        page.evaluate(() =>
          ((window as any).__thumbnailRequests as { path: string }[]).filter((r) =>
            r.path.startsWith("/archive/")
          ).length
        )
      )
      .toBeGreaterThan(0);

    await expect(page.getByRole("option", { name: "photo-0000.jpg" }).locator("img")).toHaveAttribute(
      "src",
      /^data:image\/jpeg;base64,.+/
    );
  });
});
