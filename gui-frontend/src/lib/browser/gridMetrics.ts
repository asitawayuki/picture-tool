/**
 * 写真グリッドの寸法計算。
 *
 * 列数の決定を JS の単一のソースに寄せる（spec §4-1）。
 * `auto-fill minmax(N, 1fr)` を CSS に任せる案は退けた ── 仮想スクロールの
 * 行高計算には列数が必須で、CSS 側が決めた折り返しを JS で再現すると
 * 1px のずれで行位置がずれてスクロールが飛ぶ。
 *
 * ここの px はレイアウトの構造的な寸法であり、トークン化の対象ではない。
 *
 * **`GRID_GAP` と `GRID_PADDING` は `PhotoGrid.svelte` の CSS にも現れる**
 * （`gap: var(--space-2)` = 8px、`padding-left/right: var(--space-3)` = 12px）。
 * 同じ値であることが行位置の前提なので、**片方だけ変えないこと**。
 */
export const GRID_GAP = 8;
export const GRID_PADDING = 12;
/** タイル下のファイル名 1 行分 */
export const LABEL_HEIGHT = 18;
/** 可視行の前後に余分に描く行数 */
export const OVERSCAN_ROWS = 2;
/** サムネイル要求サイズの丸め幅。1px の差で別キーにしないため */
export const SIZE_STEP = 64;
export const MIN_THUMB_SIZE = 96;
export const MAX_THUMB_SIZE = 512;

export interface GridMetrics {
  columns: number;
  tileWidth: number;
  rowHeight: number;
  totalRows: number;
  /** getThumbnail に渡す maxDimension */
  thumbnailSize: number;
}

export function computeGridMetrics(
  containerWidth: number,
  targetTileWidth: number,
  itemCount: number
): GridMetrics {
  const inner = Math.max(0, containerWidth - GRID_PADDING * 2);
  const target = Math.max(1, targetTileWidth);
  const columns = Math.max(1, Math.floor((inner + GRID_GAP) / (target + GRID_GAP)));
  const tileWidth = Math.max(0, (inner - GRID_GAP * (columns - 1)) / columns);
  const rowHeight = (tileWidth * 5) / 4 + LABEL_HEIGHT + GRID_GAP;
  const totalRows = Math.ceil(itemCount / columns);
  const thumbnailSize = Math.min(
    MAX_THUMB_SIZE,
    Math.max(MIN_THUMB_SIZE, Math.ceil(tileWidth / SIZE_STEP) * SIZE_STEP)
  );
  return { columns, tileWidth, rowHeight, totalRows, thumbnailSize };
}

export interface VisibleRange {
  firstRow: number;
  lastRow: number;
  startIndex: number;
  /** 最後に描く要素の index。要素が 0 件なら -1 */
  endIndex: number;
  paddingTop: number;
  paddingBottom: number;
}

export function computeVisibleRange(
  metrics: GridMetrics,
  scrollTop: number,
  viewportHeight: number,
  itemCount: number
): VisibleRange {
  const { columns, rowHeight, totalRows } = metrics;

  if (itemCount === 0 || totalRows === 0) {
    return {
      firstRow: 0,
      lastRow: -1,
      startIndex: 0,
      endIndex: -1,
      paddingTop: 0,
      paddingBottom: 0,
    };
  }

  // 初回描画で幅が 0 の瞬間は行高も 0 になる。割り算を避けて全件描く
  // （次のフレームで正しい幅が来て縮む）
  if (!Number.isFinite(rowHeight) || rowHeight <= 0) {
    return {
      firstRow: 0,
      lastRow: totalRows - 1,
      startIndex: 0,
      endIndex: itemCount - 1,
      paddingTop: 0,
      paddingBottom: 0,
    };
  }

  // 末尾を超えた scrollTop でも firstRow が最終行を追い越さないよう抑える。
  // 列数を増やすと総高が縮み、要素側が clamp されるまでの 1 フレームだけ
  // 古い scrollTop が残る。抑えないと paddingTop が総高を超えて画面が飛ぶ
  const firstRow = Math.min(
    totalRows - 1,
    Math.max(0, Math.floor(scrollTop / rowHeight) - OVERSCAN_ROWS)
  );
  const lastRow = Math.min(
    totalRows - 1,
    Math.floor((scrollTop + viewportHeight) / rowHeight) + OVERSCAN_ROWS
  );

  return {
    firstRow,
    lastRow,
    startIndex: firstRow * columns,
    endIndex: Math.min(itemCount - 1, (lastRow + 1) * columns - 1),
    paddingTop: firstRow * rowHeight,
    paddingBottom: (totalRows - 1 - lastRow) * rowHeight,
  };
}
