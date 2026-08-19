<script lang="ts">
  import type { Snippet } from "svelte";
  import NavigationRail from "./NavigationRail.svelte";
  import { COLUMN_SPECS, type ColumnKey } from "./columns";
  import type { createLayout } from "./layout.svelte";
  import type { AppMode } from "./modes";

  interface Props {
    mode: AppMode;
    onModeChange: (mode: AppMode) => void;
    layout: ReturnType<typeof createLayout>;
    left: Snippet;
    center: Snippet;
    right: Snippet;
  }

  let { mode, onModeChange, layout, left, center, right }: Props = $props();

  /**
   * フレームモードだけ左 2 カラムが変わる（spec §3-1）。
   * プリセット編集にフォルダーツリーは不要で、必要なのは見本の写真 1 枚だけ。
   */
  let leftKey = $derived<ColumnKey>(mode === "frame" ? "presets" : "folder");
  let rightKey = $derived<ColumnKey>(
    mode === "convert" ? "convert" : mode === "metadata" ? "metadata" : "frame"
  );

  let shell: HTMLDivElement | undefined = $state();
  let dragging = $state<"left" | "right" | null>(null);

  const RAIL_WIDTH = 80;
  const KEYBOARD_STEP = 16;

  function startDrag(event: PointerEvent, side: "left" | "right") {
    const handle = event.currentTarget as HTMLElement;
    handle.setPointerCapture(event.pointerId);
    dragging = side;
  }

  function endDrag(event: PointerEvent) {
    const handle = event.currentTarget as HTMLElement;
    if (handle.hasPointerCapture(event.pointerId)) {
      handle.releasePointerCapture(event.pointerId);
    }
    dragging = null;
  }

  function drag(event: PointerEvent) {
    if (dragging === null || !shell) return;
    const rect = shell.getBoundingClientRect();
    if (dragging === "left") {
      layout.setWidth(leftKey, event.clientX - rect.left - RAIL_WIDTH);
    } else {
      layout.setWidth(rightKey, rect.right - event.clientX);
    }
  }

  /** ポインタを持たない利用者のための経路。左右キーで 16px ずつ動かす */
  function nudge(event: KeyboardEvent, side: "left" | "right") {
    const delta =
      event.key === "ArrowLeft" ? -KEYBOARD_STEP : event.key === "ArrowRight" ? KEYBOARD_STEP : 0;
    if (delta === 0) return;
    event.preventDefault();
    const key = side === "left" ? leftKey : rightKey;
    // 右のハンドルは「左へ動かすと広くなる」ので符号を反転する
    const signed = side === "left" ? delta : -delta;
    layout.setWidth(key, layout.widths[key] + signed);
  }
</script>

<div class="shell" bind:this={shell} class:dragging={dragging !== null}>
  <NavigationRail {mode} {onModeChange} />

  <div class="column" style="width: {layout.widths[leftKey]}px;">
    {@render left()}
  </div>

  <!--
    幅を変えられる separator は ARIA の window splitter であり、フォーカス可能な
    ウィジェットとして正しい。svelte の a11y 規則は separator を常に非対話として
    扱うため誤検出になる。ポインタを持たない利用者の唯一の経路なので tabindex は外せない
  -->
  <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
  <div
    class="handle"
    role="separator"
    aria-orientation="vertical"
    aria-label="左カラムの幅"
    aria-valuemin={COLUMN_SPECS[leftKey].min}
    aria-valuemax={COLUMN_SPECS[leftKey].max}
    aria-valuenow={layout.widths[leftKey]}
    tabindex="0"
    onpointerdown={(e) => startDrag(e, "left")}
    onpointermove={drag}
    onpointerup={endDrag}
    onpointercancel={endDrag}
    onkeydown={(e) => nudge(e, "left")}
  ></div>

  <div class="center">
    {@render center()}
  </div>

  {#if !layout.rightPanelCollapsed}
    <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
    <div
      class="handle"
      role="separator"
      aria-orientation="vertical"
      aria-label="右パネルの幅"
      aria-valuemin={COLUMN_SPECS[rightKey].min}
      aria-valuemax={COLUMN_SPECS[rightKey].max}
      aria-valuenow={layout.widths[rightKey]}
      tabindex="0"
      onpointerdown={(e) => startDrag(e, "right")}
      onpointermove={drag}
      onpointerup={endDrag}
      onpointercancel={endDrag}
      onkeydown={(e) => nudge(e, "right")}
    ></div>

    <div class="column right" style="width: {layout.widths[rightKey]}px;">
      {@render right()}
    </div>
  {/if}
</div>

<style>
  .shell {
    display: flex;
    height: 100vh;
    overflow: hidden;
    background: var(--md-sys-color-surface);
    color: var(--md-sys-color-on-surface);
    font: var(--md-sys-typescale-body-md);
  }

  /* ドラッグ中はカラム内のテキスト選択を止める */
  .shell.dragging {
    user-select: none;
    cursor: col-resize;
  }

  .column {
    flex-shrink: 0;
    overflow: hidden;
    background: var(--md-sys-color-surface-container-low);
  }

  .column.right {
    background: var(--md-sys-color-surface-container);
  }

  .center {
    flex: 1;
    min-width: 0;
    overflow: hidden;
  }

  /* 見た目は 1px の境界線、当たり判定は 8px */
  .handle {
    flex-shrink: 0;
    width: 8px;
    margin: 0 -4px;
    z-index: 1;
    cursor: col-resize;
    background: linear-gradient(
      to right,
      transparent 3px,
      var(--md-sys-color-outline-variant) 3px,
      var(--md-sys-color-outline-variant) 4px,
      transparent 4px
    );
  }

  .handle:hover,
  .handle:focus-visible {
    background: linear-gradient(
      to right,
      transparent 3px,
      var(--md-sys-color-primary) 3px,
      var(--md-sys-color-primary) 5px,
      transparent 5px
    );
  }
</style>
