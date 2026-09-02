import { readImageMetadata, writeImageMetadata } from "../api";
import {
  emptyValues,
  sameValues,
  toMetadata,
  toValues,
  type MetadataValues,
} from "./metadataValues";

/**
 * メタデータの下書き。
 *
 * 離脱経路（写真の切替 / rail でのモード移動 / フォルダー変更 / ウィンドウを閉じる）は
 * すべて `isDirty` を通す（刷新 spec §3-4）。判定をここ 1 箇所に置いてあるので、
 * 呼び出し側は分岐を増やさずに済む。
 */
export function createMetadataDraft() {
  let path = $state<string | null>(null);
  let values = $state<MetadataValues>(emptyValues());
  let saved = $state<MetadataValues>(emptyValues());
  let warnings = $state<string[]>([]);
  let loading = $state(false);
  let saving = $state(false);

  /** 写真を連打したときに、古い応答が新しい下書きを上書きしないよう守る */
  let token = 0;

  return {
    get path() {
      return path;
    },
    get values() {
      return values;
    },
    /** 箇所ごとの値の食い違い。保存すると全箇所が揃うので自然に消える */
    get warnings() {
      return warnings;
    },
    get loading() {
      return loading;
    },
    get saving() {
      return saving;
    },
    get isDirty(): boolean {
      return !sameValues(values, saved);
    },

    async load(next: string | null) {
      path = next;
      const current = ++token;
      warnings = [];
      saved = emptyValues();
      values = emptyValues();
      if (next === null) {
        loading = false;
        return;
      }
      loading = true;
      try {
        const read = await readImageMetadata(next);
        if (current !== token) return;
        saved = toValues(read.metadata);
        values = toValues(read.metadata);
        warnings = read.warnings;
      } finally {
        if (current === token) loading = false;
      }
    },

    /**
     * 保存する。**戻り値は「ファイルの実際の中身」**なので、それを保存済みの値に採る。
     * 送った値をそのまま saved にすると、Rust 側の正規化（空文字→タグ削除など）と
     * 食い違って未保存表示が消えなくなる。
     */
    async save() {
      if (path === null || saving) return;
      const target = path;
      const sending = toMetadata($state.snapshot(values));
      saving = true;
      try {
        const read = await writeImageMetadata(target, sending);
        // 保存中に別の写真へ移っていたら、いまの下書きを上書きしない
        if (target !== path) return;
        saved = toValues(read.metadata);
        values = toValues(read.metadata);
        warnings = read.warnings;
      } finally {
        saving = false;
      }
    },

    discard() {
      values = { ...saved };
    },
  };
}
