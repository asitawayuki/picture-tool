import { deletePreset, listPresets, savePreset } from "../api";
import { describeError, toast } from "../toasts.svelte";
import type { ExifFrameConfig } from "../types";

/**
 * Exif フレームのプリセット一覧の唯一の保持者。
 * パネル側は props で受け取るだけで、自分では一覧を持たない。
 */
export function createPresetStore() {
  let presets = $state<ExifFrameConfig[]>([]);
  let selectedName = $state("default");

  async function reload() {
    try {
      presets = await listPresets();
      // 選択中のプリセットが消えていたら先頭へ落とす
      if (!presets.some((p) => p.name === selectedName)) {
        selectedName = presets[0]?.name ?? "default";
      }
    } catch (e) {
      toast.error(`プリセットの読み込みに失敗しました: ${describeError(e)}`);
    }
  }

  return {
    get presets() {
      return presets;
    },
    get selectedName() {
      return selectedName;
    },
    set selectedName(name: string) {
      selectedName = name;
    },
    get active(): ExifFrameConfig | null {
      return presets.find((p) => p.name === selectedName) ?? null;
    },
    reload,
    /** 保存できたら true。呼び出し側はこれを見てモードを閉じるか決める */
    async save(preset: ExifFrameConfig): Promise<boolean> {
      try {
        await savePreset(preset);
        selectedName = preset.name;
        await reload();
        toast.success(`プリセット「${preset.name}」を保存しました`);
        return true;
      } catch (e) {
        toast.error(`プリセットの保存に失敗しました: ${describeError(e)}`);
        return false;
      }
    },
    async remove(name: string): Promise<void> {
      try {
        await deletePreset(name);
        await reload();
        toast.success(`プリセット「${name}」を削除しました`);
      } catch (e) {
        toast.error(`プリセットの削除に失敗しました: ${describeError(e)}`);
      }
    },
  };
}
