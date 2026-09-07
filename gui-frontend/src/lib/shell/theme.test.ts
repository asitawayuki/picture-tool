/**
 * テーマ設定（ライト / ダーク / システム追従）の規則。
 *
 * 検査するのは 3 点:
 *   1. 壊れた永続値・未知の IPC 値が来ても操作不能にならない（columns.test.ts と同じ線）
 *   2. 明示指定は OS の状態を無視する ── これが「ダークモードに対応する」の要求そのもの
 *   3. 1 つのボタンで 3 状態すべてに到達でき、押し続ければ元に戻る
 */
import { describe, expect, test } from "bun:test";
import {
  THEME_PREFERENCES,
  nextTheme,
  parseOsScheme,
  parseTheme,
  resolveTheme,
  type ThemePreference,
} from "./theme";

describe("parseTheme", () => {
  test("未保存（null）はシステム追従", () => {
    expect(parseTheme(null)).toBe("system");
  });

  test("有効な 3 値はそのまま復元する", () => {
    expect(parseTheme("system")).toBe("system");
    expect(parseTheme("light")).toBe("light");
    expect(parseTheme("dark")).toBe("dark");
  });

  test("未知の値はシステム追従へ落とす", () => {
    // localStorage は信頼できない入力。別バージョンのアプリが書いた値、
    // devtools で書かれた値、壊れた値のいずれもありうる
    expect(parseTheme("")).toBe("system");
    expect(parseTheme("auto")).toBe("system");
    expect(parseTheme("Dark")).toBe("system");
    expect(parseTheme('{"theme":"dark"}')).toBe("system");
  });
});

describe("nextTheme", () => {
  test("システム → ライト → ダーク の順に進む", () => {
    expect(nextTheme("system")).toBe("light");
    expect(nextTheme("light")).toBe("dark");
  });

  test("3 回押すと元に戻る（どの状態からも全状態へ到達できる）", () => {
    for (const start of THEME_PREFERENCES) {
      const seen = [nextTheme(start), nextTheme(nextTheme(start)), nextTheme(nextTheme(nextTheme(start)))];
      expect(seen[2]).toBe(start);
      expect(new Set(seen).size).toBe(3);
    }
  });
});

describe("resolveTheme", () => {
  test("ライト指定は OS がダークでもライトのまま", () => {
    expect(resolveTheme("light", "dark")).toBe("light");
  });

  test("ダーク指定は OS がライトでもダークのまま", () => {
    expect(resolveTheme("dark", "light")).toBe("dark");
  });

  test("システム追従は OS の値をそのまま採る", () => {
    expect(resolveTheme("system", "dark")).toBe("dark");
    expect(resolveTheme("system", "light")).toBe("light");
  });
});

describe("parseOsScheme", () => {
  test("dark だけがダーク", () => {
    expect(parseOsScheme("dark")).toBe("dark");
    expect(parseOsScheme("light")).toBe("light");
  });

  test("未知の値・型違いはライトに倒す", () => {
    // Rust 側が黙って形を変えたときに画面を真っ暗にしないための保守側。
    // 既定のデスクトップはライトであり、外したときの被害が小さい方へ落とす
    expect(parseOsScheme(null)).toBe("light");
    expect(parseOsScheme(undefined)).toBe("light");
    expect(parseOsScheme("prefer-dark")).toBe("light");
    expect(parseOsScheme(1)).toBe("light");
    expect(parseOsScheme({ scheme: "dark" })).toBe("light");
  });
});

describe("型の網羅", () => {
  test("THEME_PREFERENCES に 3 状態が揃っている", () => {
    // 循環の検査が「2 状態しか無い」実装でも通ってしまうのを防ぐ前提条件
    const expected: ThemePreference[] = ["system", "light", "dark"];
    expect([...THEME_PREFERENCES].sort()).toEqual(expected.sort());
  });
});
