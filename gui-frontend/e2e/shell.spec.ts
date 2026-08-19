import { expect, test } from "@playwright/test";
import { clearStorageOnce, installTauriStub } from "./stub";

test.describe("アプリシェル", () => {
  test.beforeEach(async ({ page }) => {
    await installTauriStub(page);
    await clearStorageOnce(page);
  });

  test("rail の 3 destination が切り替わる", async ({ page }) => {
    await page.goto("/");
    const rail = page.getByRole("navigation", { name: "モード" });
    for (const label of ["変換", "情報", "フレーム"]) {
      await rail.getByRole("button", { name: label }).click();
      await expect(rail.getByRole("button", { name: label })).toHaveAttribute(
        "aria-current",
        "page"
      );
    }
  });

  test("rail のラベルが 80px に収まる（spec §8）", async ({ page }) => {
    await page.goto("/");
    const rail = page.getByRole("navigation", { name: "モード" });
    // 前提条件: rail の幅が仕様どおり 80px であること
    expect((await rail.boundingBox())!.width).toBe(80);
    for (const label of ["変換", "情報", "フレーム"]) {
      const box = await rail.getByText(label, { exact: true }).boundingBox();
      expect(box!.width, `${label} が rail からはみ出す`).toBeLessThanOrEqual(80);
    }
  });

  test("左カラムはドラッグで幅が変わり、リロード後も保たれる", async ({ page }) => {
    await page.goto("/");
    const handle = page.getByRole("separator", { name: "左カラムの幅" });

    // 前提条件: 既定幅は 240px（columns.ts の COLUMN_SPECS.folder.default）
    await expect(handle).toHaveAttribute("aria-valuenow", "240");

    const box = (await handle.boundingBox())!;
    const centerX = box.x + box.width / 2;
    const centerY = box.y + box.height / 2;
    // 前提条件: ハンドルの中心が rail(80) + 既定幅(240) の位置にあること。
    // ここがずれているならハンドルの配置か margin の相殺が間違っている
    expect(Math.round(centerX)).toBe(80 + 240);

    const targetX = centerX + 60;
    await page.mouse.move(centerX, centerY);
    await page.mouse.down();
    await page.mouse.move(targetX, centerY, { steps: 5 });
    await page.mouse.up();

    // 幅は AppShell の drag() が `clientX - rect.left - RAIL_WIDTH` で出す。
    // シェルは左端から始まるので rect.left は 0
    const expected = String(Math.round(targetX - 80));
    await expect(handle).toHaveAttribute("aria-valuenow", expected);

    await page.reload();
    await expect(page.getByRole("separator", { name: "左カラムの幅" })).toHaveAttribute(
      "aria-valuenow",
      expected
    );
  });

  test("localStorage に壊れた値が入っていても既定幅で起動する", async ({ page }) => {
    await page.addInitScript(() => {
      localStorage.setItem(
        "picture-tool.layout.widths.v1",
        JSON.stringify({ folder: -99999, convert: "wide" })
      );
    });
    await page.goto("/");
    await expect(page.getByRole("separator", { name: "左カラムの幅" })).toHaveAttribute(
      "aria-valuenow",
      "180" // COLUMN_SPECS.folder.min
    );
    await expect(page.getByRole("separator", { name: "右パネルの幅" })).toHaveAttribute(
      "aria-valuenow",
      "320" // COLUMN_SPECS.convert.default
    );
  });

  test("フレームモードだけ左カラムの幅が別に持たれる（spec §3-1）", async ({ page }) => {
    await page.goto("/");
    const rail = page.getByRole("navigation", { name: "モード" });
    const handle = () => page.getByRole("separator", { name: "左カラムの幅" });

    await expect(handle()).toHaveAttribute("aria-valuenow", "240"); // folder
    await rail.getByRole("button", { name: "フレーム" }).click();
    await expect(handle()).toHaveAttribute("aria-valuenow", "220"); // presets
  });
});
