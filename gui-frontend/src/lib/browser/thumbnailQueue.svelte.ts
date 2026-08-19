import { SvelteMap } from "svelte/reactivity";
import { getThumbnail } from "../api";
import { describeError, toast } from "../toasts.svelte";
import { CACHE_BYTE_LIMIT, LruBudget } from "./thumbnailCache";
import { RequestQueue, type RequestKind } from "./requestQueue";

/**
 * サムネイルの取得キューとキャッシュ。
 *
 * サムネイルは解像度ごとに別物なので `path:maxDimension` をキーにする。
 * path だけで持つと列数を変えても再取得されず、低解像度が引き伸ばされる。
 *
 * 値は SvelteMap（リアクティブ）、順序とバイト数は LruBudget、
 * 待ち行列は RequestQueue が持つ。3 つとも役割が分かれている。
 */
export interface ThumbnailQueue {
  get(path: string, maxDimension: number): string | undefined;
  request(path: string, maxDimension: number, kind?: RequestKind, index?: number): void;
  setVisibleRange(start: number, end: number): void;
  resetForFolder(primeCount: number): void;
  stats(): { bytes: number; entries: number };
}

const MAX_CONCURRENT = 3;

function keyOf(path: string, maxDimension: number): string {
  return `${path}:${maxDimension}`;
}

export function createThumbnailQueue(): ThumbnailQueue {
  const values = new SvelteMap<string, string>();
  const budget = new LruBudget(CACHE_BYTE_LIMIT);
  const queue = new RequestQueue();
  /** 同一キーの失敗を繰り返し再要求しないための記録 */
  const failed = new Set<string>();
  /** 処理中のキー。範囲外の破棄はここには効かない（spec §4-2） */
  const inFlight = new Set<string>();

  let active = 0;
  let errorReported = false;

  function pump() {
    while (active < MAX_CONCURRENT) {
      const request = queue.take();
      if (!request) return;
      if (values.has(request.key)) continue;

      active++;
      inFlight.add(request.key);
      getThumbnail(request.path, request.size)
        .then((base64) => {
          values.set(request.key, base64);
          // base64 は ASCII なので、文字数がそのまま保持バイト数の目安になる
          for (const evicted of budget.admit(request.key, base64.length)) {
            values.delete(evicted);
          }
        })
        .catch((e) => {
          failed.add(request.key);
          // 1 枚ごとにトーストを出すと壊れたフォルダーで埋め尽くされるため
          // 最初の 1 件だけ通知する
          if (!errorReported) {
            errorReported = true;
            toast.error(`サムネイルを生成できない画像があります: ${describeError(e)}`);
          }
        })
        .finally(() => {
          inFlight.delete(request.key);
          active--;
          pump();
        });
    }
  }

  return {
    get(path, maxDimension) {
      const key = keyOf(path, maxDimension);
      const value = values.get(key);
      // 参照されたら LRU 上で新しい扱いにする。値そのものは変えないので
      // リアクティブな読み取りの中から呼んでも再描画は誘発しない
      if (value !== undefined) budget.touch(key);
      return value;
    },

    request(path, maxDimension, kind = "pinned", index = -1) {
      const key = keyOf(path, maxDimension);
      if (values.has(key) || failed.has(key) || inFlight.has(key)) return;
      queue.push({ key, path, size: maxDimension, kind, index });
      pump();
    },

    setVisibleRange(start, end) {
      queue.setVisibleRange(start, end);
    },

    resetForFolder(primeCount) {
      queue.reset(primeCount);
      // キャッシュは残す。同じフォルダーへ戻ったときに再取得しないため。
      // 溢れれば LRU が古いフォルダー分から順に落とす
    },

    stats() {
      return { bytes: budget.bytes, entries: budget.size };
    },
  };
}
