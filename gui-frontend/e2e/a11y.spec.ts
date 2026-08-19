import { expect, test } from "@playwright/test";
import { clearStorageOnce, installTauriStub } from "./stub";

/**
 * spec §7-3 の総点検。
 *
 * - `prefers-reduced-motion: reduce` で全トランジションが止まること
 * - Tab 順が rail → 左カラム → グリッド → 右パネル であること
 * - ライトとダーク両方のスクリーンショットを残すこと（Step 7 の目視用）
 */
test.describe("総点検（spec §7-3）", () => {
  test.beforeEach(async ({ page }) => {
    await installTauriStub(page, { imageCount: 40 });
    await clearStorageOnce(page);
  });

  /** 画面上のすべての要素の transition / animation の長さを秒で集める */
  async function motionDurations(page: import("@playwright/test").Page) {
    return page.evaluate(() =>
      Array.from(document.querySelectorAll("*"))
        .flatMap((el) => {
          const style = getComputedStyle(el);
          return [style.transitionDuration, style.animationDuration];
        })
        .flatMap((v) => v.split(",").map((s) => Number.parseFloat(s)))
        .filter((n) => Number.isFinite(n))
    );
  }

  test("prefers-reduced-motion で全トランジションが止まる", async ({ page }) => {
    // 前提条件: 既定（reduce 無し）では実際に動く要素があること。
    // 1 つも無ければ「止まっている」は自明に成立してしまう
    await page.emulateMedia({ reducedMotion: "no-preference" });
    await page.goto("/");
    await page.getByRole("button", { name: "photos", exact: false }).first().click();
    await expect(page.getByRole("option").first()).toBeVisible();
    expect(Math.max(...(await motionDurations(page)))).toBeGreaterThan(0.01);

    await page.emulateMedia({ reducedMotion: "reduce" });
    await page.goto("/");
    await page.getByRole("button", { name: "photos", exact: false }).first().click();
    await expect(page.getByRole("option").first()).toBeVisible();

    // トークン側は 0.01ms（= 0.00001 秒）へ潰す。0 にしないのは
    // transitionend を前提にしたコードを壊さないため（tokens.css）
    expect(Math.max(...(await motionDurations(page)))).toBeLessThan(0.01);
  });

  test("Tab 順が rail → 左カラム → グリッド → 右パネル", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "photos", exact: false }).first().click();
    await expect(page.getByRole("option").first()).toBeVisible();

    // フォルダーを開くのにツリーをクリックしているので、フォーカスは左カラムの
    // 中にある。そこから Tab を始めると rail を通らない ── 検査したいのは
    // 「先頭から Tab を押していったときの順序」なので、起点を文書の先頭へ戻す。
    // `blur()` では戻らない（sequential focus navigation starting point は
    // 直前の要素に残る）ので、body 自体にフォーカスを移す
    await page.evaluate(() => {
      document.body.setAttribute("tabindex", "-1");
      document.body.focus();
    });

    const RANK = { rail: 0, left: 1, center: 2, right: 3 };
    const seen: (keyof typeof RANK)[] = [];

    // 上限は「右パネルへ着くまで」。左カラムのツリーは項目数だけ Tab を消費するので
    // 固定回数だと届かない。届かないまま打ち切ると順序を検査したことにならない
    for (let i = 0; i < 80; i++) {
      await page.keyboard.press("Tab");
      const region = await page.evaluate(
        () =>
          document.activeElement?.closest("[data-region]")?.getAttribute("data-region") ??
          null
      );
      // カラム間のリサイザーはどの領域にも属さない（AppShell の直接の子）。
      // spec が順序を定めているのは 4 つの領域だけなので、ここでは数えない
      if (region === null) continue;
      const key = region as keyof typeof RANK;
      if (seen[seen.length - 1] !== key) seen.push(key);
      if (key === "right") break;
    }

    // 4 領域すべてを通ること。1 つでも欠けたら「順序どおり」は検査できていない
    expect(new Set(seen)).toEqual(new Set(["rail", "left", "center", "right"]));
    // 一度出た領域へ戻らないこと（= 順序どおりに 1 回ずつ通る）
    const ranks = seen.map((k) => RANK[k]);
    expect(ranks).toEqual([...ranks].sort((a, b) => a - b));
    expect(new Set(ranks).size).toBe(ranks.length);
  });

  test("トーストが右パネル最下部の主ボタンを覆わない", async ({ page }) => {
    // 主ボタン（「N 枚を変換」「保存して次の写真へ」「保存」）は 3 モードとも
    // 右パネルの最下部にある。トーストをそこへ重ねると、消えるまで
    // （成功 4 秒・エラー 8 秒）押せず、見えもしない
    await page.goto("/");
    await page.getByRole("button", { name: "photos", exact: false }).first().click();
    await page.getByRole("option").first().click();
    await page
      .getByRole("navigation", { name: "モード" })
      .getByRole("button", { name: "フレーム" })
      .click();

    const save = page.getByRole("button", { name: "保存", exact: true });
    await save.click();

    const toast = page.getByRole("region", { name: "通知" }).getByRole("status").first();
    await expect(toast).toBeVisible();

    const a = (await toast.boundingBox())!;
    const b = (await save.boundingBox())!;
    const overlaps =
      a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height;
    expect(overlaps).toBe(false);

    // 覆っていないなら、消えるのを待たずにもう一度押せる
    await save.click({ timeout: 2000 });
  });

  test("ライトとダークの両方でスクリーンショットを撮る", async ({ page }) => {
    for (const scheme of ["light", "dark"] as const) {
      await page.emulateMedia({ colorScheme: scheme });
      await page.goto("/");
      await page.getByRole("button", { name: "photos", exact: false }).first().click();
      await expect(page.getByRole("option").first()).toBeVisible();
      // 1 枚選んでおく。選ばないとメタデータもフレームも「対象なし」の
      // 空表示になり、目視（Step 7）で見たいものが写らない
      await page.getByRole("option").first().click();

      const rail = page.getByRole("navigation", { name: "モード" });
      for (const [modeLabel, file] of [
        ["変換", "convert"],
        ["情報", "metadata"],
        ["フレーム", "frame"],
      ] as const) {
        await rail.getByRole("button", { name: modeLabel }).click();
        // 撮る前に、そのモードが実際に立っていること。
        // 立っていない画面を撮ると目視（Step 7）が空振りする
        await expect(rail.getByRole("button", { name: modeLabel })).toHaveAttribute(
          "aria-current",
          "page"
        );
        // フレームのプレビューは 300ms の debounce の先にある。
        // 待たないと中央が「読み込み中...」のまま写る
        if (file === "frame") {
          await expect(
            page.getByRole("img", { name: "Exif フレームのプレビュー" })
          ).toBeVisible();
        }
        // animations: "disabled" が無いと、パネル差し替えの 150ms フェード
        // （spec §3-3）の途中で撮れてしまい、右パネルが opacity 0 の
        // 真っ白として写る。有限のアニメーションは完了状態へ送られる
        await page.screenshot({
          path: `e2e/__screenshots__/${file}-${scheme}.png`,
          animations: "disabled",
        });
      }
    }
  });
});
