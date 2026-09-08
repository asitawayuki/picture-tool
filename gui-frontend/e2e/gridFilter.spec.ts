import { expect, test } from "@playwright/test";
import { clearStorageOnce, installTauriStub } from "./stub";

/**
 * グリッドの絞り込みと並び順。
 *
 * 順序と絞り込みの規則そのものは `gridFilter.test.ts` が純粋ロジックとして
 * 見ている（変異テストで両方向を実測済み）。ここで見るのは **runes と DOM を
 * 繋いだ結果**：入力がグリッドの中身に届くか、選択との整合が画面に出るか、
 * 「次へ」が絞り込みの外へ飛ばないか。
 *
 * スタブの更新日は隣り合う 2 枚を入れ替えた順（0↔1, 2↔3, ...）。名前順とも
 * その逆順とも一致しないので、「名前で並べたまま向きだけ変える」実装は
 * ここを通れない。
 */
const gridOf = (page: import("@playwright/test").Page) =>
  page.getByRole("listbox", { name: "写真" });

const tileNames = async (page: import("@playwright/test").Page, count: number) => {
  const options = gridOf(page).getByRole("option");
  const names: string[] = [];
  for (let at = 0; at < count; at += 1) {
    names.push((await options.nth(at).getAttribute("aria-label")) ?? "");
  }
  return names;
};

test.describe("グリッドの絞り込みと並び順", () => {
  test.beforeEach(async ({ page }) => {
    // 20 枚。仮想化の外に出ない枚数にして、並び順を DOM の先頭で読めるようにする
    await installTauriStub(page, { imageCount: 20 });
    await clearStorageOnce(page);
    await page.goto("/");
    await page.getByRole("button", { name: "photos", exact: false }).first().click();
    await expect(gridOf(page).getByRole("option").first()).toBeVisible();
  });

  test("検索でグリッドが絞られ、ヘッダーが分母付きになる", async ({ page }) => {
    // 前提: 絞り込む前は全件で、分母が出ていない
    await expect(page.getByText("20 枚", { exact: true })).toBeVisible();

    await page.getByLabel("検索").fill("photo-001");
    // photo-0010〜0019 の 10 枚
    await expect(page.getByText("10 / 20 枚")).toBeVisible();
    await expect(gridOf(page).getByRole("option")).toHaveCount(10);
  });

  test("一致しない検索では 0 枚になる（エラーにしない）", async ({ page }) => {
    await page.getByLabel("検索").fill("存在しない名前");
    await expect(page.getByText("0 / 20 枚")).toBeVisible();
    await expect(gridOf(page).getByRole("option")).toHaveCount(0);
  });

  test("検索欄を空に戻すと全件に戻る", async ({ page }) => {
    const search = page.getByLabel("検索");
    await search.fill("photo-0001");
    await expect(gridOf(page).getByRole("option")).toHaveCount(1);

    await search.fill("");
    // 枚数はヘッダーで見る。20 枚は仮想スクロールの可視範囲を超えるので、
    // DOM のタイル数を数えても全件には届かない
    await expect(page.getByText("20 枚", { exact: true })).toBeVisible();
    expect(await tileNames(page, 1)).toEqual(["photo-0000.jpg"]);
  });

  test("更新日順は名前順と別の並びになる", async ({ page }) => {
    // 前提: 既定は名前の昇順
    expect(await tileNames(page, 3)).toEqual([
      "photo-0000.jpg",
      "photo-0001.jpg",
      "photo-0002.jpg",
    ]);

    await page.getByLabel("並び順").selectOption("modified");
    // キーを変えると日時は「新しい順」から見せる（GridFilterBar）
    await expect(page.getByRole("button", { name: /降順/ })).toBeVisible();
    // 更新日の降順 ── 入れ替えた順の逆から。名前の降順（0019, 0018, 0017）とは違う
    expect(await tileNames(page, 3)).toEqual([
      "photo-0018.jpg",
      "photo-0019.jpg",
      "photo-0016.jpg",
    ]);

    await page.getByRole("button", { name: /降順/ }).click();
    // 更新日の昇順 ── 名前の昇順（0000, 0001, 0002）とは違う
    expect(await tileNames(page, 3)).toEqual([
      "photo-0001.jpg",
      "photo-0000.jpg",
      "photo-0003.jpg",
    ]);
  });

  test("絞り込みで隠れた選択は枚数が画面に出て、変換対象からは減らない", async ({
    page,
  }) => {
    const options = gridOf(page).getByRole("option");
    await options.nth(0).click(); // photo-0000
    await options.nth(1).click(); // photo-0001
    await expect(page.getByRole("button", { name: "2 枚を変換" })).toBeVisible();
    // 前提: 絞り込む前は非表示の注記が出ていない
    await expect(page.getByText("枚は非表示")).toHaveCount(0);

    await page.getByLabel("検索").fill("photo-0019");
    // 前提: 選んだ 2 枚が実際にグリッドから消えていること。これが無いと
    // 「隠れていないから注記も出ない」でも緑になる
    await expect(options).toHaveCount(1);
    await expect(options.first()).toHaveAttribute("aria-label", /photo-0019/);

    await expect(page.getByText("2 枚選択中")).toBeVisible();
    await expect(page.getByText("（2 枚は非表示）")).toBeVisible();
    // **減らない。** 選択は表示と独立しているので、変換されるのは 2 枚のまま
    await expect(page.getByRole("button", { name: "2 枚を変換" })).toBeVisible();
  });

  test("最小ウィンドウ幅でもヘッダーが横に溢れない", async ({ page }) => {
    // `gui/tauri.conf.json` の minWidth。ここより狭くはならない。
    // 検索欄・並び順・サイズスライダー・開閉ハンドルが 1 行に同居するので、
    // 部品を足すと真っ先に溢れるのがこのヘッダーである
    await page.setViewportSize({ width: 1100, height: 700 });
    await gridOf(page).getByRole("option").first().click(); // 「N 枚選択中／全解除」も出す

    const overflow = await page.locator(".grid-header").evaluate((el) => ({
      scrollWidth: el.scrollWidth,
      clientWidth: el.clientWidth,
    }));
    // 前提: 実際に測れる幅があること（0 同士の比較で通らないように）
    expect(overflow.clientWidth).toBeGreaterThan(0);
    expect(overflow.scrollWidth).toBeLessThanOrEqual(overflow.clientWidth);
  });

  test("情報モードの「次へ」は絞り込みの中だけを進む", async ({ page }) => {
    const options = gridOf(page).getByRole("option");
    await options.nth(9).click(); // photo-0009
    await page
      .getByRole("navigation", { name: "モード" })
      .getByRole("button", { name: "情報" })
      .click();

    // 「保存して次の写真へ」は canSave も見る（承認済み・変更あり）。
    // それを満たさないまま disabled を見ても、次があるかどうかの検査にならない
    await page.getByRole("button", { name: "このフォルダーへの書き込みを許可..." }).click();
    await page.getByLabel("タイトル").fill("夕暮れの港");

    const next = page.getByRole("button", { name: "保存して次の写真へ" });
    // **前提**: 絞り込む前の photo-0009 には次（photo-0010）があり、送れる状態
    await expect(next).toBeEnabled();

    // photo-0009 が絞り込みの最後になるようにする。編集対象は動かない
    await page.getByLabel("検索").fill("photo-000");
    await expect(options).toHaveCount(10);
    await expect(options.nth(9)).toHaveAttribute("aria-label", /photo-0009/);

    // 全体では photo-0010 が続いているが、画面に無いので送り先にしない
    await expect(next).toBeDisabled();
  });
});
