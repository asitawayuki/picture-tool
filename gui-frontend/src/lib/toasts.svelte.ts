/**
 * 画面右下に一時表示する通知のストア。
 *
 * これまで握りつぶしていた例外（フォルダー読み込み失敗・サムネイル生成失敗など）と
 * `alert()` の置き換え先。変換結果のような「読ませたい情報」は ResultDialog を使い、
 * ここは「気づかせたい事象」に限定する。
 */
export type ToastKind = "error" | "warning" | "success";

export interface Toast {
  id: number;
  kind: ToastKind;
  message: string;
}

const DURATION_MS: Record<ToastKind, number> = {
  error: 8000,
  warning: 6000,
  success: 4000,
};

let nextId = 0;

export const toasts = $state<Toast[]>([]);

export function dismissToast(id: number) {
  const idx = toasts.findIndex((t) => t.id === id);
  if (idx >= 0) toasts.splice(idx, 1);
}

function push(kind: ToastKind, message: string) {
  const id = nextId++;
  toasts.push({ id, kind, message });
  setTimeout(() => dismissToast(id), DURATION_MS[kind]);
}

export const toast = {
  error: (message: string) => push("error", message),
  warning: (message: string) => push("warning", message),
  success: (message: string) => push("success", message),
};

/** 例外オブジェクトを人が読める1行にする。 */
export function describeError(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return String(e);
}
