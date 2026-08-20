export interface MetadataValues {
  title: string;
  comment: string;
  /** 0〜5。0 は未設定 */
  rating: number;
}

function empty(): MetadataValues {
  return { title: "", comment: "", rating: 0 };
}

/**
 * メタデータの下書き。
 *
 * 本刷新ではレイアウトのためだけに存在する。値の読み込み（read_image_metadata）と
 * 保存（write_image_metadata）は次工程で追加される Tauri コマンドなので、
 * load() は「保存済みの値」を空にリセットするだけ（spec §5-2）。
 *
 * isDirty は最初から持たせる。3-4 の離脱経路（メタデータモード内のフォーカス移動 /
 * rail での別モードへの移動 / フォルダー変更）をここへ通すのは次工程だが、
 * 判定そのものをここに置いておけば、繋ぎ込みが分岐を増やさずに済む。
 */
export function createMetadataDraft() {
  let path = $state<string | null>(null);
  let values = $state<MetadataValues>(empty());
  let saved = $state<MetadataValues>(empty());

  return {
    get path() {
      return path;
    },
    get values() {
      return values;
    },
    get isDirty(): boolean {
      return (
        values.title !== saved.title ||
        values.comment !== saved.comment ||
        values.rating !== saved.rating
      );
    },

    /** 次工程で read_image_metadata の結果を saved に入れる */
    load(next: string | null) {
      path = next;
      saved = empty();
      values = empty();
    },

    discard() {
      values = { ...saved };
    },
  };
}
