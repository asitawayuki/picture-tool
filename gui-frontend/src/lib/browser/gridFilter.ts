/**
 * 写真グリッドの絞り込みと並び替え。
 *
 * **一覧の順序を決めるのはここ 1 箇所だけ。** `App.svelte` は絞り込み後の配列を
 * サムネイル要求・情報モードの「次の写真」・変換対象の 3 箇所で index 参照する。
 * 順序が 2 箇所で決まると、その index がずれて**別の写真を処理する**に化ける。
 *
 * `list_images` が返す `ImageEntry` に必要な値（名前・更新日・作成日）は
 * すべて入っているので、ここは IPC を一切しない純関数で済む。★や撮影日で
 * 絞り込むにはフォルダー全体のメタデータ走査が必要で、それは別の設計になる。
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
  // Rust 側の `sort_by_key(|a| a.name.to_lowercase())` と同じ比較にする。
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

/** 絞り込み → 並び替え。画面に出す一覧はこれ */
export function arrangeImages(
  images: readonly ImageEntry[],
  query: string,
  sort: GridSort
): ImageEntry[] {
  return sortImages(filterImages(images, query), sort);
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
