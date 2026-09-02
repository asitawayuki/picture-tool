import { expect, test } from "@playwright/test";
import { clearStorageOnce, installTauriStub } from "./stub";

/**
 * spec §5-2（メタデータパネル）と §3-2（editingPath と focusedPath の分離）。
 *
 * `read_image_metadata` / `write_image_metadata` / `grant_metadata_editing`
 * はスタブ（`stub.ts`）が受ける。実データは撮影情報（`get_exif_info`）と同様、
 * スタブが返す形を dto.contract.ts から取っている。
 *
 * ファイル名はグリッドのタイルにも出るので、パネル側の表示を見るときは
 * 右カラム（`[data-region="right"]`）に絞る。絞らないと strict mode で
 * 「2 つ当たった」として落ちる ── **落ちるのは正しく、
 * どちらを見ているのか曖昧なテストにしないための制約**。
 */
test.describe("メタデータモード", () => {
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

  test("承認前は保存できない（spec §衝突2）", async ({ page }) => {
    await page.getByLabel("タイトル").fill("夕暮れの港");
    // 前提条件: 未保存の変更はある（＝「変更が無いから disabled」ではない）
    await expect(
      page.locator('[data-region="right"]').getByText("未保存の変更があります")
    ).toBeVisible();
    await expect(page.getByRole("button", { name: "保存", exact: true })).toBeDisabled();
  });

  test("承認すると保存でき、値がファイルに残る", async ({ page }) => {
    await page.getByRole("button", { name: "このフォルダーへの書き込みを許可..." }).click();
    await expect(page.getByText("このフォルダーへ書き込めます", { exact: false })).toBeVisible();

    const panel = page.locator('[data-region="right"]');
    await page.getByLabel("タイトル").fill("夕暮れの港");
    // 前提条件: 保存前は未保存表示が出ている（fill が効いていることの確認。
    // これが無いと「fill が効かず最初から hidden」でも green になる）
    await expect(panel.getByText("未保存の変更があります")).toBeVisible();

    await page.getByRole("button", { name: "保存", exact: true }).click();
    await expect(panel.getByText("未保存の変更があります")).toBeHidden();

    // 別の写真へ移って戻ると、保存した値が読み直される
    await page.getByRole("option").nth(1).click();
    await expect(page.getByLabel("タイトル")).toHaveValue("");
    await page.getByRole("option").nth(0).click();
    await expect(page.getByLabel("タイトル")).toHaveValue("夕暮れの港");
  });

  test("拒否すると承認済みの表示にならず、保存もできないまま（spec §衝突2）", async ({
    page,
  }) => {
    // beforeEach とは別条件なのでこのテスト内で貼り直す
    await installTauriStub(page, { imageCount: 4, denyGrant: true });
    await page.goto("/");
    await page.getByRole("button", { name: "photos", exact: false }).first().click();
    await page.getByRole("option").first().click();
    await page
      .getByRole("navigation", { name: "モード" })
      .getByRole("button", { name: "情報" })
      .click();

    await page.getByRole("button", { name: "このフォルダーへの書き込みを許可..." }).click();

    // grants.request() が拒否を承認と取り違えると、この通知だけが出なくなる
    // （表示状態自体は roots が変わらない限り崩れないので、通知を見ないと
    // 「戻り値を確かめずに承認したことにしている」退行を検出できない）
    await expect(page.getByText("書き込みは許可されませんでした。")).toBeVisible();

    // 拒否されても承認済みの表示にはならず、許可ボタンも消えない
    await expect(page.getByText("このフォルダーへ書き込めます", { exact: false })).toBeHidden();
    await expect(
      page.getByRole("button", { name: "このフォルダーへの書き込みを許可..." })
    ).toBeVisible();

    // 前提条件: 未保存の変更はある
    await page.getByLabel("タイトル").fill("夕暮れの港");
    await expect(
      page.locator('[data-region="right"]').getByText("未保存の変更があります")
    ).toBeVisible();
    await expect(page.getByRole("button", { name: "保存", exact: true })).toBeDisabled();
  });

  test("保存で正規化された値が saved に採られる（送った値ではなく戻り値を使う）", async ({
    page,
  }) => {
    await page.getByRole("button", { name: "このフォルダーへの書き込みを許可..." }).click();

    const panel = page.locator('[data-region="right"]');
    // 前後に空白を付けて保存する。スタブは保存時に trim して返す（core の正規化を模す）
    await page.getByLabel("タイトル").fill("  夕暮れの港  ");
    await page.getByRole("button", { name: "保存", exact: true }).click();

    // 戻り値（trim 済み）が saved に採られていれば、表示は空白の落ちた値になり、
    // かつ values も saved と揃うので未保存表示は出ない。
    // 送った値をそのまま saved にする退行だと、表示は空白付きのまま変わらない
    await expect(page.getByLabel("タイトル")).toHaveValue("夕暮れの港");
    await expect(panel.getByText("未保存の変更があります")).toBeHidden();
  });

  test("食い違い警告が編集欄の脇に出る（spec §食い違いは警告として返す）", async ({
    page,
  }) => {
    // beforeEach とは別条件なのでこのテスト内で貼り直す
    await installTauriStub(page, {
      imageCount: 4,
      metadataWarnings: ["コメントに、他のアプリが書いた別の値があります"],
    });
    await page.goto("/");
    await page.getByRole("button", { name: "photos", exact: false }).first().click();
    await page.getByRole("option").first().click();
    await page
      .getByRole("navigation", { name: "モード" })
      .getByRole("button", { name: "情報" })
      .click();

    await expect(
      page.locator('[data-region="right"]').getByText("他のアプリが書いた別の値", { exact: false })
    ).toBeVisible();
  });

  test("「保存して次の写真へ」は編集対象を 1 つ進める", async ({ page }) => {
    await page.getByRole("button", { name: "このフォルダーへの書き込みを許可..." }).click();
    await page.getByLabel("タイトル").fill("一枚目");
    await page.getByRole("button", { name: "保存して次の写真へ" }).click();
    await expect(
      page.locator('[data-region="right"]').getByText("photo-0001.jpg")
    ).toBeVisible();
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

    // まだ承認していないので、編集できても保存ボタンは disabled のまま
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

  test("送り続けても編集対象がグリッドの外へ出ない", async ({ page }) => {
    await installTauriStub(page, { imageCount: 120 });
    await page.goto("/");
    await page.getByRole("button", { name: "photos", exact: false }).first().click();
    await page.getByRole("option").first().click();
    await page
      .getByRole("navigation", { name: "モード" })
      .getByRole("button", { name: "情報" })
      .click();
    await page.getByRole("button", { name: "このフォルダーへの書き込みを許可..." }).click();

    for (let i = 0; i < 30; i++) {
      // 「保存して次の写真へ」は isDirty を要求する（spec の disabled 判定どおり）。
      // 変更が無いままでは 1 回目以降ずっと disabled のままになるので、毎回 1 文字
      // 変える。この検査の主眼は canSave の判定ではなくスクロール追従なので、
      // dirty を保つのが目的で内容そのものに意味は無い
      await page.getByLabel("タイトル").fill(`t${i}`);
      await page.getByRole("button", { name: "保存して次の写真へ" }).click();
    }
    // 選択中のタイルが描画されていて、画面内に見えていること
    const selected = page.getByRole("option").and(page.locator('[aria-selected="true"]'));
    await expect(selected).toHaveCount(1);
    await expect(selected).toBeInViewport();
  });
});
