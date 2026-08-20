/**
 * spec §4-2「取得キューの長さ」。
 *
 * 検査するのは 3 点:
 *  - LIFO であること（可視分の要求が過去の要求の後ろで待たない）
 *  - 初回の 1 画面分だけは投入順（上から）で流れること
 *  - 範囲外の破棄が discardable にだけ効き、pinned に効かないこと
 */
import { describe, expect, test } from "bun:test";
import { RequestQueue, type ThumbnailRequest } from "./requestQueue";

function req(
  index: number,
  kind: ThumbnailRequest["kind"] = "discardable"
): ThumbnailRequest {
  return { key: `p${index}:200`, path: `/photos/${index}.jpg`, size: 200, kind, index };
}

describe("RequestQueue", () => {
  test("priming が無ければ LIFO で取り出す", () => {
    const q = new RequestQueue();
    q.push(req(1));
    q.push(req(2));
    q.push(req(3));
    expect(q.take()?.index).toBe(3);
    expect(q.take()?.index).toBe(2);
    expect(q.take()?.index).toBe(1);
    expect(q.take()).toBeUndefined();
  });

  test("初回の 1 画面分は投入順（上から）で流れる", () => {
    // LIFO だけにすると最上行が最後に読まれ、下から埋まって見える。
    // 実害は無いが印象に効くので初回だけ FIFO にする（spec §4-2）
    const q = new RequestQueue();
    q.reset(3);
    q.push(req(1));
    q.push(req(2));
    q.push(req(3));
    q.push(req(4));
    q.push(req(5));
    expect([q.take()?.index, q.take()?.index, q.take()?.index]).toEqual([1, 2, 3]);
    // priming を使い切ったら LIFO に戻る
    expect(q.take()?.index).toBe(5);
    expect(q.take()?.index).toBe(4);
  });

  test("同じキーを二重に積まない", () => {
    const q = new RequestQueue();
    q.push(req(1));
    q.push(req(1));
    expect(q.pendingCount).toBe(1);
  });

  test("取り出したキーは再度 push できる", () => {
    const q = new RequestQueue();
    q.push(req(1));
    q.take();
    q.push(req(1));
    expect(q.pendingCount).toBe(1);
  });

  test("可視範囲を外れた discardable を捨てる", () => {
    const q = new RequestQueue();
    for (const i of [1, 2, 30, 31]) q.push(req(i));
    // 前提条件: 捨てる前は 4 件ある（0 件だと「捨てた」が自明に成立する）
    expect(q.pendingCount).toBe(4);

    expect(q.setVisibleRange(0, 10)).toBe(2);
    expect(q.pendingCount).toBe(2);
    expect(q.has("p30:200")).toBe(false);
    expect(q.has("p1:200")).toBe(true);
  });

  test("捨てたキーは再度 push できる", () => {
    // 破棄でキーの記録だけ残ると、スクロールで戻ってきた写真が
    // 二重登録扱いになって永久に取得されない
    const q = new RequestQueue();
    q.push(req(30));
    q.setVisibleRange(0, 10);
    // 前提条件: いま捨てられていること
    expect(q.pendingCount).toBe(0);

    q.push(req(30));
    expect(q.pendingCount).toBe(1);
    expect(q.take()?.index).toBe(30);
  });

  test("pinned は範囲外でも捨てない（spec §4-2）", () => {
    // フィルムストリップ・メタデータパネルのサムネイル・フレームの見本写真は
    // グリッドの index 範囲に入らない。捨てると永久に埋まらなくなる
    const q = new RequestQueue();
    q.push(req(500, "pinned"));
    q.push(req(501, "discardable"));
    expect(q.pendingCount).toBe(2);

    expect(q.setVisibleRange(0, 10)).toBe(1);
    expect(q.has("p500:200")).toBe(true);
    expect(q.has("p501:200")).toBe(false);
  });

  test("priming に積まれた discardable も範囲外なら捨てる", () => {
    const q = new RequestQueue();
    q.reset(5);
    q.push(req(1));
    q.push(req(99));
    q.setVisibleRange(0, 10);
    expect(q.has("p99:200")).toBe(false);
    expect(q.take()?.index).toBe(1);
  });

  test("reset で残った要求を捨て、priming を張り直す", () => {
    const q = new RequestQueue();
    q.push(req(1));
    q.push(req(2));
    q.reset(2);
    expect(q.pendingCount).toBe(0);
    q.push(req(10));
    q.push(req(11));
    q.push(req(12));
    expect([q.take()?.index, q.take()?.index]).toEqual([10, 11]);
  });
});
