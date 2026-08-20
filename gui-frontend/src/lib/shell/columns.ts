/**
 * カラム幅の仕様と、localStorage 文字列の解釈。
 *
 * runes を含まない純粋なモジュールにしてある。壊れた永続値を弾く規則は
 * ここでしか書かれておらず、UI を起動せずに検査できることに意味がある。
 *
 * 数値の px がここに直接書かれているのは、spec §3-1 が定めた
 * 「レイアウトの構造的な寸法」だからである（色や余白のトークンとは別物）。
 */
export type ColumnKey = "folder" | "presets" | "convert" | "metadata" | "frame";

export const COLUMN_KEYS = [
  "folder",
  "presets",
  "convert",
  "metadata",
  "frame",
] as const satisfies readonly ColumnKey[];

export interface ColumnSpec {
  default: number;
  min: number;
  max: number;
}

/**
 * 既定値は spec §3-1 のレイアウト表から。
 * min は「そのカラムが役目を果たせる最小」、max は「グリッドを潰さない最大」。
 * minWidth 1100 のウィンドウで rail 80 + 左 max + 右 max を引いても
 * グリッドに 1 列分（内側 200px 弱）が残るように取ってある。
 */
export const COLUMN_SPECS: Record<ColumnKey, ColumnSpec> = {
  folder: { default: 240, min: 180, max: 400 },
  presets: { default: 220, min: 160, max: 360 },
  convert: { default: 320, min: 260, max: 480 },
  metadata: { default: 360, min: 280, max: 520 },
  frame: { default: 360, min: 280, max: 520 },
};

export type ColumnWidths = Record<ColumnKey, number>;

export const WIDTHS_STORAGE_KEY = "picture-tool.layout.widths.v1";
export const RIGHT_COLLAPSED_STORAGE_KEY = "picture-tool.layout.right-collapsed.v1";

export function defaultWidths(): ColumnWidths {
  return {
    folder: COLUMN_SPECS.folder.default,
    presets: COLUMN_SPECS.presets.default,
    convert: COLUMN_SPECS.convert.default,
    metadata: COLUMN_SPECS.metadata.default,
    frame: COLUMN_SPECS.frame.default,
  };
}

/**
 * 数値でない値・NaN・Infinity は既定値へ落とす。
 * min へ落とさないのは、「壊れた値」と「利用者が最小まで縮めた」を
 * 区別できるようにしておくため。
 */
export function clampWidth(key: ColumnKey, value: unknown): number {
  const spec = COLUMN_SPECS[key];
  if (typeof value !== "number" || !Number.isFinite(value)) return spec.default;
  return Math.min(spec.max, Math.max(spec.min, Math.round(value)));
}

export function parseWidths(raw: string | null): ColumnWidths {
  if (raw === null) return defaultWidths();

  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return defaultWidths();
  }

  if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
    return defaultWidths();
  }

  const source = parsed as Record<string, unknown>;
  const widths = defaultWidths();
  for (const key of COLUMN_KEYS) {
    widths[key] = clampWidth(key, source[key]);
  }
  return widths;
}

export function serializeWidths(widths: ColumnWidths): string {
  return JSON.stringify(widths);
}

/** 折りたたみ状態は幅と別キーなので、クランプの対象外（spec §3-1） */
export function parseCollapsed(raw: string | null): boolean {
  return raw === "true";
}
