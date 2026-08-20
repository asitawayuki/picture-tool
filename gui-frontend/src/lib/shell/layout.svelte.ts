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

/**
 * カラム幅と右パネル折りたたみの保持と永続化。
 *
 * localStorage が使えない環境（WebKitGTK で永続が不安定な例が知られている）でも
 * 動く。読めなければ既定値、書けなければ黙って捨てる。
 * 失って困る情報ではないのでトーストも出さない（spec §3-1 / §8）。
 */
function read(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function write(key: string, value: string): void {
  try {
    localStorage.setItem(key, value);
  } catch {
    // 保存できないだけで操作は続けられる
  }
}

export function createLayout() {
  let widths = $state<ColumnWidths>(parseWidths(read(WIDTHS_STORAGE_KEY)));
  let collapsed = $state(parseCollapsed(read(RIGHT_COLLAPSED_STORAGE_KEY)));

  return {
    get widths(): ColumnWidths {
      return widths;
    },

    /** ドラッグ中に毎フレーム呼ばれる。クランプと永続化はここに閉じる */
    setWidth(key: ColumnKey, value: number): void {
      widths[key] = clampWidth(key, value);
      write(WIDTHS_STORAGE_KEY, serializeWidths($state.snapshot(widths)));
    },

    get rightPanelCollapsed(): boolean {
      return collapsed;
    },

    set rightPanelCollapsed(next: boolean) {
      collapsed = next;
      write(RIGHT_COLLAPSED_STORAGE_KEY, String(next));
    },
  };
}
