<script lang="ts">
  import type { Snippet } from "svelte";
  import Button from "../ui/Button.svelte";
  import IconButton from "../ui/IconButton.svelte";

  interface Props {
    /** いま画面に出ている枚数（絞り込み後） */
    totalCount: number;
    /** フォルダーにある枚数。totalCount と違うときだけ「N / M 枚」に切り替える */
    unfilteredCount: number;
    selectedCount: number;
    /**
     * 選択済みだが絞り込みで画面に出ていない枚数。
     *
     * **これを出さないと「N 枚選択中」が嘘になる。** 絞り込みは表示だけを変えて
     * 選択には触らないので、変換されるのは隠れている分も含めた N 枚である
     */
    hiddenSelectedCount: number;
    selectionMode: "multi" | "single";
    rightPanelCollapsed: boolean;
    onToggleRightPanel: () => void;
    onClearSelection: () => void;
    /** グリッド固有の操作（サイズスライダーなど） */
    controls?: Snippet;
    /** 絞り込みと並び順。状態は App が持つのでここへは snippet で届く */
    filters?: Snippet;
    /** 右パネルを畳んでいる間だけ出す主アクション（spec §3-1） */
    primaryAction?: Snippet;
  }

  let {
    totalCount,
    unfilteredCount,
    selectedCount,
    hiddenSelectedCount,
    selectionMode,
    rightPanelCollapsed,
    onToggleRightPanel,
    onClearSelection,
    controls,
    filters,
    primaryAction,
  }: Props = $props();
</script>

<div class="grid-header">
  <!-- 絞り込み中は分母も出す。「3 枚」だけだとフォルダーが空なのか
       絞り込んだ結果なのか区別できない -->
  <span class="count">
    {#if unfilteredCount !== totalCount}{totalCount} / {unfilteredCount} 枚{:else}{totalCount} 枚{/if}
  </span>

  {#if selectionMode === "multi" && selectedCount > 0}
    <!-- 3,000 枚あれば選択中の写真は画面外に出る。✓ だけでは全容が分からないので
         ここに枚数と全解除を置く（spec §5-1） -->
    <span class="selected">
      {selectedCount} 枚選択中{#if hiddenSelectedCount > 0}<span class="hidden-note"
          >（{hiddenSelectedCount} 枚は非表示）</span
        >{/if}
    </span>
    <Button variant="text" onclick={onClearSelection}>全解除</Button>
  {/if}

  {#if filters}
    <div class="filters">{@render filters()}</div>
  {/if}

  <div class="spacer"></div>

  {#if controls}
    <div class="controls">{@render controls()}</div>
  {/if}

  {#if rightPanelCollapsed && primaryAction}
    <div class="primary">{@render primaryAction()}</div>
  {/if}

  <!-- 開閉ハンドルはパネルの外に置く。中にあると畳んだ瞬間に消える（spec §3-1） -->
  <IconButton
    label={rightPanelCollapsed ? "右パネルを開く" : "右パネルを畳む"}
    icon={rightPanelCollapsed ? "◧" : "◨"}
    toggle
    pressed={rightPanelCollapsed}
    onclick={onToggleRightPanel}
  />
</div>

<style>
  .grid-header {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-3);
    border-bottom: 1px solid var(--md-sys-color-outline-variant);
    background: var(--md-sys-color-surface);
    font: var(--md-sys-typescale-body-sm);
    color: var(--md-sys-color-on-surface-variant);
  }

  /* 絞り込みで桁が変わるたびに右の部品が折り返すのを防ぐ */
  .count {
    white-space: nowrap;
  }

  .selected {
    color: var(--md-sys-color-primary);
  }

  /* 主張は控えめに。選択枚数そのものより一段落とす */
  .hidden-note {
    color: var(--md-sys-color-on-surface-variant);
  }

  .filters {
    min-width: 0;
  }

  .spacer {
    flex: 1;
    min-width: 0;
  }

  .controls,
  .primary {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }
</style>
