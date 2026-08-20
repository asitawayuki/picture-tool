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
    /**
     * プリセットの改名。**新しい名前で保存してから旧名を消す。**
     * 逆順にすると、保存に失敗したときにプリセットが消えるだけになる。
     * `from === preset.name` のときは単なる保存として振る舞う。
     *
     * `remove` を続けて呼ぶ形にはしない。`remove` は削除の toast を出すので、
     * 改名という 1 つの操作に対して削除の通知が出てしまう。
     */
    async rename(from: string, preset: ExifFrameConfig): Promise<boolean> {
      try {
        await savePreset(preset);
        if (from !== preset.name) await deletePreset(from);
        selectedName = preset.name;
        await reload();
        toast.success(`プリセット「${from}」を「${preset.name}」に変更しました`);
        return true;
      } catch (e) {
        toast.error(`プリセットの改名に失敗しました: ${describeError(e)}`);
        // 保存だけ通って削除で落ちた場合、ディスク上は 2 件になっている。
        // 実際の状態を見せるために読み直す
        await reload();
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
