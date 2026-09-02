import type { EditableMetadata } from "../types";

/** 入力欄が扱う形。空欄は "" で、未評価は 0 で表す */
export interface MetadataValues {
  title: string;
  comment: string;
  /** 0〜5。0 は未評価 */
  rating: number;
}

export function emptyValues(): MetadataValues {
  return { title: "", comment: "", rating: 0 };
}

export function toValues(metadata: EditableMetadata): MetadataValues {
  return {
    title: metadata.title ?? "",
    comment: metadata.comment ?? "",
    rating: metadata.rating ?? 0,
  };
}

/**
 * 保存のために境界の形へ戻す。
 *
 * 空欄は `null`（タグを削除）にする。`""` のまま送ると「空文字のタグがある」状態になり、
 * 他アプリで「空のコメントがある」と見える。
 */
export function toMetadata(values: MetadataValues): EditableMetadata {
  return {
    title: values.title === "" ? null : values.title,
    comment: values.comment === "" ? null : values.comment,
    rating: values.rating >= 1 && values.rating <= 5 ? values.rating : null,
  };
}

export function sameValues(a: MetadataValues, b: MetadataValues): boolean {
  return a.title === b.title && a.comment === b.comment && a.rating === b.rating;
}
