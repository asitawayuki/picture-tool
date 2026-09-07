/**
 * テーマ設定の仕様と、信頼できない入力（localStorage / IPC）の解釈。
 *
 * runes を含まない純粋なモジュールにしてある（`columns.ts` と同じ理由）。
 * 「壊れた値をどこへ落とすか」の規則はここにしか無く、UI を起動せずに検査できる。
 *
 * 色の割り当てそのものは `styles/tokens.css` の 4 ブロックが持つ。ここが決めるのは
 * `documentElement` の `data-theme` に何を書くか、それだけである。
 */
import type { ColorSchemePayload, OsColorSchemeChangedEvent } from "../types";

export type ThemePreference = "system" | "light" | "dark";

/** OS が示す実際の明暗。`system` を解決した結果でもある。綴りは Rust と共有 */
export type ColorScheme = ColorSchemePayload;

/**
 * 循環の順序。rail のボタンはこの順に進む。
 *
 * `system` を先頭に置くのは、既定がシステム追従で、そこから 1 回押した先が
 * 「明示的にライト」になるのが素直なため。
 */
export const THEME_PREFERENCES = ["system", "light", "dark"] as const satisfies readonly ThemePreference[];

export const THEME_LABELS: Record<ThemePreference, string> = {
  system: "システム",
  light: "ライト",
  dark: "ダーク",
};

/** rail のアイコン。`modes.ts` と同じく絵文字を直に持つ */
export const THEME_ICONS: Record<ThemePreference, string> = {
  system: "🖥",
  light: "☀",
  dark: "🌙",
};

export const THEME_STORAGE_KEY = "picture-tool.theme.v1";

/**
 * OS の明暗が変わったときに Rust が流すイベント。
 * 型を `types.ts` から取ることで、Rust 側だけ改名すると typecheck が落ちる
 * （`dto.contract.ts` が突き合わせる）。
 */
export const OS_SCHEME_CHANGED_EVENT: OsColorSchemeChangedEvent = "os-color-scheme-changed";

/**
 * 直近に判明した OS の明暗のキャッシュ。
 *
 * 起動直後は Rust への問い合わせが返るまで OS の値が分からない。何も無いと
 * ダークのデスクトップで一瞬白い画面が出るので、前回の値を先に当てておく。
 * 外れても次のフレームで訂正されるだけなので、壊れていれば黙って light でよい。
 */
export const OS_SCHEME_STORAGE_KEY = "picture-tool.theme.os-scheme.v1";

const THEME_SET: ReadonlySet<string> = new Set<string>(THEME_PREFERENCES);

/** 未保存・未知の値はすべてシステム追従へ。既定に落とすので操作不能にはならない */
export function parseTheme(raw: string | null): ThemePreference {
  return raw !== null && THEME_SET.has(raw) ? (raw as ThemePreference) : "system";
}

/**
 * IPC / localStorage から来た明暗の解釈。
 *
 * `"dark"` 以外はすべてライトへ倒す。外したときに画面が真っ暗になる方が
 * 被害が大きく、既定のデスクトップはライトだからである。
 */
export function parseOsScheme(raw: unknown): ColorScheme {
  return raw === "dark" ? "dark" : "light";
}

export function nextTheme(current: ThemePreference): ThemePreference {
  const index = THEME_PREFERENCES.indexOf(current);
  return THEME_PREFERENCES[(index + 1) % THEME_PREFERENCES.length];
}

/** 明示指定は OS の状態を見ない。`system` のときだけ OS に従う */
export function resolveTheme(preference: ThemePreference, osScheme: ColorScheme): ColorScheme {
  return preference === "system" ? osScheme : preference;
}
