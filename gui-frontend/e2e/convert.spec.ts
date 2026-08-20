import { expect, test } from "@playwright/test";
import { clearStorageOnce, installTauriStub, selectSegment, toggleSwitch } from "./stub";

/**
 * spec §5-1（変換パネル）と §3-2（選択の寿命）。
 * 出力幅と最大サイズの規則は CLAUDE.md の CLI 仕様と同じもの
 * （`--max-width` は crop / pad 限定で 4 の倍数へ切り捨て、`--max-size` は 1〜1024）。
 */
test.describe("変換パネル", () => {
  test.beforeEach(async ({ page }) => {
    await installTauriStub(page, { imageCount: 12 });
    await clearStorageOnce(page);
    await page.goto("/");
    await page.getByRole("button", { name: "photos", exact: false }).first().click();
  });

  test("主ボタンが選択枚数を持つ", async ({ page }) => {
    await expect(page.getByRole("button", { name: "0 枚を変換" })).toBeDisabled();
    await page.getByRole("option", { name: /photo-0000/ }).click();
    await expect(page.getByRole("button", { name: "1 枚を変換" })).toBeVisible();
  });

  test("フォルダーを変えると選択がクリアされる（spec §3-2）", async ({ page }) => {
    await page.getByRole("option", { name: /photo-0000/ }).click();
    // 前提条件: いま実際に 1 枚選ばれていること。0 枚のままだと
    // 「クリアされた」は自明に成立してしまう
    await expect(page.getByRole("button", { name: "1 枚を変換" })).toBeVisible();

    await page.getByRole("button", { name: "archive", exact: false }).first().click();
    await expect(page.getByRole("button", { name: "0 枚を変換" })).toBeVisible();
  });

  test("quality モードでは出力幅の制限が無効になる", async ({ page }) => {
    await selectSegment(page, "Quality");
    await expect(page.getByRole("checkbox", { name: "出力幅を制限する" })).toBeDisabled();
    await expect(
      page.getByText("Quality モードは 4:5 に変換しないため")
    ).toBeVisible();
  });

  test("出力幅は 4 の倍数へ切り捨てられる", async ({ page }) => {
    await selectSegment(page, "Pad");
    // Switch の input は .track に覆われて直接クリックできない（Task 4 Step 2）
    await toggleSwitch(page, "出力幅を制限する");

    const input = page.getByLabel("出力幅の上限");
    await expect(input).toHaveValue("1080");
    await input.fill("1002");
    await input.blur();
    await expect(input).toHaveValue("1000");
    await expect(page.getByText("→ 1000×1250")).toBeVisible();
  });

  test("出力幅を空欄にしても無制限にはならず直前の値へ戻る", async ({ page }) => {
    // 「無制限」を表すのは上の Switch であって空欄ではない。空欄を素通しすると
    // トグルが on のまま max_width が null になり、表示と送る値が食い違う
    await selectSegment(page, "Pad");
    await toggleSwitch(page, "出力幅を制限する");

    const input = page.getByLabel("出力幅の上限");
    // 前提条件: 消す前に値が入っていること
    await expect(input).toHaveValue("1080");

    await input.fill("");
    await input.blur();
    await expect(input).toHaveValue("1080");
    await expect(page.getByRole("checkbox", { name: "出力幅を制限する" })).toBeChecked();
  });

  test("最大サイズは 1〜1024MB に収まり、空欄にしても直前の値へ戻る", async ({ page }) => {
    const input = page.getByLabel("最大サイズ");
    await expect(input).toHaveValue("8");

    await input.fill("2000");
    await input.blur();
    await expect(input).toHaveValue("1024");

    await input.fill("0");
    await input.blur();
    await expect(input).toHaveValue("1");

    // 最大サイズに「無指定」は無い。空欄のまま送ると Rust 側で落ちる
    await input.fill("");
    await input.blur();
    await expect(input).toHaveValue("1");
  });

  /**
   * 段階 5 の完了の目印（Task 10 Step 7）。実機の GUI ウィンドウを操作する手段が
   * このセッションには無いため、スタブ越しに「選択 → 出力先 → 変換 → 結果」の
   * 通しを検査する。**バックエンドへ渡る config が画面の表示どおりであること**まで
   * 見るのが目的で、パネルを組み直したときの写し間違いはここで落ちる。
   */
  test("選択から結果ダイアログまで通り、画面どおりの設定が渡る", async ({ page }) => {
    await selectSegment(page, "Pad");
    await toggleSwitch(page, "出力幅を制限する");
    const maxWidth = page.getByLabel("出力幅の上限");
    await maxWidth.fill("1002");
    await maxWidth.blur();

    await page.getByRole("option", { name: /photo-0000/ }).click();
    await page.getByRole("option", { name: /photo-0001/ }).click();
    await page.getByRole("button", { name: "フォルダーを選択..." }).click();
    await expect(page.getByText("/output")).toBeVisible();

    await page.getByRole("button", { name: "2 枚を変換" }).click();

    const dialog = page.getByRole("dialog", { name: "変換結果" });
    await expect(dialog).toBeVisible();
    await expect(dialog.getByText("成功")).toBeVisible();

    const args = await page.evaluate(() => (window as any).__lastProcessArgs);
    expect(args.files).toEqual(["/photos/0.jpg", "/photos/1.jpg"]);
    expect(args.outputFolder).toBe("/output");
    // 画面は 1000 を表示している。4 の倍数へ落ちた値がそのまま渡ること
    expect(args.config).toMatchObject({
      mode: "pad",
      bg_color: "white",
      quality: 90,
      max_size_mb: 8,
      delete_originals: false,
      max_width: 1000,
    });
    // Exif フレームは off。pad モードでも勝手に付かない
    expect(args.exifFrameConfig).toBeNull();
  });

  test("pad + Exif フレーム on では選択中のプリセットが渡る", async ({ page }) => {
    await selectSegment(page, "Pad");
    await toggleSwitch(page, "Exif フレームを付ける");
    // 前提条件: プリセットの一覧が読めていること。空なら渡る値も null になり、
    // 「渡らない」が配線の誤りと区別できなくなる
    await expect(page.getByLabel("プリセット")).toHaveValue("default");

    await page.getByRole("option", { name: /photo-0000/ }).click();
    await page.getByRole("button", { name: "フォルダーを選択..." }).click();
    await page.getByRole("button", { name: "1 枚を変換" }).click();
    await expect(page.getByRole("dialog", { name: "変換結果" })).toBeVisible();

    const args = await page.evaluate(() => (window as any).__lastProcessArgs);
    expect(args.exifFrameConfig).toMatchObject({ name: "default" });
  });

  test("元ファイル削除は確認ダイアログを挟む", async ({ page }) => {
    await page.getByRole("option", { name: /photo-0000/ }).click();
    await page.getByRole("button", { name: "フォルダーを選択..." }).click();
    await toggleSwitch(page, "元ファイルを削除");
    await page.getByRole("button", { name: "1 枚を変換" }).click();

    const dialog = page.getByRole("alertdialog", { name: "元ファイルを削除します" });
    await expect(dialog).toBeVisible();
    // 破壊的操作なので初期フォーカスはキャンセル側
    await expect(dialog.getByRole("button", { name: "キャンセル" })).toBeFocused();
  });
});
