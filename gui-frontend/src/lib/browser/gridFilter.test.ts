/**
 * 写真グリッドの絞り込みと並び替え。
 *
 * 検査したいのは「表示される一覧」そのものである。ここが狂うと
 * **変換対象と表示が食い違う** ── `App.svelte` は絞り込み後の配列を
 * サムネイル要求・情報モードの「次の写真」・変換対象の 3 箇所で index 参照
 * するため、順序の誤りはそのまま「別の写真を処理する」に化ける。
 *
 * 期待値は仕様から取った固定値で書く。実装の式を書き写すと、式ごと
 * 間違えていても一致してしまう。
 */
import { describe, expect, test } from "bun:test";
import type { ImageEntry } from "../types";
import {
  RATING_PRESETS,
  arrangeImages,
  filterByRating,
  filterImages,
  hiddenSelectedCount,
  ratingFilterOf,
  sortImages,
  type RatingPresetId,
} from "./gridFilter";

/** 日時と★を持つ 1 枚。どちらも「取れないことがある」ので既定は null にしておく */
function image(
  name: string,
  opts: { modified?: number | null; created?: number | null; rating?: number | null } = {}
): ImageEntry {
  return {
    name,
    path: `/photos/${name}`,
    width: 4000,
    height: 3000,
    size_bytes: 4_500_000,
    modified_ms: opts.modified ?? null,
    created_ms: opts.created ?? null,
    rating: opts.rating ?? null,
  };
}

const names = (images: ImageEntry[]) => images.map((i) => i.name);

describe("filterImages", () => {
  const IMAGES = [image("DSC_0001.JPG"), image("sunset-osaka.jpg"), image("Sunrise.png")];

  test("検索していない状態（空文字）は全件そのまま", () => {
    // 既定の状態。ここで 0 件になると、フォルダーを開いた瞬間に何も出ない
    expect(names(filterImages(IMAGES, ""))).toEqual([
      "DSC_0001.JPG",
      "sunset-osaka.jpg",
      "Sunrise.png",
    ]);
  });

  test("空白だけの入力も「検索していない」として全件", () => {
    // スペースを 1 つ打った瞬間に一覧が消えるのは誤操作としてありふれている
    expect(filterImages(IMAGES, "   ")).toHaveLength(3);
  });

  test("名前の途中に含まれていれば一致する（部分一致）", () => {
    expect(names(filterImages(IMAGES, "osaka"))).toEqual(["sunset-osaka.jpg"]);
  });

  test("大文字小文字を区別しない", () => {
    // 入力側・ファイル名側の両方を無視できていること。片側だけの実装だと
    // どちらかが落ちる
    expect(names(filterImages(IMAGES, "dsc"))).toEqual(["DSC_0001.JPG"]);
    expect(names(filterImages(IMAGES, "SUNRISE"))).toEqual(["Sunrise.png"]);
  });

  test("前後の空白は無視して照合する", () => {
    expect(names(filterImages(IMAGES, "  osaka  "))).toEqual(["sunset-osaka.jpg"]);
  });

  test("拡張子でも絞り込める", () => {
    // name は拡張子込み。JPEG だけ見たい、は実際にある要求
    expect(names(filterImages(IMAGES, ".png"))).toEqual(["Sunrise.png"]);
  });

  test("一致しなければ 0 件を返す（例外にしない）", () => {
    expect(filterImages(IMAGES, "存在しない")).toEqual([]);
  });

  test("元の配列を書き換えない", () => {
    // `$derived` から呼ぶ。引数の `images` は Svelte の state そのもの
    const source = [...IMAGES];
    filterImages(source, "osaka");
    expect(names(source)).toEqual(["DSC_0001.JPG", "sunset-osaka.jpg", "Sunrise.png"]);
  });
});

describe("sortImages", () => {
  test("名前順は大文字小文字を無視する", () => {
    // ASCII の素の比較だと "Banana"(B=66) < "apple"(a=97) で逆になる。
    // Rust 側（`list_images` の `sort_by_key(name.to_lowercase())`）と
    // 同じ順序でなければ、既定の並び順を選び直した瞬間に一覧が並び替わる
    const images = [image("Banana.jpg"), image("apple.jpg")];
    expect(names(sortImages(images, { key: "name", desc: false }))).toEqual([
      "apple.jpg",
      "Banana.jpg",
    ]);
  });

  test("名前の降順は昇順の逆", () => {
    const images = [image("apple.jpg"), image("Banana.jpg"), image("cherry.jpg")];
    expect(names(sortImages(images, { key: "name", desc: true }))).toEqual([
      "cherry.jpg",
      "Banana.jpg",
      "apple.jpg",
    ]);
  });

  test("更新日の昇順は古いものが先", () => {
    const images = [
      image("new.jpg", { modified: 3_000 }),
      image("old.jpg", { modified: 1_000 }),
      image("mid.jpg", { modified: 2_000 }),
    ];
    expect(names(sortImages(images, { key: "modified", desc: false }))).toEqual([
      "old.jpg",
      "mid.jpg",
      "new.jpg",
    ]);
  });

  test("更新日の降順は新しいものが先", () => {
    const images = [
      image("old.jpg", { modified: 1_000 }),
      image("new.jpg", { modified: 3_000 }),
      image("mid.jpg", { modified: 2_000 }),
    ];
    expect(names(sortImages(images, { key: "modified", desc: true }))).toEqual([
      "new.jpg",
      "mid.jpg",
      "old.jpg",
    ]);
  });

  test("作成日でも並べ替えられる（更新日とは別のキーを見ている）", () => {
    // 2 つのキーを取り違える実装（両方 modified を読む）を落とすため、
    // created と modified を**逆順**に仕込む
    const images = [
      image("a.jpg", { modified: 1_000, created: 3_000 }),
      image("b.jpg", { modified: 3_000, created: 1_000 }),
    ];
    expect(names(sortImages(images, { key: "created", desc: false }))).toEqual([
      "b.jpg",
      "a.jpg",
    ]);
  });

  // 以下 2 件は**どちらも必要**。null を数値に潰す実装は片方だけ通る ──
  // 最小値(-Infinity)扱いなら昇順で先頭に、最大値(+Infinity)扱いなら降順で
  // 先頭に来る。向きの掛け算の外で null を捌かないと両方は通らない
  // （変異テストで両方向を実測して確認済み）
  test("日時が取れなかった写真は昇順で末尾", () => {
    const images = [
      image("unknown.jpg", { modified: null }),
      image("known.jpg", { modified: 1_000 }),
    ];
    expect(names(sortImages(images, { key: "modified", desc: false }))).toEqual([
      "known.jpg",
      "unknown.jpg",
    ]);
  });

  test("日時が取れなかった写真は降順でも末尾（反転させない）", () => {
    // null を「最大値」として扱う実装がここで落ちる。Linux で作成日が
    // 取れない環境では**一覧の先頭が全部 unknown** になる症状
    const images = [
      image("unknown.jpg", { created: null }),
      image("older.jpg", { created: 1_000 }),
      image("newer.jpg", { created: 2_000 }),
    ];
    expect(names(sortImages(images, { key: "created", desc: true }))).toEqual([
      "newer.jpg",
      "older.jpg",
      "unknown.jpg",
    ]);
  });

  test("日時が同じなら名前の昇順で並ぶ", () => {
    // 一括コピーした写真は更新日が秒単位で揃う。並びが不定だと
    // 再描画のたびにタイルが入れ替わって見える
    const images = [
      image("c.jpg", { modified: 1_000 }),
      image("a.jpg", { modified: 1_000 }),
      image("b.jpg", { modified: 1_000 }),
    ];
    expect(names(sortImages(images, { key: "modified", desc: false }))).toEqual([
      "a.jpg",
      "b.jpg",
      "c.jpg",
    ]);
  });

  test("降順でも同値の名前は昇順のまま（同値の並びは反転させない）", () => {
    const images = [
      image("c.jpg", { modified: 1_000 }),
      image("a.jpg", { modified: 1_000 }),
      image("b.jpg", { modified: 1_000 }),
    ];
    expect(names(sortImages(images, { key: "modified", desc: true }))).toEqual([
      "a.jpg",
      "b.jpg",
      "c.jpg",
    ]);
  });

  test("元の配列を書き換えない", () => {
    // Array.prototype.sort はインプレース。ここを守らないと Svelte の
    // state を直接並び替えることになる
    const source = [image("b.jpg"), image("a.jpg")];
    sortImages(source, { key: "name", desc: false });
    expect(names(source)).toEqual(["b.jpg", "a.jpg"]);
  });
});

describe("arrangeImages", () => {
  test("絞り込んだ結果に並び順を適用する", () => {
    const images = [
      image("sunset-b.jpg", { modified: 1_000 }),
      image("other.jpg", { modified: 9_000 }),
      image("sunset-a.jpg", { modified: 2_000 }),
    ];
    // other.jpg は更新日が最も新しいが、絞り込みで落ちるので出てこない
    expect(
      names(
        arrangeImages(images, "sunset", ratingFilterOf("all"), {
          key: "modified",
          desc: true,
        })
      )
    ).toEqual(["sunset-a.jpg", "sunset-b.jpg"]);
  });
});

describe("filterByRating", () => {
  const IMAGES = [
    image("r1.jpg", { rating: 1 }),
    image("r2.jpg", { rating: 2 }),
    image("r3.jpg", { rating: 3 }),
    image("r4.jpg", { rating: 4 }),
    image("r5.jpg", { rating: 5 }),
    image("none-a.jpg"),
    image("none-b.jpg"),
  ];

  /**
   * 画面に出す 7 択（設計 §3 の対応表がそのまま期待値）。
   *
   * **`RATING_PRESETS` を回して照合する。** ここで `{ min, includeUnrated }` を
   * 手で書き写すと、対応表を写し間違えても両方同じ間違いになって緑になる
   */
  const EXPECTED: Record<RatingPresetId, string[]> = {
    all: ["r1.jpg", "r2.jpg", "r3.jpg", "r4.jpg", "r5.jpg", "none-a.jpg", "none-b.jpg"],
    rated: ["r1.jpg", "r2.jpg", "r3.jpg", "r4.jpg", "r5.jpg"],
    min2: ["r2.jpg", "r3.jpg", "r4.jpg", "r5.jpg"],
    min3: ["r3.jpg", "r4.jpg", "r5.jpg"],
    min4: ["r4.jpg", "r5.jpg"],
    only5: ["r5.jpg"],
    unrated: ["none-a.jpg", "none-b.jpg"],
  };

  test("7 択がすべて対応表どおりの集合を返す", () => {
    // 前提条件: 選択肢が 7 つある（表から漏れた選択肢が黙って通らないように）
    expect(RATING_PRESETS).toHaveLength(7);
    for (const preset of RATING_PRESETS) {
      expect(names(filterByRating(IMAGES, preset.filter))).toEqual(EXPECTED[preset.id]);
    }
  });

  test("既定は「すべて」── フォルダーを開いた瞬間に何かが隠れていない", () => {
    expect(names(filterByRating(IMAGES, ratingFilterOf("all")))).toEqual(names(IMAGES));
  });

  test("閾値は「以上」── ★3 以上に★3 は入り、★2 は入らない", () => {
    const shown = names(filterByRating(IMAGES, ratingFilterOf("min3")));
    expect(shown).toContain("r3.jpg");
    // 前提条件: 除かれる側が母集合に実在する（「元から居ないから出ない」を防ぐ）
    expect(names(IMAGES)).toContain("r2.jpg");
    expect(shown).not.toContain("r2.jpg");
  });

  test("「★が付いているもの」は未評価を落とす", () => {
    const shown = names(filterByRating(IMAGES, ratingFilterOf("rated")));
    // 前提条件: 未評価が母集合に 2 枚ある
    expect(IMAGES.filter((i) => i.rating === null)).toHaveLength(2);
    expect(shown).not.toContain("none-a.jpg");
    expect(shown).not.toContain("none-b.jpg");
  });

  test("「未評価だけ」は★の付いたものを全部落とす", () => {
    const shown = names(filterByRating(IMAGES, ratingFilterOf("unrated")));
    // 前提条件: ★付きが母集合に 5 枚ある
    expect(IMAGES.filter((i) => i.rating !== null)).toHaveLength(5);
    expect(shown).toEqual(["none-a.jpg", "none-b.jpg"]);
  });

  test("0 件になってもエラーにならず空配列を返す", () => {
    const onlyRated = [image("r1.jpg", { rating: 1 })];
    expect(filterByRating(onlyRated, ratingFilterOf("unrated"))).toEqual([]);
  });

  /**
   * **★0 は「未評価」。** Rust 側が `xmp:Rating = 0` を `None` に落とすので
   * ここへは来ないが、その不変条件が破れたときに「すべて」が写真を隠さないことを
   * 固定しておく（0 が意味するのは「評価が無い」であって「評価が 0」ではない）
   */
  test("★0 は未評価と同じ扱いになる", () => {
    const withZero = [...IMAGES, image("zero.jpg", { rating: 0 })];
    // 前提条件: 0 を持つ写真が母集合に居る（型の上では来ない値なので明示する）
    expect(withZero.some((i) => i.rating === 0)).toBe(true);

    expect(names(filterByRating(withZero, ratingFilterOf("all")))).toContain("zero.jpg");
    expect(names(filterByRating(withZero, ratingFilterOf("unrated")))).toContain("zero.jpg");
    expect(names(filterByRating(withZero, ratingFilterOf("rated")))).not.toContain("zero.jpg");
  });

  test("元の配列を書き換えない", () => {
    const before = names(IMAGES);
    filterByRating(IMAGES, ratingFilterOf("only5"));
    expect(names(IMAGES)).toEqual(before);
  });
});

describe("arrangeImages と★の合成", () => {
  const IMAGES = [
    image("sunset-a.jpg", { rating: 5, modified: 1_000 }),
    image("sunset-b.jpg", { rating: 2, modified: 2_000 }),
    image("other.jpg", { rating: 5, modified: 3_000 }),
  ];

  test("名前で落ちた写真は★の条件を満たしても出ない", () => {
    // 前提条件: other.jpg は★5 なので、★だけで見れば残る側である
    expect(names(filterByRating(IMAGES, ratingFilterOf("only5")))).toContain("other.jpg");

    const shown = names(
      arrangeImages(IMAGES, "sunset", ratingFilterOf("only5"), { key: "name", desc: false })
    );
    expect(shown).toEqual(["sunset-a.jpg"]);
  });
});

describe("hiddenSelectedCount", () => {
  const IMAGES = [image("a.jpg"), image("b.jpg"), image("sunset.jpg")];

  test("選択が無ければ 0", () => {
    const visible = filterImages(IMAGES, "");
    expect(hiddenSelectedCount(visible, new Set())).toBe(0);
  });

  test("選択が全部見えているなら 0", () => {
    const visible = filterImages(IMAGES, "");
    expect(hiddenSelectedCount(visible, new Set(["/photos/a.jpg"]))).toBe(0);
  });

  test("絞り込みで隠れた選択の枚数を数える", () => {
    const visible = filterImages(IMAGES, "sunset");
    // **前提**: 数えたい状況が実際に起きていること。これが無いと
    // 「隠れていないから 0」でも緑になる否定形テストになる
    expect(names(visible)).toEqual(["sunset.jpg"]);

    const selected = new Set(["/photos/a.jpg", "/photos/b.jpg", "/photos/sunset.jpg"]);
    expect(hiddenSelectedCount(visible, selected)).toBe(2);
  });
});
