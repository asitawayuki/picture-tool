export type RequestKind = "discardable" | "pinned";

export interface ThumbnailRequest {
  /** `path:size`。キャッシュのキーと同じもの */
  key: string;
  path: string;
  size: number;
  /**
   * discardable — グリッド由来。可視範囲を外れたら未処理のものを捨てる
   * pinned      — それ以外の要求元。範囲による破棄の対象外（spec §4-2）
   */
  kind: RequestKind;
  /** グリッド上の通し番号。pinned は -1 でよい */
  index: number;
}

/**
 * サムネイル取得の待ち行列。
 *
 * 基本は LIFO。最後に要求されたもの＝いま見えているものを先に処理する。
 * ただし初回の 1 画面分だけは投入順で流す（下から埋まって見えるのを避ける）。
 *
 * 可視範囲による破棄は setVisibleRange が呼ばれたときにだけ起きる。
 * グリッドが unmount している間（フレームモード）は呼ばれないので、
 * 最後の範囲を保ったまま破棄も起きない ── これが spec §4-2 の求める挙動。
 */
export class RequestQueue {
  /** 初回の 1 画面分。FIFO で流す */
  #priming: ThumbnailRequest[] = [];
  /** それ以降。LIFO で流す */
  #stack: ThumbnailRequest[] = [];
  #keys = new Set<string>();
  #primingRemaining = 0;

  get pendingCount(): number {
    return this.#priming.length + this.#stack.length;
  }

  has(key: string): boolean {
    return this.#keys.has(key);
  }

  push(request: ThumbnailRequest): void {
    if (this.#keys.has(request.key)) return;
    this.#keys.add(request.key);
    if (this.#primingRemaining > 0) {
      this.#primingRemaining--;
      this.#priming.push(request);
    } else {
      this.#stack.push(request);
    }
  }

  take(): ThumbnailRequest | undefined {
    const next = this.#priming.shift() ?? this.#stack.pop();
    if (next) this.#keys.delete(next.key);
    return next;
  }

  /**
   * グリッドの可視範囲を通知する。範囲外の未処理 discardable を捨てる。
   * 戻り値は捨てた件数（検査と計測のため）。
   */
  setVisibleRange(start: number, end: number): number {
    const keep = (r: ThumbnailRequest) =>
      r.kind === "pinned" || (r.index >= start && r.index <= end);

    let dropped = 0;
    const drop = (list: ThumbnailRequest[]): ThumbnailRequest[] =>
      list.filter((r) => {
        if (keep(r)) return true;
        this.#keys.delete(r.key);
        dropped++;
        return false;
      });

    this.#priming = drop(this.#priming);
    this.#stack = drop(this.#stack);
    return dropped;
  }

  /** フォルダーを変えたとき。残りを捨てて priming を張り直す */
  reset(primeCount: number): void {
    this.clear();
    this.#primingRemaining = Math.max(0, primeCount);
  }

  clear(): void {
    this.#priming = [];
    this.#stack = [];
    this.#keys.clear();
  }
}
