import { expect, test } from "@playwright/test";
import { clearStorageOnce, installTauriStub } from "./stub";
import { RATING_PRESETS, type RatingPresetId } from "../src/lib/browser/gridFilter";

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

/**
 * 並び順の検査用にタイルの**名前だけ**を取り出す。
 *
 * `tileLabel()`（`gridMetrics.ts`）は★と未保存印を aria-label に連結するので、
 * 生の aria-label と完全一致させると、スタブの★が変わるたびに並び順のテストが
 * 巻き添えで落ちる。ここで見たいのは順序であって★ではない
 */
const tileNames = async (page: import("@playwright/test").Page, count: number) => {
  const options = gridOf(page).getByRole("option");
  const names: string[] = [];
  for (let at = 0; at < count; at += 1) {
    const label = (await options.nth(at).getAttribute("aria-label")) ?? "";
    names.push(label.split(" ")[0]);
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

  /**
   * スタブの `RATING_PATTERN`（`stub.ts`）を 20 枚に当てたときの内訳。
   * **選択肢のラベルではなく `RATING_PRESETS` を回す** ── 対応表の写しを
   * ここにもう 1 つ作ると、写し間違えても緑になる
   */
  const EXPECTED_COUNTS: Record<RatingPresetId, number> = {
    all: 20,
    rated: 12,
    min2: 10,
    min3: 8,
    min4: 5,
    only5: 3,
    unrated: 8,
  };

  test("★の 7 択それぞれでグリッドが絞られ、ヘッダーが分母付きになる", async ({ page }) => {
    // 前提条件: 絞り込む前は全件で、分母が出ていない
    await expect(page.getByText("20 枚", { exact: true })).toBeVisible();
    // 前提条件: スタブが★あり・なしを混ぜている（全部★なし／全部★ありだと
    // どの選択肢も同じ枚数になり、検査が成立しない）
    expect(EXPECTED_COUNTS.rated).toBeGreaterThan(0);
    expect(EXPECTED_COUNTS.unrated).toBeGreaterThan(0);

    const rating = page.getByLabel("評価");
    for (const preset of RATING_PRESETS) {
      await rating.selectOption(preset.id);
      const expected = EXPECTED_COUNTS[preset.id];
      if (expected === 20) {
        await expect(page.getByText("20 枚", { exact: true })).toBeVisible();
      } else {
        await expect(page.getByText(`${expected} / 20 枚`)).toBeVisible();
      }
    }
  });

  test("★の条件を変えるとスクロールが先頭へ戻る", async ({ page }) => {
    // 20 枚は viewport 1440x800 に収まってしまい、そもそもスクロールできない。
    // この検査だけ枚数を増やして貼り直す（★の内訳は見ないので枚数は自由）
    await installTauriStub(page, { imageCount: 300 });
    await page.goto("/");
    await page.getByRole("button", { name: "photos", exact: false }).first().click();
    await expect(gridOf(page).getByRole("option").first()).toBeVisible();

    const scroller = page.locator(".scroller");
    await scroller.evaluate((el) => (el.scrollTop = el.scrollHeight));
    // 前提条件: 実際に下までスクロールできている（0 のままだと自明に成立する）
    await expect
      .poll(() => scroller.evaluate((el) => el.scrollTop))
      .toBeGreaterThan(0);

    await page.getByLabel("評価").selectOption("min4");
    // 前提条件: 実際に絞り込まれている。これが無いと「絞り込み自体が効いていない」
    // 壊れ方と「絞り込んだがスクロールが戻らない」壊れ方が同じ赤になる。
    // 75 の導出: `stub.ts` の `RATING_PATTERN` は 12 周期で、★4 以上は 3 枚
    // （★5, ★4, ★5）。300 = 12 × 25 なので 3 × 25 = 75。**`imageCount` を
    // 12 の倍数から外すとこの数は変わる**
    await expect(page.getByText("75 / 300 枚")).toBeVisible();

    await expect.poll(() => scroller.evaluate((el) => el.scrollTop)).toBe(0);
  });

  test("★で隠れた選択も枚数が画面に出て、変換対象からは減らない", async ({ page }) => {
    const options = gridOf(page).getByRole("option");
    // photo-0000 は未評価（スタブの既定パターン）
    await options.nth(0).click();
    await expect(page.getByRole("button", { name: "1 枚を変換" })).toBeVisible();
    // 前提条件: 絞り込む前は非表示の注記が出ていない
    await expect(page.getByText("枚は非表示")).toHaveCount(0);

    await page.getByLabel("評価").selectOption("only5");

    // 前提条件: 選んだ 1 枚が実際にグリッドから消えていること
    await expect(options).toHaveCount(3);
    await expect(page.getByText("（1 枚は非表示）")).toBeVisible();
    // **減らない。** 選択は表示と独立している
    await expect(page.getByRole("button", { name: "1 枚を変換" })).toBeVisible();
  });

  test("絞り込み中に「保存して次の写真へ」を押すと、その写真は消えるがパネルは次へ送られる", async ({
    page,
  }) => {
    // photo-0001 は★5。「★5」に絞ると 3 枚（index 1, 8, 13）が残る
    await page.getByLabel("評価").selectOption("only5");
    const options = gridOf(page).getByRole("option");
    await expect(options).toHaveCount(3);
    await options.nth(0).click();
    await page
      .getByRole("navigation", { name: "モード" })
      .getByRole("button", { name: "情報" })
      .click();

    // canSave を満たす（承認済み ＋ 変更あり）。満たさないままボタンを押しても
    // 何も起きないので、検査そのものが成立しない
    await page.getByRole("button", { name: "このフォルダーへの書き込みを許可..." }).click();
    await page.getByLabel("タイトル").fill("夕暮れの港");
    // ★を 5 から 3 へ落とす。保存すると「★5」の条件から外れる
    const rating = page.getByRole("slider", { name: "レーティング" });
    await rating.focus();
    await rating.press("ArrowLeft");
    await rating.press("ArrowLeft");

    await page.getByRole("button", { name: "保存して次の写真へ" }).click();

    // グリッドから消える（3 → 2 枚）
    await expect(options).toHaveCount(2);
    // **パネルは次の写真へ送られている** ── 保存前に次を確定しているため
    // （`App.svelte` の `const next = nextImagePath;`）
    await expect(page.locator('[data-region="right"]').getByText("photo-0008.jpg")).toBeVisible();
  });

  test("絞り込み中に「保存」だけを押すと、写真は消えて両方のボタンが無効になる（仕様）", async ({
    page,
  }) => {
    await page.getByLabel("評価").selectOption("only5");
    const options = gridOf(page).getByRole("option");
    await expect(options).toHaveCount(3);
    await options.nth(0).click();
    await page
      .getByRole("navigation", { name: "モード" })
      .getByRole("button", { name: "情報" })
      .click();
    await page.getByRole("button", { name: "このフォルダーへの書き込みを許可..." }).click();
    await page.getByLabel("タイトル").fill("夕暮れの港");
    const rating = page.getByRole("slider", { name: "レーティング" });
    await rating.focus();
    await rating.press("ArrowLeft");
    await rating.press("ArrowLeft");

    const save = page.getByRole("button", { name: "保存", exact: true });
    const next = page.getByRole("button", { name: "保存して次の写真へ" });
    // 前提条件: 押す前はどちらも有効（＝「元から無効」で緑にならないように）
    await expect(save).toBeEnabled();
    await expect(next).toBeEnabled();

    await save.click();

    await expect(options).toHaveCount(2);
    // **「次へが有効であること」を検査してはならない。** 保存に成功すると
    // `draft.isDirty` が false になり `canSave` が false になるので、
    // hasNext の値に関係なく両方押せない（設計 §5 / R-16）。
    // これは絞り込みが持ち込む欠陥ではなく、保存ボタンの既存の振る舞いである
    await expect(save).toBeDisabled();
    await expect(next).toBeDisabled();
  });
});

/**
 * グループの実幅・自然幅と、**折り返しの無い状態の高さ**を測る。
 *
 * クローンに `width: max-content` / `min-content` を当てて測るのは、
 * **中身が潰れていることは `scrollWidth` に現れない**ため
 * （`.selected` の縦書きは溢れではなく折り返しなので `scrollWidth === clientWidth`）。
 *
 * **高さは「そのグループ自身の 1 行分」と比べる。** `.grid-header` の line-height から
 * 出した固定値を全グループに当てると、ラベルが枠の上に乗る `TextField` / `Select` を含む
 * `.filters-area`（1 行でも 58px）が、折り返していないのに落ちる
 */
const measureGroup = (page: import("@playwright/test").Page, selector: string, mode: string) =>
  page.locator(selector).evaluate(
    (el, sizing) => {
      const clone = el.cloneNode(true) as HTMLElement;
      clone.style.width = sizing;
      clone.style.position = "absolute";
      clone.style.left = "-99999px";
      clone.style.top = "0";
      el.parentElement!.appendChild(clone);
      const natural = clone.getBoundingClientRect();
      clone.remove();
      const rect = el.getBoundingClientRect();
      return {
        actual: rect.width,
        natural: natural.width,
        height: rect.height,
        naturalHeight: natural.height,
      };
    },
    mode
  );

test.describe("グリッドヘッダーの段組み", () => {
  test.beforeEach(async ({ page }) => {
    await installTauriStub(page, { imageCount: 20 });
    await clearStorageOnce(page);
    await page.goto("/");
    await page.getByRole("button", { name: "photos", exact: false }).first().click();
    await expect(gridOf(page).getByRole("option").first()).toBeVisible();
    // 「N 枚選択中 / 全解除」も出す ── `.selected` の縦書きはここでしか出ない
    await gridOf(page).getByRole("option").first().click();
  });

  /**
   * `.status` と `.tools` は縮んではいけない（中に伸縮する部品が無い）。
   * `.filters-area` は検索欄が `min-width: 72px` まで縮むことを設計が許しているので、
   * **幅は「溢れていないか」で見る** ── 縮み切った先は `.sort` / `.rating` が
   * `flex: 0 0 auto` の固定幅で逃げ場が無く、足りなくなれば必ず溢れる。
   * クローンに `width: min-content` を当てる測り方はここでは使えない。
   * `<input>` の min-content は自身の内在サイズ（約 200px）を返し、
   * `.search` の `min-width` を反映しないので、設計どおり縮んだ状態を「潰れた」と誤判定する
   */
  for (const width of [1100, 1600]) {
    test(`ヘッダー幅 ${width}px で各グループが潰れない`, async ({ page }) => {
      await page.setViewportSize({ width, height: 800 });

      for (const selector of [".status", ".tools", ".filters-area"] as const) {
        const m = await measureGroup(page, selector, "max-content");
        // 前提条件: 実際に測れる幅と高さがある（0 同士の比較で通らないように）
        expect(m.natural, `${selector} の自然幅`).toBeGreaterThan(0);
        expect(m.naturalHeight, `${selector} の自然高さ`).toBeGreaterThan(0);
        // 各グループは折り返さない。`.selected` の縦書きはここで落ちる
        expect(m.height, `${selector} が 1 行に収まっていない`).toBeLessThanOrEqual(
          m.naturalHeight + 1
        );
      }

      // 縮まない 2 つは max-content のまま出ていること
      for (const selector of [".status", ".tools"] as const) {
        const m = await measureGroup(page, selector, "max-content");
        expect(m.actual, `${selector} が max-content を下回った`).toBeGreaterThanOrEqual(
          m.natural - 1
        );
      }

      // 絞り込み群は溢れていないこと
      const filters = await page.locator(".filters-area .filters").evaluate((el) => ({
        scrollWidth: el.scrollWidth,
        clientWidth: el.clientWidth,
        search: el.querySelector(".search")!.getBoundingClientRect().width,
        sort: el.querySelector(".sort")!.getBoundingClientRect().width,
        rating: el.querySelector(".rating")!.getBoundingClientRect().width,
      }));
      // 前提条件: 検索欄が設計の下限（72px）以上で、固定幅の 2 つが縮んでいない
      // （0 幅同士の比較や、全部潰れた状態で「溢れていない」が成立するのを防ぐ）
      expect(filters.search, "検索欄が設計の下限を下回った").toBeGreaterThanOrEqual(72);
      expect(filters.sort, "並び順が固定幅を下回った").toBeGreaterThanOrEqual(104);
      expect(filters.rating, "★が固定幅を下回った").toBeGreaterThanOrEqual(166);
      expect(filters.clientWidth, ".filters の実幅").toBeGreaterThan(0);
      expect(filters.scrollWidth, ".filters-area が溢れた").toBeLessThanOrEqual(
        filters.clientWidth
      );
    });
  }

  /**
   * **変換モードでの検査である。** 実効幅は「窓幅 −（rail 80 ＋ 左カラム ＋ 右カラム）」で、
   * 右カラムの既定は変換 320px / 情報 360px。情報モードは 1600px の窓でも 920px にしかならず、
   * 1 段に要る幅を下回るので**常に 2 段**になる（許容。設計 §6 の 960px は変換モードの数字）
   */
  test("狭いと 2 段・広いと 1 段に切り替わる（変換モード）", async ({ page }) => {
    /**
     * **`top` ではなく垂直方向の中心で比べる。** `.grid-header` は `align-items: center` なので、
     * ラベルの分だけ 18px 高い `.filters-area` は、同じ行にあっても `top` が `.status` と一致しない
     */
    const rowOf = (selector: string) =>
      page.locator(selector).evaluate((el) => {
        const r = el.getBoundingClientRect();
        return Math.round(r.top + r.height / 2);
      });

    await page.setViewportSize({ width: 1100, height: 800 });
    // 前提条件: 2 段になっている（`.filters-area` が `.status` より下の行にある）
    expect(await rowOf(".filters-area")).toBeGreaterThan(await rowOf(".status"));

    await page.setViewportSize({ width: 1600, height: 800 });
    await expect
      .poll(async () => (await rowOf(".filters-area")) - (await rowOf(".status")))
      .toBe(0);
  });

  test("スクリーンショットを残す（px を並べただけでは破綻が見えない）", async ({ page }) => {
    for (const width of [1100, 1600]) {
      await page.setViewportSize({ width, height: 800 });
      await page
        .locator(".grid-header")
        .screenshot({ path: `e2e/__screenshots__/grid-header-${width}.png` });
    }
  });
});
