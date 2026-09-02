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

  test("未保存のまま別の写真へ移ろうとすると止められる（刷新 spec §3-4）", async ({ page }) => {
    await page.getByLabel("タイトル").fill("書きかけ");
    await page.getByRole("option").nth(1).click();

    await expect(
      page.getByRole("alertdialog", { name: "保存していない変更があります" })
    ).toBeVisible();
    // 前提条件: まだ移動していない
    await expect(page.locator('[data-region="right"]').getByText("photo-0000.jpg")).toBeVisible();

    await page.getByRole("button", { name: "破棄して移動" }).click();
    await expect(page.locator('[data-region="right"]').getByText("photo-0001.jpg")).toBeVisible();
    await expect(page.getByLabel("タイトル")).toHaveValue("");
  });

  test("キャンセルすると編集内容が残る", async ({ page }) => {
    await page.getByLabel("タイトル").fill("書きかけ");
    await page.getByRole("option").nth(1).click();
    await page.getByRole("button", { name: "キャンセル" }).click();

    await expect(page.getByLabel("タイトル")).toHaveValue("書きかけ");
    await expect(page.locator('[data-region="right"]').getByText("photo-0000.jpg")).toBeVisible();
  });

  test("rail で別モードへ移るときもガードが出る", async ({ page }) => {
    await page.getByLabel("タイトル").fill("書きかけ");
    await page
      .getByRole("navigation", { name: "モード" })
      .getByRole("button", { name: "変換" })
      .click();
    await expect(
      page.getByRole("alertdialog", { name: "保存していない変更があります" })
    ).toBeVisible();
  });

  /**
   * Task 15 裁定 T2: `FolderTree` はハイライトを自分の `$state` ではなく
   * `currentFolder` prop から導く。先に自分の state を動かしていた頃は、
   * ガードでキャンセルしてもツリーのハイライトだけ新しいフォルダーへ移っていた
   * （グリッドと `currentFolder` は元のまま ── ブリーフが `focusedPath` について
   * 警告した失敗モードと同型）。
   */
  test("未保存でフォルダーを変えようとしてキャンセルすると、ツリーのハイライトは元のフォルダーのまま（Task 15 裁定 T2）", async ({
    page,
  }) => {
    const photosRow = page
      .locator(".tree-row")
      .filter({ has: page.getByRole("button", { name: "photos", exact: false }) })
      .first();
    const archiveRow = page
      .locator(".tree-row")
      .filter({ has: page.getByRole("button", { name: "archive", exact: false }) })
      .first();
    // 前提条件: 最初は beforeEach で選んだ photos がハイライトされている
    await expect(photosRow).toHaveClass(/selected/);
    await expect(archiveRow).not.toHaveClass(/selected/);

    await page.getByLabel("タイトル").fill("書きかけ");
    await page.getByRole("button", { name: "archive", exact: false }).first().click();
    await expect(
      page.getByRole("alertdialog", { name: "保存していない変更があります" })
    ).toBeVisible();

    await page.getByRole("button", { name: "キャンセル" }).click();

    // ハイライトは archive へ移っていない（currentFolder はキャンセルで動いていない）
    await expect(photosRow).toHaveClass(/selected/);
    await expect(archiveRow).not.toHaveClass(/selected/);
  });

  /**
   * 「破棄して移動」でダイアログが解決したら、そのまま変換モードへ入り、
   * ダイアログも閉じたままである（`resumeLeave` が `pendingLeave` を確実に消している）。
   *
   * かつては続けて変換モードのグリッドをクリックし「ダイアログが出ない」まで見ていたが、
   * discard 直後は isDirty が偽になっており、以降 metadata モードへ戻らない限り
   * 再び真にはなり得ない（メタデータの編集 UI は metadata モードにしか無い）ため、
   * その後半 2 行は「変換モードのクリックもガードに通す」欠陥を入れても落ちないことを
   * 欠陥注入 (B) で確認し、削除した。変換モードのクリックが editingPath に触れないことは
   * 「変換モードのクリックは editingPath を動かさない（spec §3-2）」が別に検査している。
   */
  test("破棄して移動すると変換モードへ入り、ダイアログは閉じたまま", async ({ page }) => {
    await page.getByLabel("タイトル").fill("書きかけ");
    const rail = page.getByRole("navigation", { name: "モード" });
    await rail.getByRole("button", { name: "変換" }).click();
    await page.getByRole("button", { name: "破棄して移動" }).click(); // ここは出てよい

    // 変換モードに入った（タイトル欄はもう無い）。discard で isDirty が偽になった直後なので、
    // ここから先で変換モードのグリッドをクリックしても isDirty は再び真になり得ない
    // （メタデータの編集 UI は metadata モードにしか無い）。つまりこの時点で「クリックしても
    // ダイアログが出ない」を検査しても、isDirty が真になる余地が無いぶん、
    // editingPath を変換モードで動かす欠陥を入れても素通りしてしまい検査にならない
    // （欠陥注入 (B) で実測済み）。変換モードのクリックが editingPath に触れないことは
    // 「変換モードのクリックは editingPath を動かさない（spec §3-2）」で別に検査している。
    await expect(page.getByLabel("タイトル")).toHaveCount(0);
    await expect(page.getByRole("alertdialog")).toBeHidden();
  });

  /**
   * 裁定 R1: `metadataDraft.save()` は「実際に書き込めたか」を `Promise<boolean>` で返す。
   * 早期 return（`path === null` / 保存中 / 保存中に写真が変わった）を「保存できた」と
   * 取り違えると、`handleSaveAndLeave` の `if (await handleSaveMetadata()) resumeLeave();`
   * が黙って離脱してしまう。その手前で先に踏める症状として、保存できていないのに
   * 成功トーストが出る ── これを見る。
   *
   * 「保存して移動」を同じ JS タスク内で 2 回連打すると、1 回目は
   * `metadataDraft` の `saving` フラグをまだセットしていない状態で走り、
   * 2 回目は 1 回目がセットした `saving === true` を必ず踏む
   * （同期処理なので描画やスタブの応答速度に依存しない）。よって書き込みが
   * 実際に起きるのは 1 回だけ、というのは実装の正誤に関わらず成り立つ前提。
   * その上で「成功トーストが 1 回しか出ない」を見ることで、2 回目の早期 return が
   * 「保存できた」に化けていないかを検査する。
   */
  test("裁定R1: 保存して移動を連打しても成功トーストは書き込めた分しか出ない", async ({
    page,
  }) => {
    await page.getByRole("button", { name: "このフォルダーへの書き込みを許可..." }).click();
    await page.getByLabel("タイトル").fill("書きかけ");
    await page.getByRole("option").nth(1).click();

    const saveAndLeave = page.getByRole("button", { name: "保存して移動" });
    // 前提条件: 連打する対象のボタンが実在し、クリックできる状態にある
    await expect(saveAndLeave).toBeVisible();

    // Playwright の click() を 2 回連続で await すると、1 回目の描画・状態更新を
    // 待ってしまい「同じタスク内での連打」を再現できない。同期的に 2 回発火させる。
    // Svelte の disabled 反映は次の描画まで遅れるので、2 回目もこの時点では
    // まだ有効な DOM 要素にクリックが通る
    await saveAndLeave.evaluate((el: HTMLElement) => {
      el.click();
      el.click();
    });

    // 前提条件: 実際の書き込みは狙いどおり 1 回しか起きていない
    // （そうでなければ、そもそも「早期 return を成功と取り違える」状況を
    // 作れていない ── これを見ずにトースト回数だけ見ると、再現に失敗しているのか
    // 実装が直っているのかを区別できない）
    await expect
      .poll(() => page.evaluate(() => (window as any).__writeMetadataRequests?.length ?? 0))
      .toBe(1);

    // 本題: 書き込めたのは 1 回なのに、成功トーストが 2 回出ていないか
    // （早期 return を「保存できた」と取り違えていないか）
    await expect(page.getByText("保存しました。")).toHaveCount(1);
  });

  test("未保存の有無が Rust へ伝わる（刷新 spec §3-4）", async ({ page }) => {
    const lastUnsaved = () =>
      page.evaluate(() => {
        const calls = (window as any).__stubCalls as { cmd: string; args: any }[];
        const hits = calls.filter((c) => c.cmd === "set_unsaved_state");
        return hits.length === 0 ? null : hits[hits.length - 1].args.unsaved;
      });

    // 前提条件: 触る前は false（＝「常に true」ではない）
    await expect.poll(lastUnsaved).toBe(false);

    await page.getByLabel("タイトル").fill("書きかけ");
    await expect.poll(lastUnsaved).toBe(true);

    await page.getByRole("button", { name: "このフォルダーへの書き込みを許可..." }).click();
    await page.getByRole("button", { name: "保存", exact: true }).click();
    await expect.poll(lastUnsaved).toBe(false);
  });

  test("保存した★がグリッドのタイルにも出る（刷新 spec §4-3）", async ({ page }) => {
    const tile = page.getByRole("option").first();
    // 前提条件: いまは★が出ていない
    await expect(tile.getByText("★", { exact: false })).toBeHidden();

    await page.getByRole("button", { name: "このフォルダーへの書き込みを許可..." }).click();
    const rating = page.getByRole("slider", { name: "レーティング" });
    await rating.focus();
    await rating.press("ArrowRight");
    await rating.press("ArrowRight");
    await rating.press("ArrowRight");
    await page.getByRole("button", { name: "保存", exact: true }).click();

    await expect(tile.getByText("★★★")).toBeVisible();
  });

  test("未保存の写真にタイル上の印が出る", async ({ page }) => {
    const tile = page.getByRole("option").first();
    await expect(tile.getByTitle("未保存の変更があります")).toBeHidden(); // 前提条件
    await page.getByLabel("タイトル").fill("書きかけ");
    await expect(tile.getByTitle("未保存の変更があります")).toBeVisible();
  });

  test("情報モードへ入っただけで、読み込み時点の★がタイルに出る（保存を経由しない・裁定 X1）", async ({
    page,
  }) => {
    // beforeEach とは別条件（seedRatings 付き）なのでこのテスト内で貼り直す。
    // `read_image_ratings` は保存済み（`stored`）からしか値を引かないので、
    // 「保存を一度もせずに、情報モードへ入っただけで★が出る」経路は
    // この仕込みが無いと検査しようが無い（可視範囲起点の $effect を丸ごと消しても
    // 保存経由のテストだけでは気づけない、という指摘への対応）
    await installTauriStub(page, { imageCount: 12, seedRatings: { 0: 3 } });
    await page.goto("/");
    await page.getByRole("button", { name: "photos", exact: false }).first().click();

    const tile = page.getByRole("option").first();
    // 前提条件: フォルダーを開いた直後（まだ情報モードではない）は★が出ていない
    await expect(tile.getByText("★", { exact: false })).toBeHidden();

    // 保存は一切していない。モード切替だけで★が出るはず
    await page
      .getByRole("navigation", { name: "モード" })
      .getByRole("button", { name: "情報" })
      .click();

    await expect(tile.getByText("★★★")).toBeVisible();
  });
});
