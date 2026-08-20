import { expect, test } from "@playwright/test";
import { clearStorageOnce, installTauriStub } from "./stub";

/**
 * spec §4-1「密度と操作」/ §4-2「仮想スクロール」。
 *
 * 列数・行高・可視範囲の規則そのものは `gridMetrics.test.ts` が純粋ロジックとして
 * 見ている。ここで見るのは **runes と DOM を繋いだ結果**：ロール構造、
 * 仮想化で DOM に出る枚数、キー割り当て（現行から変わる）、フォーカスの行方。
 */
/**
 * スクロールする箱は `role="listbox"` の要素**ではない**。仮想化の余白は
 * listbox 側の padding で作っており、padding は要素自身の箱を膨らませるので、
 * スクローラーと兼ねられない（PhotoGrid.svelte のコメント）。
 * 位置を動かすときは外側の `.scroller` を掴むこと。
 */
const scrollerOf = (page: import("@playwright/test").Page) => page.locator(".scroller");

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
    await scrollerOf(page).evaluate((el) => (el.scrollTop = el.scrollHeight));

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
    await scrollerOf(page).evaluate((el) => (el.scrollTop = el.scrollHeight));
    await expect(grid.getByRole("option").last()).toHaveAttribute(
      "aria-posinset",
      "3000"
    );
  });

  test("プレビューのフィルムストリップが現在位置を示し、クリックで送れる", async ({
    page,
  }) => {
    const grid = page.getByRole("listbox", { name: "写真" });
    await grid.getByRole("option").first().dblclick();

    const viewer = page.getByRole("dialog", { name: "画像プレビュー" });
    await expect(viewer).toBeVisible();
    await expect(viewer.getByText("1 / 3000")).toBeVisible();

    // ストリップの枠は button のまま（role="listitem" で上書きしない。Step 3）
    await viewer.getByRole("button", { name: /^5 枚目/ }).click();
    await expect(viewer.getByText("5 / 3000")).toBeVisible();
    await expect(viewer.getByRole("button", { name: /^5 枚目/ })).toHaveAttribute(
      "aria-current",
      "true"
    );
  });

  test("フィルムストリップの枠が実データで埋まる（spec §4-1）", async ({ page }) => {
    // 見ているのは「グリッドと同じキャッシュから、ストリップ用の解像度で
    // 実際に絵が届く」ところまで。要求種別（pinned / discardable）の規則そのものは
    // requestQueue.test.ts が見ており、**ここでは判別できない** ──
    // 取得キューへの要求は `values`（SvelteMap）を読むので、
    // グリッドのサムネイルが届くたびにストリップの $effect が再走し、
    // 捨てられた要求が出し直されるため（実測で確認済み）
    const grid = page.getByRole("listbox", { name: "写真" });
    await grid.getByRole("option").first().dblclick();

    const viewer = page.getByRole("dialog", { name: "画像プレビュー" });
    await expect(viewer).toBeVisible();
    // ストリップの要求が捌ける前にグリッドを動かす（＝破棄の機会を作る）
    await scrollerOf(page).evaluate((el) => (el.scrollTop = el.scrollHeight));

    await expect(
      viewer.getByRole("button", { name: /^1 枚目/ }).locator("img")
    ).toHaveAttribute("src", /^data:image\/jpeg;base64,.+/);
  });

  test("プレビューは ← → で送れ、Esc で閉じる", async ({ page }) => {
    const grid = page.getByRole("listbox", { name: "写真" });
    await grid.getByRole("option").first().dblclick();
    const viewer = page.getByRole("dialog", { name: "画像プレビュー" });

    await page.keyboard.press("ArrowRight");
    await expect(viewer.getByText("2 / 3000")).toBeVisible();
    await page.keyboard.press("ArrowLeft");
    await expect(viewer.getByText("1 / 3000")).toBeVisible();

    await page.keyboard.press("Escape");
    await expect(viewer).toHaveCount(0);
  });

  test("プレビューの矩形ドラッグで拡大し、クリックで解除する", async ({ page }) => {
    // ズームは Task 14 で「移設したが触っていない」唯一の部分。
    // 壊れていたら写し間違いなので、ここだけは実際にドラッグして見る
    const grid = page.getByRole("listbox", { name: "写真" });
    await grid.getByRole("option").first().dblclick();
    const viewer = page.getByRole("dialog", { name: "画像プレビュー" });
    const image = viewer.locator("img.preview-image");
    await expect(image).toBeVisible();

    // 前提条件: 拡大前は transform を持たないこと。
    // 最初から scale が乗っていたら、以降の期待は何も検出しない
    expect(await image.evaluate((el) => el.style.transform)).toBe("");

    const box = (await image.boundingBox())!;
    await page.mouse.move(box.x + 40, box.y + 40);
    await page.mouse.down();
    await page.mouse.move(box.x + 240, box.y + 290, { steps: 8 });
    await page.mouse.up();

    await expect
      .poll(() => image.evaluate((el) => el.style.transform))
      .toContain("scale(");

    // ズーム中の画面をクリックすると解除される
    await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
    await expect.poll(() => image.evaluate((el) => el.style.transform)).toBe("");
  });
});