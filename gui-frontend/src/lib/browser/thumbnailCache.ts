/**
 * サムネイルキャッシュの追い出し規則（LRU + バイト上限）。
 *
 * 値は持たない。「どのキーを、どの順で、何バイトで持っているか」だけを持つ台帳。
 * 値は thumbnailQueue.svelte.ts の SvelteMap 側にある。
 *
 * Map の反復順は挿入順なので、先頭が最も古い＝次に追い出す対象になる。
 */
export class LruBudget {
  readonly byteLimit: number;
  /** key -> bytes。反復順が LRU 順（先頭が最古） */
  #entries = new Map<string, number>();
  #bytes = 0;

  constructor(byteLimit: number) {
    this.byteLimit = byteLimit;
  }

  get bytes(): number {
    return this.#bytes;
  }

  get size(): number {
    return this.#entries.size;
  }

  has(key: string): boolean {
    return this.#entries.has(key);
  }

  /** 参照された。順序だけ最新へ動かす */
  touch(key: string): void {
    const bytes = this.#entries.get(key);
    if (bytes === undefined) return;
    this.#entries.delete(key);
    this.#entries.set(key, bytes);
  }

  /**
   * 追加し、上限を超えた分として追い出すべきキーを古い順に返す。
   *
   * 新しく入れた項目自体は追い出さない。1 件で上限を超える大きさでも保持する
   * （捨てると毎回 IPC が走るだけで、上限を守る意味が無い）。
   */
  admit(key: string, bytes: number): string[] {
    this.remove(key);
    this.#entries.set(key, bytes);
    this.#bytes += bytes;

    const evicted: string[] = [];
    for (const oldest of this.#entries.keys()) {
      if (this.#bytes <= this.byteLimit) break;
      if (oldest === key) continue;
      evicted.push(oldest);
      this.#bytes -= this.#entries.get(oldest)!;
      this.#entries.delete(oldest);
    }
    return evicted;
  }

  remove(key: string): void {
    const bytes = this.#entries.get(key);
    if (bytes === undefined) return;
    this.#entries.delete(key);
    this.#bytes -= bytes;
  }

  clear(): void {
    this.#entries.clear();
    this.#bytes = 0;
  }
}
