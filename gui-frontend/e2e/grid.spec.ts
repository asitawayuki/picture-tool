import { expect, test } from "@playwright/test";
import { clearStorageOnce, installTauriStub } from "./stub";

/**
 * spec §4-1「密度と操作」/ §4-2「仮想スクロール」。
 *
 * 列数・行高・可視範囲の規則そのものは `gridMetrics.test.ts` が純粋ロジックとして
 * 見ている。ここで見るのは **runes と DOM を繋いだ結果**：ロール構造、
 * 仮想化で DOM に出る枚数、キー割り当て（現行から変わる）、フォーカスの行方。
 */
test.describe("写真グリッド", () => {
  test.beforeEach(async ({ page }) => {
    await installTauriStub(page, { imageCount: 3000 });
    await clearStorageOnce(page);
    await page.goto("/");
    await page.getByRole("button", { name: "photos", exact: false }).first().click();
  });

  test("3,000 枚でも DOM 上のタイルは可視分だけ（spec §4-2）", async ({ page }) => {
    const grid = page.getByRole("listbox", { name: "写真" });
    await expect(grid.getByRole("option").first()).toBeVisible();

    const rendered = await grid.getByRole("option").count();
    // 前提条件: 3,000 枚が読み込まれていること
    expect(await grid.getByRole("option").first().getAttribute("aria-setsize")).toBe(
      "3000"
    );
    expect(rendered).toBeGreaterThan(0);
    expect(rendered).toBeLessThan(120);
  });

  test("各タイルが aria-setsize と aria-posinset を持つ（spec §4-1）", async ({ page }) => {
    const first = page.getByRole("listbox", { name: "写真" }).getByRole("option").first();
    await expect(first).toHaveAttribute("aria-setsize", "3000");
    await expect(first).toHaveAttribute("aria-posinset", "1");
  });

  test("listbox の直接の子は option だけ（spec §4-1）", async ({ page }) => {
    const kinds = await page
      .getByRole("listbox", { name: "写真" })
      .evaluate((el) => Array.from(el.children).map((c) => c.getAttribute("role")));
    expect(kinds.length).toBeGreaterThan(0);
    expect(new Set(kinds)).toEqual(new Set(["option"]));
  });

  test("Space は選択、Enter はプレビュー（現行から変わる。spec §4-1）", async ({ page }) => {
    const grid = page.getByRole("listbox", { name: "写真" });
    await grid.getByRole("option").first().click();
    await expect(page.getByRole("button", { name: "1 枚を変換" })).toBeVisible();

    await page.keyboard.press("Space");
    await expect(page.getByRole("button", { name: "0 枚を変換" })).toBeVisible();

    await page.keyboard.press("Enter");
    await expect(page.getByRole("dialog", { name: "画像プレビュー" })).toBeVisible();
  });

  test("矢印キーでフォーカスが動き、下キーは 1 行分動く", async ({ page }) => {
    const grid = page.getByRole("listbox", { name: "写真" });
    await grid.getByRole("option").first().click();

    const focusedPos = () =>
      page.evaluate(() => document.activeElement?.getAttribute("aria-posinset"));

    // 前提条件: クリックでタイル自身に DOM フォーカスが入っていること。
    // ここが null（body）だと、以降の期待は roving tabindex が
    // 効いていないことすら検出できずに落ちるだけになる
    expect(await focusedPos()).toBe("1");

    await page.keyboard.press("ArrowRight");
    expect(await focusedPos()).toBe("2");

    // 列数はウィンドウ幅から決まる。1 行下は「現在 + 列数」
    const columns = await grid.evaluate(
      (el) => getComputedStyle(el).gridTemplateColumns.split(" ").length
    );
    await page.keyboard.press("ArrowDown");
    expect(Number(await focusedPos())).toBe(2 + columns);
  });

  test("仮想化でタイルが消えてもフォーカスがグリッドの外へ落ちない（spec §4-1）", async ({
    page,
  }) => {
    const grid = page.getByRole("listbox", { name: "写真" });
    await grid.getByRole("option").first().click();
    // 前提条件: タイル自身に DOM フォーカスがあること。
    // ここが body だと「落ちなかった」は検査になっていない
    expect(await page.evaluate(() => document.activeElement?.getAttribute("role"))).toBe(
      "option"
    );

    // 1 枚目が描画範囲から確実に外れるところまで飛ばす
    await grid.evaluate((el) => (el.scrollTop = el.scrollHeight));

    await expect
      .poll(() =>
        page.evaluate(() => {
          const el = document.activeElement;
          if (!el || el === document.body) return "body";
          return el.closest('[role="listbox"]') ? "grid" : "outside";
        })
      )
      .toBe("grid");
  });

  test("末尾までスクロールしても最後の 1 枚に到達できる", async ({ page }) => {
    const grid = page.getByRole("listbox", { name: "写真" });
    await grid.evaluate((el) => (el.scrollTop = el.scrollHeight));
    await expect(grid.getByRole("option").last()).toHaveAttribute(
      "aria-posinset",
      "3000"
    );
  });
});
