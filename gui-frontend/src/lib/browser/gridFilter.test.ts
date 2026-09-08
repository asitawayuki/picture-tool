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
import { arrangeImages, filterImages, hiddenSelectedCount, sortImages } from "./gridFilter";

/** 日時を持つ 1 枚。日時は「取れないことがある」ので既定は null にしておく */
function image(
  name: string,
  times: { modified?: number | null; created?: number | null } = {}
): ImageEntry {
  return {
    name,
    path: `/photos/${name}`,
    width: 4000,
    height: 3000,
    size_bytes: 4_500_000,
    modified_ms: times.modified ?? null,
    created_ms: times.created ?? null,
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
    expect(names(arrangeImages(images, "sunset", { key: "modified", desc: true }))).toEqual([
      "sunset-a.jpg",
      "sunset-b.jpg",
    ]);
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
