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
  <div class="status">
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
  </div>

  {#if filters}
    <!-- **クラス名は `.filters` にしない。** 中に描かれる GridFilterBar の
         ルートが同じ `.filters` で、入れ子の同名は e2e の locator が
         2 つに一致して例外になる -->
    <div class="filters-area">{@render filters()}</div>
  {/if}

  <div class="tools">
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
</div>

<style>
  /**
   * **段は `grid-template-areas` で明示する。`flex-wrap` では出せない。**
   *
   * flex アイテムはソース順にしか折り返せないので、「1 段目 = 枚数群 ＋ サイズ ＋
   * ハンドル / 2 段目 = 検索群」という並びは絶対に出ない（実測でヘッダー高さ 179px。
   * 現行の破綻した 97px よりさらに高い）。
   */
  .grid-header {
    display: grid;
    grid-template-columns: 1fr auto;
    grid-template-areas:
      "status tools"
      "filters filters";
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-3);
    border-bottom: 1px solid var(--md-sys-color-outline-variant);
    background: var(--md-sys-color-surface);
    font: var(--md-sys-typescale-body-sm);
    color: var(--md-sys-color-on-surface-variant);
  }

  /* **窓幅ではなくヘッダー自身の幅で切る。** 左右のパネルは利用者がドラッグで
     変えられるので、同じ窓幅でもヘッダーの実効幅は 460px にも 780px にもなる。
     コンテナは `.photo-grid`（PhotoGrid.svelte）が `container-type: inline-size` で作る */
  @container (min-width: 952px) {
    .grid-header {
      grid-template-columns: auto 1fr auto;
      grid-template-areas: "status filters tools";
    }
  }

  .status {
    grid-area: status;
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-width: 0;
  }

  .filters-area {
    grid-area: filters;
    min-width: 0;
  }

  .tools {
    grid-area: tools;
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }

  /* 絞り込みで桁が変わるたびに右の部品が折り返すのを防ぐ */
  .count {
    white-space: nowrap;
  }

  /* **`nowrap` が要る。** 入れないと縦書きに折り返すだけで溢れないので、
     破綻が `scrollWidth` に現れずテストで検知できない */
  .selected {
    color: var(--md-sys-color-primary);
    white-space: nowrap;
  }

  /* 主張は控えめに。選択枚数そのものより一段落とす */
  .hidden-note {
    color: var(--md-sys-color-on-surface-variant);
  }

  .controls,
  .primary {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }
</style>
