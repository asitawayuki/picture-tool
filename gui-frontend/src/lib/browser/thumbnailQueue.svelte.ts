import { SvelteMap } from "svelte/reactivity";
import { getThumbnail } from "../api";
import { describeError, toast } from "../toasts.svelte";

/**
 * サムネイルの取得キューとキャッシュ。
 *
 * サムネイルは解像度ごとに別物なので `path:maxDimension` をキーにする。
 * path だけで持つと列数を変えても再取得されず、低解像度が引き伸ばされる。
 *
 * 本モジュールは Task 7 では App.svelte からの移設のみで、仕様は現行どおり
 * （FIFO・eviction なし）。LIFO 化・可視範囲による破棄・LRU 上限は
 * spec §4-2 に従って Task 12 で入れる。
 */
export interface ThumbnailQueue {
  get(path: string, maxDimension: number): string | undefined;
  request(path: string, maxDimension: number): void;
}

const MAX_CONCURRENT = 3;

function keyOf(path: string, maxDimension: number): string {
  return `${path}:${maxDimension}`;
}

export function createThumbnailQueue(): ThumbnailQueue {
  const cache = new SvelteMap<string, string>();
  const pending: { path: string; maxDimension: number }[] = [];
  /** 同一キーの失敗を繰り返し再要求しないための記録 */
  const failed = new Set<string>();

  let active = 0;
  let errorReported = false;

  function pump() {
    while (active < MAX_CONCURRENT && pending.length > 0) {
      const { path, maxDimension } = pending.shift()!;
      const key = keyOf(path, maxDimension);
      if (cache.has(key)) continue;
      active++;
      getThumbnail(path, maxDimension)
        .then((base64) => {
          cache.set(key, base64);
        })
        .catch((e) => {
          failed.add(key);
          // 1 枚ごとにトーストを出すと壊れたフォルダーで埋め尽くされるため
          // 最初の 1 件だけ通知する
          if (!errorReported) {
            errorReported = true;
            toast.error(`サムネイルを生成できない画像があります: ${describeError(e)}`);
          }
        })
        .finally(() => {
          active--;
          pump();
        });
    }
  }

  return {
    get(path, maxDimension) {
      return cache.get(keyOf(path, maxDimension));
    },
    request(path, maxDimension) {
      const key = keyOf(path, maxDimension);
      if (cache.has(key) || failed.has(key)) return;
      if (pending.some((r) => r.path === path && r.maxDimension === maxDimension)) return;
      pending.push({ path, maxDimension });
      pump();
    },
  };
}
