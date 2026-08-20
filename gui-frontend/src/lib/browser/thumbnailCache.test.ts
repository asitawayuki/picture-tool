/**
 * spec §4-2「キャッシュの総量」。
 *
 * LRU は「値の保管庫」ではなく「どのキーをどの順で持っているかの台帳」として
 * 実装する。値そのものは reactive な SvelteMap 側が持ち、ここは順序とバイト数
 * だけを見る。こうすると UI を起動せずに追い出し規則を検査できる。
 */
import { describe, expect, test } from "bun:test";
import { LruBudget } from "./thumbnailCache";

describe("LruBudget", () => {
  test("上限内なら何も追い出さない", () => {
    const lru = new LruBudget(100);
    expect(lru.admit("a", 30)).toEqual([]);
    expect(lru.admit("b", 30)).toEqual([]);
    expect(lru.bytes).toBe(60);
    expect(lru.size).toBe(2);
  });

  test("上限を超えたら古い順に追い出す", () => {
    const lru = new LruBudget(100);
    lru.admit("a", 40);
    lru.admit("b", 40);
    expect(lru.admit("c", 40)).toEqual(["a"]);
    expect(lru.has("a")).toBe(false);
    expect(lru.bytes).toBe(80);
  });

  test("1 件では足りなければ上限を下回るまで追い出す", () => {
    // 「1 回の admit につき 1 件だけ追い出す」実装だと上限を超えたまま残る。
    // 上限はスクロール中に何度も跨ぐので、超過が積み上がると意味を成さない
    const lru = new LruBudget(100);
    lru.admit("a", 30);
    lru.admit("b", 30);
    lru.admit("c", 30);
    // 30+30+30+50 = 140。a を出して 110、まだ超えるので b も出して 80 で止まる
    expect(lru.admit("d", 50)).toEqual(["a", "b"]);
    expect(lru.bytes).toBe(80);
    expect(lru.has("c")).toBe(true);
  });

  test("touch した項目は新しい扱いになり、次の追い出しを免れる", () => {
    const lru = new LruBudget(100);
    lru.admit("a", 40);
    lru.admit("b", 40);
    // 前提条件: touch しなければ a が追い出される（上の test で確認済みの挙動）
    lru.touch("a");
    expect(lru.admit("c", 40)).toEqual(["b"]);
    expect(lru.has("a")).toBe(true);
  });

  test("上限を 1 件で超える項目は保持する（追い出して空にしない）", () => {
    // ここを「新入りごと捨てる」にすると、大きなサムネイルが永久にキャッシュ
    // されず毎回 IPC が走る。上限は目安であって不変条件ではない
    const lru = new LruBudget(100);
    lru.admit("small", 50);
    expect(lru.admit("huge", 500)).toEqual(["small"]);
    expect(lru.has("huge")).toBe(true);
    expect(lru.bytes).toBe(500);
  });

  test("同じキーを再度 admit してもバイト数が二重に積まれない", () => {
    const lru = new LruBudget(1000);
    lru.admit("a", 40);
    lru.admit("a", 60);
    expect(lru.size).toBe(1);
    expect(lru.bytes).toBe(60);
  });

  test("remove と clear がバイト数を戻す", () => {
    const lru = new LruBudget(1000);
    lru.admit("a", 40);
    lru.admit("b", 60);
    lru.remove("a");
    expect(lru.bytes).toBe(60);
    lru.clear();
    expect(lru.bytes).toBe(0);
    expect(lru.size).toBe(0);
  });

  test("存在しないキーへの touch / remove は無視される", () => {
    const lru = new LruBudget(100);
    lru.admit("a", 40);
    lru.touch("missing");
    lru.remove("missing");
    expect(lru.size).toBe(1);
    expect(lru.bytes).toBe(40);
  });
});
