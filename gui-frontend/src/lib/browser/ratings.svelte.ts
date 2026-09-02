import { SvelteMap } from "svelte/reactivity";
import { readImageRatings } from "../api";

/**
 * 写真グリッドに出す★の保持。
 *
 * **可視範囲だけ読む。** 3,000 枚を先読みすると、1 枚ごとにファイル全体を読んで
 * Exif と XMP を解析することになる（サムネイルと同じ理由で可視範囲に閉じる）。
 * 1 回の要求は 1 度の IPC にまとめる ── 40 回の往復にすると、
 * スクロールのたびに IPC が詰まる。
 */
/** 1 回の要求で送る枚数。Rust 側の `MAX_BATCH` と揃える */
const CHUNK = 64;

export function createRatingStore() {
  const ratings = new SvelteMap<string, number | null>();
  let inFlight = new Set<string>();

  return {
    /** 未取得なら undefined。読んだ結果が「未評価」なら null */
    get(path: string): number | null | undefined {
      return ratings.get(path);
    },

    request(paths: string[]) {
      const missing = paths.filter((p) => !ratings.has(p) && !inFlight.has(p));
      if (missing.length === 0) return;
      // **Rust 側の上限（64）で分割してから送る。**
      // 超過分をサーバーに切らせると、返ってこなかったパスは ratings に入らないまま
      // inFlight から消え、可視範囲が動くたびに同じ要求を繰り返して永遠に埋まらない
      for (let at = 0; at < missing.length; at += CHUNK) {
        const chunk = missing.slice(at, at + CHUNK);
        chunk.forEach((p) => inFlight.add(p));
        readImageRatings(chunk)
          .then((entries) => {
            for (const entry of entries) ratings.set(entry.path, entry.rating);
          })
          .catch(() => {
            // ★はグリッドの飾り。読めなくても操作は成立するので通知しない
          })
          .finally(() => {
            chunk.forEach((p) => inFlight.delete(p));
          });
      }
    },

    /** 保存した 1 枚だけ取り直す */
    invalidate(path: string) {
      ratings.delete(path);
    },

    /** フォルダーを変えたら捨てる */
    forget() {
      ratings.clear();
      inFlight = new Set();
    },
  };
}
