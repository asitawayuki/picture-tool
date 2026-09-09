/**
 * 写真グリッドの絞り込みと並び替え。
 *
 * **一覧の順序を決めるのはここ 1 箇所だけ。** `App.svelte` は絞り込み後の配列を
 * サムネイル要求・情報モードの「次の写真」・変換対象の 3 箇所で index 参照する。
 * 順序が 2 箇所で決まると、その index がずれて**別の写真を処理する**に化ける。
 *
 * `list_images` が返す `ImageEntry` に必要な値（名前・更新日・作成日・★）は
 * すべて入っているので、ここは IPC を一切しない純関数で済む。★は
 * `core::metadata::read_metadata_fast`（JPEG の先頭だけを読む）を `list_images` に
 * 畳んだことで、フォルダーを開いた時点で全件揃っている。撮影日での絞り込みは
 * Exif `DateTimeOriginal` の解釈が別の設計になるので、まだ持っていない。
 */
import type { ImageEntry } from "../types";

export type SortKey = "name" | "modified" | "created";

export interface GridSort {
  key: SortKey;
  /** 降順。日時は「新しい順」、名前は「Z→A」になる */
  desc: boolean;
}

/** 並び順の既定。Rust 側（`list_images`）が返す順序と同じものを選んでおく */
export const DEFAULT_SORT: GridSort = { key: "name", desc: false };

export const SORT_LABELS: Record<SortKey, string> = {
  name: "名前",
  modified: "更新日",
  created: "作成日",
};

/**
 * ★の絞り込み。
 *
 * **`min: 1` が「★で絞り込んでいない」である。** ★は 1〜5 しか取らない
 * （★0 は書き込まず、他アプリが書いた `xmp:Rating = 0` も Rust 側の
 * `read_metadata` が `None` に落とす）。だから `min` に「絞り込まない」専用の値は
 * 要らず、**`null` を「★の付いた写真は出さない」に使える**。
 */
export interface RatingFilter {
  /** ★の下限。**null は「★の付いた写真は出さない」** */
  min: 1 | 2 | 3 | 4 | 5 | null;
  /** 未評価（rating === null）も出すか */
  includeUnrated: boolean;
}

export type RatingPresetId =
  | "all"
  | "rated"
  | "min2"
  | "min3"
  | "min4"
  | "only5"
  | "unrated";

/**
 * 画面に出す 7 択。**2 つのつまみを利用者に組み立てさせない。**
 *
 * `min` と `includeUnrated` をそのまま 2 つの部品にすると、「なし / ★1 / …」の
 * 「なし」が日常語で「★での絞り込みなし」と読めてしまう。実際の意味は
 * 「★ありを全部隠す」なので、**絞り込みを解除しようとした利用者の画面から
 * ★付きが全部消える**。7 択なら「両方切った状態＝常に 0 件」という
 * 意味の無い組み合わせも利用者が選べない。
 *
 * **この配列が唯一の対応表。** `GridFilterBar` はこれを `<select>` に流し、
 * テストも同じ配列を回す。部品の中に書くと純関数のテストから参照できず、
 * 写し間違えても緑になる。
 *
 * `min: n` ＋ `includeUnrated: true`（「★n 以上 ＋ 未評価」）は 7 択から漏れるが、
 * 内部表現には残るので、要望が出たら選択肢を足すだけで足りる。
 */
export const RATING_PRESETS: {
  id: RatingPresetId;
  label: string;
  filter: RatingFilter;
}[] = [
  { id: "all", label: "すべて", filter: { min: 1, includeUnrated: true } },
  { id: "rated", label: "★が付いているもの", filter: { min: 1, includeUnrated: false } },
  { id: "min2", label: "★2 以上", filter: { min: 2, includeUnrated: false } },
  { id: "min3", label: "★3 以上", filter: { min: 3, includeUnrated: false } },
  { id: "min4", label: "★4 以上", filter: { min: 4, includeUnrated: false } },
  { id: "only5", label: "★5", filter: { min: 5, includeUnrated: false } },
  { id: "unrated", label: "未評価だけ", filter: { min: null, includeUnrated: true } },
];

/** ★の絞り込みの既定。起動直後に何かが隠れている状態を作らない */
export const DEFAULT_RATING_PRESET: RatingPresetId = "all";

export function ratingFilterOf(id: RatingPresetId): RatingFilter {
  // 見つからないことは型の上で起きない。`!` ではなく既定へ落として、
  // 万一 id が増えたときにグリッドが空になるより「すべて」に倒す
  return (
    RATING_PRESETS.find((preset) => preset.id === id)?.filter ?? {
      min: 1,
      includeUnrated: true,
    }
  );
}

/**
 * ファイル名の部分一致で絞り込む。大文字小文字は区別しない。
 *
 * 空文字と空白だけの入力は「絞り込んでいない」として全件返す ── スペースを
 * 1 つ打った瞬間に一覧が消えるのは、誤操作としてありふれている。
 */
export function filterImages(images: readonly ImageEntry[], query: string): ImageEntry[] {
  const needle = query.trim().toLowerCase();
  if (needle === "") return [...images];
  return images.filter((image) => image.name.toLowerCase().includes(needle));
}

/**
 * ★で絞り込む。**元の配列は書き換えない。**
 *
 * **`rating === 0` は `null` と同じ「未評価」として扱う。**
 * ここへは来ない（Rust 側の `read_metadata` が `xmp:Rating = 0` を `None` に落とす。
 * `xmp.rs` の `parse_stars` と `mod.rs` の `percent_to_stars`）が、
 * 来た場合に「どの下限も満たさない」として全部落とすと、
 * **「すべて」という名前の選択肢が写真を静かに隠す**ことになる。
 * 0 が意味するのは「評価が無い」であって「評価が 0 である」ではないので、
 * **Rust 側と同じ規則をここでも守る**のが唯一の一貫した扱いである。
 */
export function filterByRating(
  images: readonly ImageEntry[],
  filter: RatingFilter
): ImageEntry[] {
  return images.filter((image) => {
    const stars = image.rating === null || image.rating === 0 ? null : image.rating;
    if (stars === null) return filter.includeUnrated;
    return filter.min !== null && stars >= filter.min;
  });
}

/** 並び替えのキーを取り出す。null は「取れなかった」 */
function timeOf(image: ImageEntry, key: SortKey): number | null {
  return key === "modified" ? image.modified_ms : image.created_ms;
}

/**
 * 並び替える。**元の配列は書き換えない**（`$derived` から Svelte の state を渡される）。
 *
 * 日時が取れなかった写真は**昇順でも降順でも末尾**へ送る。null を数値へ潰すと
 * （最小値なら昇順で、最大値なら降順で）どちらかの向きで先頭に来てしまい、
 * 作成日が取れないファイルシステム（Linux に実在する）では一覧の先頭が
 * 全部「不明」で埋まる。だから向きの掛け算の外で捌く。
 *
 * 同値のときは名前の昇順で解決する。これも `desc` では反転させない ── 一括
 * コピーした写真は更新日が揃うので、そこが不定だと再描画ごとにタイルが
 * 入れ替わって見える。
 */
export function sortImages(images: readonly ImageEntry[], sort: GridSort): ImageEntry[] {
  const direction = sort.desc ? -1 : 1;
  // Rust 側（`gui/src/commands.rs` の `sort_entries`）は「小文字化した名前 → 元の名前」の
  // 2 段比較で、同じ小文字名を持つ `A.jpg` と `a.jpg` の間にも決定的な順序をつける。
  // ここは 1 段目（小文字）だけを見て同値なら 0 を返す ── tie の相対順は
  // `Array.sort` の安定性（ES2019 以降で仕様保証）に委ね、入力順＝ Rust が決めた
  // 順序をそのまま残す。昇順は 0 * 1 = 0 なので Rust の順序と一致し、降順も
  // 0 * -1 = 0 で tie の相対順だけは反転しない（他の値は反転する）。
  // どちらの向きも決定的なので、spec §4 の「順序の決定性・3 参照元の index 一致」は保たれる。
  // `localeCompare` にすると既定の並び順を選び直した瞬間に一覧が並び替わる
  const byName = (a: ImageEntry, b: ImageEntry) => {
    const left = a.name.toLowerCase();
    const right = b.name.toLowerCase();
    return left < right ? -1 : left > right ? 1 : 0;
  };

  return [...images].sort((a, b) => {
    if (sort.key === "name") return byName(a, b) * direction;

    const left = timeOf(a, sort.key);
    const right = timeOf(b, sort.key);
    // 「不明」同士は名前で決める。片方だけ不明なら向きに関係なく後ろへ
    if (left === null && right === null) return byName(a, b);
    if (left === null) return 1;
    if (right === null) return -1;
    if (left === right) return byName(a, b);
    return (left < right ? -1 : 1) * direction;
  });
}

/** 名前で絞る → ★で絞る → 並び替える。画面に出す一覧はこれ */
export function arrangeImages(
  images: readonly ImageEntry[],
  query: string,
  rating: RatingFilter,
  sort: GridSort
): ImageEntry[] {
  return sortImages(filterByRating(filterImages(images, query), rating), sort);
}

/**
 * 選択済みだが絞り込みで画面に出ていない枚数。
 *
 * 絞り込みは表示の話なので選択には触らない。ただし黙って隠すと
 * 「N 枚選択中」が嘘になり、**見えていない写真まで変換される**。
 * その枚数を画面に出すためにここで数える。
 */
export function hiddenSelectedCount(
  visible: readonly ImageEntry[],
  selectedPaths: ReadonlySet<string>
): number {
  if (selectedPaths.size === 0) return 0;
  const shown = new Set(visible.map((image) => image.path));
  let hidden = 0;
  for (const path of selectedPaths) {
    if (!shown.has(path)) hidden += 1;
  }
  return hidden;
}
