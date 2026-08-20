import { expect, test } from "@playwright/test";

/**
 * スタブ無しで開く。Tauri の外では `window.__TAURI_INTERNALS__` が無く、
 * `invoke` が全部 reject するので、エラー経路が一度に全部出る（spec §7-3）。
 *
 *  - 画面が真っ白にならない
 *  - 握りつぶさずトーストで知らせる
 *  - 例外がコンソールへ漏れない
 */
test.describe("エラー経路（スタブ無し）", () => {
  test("IPC が全部失敗しても画面は立ち上がり、トーストで知らせる", async ({ page }) => {
    const pageErrors: string[] = [];
    page.on("pageerror", (e) => pageErrors.push(String(e)));

    await page.goto("/");

    // シェルは出る
    await expect(page.getByRole("navigation", { name: "モード" })).toBeVisible();
    await expect(page.getByRole("listbox", { name: "写真" })).toBeVisible();

    // 握りつぶさずに知らせている（ドライブ一覧・お気に入り・プリセットの取得失敗）
    const alert = page.getByRole("region", { name: "通知" }).getByRole("alert").first();
    await expect(alert).toBeVisible();
    // 中身のある文言であること。describeError が undefined を返しても
    // 「トーストは出た」だけなら通ってしまう
    await expect(alert).toContainText("失敗");

    // 捕まえ損ねた例外が無い。どこかで .catch を付け忘れると
    // unhandled rejection としてここに出る
    expect(pageErrors).toEqual([]);
  });

  test("3 モードすべてが IPC 失敗下でも描画される", async ({ page }) => {
    const pageErrors: string[] = [];
    page.on("pageerror", (e) => pageErrors.push(String(e)));

    await page.goto("/");
    const rail = page.getByRole("navigation", { name: "モード" });
    for (const label of ["情報", "フレーム", "変換"]) {
      await rail.getByRole("button", { name: label }).click();
      await expect(rail.getByRole("button", { name: label })).toHaveAttribute(
        "aria-current",
        "page"
      );
    }
    expect(pageErrors).toEqual([]);
  });
});
