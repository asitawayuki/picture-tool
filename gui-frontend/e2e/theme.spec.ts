import { expect, test, type Page } from "@playwright/test";
import { clearStorageOnce, installTauriStub } from "./stub";
// イベント名は契約から取る。ここに書き写すと、Rust 側の改名でテストだけが
// 古い名前のまま緑を出し続ける（stub.ts が形を契約から取るのと同じ理由）
import { osColorSchemeChangedEvent } from "../src/lib/dto.contract";

/**
 * テーマ（ライト / ダーク / システム追従）。
 *
 * 検査の中心は 2 つ:
 *   - 明示指定は OS の状態を**無視する**（それができないなら「対応した」と言えない）
 *   - システム追従は**起動中の変化にも追従する**（Rust から流れてくるイベント経由）
 *
 * 面色まで見るのは、`data-theme` を書いただけで色が変わらない状態
 * （トークンの割り当て漏れ）を素通りさせないため。
 */
const rail = (page: Page) => page.getByRole("navigation", { name: "モード" });
const themeButton = (page: Page) => rail(page).getByRole("button", { name: /^テーマ:/ });

/** rail の面色。トークンが解決できていれば透明にはならない */
async function railColor(page: Page): Promise<string> {
  return rail(page).evaluate((el) => getComputedStyle(el).backgroundColor);
}

/** Rust が流す OS のテーマ変更を再現する */
async function emitOsScheme(page: Page, scheme: "light" | "dark"): Promise<void> {
  await page.evaluate(
    ([event, s]) => {
      (window as unknown as Record<string, (e: string, p: unknown) => void>).__emitTauriEvent(
        event,
        s
      );
    },
    [osColorSchemeChangedEvent, scheme] as const
  );
}

test.describe("テーマ", () => {
  test.beforeEach(async ({ page }) => {
    await clearStorageOnce(page);
  });

  test("rail のボタンで システム → ライト → ダーク と巡り、3 回で元に戻る", async ({ page }) => {
    await installTauriStub(page);
    await page.goto("/");

    // 前提条件: 初期状態はシステム追従
    await expect(themeButton(page)).toHaveAttribute("aria-label", /^テーマ: システム/);

    await themeButton(page).click();
    await expect(themeButton(page)).toHaveAttribute("aria-label", /^テーマ: ライト/);
    await expect(page.locator("html")).toHaveAttribute("data-theme", "light");

    await themeButton(page).click();
    await expect(themeButton(page)).toHaveAttribute("aria-label", /^テーマ: ダーク/);
    await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");

    await themeButton(page).click();
    await expect(themeButton(page)).toHaveAttribute("aria-label", /^テーマ: システム/);
  });

  test("どの状態でもラベルが rail の 80px に収まる（spec §8）", async ({ page }) => {
    await installTauriStub(page);
    await page.goto("/");

    // 前提条件: rail の幅が仕様どおり 80px であること
    expect((await rail(page).boundingBox())!.width).toBe(80);

    for (const label of ["システム", "ライト", "ダーク"]) {
      await expect(themeButton(page)).toHaveAttribute("aria-label", new RegExp(`^テーマ: ${label}`));
      const box = await rail(page).getByText(label, { exact: true }).boundingBox();
      expect(box!.width, `${label} が rail からはみ出す`).toBeLessThanOrEqual(80);
      await themeButton(page).click();
    }
  });

  test("ライトとダークで実際に面色が変わる", async ({ page }) => {
    await installTauriStub(page);
    await page.goto("/");

    await themeButton(page).click(); // ライト
    await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
    const light = await railColor(page);

    await themeButton(page).click(); // ダーク
    await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
    const dark = await railColor(page);

    // 前提条件: 色が解決できていないと両方 "rgba(0, 0, 0, 0)" になり、
    // 「違う」という主張自体が成り立たなくなる
    expect(light).not.toBe("rgba(0, 0, 0, 0)");
    expect(dark).not.toBe("rgba(0, 0, 0, 0)");
    expect(light).not.toBe(dark);
  });

  test("選んだテーマはリロード後も残る", async ({ page }) => {
    await installTauriStub(page);
    await page.goto("/");

    await themeButton(page).click();
    await themeButton(page).click(); // ダーク
    await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");

    await page.reload();
    await expect(themeButton(page)).toHaveAttribute("aria-label", /^テーマ: ダーク/);
    await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  });

  test("システム追従は起動中の OS の切り替えに追従する", async ({ page }) => {
    await installTauriStub(page);
    await page.goto("/");

    // 前提条件: 起動時の OS はライトで、追従状態にある
    await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
    await expect(themeButton(page)).toHaveAttribute("aria-label", /^テーマ: システム/);

    await emitOsScheme(page, "dark");
    await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");

    await emitOsScheme(page, "light");
    await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
  });

  test("ライトを明示していれば OS がダークになっても変わらない", async ({ page }) => {
    await installTauriStub(page);
    await page.goto("/");

    await themeButton(page).click(); // 明示的にライト
    await expect(page.locator("html")).toHaveAttribute("data-theme", "light");

    await emitOsScheme(page, "dark");

    // 同じイベントでシステム追従なら dark になることは上のテストが示している。
    // ここで light のままなのは「明示指定が OS を無視する」の結果である
    await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
    await expect(themeButton(page)).toHaveAttribute("aria-label", /^テーマ: ライト/);
  });

  test("起動時に OS がダークならシステム追従で暗く始まる", async ({ page }) => {
    await installTauriStub(page, { osColorScheme: "dark" });
    await page.goto("/");

    await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
    await expect(themeButton(page)).toHaveAttribute("aria-label", /^テーマ: システム/);
  });

  test("解決したテーマがウィンドウへ伝わる（ネイティブのダイアログを揃えるため）", async ({
    page,
  }) => {
    await installTauriStub(page);
    await page.goto("/");

    await themeButton(page).click();
    await themeButton(page).click(); // ダーク

    await expect
      .poll(async () =>
        page.evaluate(() => {
          const calls = (window as any).__stubCalls as { cmd: string; args: any }[];
          const hits = calls.filter((c) => c.cmd === "set_window_theme");
          return hits.length === 0 ? null : hits[hits.length - 1].args.theme;
        })
      )
      .toBe("dark");
  });
});
