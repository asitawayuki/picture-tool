import { expect, test, type Page } from "@playwright/test";
import { clearStorageOnce, installTauriStub, selectSegment } from "./stub";

/**
 * spec §5-3（フレームパネル）と spec §6 段階 7 の完了の目印
 * 「プリセットの作成／編集／削除が通り、`assets.warnings` が toast される」。
 *
 * 検査は spec の記述から導いてある。プリセットの改名は
 * **「改名であって複製ではない」** が spec の要件で、`api.ts` に rename が無く
 * 「保存 → 旧名を削除」で実現しているため、片方だけ落ちると黙って
 * プリセットが増える／消える。ここが本タスクで最も壊れやすい。
 */

/** フォルダーを開いて 1 枚選び、見本写真を持たせた状態でフレームモードへ入る */
async function enterFrameMode(page: Page) {
  await page.getByRole("button", { name: "photos", exact: false }).first().click();
  await page.getByRole("option").first().click();
  await page
    .getByRole("navigation", { name: "モード" })
    .getByRole("button", { name: "フレーム" })
    .click();
}

test.describe("フレームモード", () => {
  test.beforeEach(async ({ page }) => {
    await installTauriStub(page, { imageCount: 12 });
    await clearStorageOnce(page);
    await page.goto("/");
    await enterFrameMode(page);
  });

  test("pad モード限定の注記が出る（spec §5-3）", async ({ page }) => {
    await expect(
      page.getByText("Exif フレームは pad モードでのみ出力されます。")
    ).toBeVisible();
  });

  test("背景色は変換設定と同じ値を指す（spec §5-3）", async ({ page }) => {
    // 前提条件: フレームパネルの背景色は初期状態で「白」。ここが最初から黒だと
    // 「黒になった」は何も検出しない
    await expect(page.getByRole("radio", { name: "白" })).toBeChecked();
    // SegmentedButton の input は隠してあり、当たり判定は可視ラベルが取る
    await selectSegment(page, "黒");

    // 変換モードへ戻ると、変換設定側の背景色も黒になっている。
    // 値が 1 つでなければ、変換設定は白のままになる
    await page
      .getByRole("navigation", { name: "モード" })
      .getByRole("button", { name: "変換" })
      .click();
    await selectSegment(page, "Pad");
    await expect(page.getByRole("radio", { name: "黒" })).toBeChecked();
  });

  test("見本写真のプレビューが中央に出る", async ({ page }) => {
    await expect(page.getByRole("img", { name: "Exif フレームのプレビュー" })).toBeVisible();
  });

  test("生成が始まる前に「生成できませんでした」を出さない", async ({ page }) => {
    // プレビューの生成は 300ms の debounce を挟む。その待ち時間に失敗の文言を
    // 出すと、まだ 1 度も要求していない状態を「失敗」と偽って伝えることになる。
    //
    // **`toBeHidden()` では検出できない。** 自動リトライが 300ms を待ち越して
    // しまい、窓の中に何が出ていたかを見ずに通る（実測で確認済み）。
    // 画面に出た文字列を MutationObserver で全部記録して、その履歴を見る
    await page.evaluate(() => {
      const seen: string[] = [];
      (window as unknown as { __seenText: string[] }).__seenText = seen;
      new MutationObserver(() => seen.push(document.body.innerText)).observe(
        document.body,
        { childList: true, subtree: true, characterData: true }
      );
    });

    const rail = page.getByRole("navigation", { name: "モード" });
    await rail.getByRole("button", { name: "変換" }).click();
    await rail.getByRole("button", { name: "フレーム" }).click();

    // 前提条件: この後ちゃんと絵が出ること。ずっと出ないなら
    // 「失敗と出ない」は失敗を握りつぶしただけになる
    await expect(page.getByRole("img", { name: "Exif フレームのプレビュー" })).toBeVisible();

    const seen = await page.evaluate(
      () => (window as unknown as { __seenText: string[] }).__seenText
    );
    // 前提条件: 記録が空でないこと。空なら「出なかった」は自明に成立する
    expect(seen.length).toBeGreaterThan(0);
    expect(seen.filter((t) => t.includes("プレビューを生成できませんでした"))).toEqual([]);
  });

  test("表示項目チップの入切が押下状態に出る", async ({ page }) => {
    const chip = page.getByRole("button", { name: "日時" });
    // 前提条件: 既定では日時は切
    await expect(chip).toHaveAttribute("aria-pressed", "false");
    await chip.click();
    await expect(chip).toHaveAttribute("aria-pressed", "true");
  });

  test("組み込みプリセットは削除も改名もできない", async ({ page }) => {
    // 組み込みはユーザーファイルが無くても常に存在する必要がある
    await expect(page.getByRole("button", { name: "default を削除" })).toHaveCount(0);
    await page.getByRole("button", { name: "default", exact: true }).dblclick();
    await expect(page.getByRole("textbox", { name: "プリセット名" })).toHaveCount(0);
  });

  test("新規プリセットを作ると一覧に増える", async ({ page }) => {
    await page.getByRole("button", { name: "新規プリセット" }).click();
    // 保存前はディスク上に実体が無い。押した結果が「増える」ことを文言が示す
    await expect(page.getByRole("button", { name: "新規保存" })).toBeEnabled();
    await page.getByRole("button", { name: "新規保存" }).click();
    await expect(page.getByRole("button", { name: "preset-1", exact: true })).toBeVisible();
    await expect(page.getByRole("button", { name: "default", exact: true })).toBeVisible();
  });

  test("改名は改名であって複製ではない（spec §5-3）", async ({ page }) => {
    // 改名できるプリセットを 1 つ作る（組み込みの default は改名できない）
    await page.getByRole("button", { name: "新規プリセット" }).click();
    await page.getByRole("button", { name: "新規保存" }).click();
    // 前提条件: 保存されて一覧に出ていること。ここが無いと
    // 「旧名が消えた」は「そもそも作られていない」でも成立してしまう
    await expect(page.getByRole("button", { name: "preset-1", exact: true })).toBeVisible();

    await page.getByRole("button", { name: "preset-1", exact: true }).dblclick();
    const rename = page.getByRole("textbox", { name: "プリセット名" });
    await rename.fill("夜景");
    await rename.press("Enter");

    await expect(page.getByRole("button", { name: "名前を変えて保存" })).toBeEnabled();
    await page.getByRole("button", { name: "名前を変えて保存" }).click();

    await expect(page.getByRole("button", { name: "夜景", exact: true })).toBeVisible();
    // 旧名は残らない。ここが残るなら「複製」になっている
    await expect(page.getByRole("button", { name: "preset-1", exact: true })).toHaveCount(0);
  });

  test("既存の名前に改名しようとすると保存できない", async ({ page }) => {
    // 通すと「上書き ＋ 旧名の削除」で 2 つが 1 つになり、黙って 1 つ消える
    await page.getByRole("button", { name: "新規プリセット" }).click();
    await page.getByRole("button", { name: "新規保存" }).click();
    await page.getByRole("button", { name: "preset-1", exact: true }).dblclick();
    const rename = page.getByRole("textbox", { name: "プリセット名" });
    await rename.fill("default");
    await rename.press("Enter");

    await expect(page.getByText("同じ名前のプリセットが既にあります。")).toBeVisible();
    await expect(page.getByRole("button", { name: "名前を変えて保存" })).toBeDisabled();
  });

  test("削除したプリセットは、その後保存しても復活しない", async ({ page }) => {
    await page.getByRole("button", { name: "新規プリセット" }).click();
    await page.getByRole("button", { name: "新規保存" }).click();
    // 前提条件: 消す対象が実在すること
    await expect(page.getByRole("button", { name: "preset-1", exact: true })).toBeVisible();

    await page.getByRole("button", { name: "preset-1 を削除" }).click();
    await expect(page.getByRole("button", { name: "preset-1", exact: true })).toHaveCount(0);

    // 編集中のプリセットを消したので、下書きはディスク上に無い実体を指している。
    // ここで保存すると、消したはずのプリセットが書き戻される
    await page.getByRole("button", { name: /^(保存|新規保存)$/ }).click();
    await expect(page.getByRole("button", { name: "preset-1", exact: true })).toHaveCount(0);
  });
});

/**
 * 警告のスタブ設定が上の describe と衝突するので分ける。
 * spec §5-3: アセット由来の警告は返ってくるので toast し、重複抑止も維持する。
 */
test.describe("フレームモードの警告", () => {
  const WARNING = "model_map の書式が不正です";

  test.beforeEach(async ({ page }) => {
    await installTauriStub(page, { imageCount: 4, frameWarnings: [WARNING] });
    await clearStorageOnce(page);
    await page.goto("/");
    await enterFrameMode(page);
  });

  test("アセット由来の警告は toast される（spec §5-3 / S6-M15）", async ({ page }) => {
    await expect(page.getByRole("region", { name: "通知" }).getByText(WARNING)).toBeVisible();
  });

  test("同じ警告はプレビューを作り直しても一度しか出ない（spec §5-3）", async ({ page }) => {
    const shown = page.getByRole("region", { name: "通知" }).getByText(WARNING);
    await expect(shown).toHaveCount(1);

    // プレビューを作り直させる
    await page.getByRole("button", { name: "日時" }).click();

    // 前提条件: 2 回目の生成が実際に起きたこと。起きていなければ
    // 「増えていない」は抑止が働いたからではなく、何も呼ばれていないからになる
    await expect
      .poll(() => page.evaluate(() => (window as any).__framePreviewRequests?.length ?? 0))
      .toBeGreaterThanOrEqual(2);

    await expect(shown).toHaveCount(1);
  });
});
