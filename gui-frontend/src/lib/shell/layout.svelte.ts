import {
  clampWidth,
  parseCollapsed,
  parseWidths,
  serializeWidths,
  RIGHT_COLLAPSED_STORAGE_KEY,
  WIDTHS_STORAGE_KEY,
  type ColumnKey,
  type ColumnWidths,
} from "./columns";
import { readStorage, writeStorage } from "./storage";

/**
 * カラム幅と右パネル折りたたみの保持と永続化。
 *
 * localStorage が使えない環境でも動く（`storage.ts`。読めなければ既定値、書けなければ
 * 黙って捨てる）。失って困る情報ではないのでトーストも出さない（spec §3-1 / §8）。
 */
export function createLayout() {
  let widths = $state<ColumnWidths>(parseWidths(readStorage(WIDTHS_STORAGE_KEY)));
  let collapsed = $state(parseCollapsed(readStorage(RIGHT_COLLAPSED_STORAGE_KEY)));

  return {
    get widths(): ColumnWidths {
      return widths;
    },

    /** ドラッグ中に毎フレーム呼ばれる。クランプと永続化はここに閉じる */
    setWidth(key: ColumnKey, value: number): void {
      widths[key] = clampWidth(key, value);
      writeStorage(WIDTHS_STORAGE_KEY, serializeWidths($state.snapshot(widths)));
    },

    get rightPanelCollapsed(): boolean {
      return collapsed;
    },

    set rightPanelCollapsed(next: boolean) {
      collapsed = next;
      writeStorage(RIGHT_COLLAPSED_STORAGE_KEY, String(next));
    },
  };
}
