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

  test("カラム幅はキーボードでも動く（ポインタを持たない利用者の経路）", async ({
    page,
  }) => {
    await page.goto("/");
    const handle = page.getByRole("separator", { name: "左カラムの幅" });
    // 前提条件: 既定幅は 240px（columns.ts の COLUMN_SPECS.folder.default）
    await expect(handle).toHaveAttribute("aria-valuenow", "240");

    // Tab だけで到達できること。focus() で飛ばすと「タブ順に居る」を検査できない
    let reached = false;
    for (let i = 0; i < 40 && !reached; i++) {
      await page.keyboard.press("Tab");
      reached =
        (await page.evaluate(() => document.activeElement?.getAttribute("aria-label"))) ===
        "左カラムの幅";
    }
    expect(reached).toBe(true);

    // 左右キーで 16px ずつ（AppShell の KEYBOARD_STEP）
    await page.keyboard.press("ArrowRight");
    await expect(handle).toHaveAttribute("aria-valuenow", "256");
    await page.keyboard.press("ArrowLeft");
    await page.keyboard.press("ArrowLeft");
    await expect(handle).toHaveAttribute("aria-valuenow", "224");
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

  /**
   * spec §3-1「右パネルの折りたたみ」。幅 0 で畳む実装ではないので、
   * 畳んだときにリサイザーごと消え、開くボタンと主導線はパネルの外（グリッド
   * ヘッダー）に残る ── この 3 点が同時に成り立つことが折りたたみの要件。
   */
  test("右パネルを畳んでも開くボタンと主アクションが残る（spec §3-1）", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "photos", exact: false }).first().click();

    // 前提条件: 畳む前は右パネルの中に主ボタンがある
    await expect(page.getByRole("button", { name: "0 枚を変換" })).toHaveCount(1);
    await expect(page.getByRole("separator", { name: "右パネルの幅" })).toBeVisible();

    await page.getByRole("button", { name: "右パネルを畳む" }).click();

    // 幅 0 で畳む実装ではないので、リサイザーごと消える
    await expect(page.getByRole("separator", { name: "右パネルの幅" })).toHaveCount(0);
    // 開くボタンはグリッドヘッダー（パネルの外）にあるので残る
    await expect(page.getByRole("button", { name: "右パネルを開く" })).toBeVisible();
    // 主導線もヘッダーへ移る
    await expect(page.getByRole("button", { name: "0 枚を変換" })).toHaveCount(1);

    await page.getByRole("button", { name: "右パネルを開く" }).click();
    await expect(page.getByRole("separator", { name: "右パネルの幅" })).toBeVisible();
  });

  test("折りたたみ状態はリロード後も保たれ、幅とは別キーである", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "右パネルを畳む" }).click();
    await page.reload();
    await expect(page.getByRole("button", { name: "右パネルを開く" })).toBeVisible();

    // 幅のキーは壊れていない（開いたら既定幅で戻る）
    await page.getByRole("button", { name: "右パネルを開く" }).click();
    await expect(page.getByRole("separator", { name: "右パネルの幅" })).toHaveAttribute(
      "aria-valuenow",
      "320"
    );
  });

  test("選択枚数と全解除がグリッドヘッダーに出る（spec §5-1）", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "photos", exact: false }).first().click();

    // 前提条件: 選択が 0 のときは出ない
    await expect(page.getByText(/枚選択中/)).toHaveCount(0);

    await page.getByRole("option", { name: /photo-0000/ }).click();
    await page.getByRole("option", { name: /photo-0001/ }).click();
    await expect(page.getByText("2 枚選択中")).toBeVisible();

    await page.getByRole("button", { name: "全解除" }).click();
    await expect(page.getByText(/枚選択中/)).toHaveCount(0);
  });
});
