import { expect, test } from "@playwright/test";

/**
 * 部品ギャラリーの検査。
 *
 * spec §6 の段階 2 の完了の目印は「全部品 × 全 state × 明暗を目視できる」。
 * 目視は人が行うが、その前提（節が存在する・明暗の両方が実際に別の色になる）は
 * 機械で確かめられる。ここはその前提だけを見る。
 *
 * 明暗の切り替えは data-theme で行う。tokens.css の 4 ブロック構造が
 * 両方向に効かないと、ここで同じ色が返る。
 */
const SPECIMENS = [
  "Button",
  "IconButton",
  "Card",
  "TextField",
  "Switch",
  "Slider",
  "Select",
  "SegmentedButton",
  "Rating",
  "LinearProgress",
  "Dialog",
];

test.describe("部品ギャラリー", () => {
  test("すべての節が描画される", async ({ page }) => {
    await page.goto("/gallery.html");
    for (const name of SPECIMENS) {
      await expect(page.locator(`[data-specimen="${name}"]`)).toBeVisible();
    }
  });

  test("data-theme でライトとダークが実際に別の面色になる", async ({ page }) => {
    await page.goto("/gallery.html");

    const surfaceOf = async (theme: "light" | "dark") => {
      await page.evaluate((t) => {
        document.documentElement.dataset.theme = t;
      }, theme);
      return page.locator(".gallery").evaluate(
        (el) => getComputedStyle(el).backgroundColor
      );
    };

    const light = await surfaceOf("light");
    const dark = await surfaceOf("dark");

    // 前提条件: そもそも色が解決できていないと両方 "rgba(0, 0, 0, 0)" になり、
    // 「違う」という主張が成り立たなくなる
    expect(light).not.toBe("rgba(0, 0, 0, 0)");
    expect(dark).not.toBe("rgba(0, 0, 0, 0)");
    expect(light).not.toBe(dark);
  });

  test("明暗のスクリーンショットを撮る", async ({ page }) => {
    await page.goto("/gallery.html");
    for (const theme of ["light", "dark"] as const) {
      await page.evaluate((t) => {
        document.documentElement.dataset.theme = t;
      }, theme);
      await page.screenshot({
        path: `e2e/__screenshots__/gallery-${theme}.png`,
        fullPage: true,
      });
    }
  });
});

test("TextField(number) は正規化の結果が同値でも表示を戻す", async ({ page }) => {
  await page.goto("/gallery.html");
  const input = page.getByLabel("出力幅の上限");

  // 前提条件: 初期値が 1080（4 の倍数）であること。ここが崩れていると
  // 「1002 を入れても 1000 に戻る」という主張の土台が消える
  await expect(input).toHaveValue("1080");

  // 1004 → 正規化で 1004（4 の倍数なのでそのまま）
  await input.fill("1004");
  await input.blur();
  await expect(input).toHaveValue("1004");

  // 1006 → 正規化で 1004。state は 1004 のまま動かないが、表示は戻る
  await input.fill("1006");
  await input.blur();
  await expect(input).toHaveValue("1004");
});

test("Dialog は Esc で閉じ、フォーカスを元の場所へ返す", async ({ page }) => {
  await page.goto("/gallery.html");
  const opener = page.getByRole("button", { name: "通常のダイアログ" });
  await opener.click();

  const dialog = page.getByRole("dialog", { name: "変換結果" });
  await expect(dialog).toBeVisible();

  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
  // focusTrap の destroy が元のフォーカスへ戻すこと
  await expect(opener).toBeFocused();
});

test("Dialog のフォーカスは中に閉じ込められる", async ({ page }) => {
  await page.goto("/gallery.html");
  await page.getByRole("button", { name: "通常のダイアログ" }).click();

  const inside = page.getByRole("dialog", { name: "変換結果" });
  // 前提条件: ダイアログの外にもフォーカス可能な要素が存在すること。
  // 存在しなければ「外へ出ない」は自明に成立してしまう
  expect(await page.getByRole("button", { name: "危険なダイアログ" }).count()).toBe(1);

  for (let i = 0; i < 8; i++) {
    await page.keyboard.press("Tab");
    expect(await inside.evaluate((el) => el.contains(document.activeElement))).toBe(true);
  }
});

test("Rating は同じ★の再クリックで 0 に戻る", async ({ page }) => {
  await page.goto("/gallery.html");
  const rating = page.getByRole("slider", { name: "レーティング" }).first();

  // 前提条件: 初期値が 3
  await expect(rating).toHaveAttribute("aria-valuenow", "3");

  await rating.locator("button").nth(2).click(); // 3 番目の★ = 現在値と同じ
  await expect(rating).toHaveAttribute("aria-valuenow", "0");

  await rating.locator("button").nth(4).click(); // 5 番目の★
  await expect(rating).toHaveAttribute("aria-valuenow", "5");
});

test("Rating は矢印キーで増減し 0〜5 で止まる", async ({ page }) => {
  await page.goto("/gallery.html");
  const rating = page.getByRole("slider", { name: "レーティング" }).first();
  await rating.focus();

  await page.keyboard.press("End");
  await expect(rating).toHaveAttribute("aria-valuenow", "5");
  await page.keyboard.press("ArrowRight");
  await expect(rating).toHaveAttribute("aria-valuenow", "5");

  await page.keyboard.press("Home");
  await expect(rating).toHaveAttribute("aria-valuenow", "0");
  await page.keyboard.press("ArrowLeft");
  await expect(rating).toHaveAttribute("aria-valuenow", "0");
});
