import type { ExifFrameConfig } from "../types";

/** バンドルプリセット名。ユーザーファイルが無くても常に存在するため削除させない */
export const BUNDLED_PRESET_NAME = "default";

export function defaultFrameConfig(): ExifFrameConfig {
  return {
    name: BUNDLED_PRESET_NAME,
    position: "auto",
    items: {
      maker_logo: true,
      lens_brand_logo: true,
      camera_model: true,
      lens_model: true,
      focal_length: true,
      f_number: true,
      shutter_speed: true,
      iso: true,
      date_taken: false,
      custom_text: false,
    },
    font: { font_path: null, primary_size: 0.025, secondary_size: 0.018 },
    custom_text: "",
  };
}

/**
 * プリセットは必ず深いコピーを取ってから編集する。
 * シャローコピーだと items / font が一覧側のオブジェクトと同一参照になり、
 * 編集がそのまま一覧を書き換えてしまう。
 */
function clone(preset: ExifFrameConfig): ExifFrameConfig {
  return structuredClone($state.snapshot(preset)) as ExifFrameConfig;
}

/**
 * フレーム編集の下書き。
 *
 * spec §2 のファイル構成表には無いが、この下書きは左（一覧）・中央（プレビュー）・
 * 右（設定）の 3 カラムにまたがって共有されるため、`App.svelte` に置くと
 * spec §3-5（App は 4 状態とパネルの差し替えのみ）が崩れる。
 */
export function createFrameDraft() {
  let draft = $state<ExifFrameConfig | null>(null);
  /**
   * **編集中の下書きがディスク上のどのプリセットか。** 新規作成中は `""`。
   * `draft.name` は改名で先に動くので、旧名をここに残しておく必要がある
   * （保存後にこの名前を削除するのが「改名」の実体）。
   */
  let editingName = $state("");
  let knownNames = $state<string[]>([]);

  /** 一覧の項目をダブルクリックして名前を変えた状態 */
  function renamed(): boolean {
    return draft !== null && editingName !== "" && draft.name.trim() !== editingName;
  }

  /** 別のプリセットと同じ名前になっている */
  function conflicting(): boolean {
    if (draft === null) return false;
    const name = draft.name.trim();
    return name !== editingName && knownNames.includes(name);
  }

  return {
    get draft() {
      return draft;
    },
    get editingName() {
      return editingName;
    },
    /** ディスク上に対応するプリセットが無い（＝新規作成中） */
    get isNew(): boolean {
      return draft !== null && editingName === "";
    },
    get isRenamed(): boolean {
      return renamed();
    },
    /** 保存後に削除すべき旧名。改名でなければ null */
    get renamedFrom(): string | null {
      return renamed() ? editingName : null;
    },
    /**
     * 既存の別プリセットと名前がぶつかっている。
     * 通すと「上書き ＋ 旧名の削除」で 2 つが 1 つになるため保存させない
     */
    get nameConflict(): boolean {
      return conflicting();
    },
    get canSave(): boolean {
      return draft !== null && draft.name.trim().length > 0 && !conflicting();
    },
    get canDelete(): boolean {
      return editingName !== "" && editingName !== BUNDLED_PRESET_NAME;
    },

    select(name: string, presets: ExifFrameConfig[]) {
      knownNames = presets.map((p) => p.name);
      const found = presets.find((p) => p.name === name);
      draft = found ? clone(found) : defaultFrameConfig();
      // 見つからなければディスク上の実体が無い＝新規扱い
      editingName = found ? found.name : "";
    },

    /** 一覧の項目をダブルクリックしての改名（spec §5-3）。
     *  ここでは下書きの名前を変えるだけで、旧名の削除は保存時に行う */
    rename(name: string) {
      if (draft) draft.name = name;
    },

    createNew(presets: ExifFrameConfig[]) {
      knownNames = presets.map((p) => p.name);
      draft = defaultFrameConfig();
      // 既存と衝突しない名前を作る
      let n = 1;
      while (knownNames.includes(`preset-${n}`)) n++;
      draft.name = `preset-${n}`;
      editingName = "";
    },

    snapshot(): ExifFrameConfig {
      const snap = structuredClone($state.snapshot(draft!)) as ExifFrameConfig;
      snap.name = snap.name.trim();
      return snap;
    },
  };
}
