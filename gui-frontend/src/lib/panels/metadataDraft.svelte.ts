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
 * 離脱経路のうち写真の切替 / rail でのモード移動 / フォルダー変更の 3 つは
 * `isDirty` を通してガード済み（Task 15 / 刷新 spec §3-4）。判定をここ 1 箇所に
 * 置いてあるので、呼び出し側は分岐を増やさずに済む。ウィンドウを閉じる経路も
 * `isDirty` を `App.svelte` が `setUnsavedState` 経由で Rust へ伝える形で配線済み
 * （Task 16）。
 */
export function createMetadataDraft() {
  let path = $state<string | null>(null);
  let values = $state<MetadataValues>(emptyValues());
  let saved = $state<MetadataValues>(emptyValues());
  let warnings = $state<string[]>([]);
  let loading = $state(false);
  let saving = $state(false);
  // getter で毎回比べると、読む側の $effect が値の編集 1 文字ごとに再実行される
  // （App.svelte はそのたびに set_unsaved_state を呼んでいた）。$derived は真偽が
  // 変わったときだけ通知する
  const isDirty = $derived(!sameValues(values, saved));

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
      return isDirty;
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
     * 保存する。**戻り値は「実際に書き込んだか」**（Task 15 / 裁定 R1）。
     * `path === null` / 保存中 / 保存中に写真が変わった、の早期 return は
     * すべて `false` を返す ── 呼び出し元の未保存ガードはこれを「離脱してよいか」の
     * 判断に使うので、「何もしなかった」を「保存した」と区別できないと
     * 未保存の下書きを黙って捨てて離脱してしまう。
     *
     * 実際に書けたときは、応答の**ファイルの実際の中身**を保存済みの値に採る。
     * 送った値をそのまま saved にすると、Rust 側の正規化（空文字→タグ削除など）と
     * 食い違って未保存表示が消えなくなる。
     */
    async save(): Promise<boolean> {
      if (path === null || saving) return false;
      const target = path;
      const sending = toMetadata($state.snapshot(values));
      saving = true;
      try {
        const read = await writeImageMetadata(target, sending);
        // 保存中に別の写真へ移っていたら、いまの下書きを上書きしない
        if (target !== path) return false;
        saved = toValues(read.metadata);
        values = toValues(read.metadata);
        warnings = read.warnings;
        return true;
      } finally {
        saving = false;
      }
    },

    discard() {
      values = { ...saved };
    },
  };
}
