import { expect, test } from "@playwright/test";
import { clearStorageOnce, installTauriStub } from "./stub";

/**
 * spec §5-2（メタデータパネル）と §3-2（editingPath と focusedPath の分離）。
 *
 * 本刷新で作るのは静的なレイアウトまで。`read_image_metadata` /
 * `write_image_metadata` / `grant_metadata_editing` は次工程で追加される
 * Tauri コマンドなので、実データは撮影情報（`get_exif_info`）だけである。
 *
 * ファイル名はグリッドのタイルにも出るので、パネル側の表示を見るときは
 * 右カラム（`[data-region="right"]`）に絞る。絞らないと strict mode で
 * 「2 つ当たった」として落ちる ── **落ちるのは正しく、
 * どちらを見ているのか曖昧なテストにしないための制約**。
 */
test.describe("メタデータモード（レイアウトのみ）", () => {
  test.beforeEach(async ({ page }) => {
    await installTauriStub(page, { imageCount: 12 });
    await clearStorageOnce(page);
    await page.goto("/");
    await page.getByRole("button", { name: "photos", exact: false }).first().click();
    await page.getByRole("option").first().click();
    await page
      .getByRole("navigation", { name: "モード" })
      .getByRole("button", { name: "情報" })
      .click();
  });

  test("spec §5-2 の 7 要素がすべて場所を持つ", async ({ page }) => {
    const panel = page.locator('[data-region="right"]');

    await expect(panel.getByText("photo-0000.jpg")).toBeVisible();
    await expect(page.getByLabel("タイトル")).toBeVisible();
    await expect(page.getByLabel("コメント")).toBeVisible();
    await expect(page.getByRole("slider", { name: "レーティング" })).toBeVisible();
    await expect(panel.getByText("撮影情報")).toBeVisible();
    await expect(panel.getByText("書き込みの許可")).toBeVisible();
    await expect(page.getByRole("button", { name: "保存して次の写真へ" })).toBeVisible();
    await expect(page.getByRole("button", { name: "保存", exact: true })).toBeVisible();
  });

  test("保存ボタンは disabled（データ接続は次工程。spec §5-2）", async ({ page }) => {
    await expect(page.getByRole("button", { name: "保存して次の写真へ" })).toBeDisabled();
    await expect(page.getByRole("button", { name: "保存", exact: true })).toBeDisabled();
    await expect(
      page.getByRole("button", { name: "このフォルダーへの書き込みを許可..." })
    ).toBeDisabled();
  });

  test("タイトル・コメント・★は編集でき、未保存として見える（spec §5-2）", async ({
    page,
  }) => {
    const panel = page.locator('[data-region="right"]');
    // 前提条件: 触る前は未保存表示が無いこと。最初から出ていたら
    // 「編集すると出る」は自明に成立する
    await expect(panel.getByText("未保存の変更があります")).toBeHidden();

    await page.getByLabel("タイトル").fill("夕暮れの港");
    await expect(panel.getByText("未保存の変更があります")).toBeVisible();

    await page.getByLabel("コメント").fill("順光。もう少し寄れた");
    await expect(page.getByLabel("コメント")).toHaveValue("順光。もう少し寄れた");

    // ★はキーボードで動かす。個々の星は aria-hidden の装飾で、
    // 支援技術に見えているのは親の role="slider" だけ（Rating.svelte）
    const rating = page.getByRole("slider", { name: "レーティング" });
    await expect(rating).toHaveAttribute("aria-valuenow", "0");
    await rating.focus();
    await rating.press("ArrowRight");
    await rating.press("ArrowRight");
    await expect(rating).toHaveAttribute("aria-valuenow", "2");

    // 保存先はまだ無いので、編集できても保存ボタンは disabled のまま
    await expect(page.getByRole("button", { name: "保存", exact: true })).toBeDisabled();
  });

  test("撮影情報は実データ（get_exif_info）が出る", async ({ page }) => {
    const panel = page.locator('[data-region="right"]');
    await expect(panel.getByText("ILCE-7M4", { exact: false })).toBeVisible();
    await expect(panel.getByText("FE 35mm F1.4 GM")).toBeVisible();
  });

  test("メタデータモードのグリッドは単一フォーカス（spec §3-2）", async ({ page }) => {
    const grid = page.getByRole("listbox", { name: "写真" });
    await expect(grid).toHaveAttribute("aria-multiselectable", "false");

    await grid.getByRole("option").nth(2).click();
    await expect(grid.getByRole("option").nth(2)).toHaveAttribute("aria-selected", "true");
    await expect(grid.getByRole("option").nth(0)).toHaveAttribute("aria-selected", "false");
    await expect(
      page.locator('[data-region="right"]').getByText("photo-0002.jpg")
    ).toBeVisible();
  });

  test("変換モードのクリックは editingPath を動かさない（spec §3-2）", async ({
    page,
  }) => {
    const rail = page.getByRole("navigation", { name: "モード" });
    const panel = page.locator('[data-region="right"]');

    // 前提条件: いまメタデータの対象は 1 枚目
    await expect(panel.getByText("photo-0000.jpg")).toBeVisible();

    await rail.getByRole("button", { name: "変換" }).click();
    await page.getByRole("option").nth(5).click();
    await rail.getByRole("button", { name: "情報" }).click();

    // 変換モードでの選択は focusedPath しか動かさないので、編集対象は 1 枚目のまま
    await expect(panel.getByText("photo-0000.jpg")).toBeVisible();
  });
});
