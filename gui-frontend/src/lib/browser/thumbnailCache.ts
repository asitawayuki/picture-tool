/**
 * サムネイルキャッシュの追い出し規則（LRU + バイト上限）。
 *
 * 値は持たない。「どのキーを、どの順で、何バイトで持っているか」だけを持つ台帳。
 * 値は thumbnailQueue.svelte.ts の SvelteMap 側にある。
 *
 * Map の反復順は挿入順なので、先頭が最も古い＝次に追い出す対象になる。
 */
/**
 * サムネイルキャッシュのバイト上限（spec §7-2 / §8 の実測で確定）。
 *
 * **上限 = 1 枚あたりのバイト数 × 保持したい枚数 × 解像度の種類。**
 * 保持したい枚数は「3,000 枚のフォルダーを 1 往復しても戻ったときに
 * 再取得が起きない程度」＝ 1 解像度あたり 3,000 枚。サイズスライダーで
 * 解像度が 2〜3 種類できるので 2 倍する。
 *
 * 1 枚あたりは **20KB** を採った。根拠（すべて base64 の文字数。core は
 * 長辺 max_dimension・JPEG 品質 75 で焼くので、文字数 ≒ 保持バイト数）:
 *
 * | 絵柄 | 320px | 512px |
 * |---|---|---|
 * | e2e スタブのグラデーション | 3.8KB | ─ |
 * | 写真に近い帯域制限ノイズ | 6.6KB | 15.1KB |
 * | 一様ノイズ（病的な上限） | 59.6KB | 151KB |
 *
 * **e2e の実測値（3.8KB）をそのまま使ってはいけない。** スタブの絵は
 * 単調なグラデーションで、実際の写真より 1 桁小さく焼ける。実写は上表の
 * 帯域制限ノイズより細かいので、その 512px の値に余裕を持たせて 20KB とする。
 *
 * 20KB × 3,000 枚 × 2 種類 = 120MB → 切りのよい 128MB。
 * 上限が高すぎるとメモリを食うだけだが、低すぎると往復のたびに
 * IPC と Rust 側のデコードをやり直すことになる（体感に直結する）ため、
 * 迷ったら高い側へ倒してある。
 */
export const CACHE_BYTE_LIMIT = 128 * 1024 * 1024;

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
