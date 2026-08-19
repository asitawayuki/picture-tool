/**
 * spec §3-1「カラム幅の永続化」。
 *
 * 検査するのは「壊れた値が入っていても操作不能にならない」こと。
 * localStorage は信頼できない入力として扱う（利用者が devtools で書ける、
 * 別バージョンのアプリが書いた、途中で壊れた、のいずれもありうる）。
 */
import { describe, expect, test } from "bun:test";
import {
  COLUMN_KEYS,
  COLUMN_SPECS,
  clampWidth,
  defaultWidths,
  parseCollapsed,
  parseWidths,
  serializeWidths,
} from "./columns";

describe("clampWidth", () => {
  test("範囲内の値はそのまま（整数へ丸める）", () => {
    expect(clampWidth("folder", 300)).toBe(300);
    expect(clampWidth("folder", 300.6)).toBe(301);
  });

  test("下限未満は下限へ、上限超過は上限へ", () => {
    expect(clampWidth("folder", 0)).toBe(COLUMN_SPECS.folder.min);
    expect(clampWidth("folder", -1000)).toBe(COLUMN_SPECS.folder.min);
    expect(clampWidth("folder", 999999)).toBe(COLUMN_SPECS.folder.max);
  });

  test("数値でない値と NaN / Infinity は既定値へ落とす", () => {
    // 幅 0 のカラムを作らせないための線。既定値へ落とすのが正しく、
    // min へ落とすと「壊れた値」と「利用者が最小まで縮めた」が区別できなくなる
    expect(clampWidth("folder", Number.NaN)).toBe(COLUMN_SPECS.folder.default);
    expect(clampWidth("folder", Number.POSITIVE_INFINITY)).toBe(COLUMN_SPECS.folder.default);
    expect(clampWidth("folder", "300")).toBe(COLUMN_SPECS.folder.default);
    expect(clampWidth("folder", null)).toBe(COLUMN_SPECS.folder.default);
    expect(clampWidth("folder", undefined)).toBe(COLUMN_SPECS.folder.default);
    expect(clampWidth("folder", {})).toBe(COLUMN_SPECS.folder.default);
  });
});

describe("parseWidths", () => {
  test("未保存（null）なら既定値", () => {
    expect(parseWidths(null)).toEqual(defaultWidths());
  });

  test("JSON として壊れていたら既定値", () => {
    expect(parseWidths("{")).toEqual(defaultWidths());
    expect(parseWidths("")).toEqual(defaultWidths());
  });

  test("オブジェクトでない JSON なら既定値", () => {
    expect(parseWidths("42")).toEqual(defaultWidths());
    expect(parseWidths("null")).toEqual(defaultWidths());
    expect(parseWidths('"folder"')).toEqual(defaultWidths());
    expect(parseWidths("[240, 320]")).toEqual(defaultWidths());
  });

  test("既知のキーだけを採り、欠けている分は既定値で埋める", () => {
    const parsed = parseWidths(JSON.stringify({ folder: 300, unknown: 9999 }));
    expect(parsed.folder).toBe(300);
    expect(parsed.convert).toBe(COLUMN_SPECS.convert.default);
    expect(Object.keys(parsed).sort()).toEqual([...COLUMN_KEYS].sort());
  });

  test("壊れた値が混ざっていても他のカラムは生き残る", () => {
    const parsed = parseWidths(JSON.stringify({ folder: -5, convert: 400 }));
    expect(parsed.folder).toBe(COLUMN_SPECS.folder.min);
    expect(parsed.convert).toBe(400);
  });

  test("書いて読むと同じ値に戻る", () => {
    const widths = { ...defaultWidths(), folder: 300, metadata: 420 };
    expect(parseWidths(serializeWidths(widths))).toEqual(widths);
  });
});

describe("parseCollapsed", () => {
  test('"true" だけが true。それ以外はすべて false', () => {
    expect(parseCollapsed("true")).toBe(true);
    expect(parseCollapsed("false")).toBe(false);
    expect(parseCollapsed(null)).toBe(false);
    expect(parseCollapsed("1")).toBe(false);
    expect(parseCollapsed("")).toBe(false);
  });
});

describe("COLUMN_SPECS", () => {
  // 前提条件: これが崩れていると上のクランプ検査はすべて無意味になる
  test("すべてのカラムで min <= default <= max", () => {
    for (const key of COLUMN_KEYS) {
      const spec = COLUMN_SPECS[key];
      expect(spec.min).toBeLessThanOrEqual(spec.default);
      expect(spec.default).toBeLessThanOrEqual(spec.max);
      expect(spec.min).toBeGreaterThan(0);
    }
  });
});
