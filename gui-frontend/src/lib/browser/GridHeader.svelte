<script lang="ts">
  import type { Snippet } from "svelte";
  import Button from "../ui/Button.svelte";
  import IconButton from "../ui/IconButton.svelte";

  interface Props {
    totalCount: number;
    selectedCount: number;
    selectionMode: "multi" | "single";
    rightPanelCollapsed: boolean;
    onToggleRightPanel: () => void;
    onClearSelection: () => void;
    /** グリッド固有の操作（サイズスライダーなど） */
    controls?: Snippet;
    /** 右パネルを畳んでいる間だけ出す主アクション（spec §3-1） */
    primaryAction?: Snippet;
  }

  let {
    totalCount,
    selectedCount,
    selectionMode,
    rightPanelCollapsed,
    onToggleRightPanel,
    onClearSelection,
    controls,
    primaryAction,
  }: Props = $props();
</script>

<div class="grid-header">
  <span class="count">{totalCount} 枚</span>

  {#if selectionMode === "multi" && selectedCount > 0}
    <!-- 3,000 枚あれば選択中の写真は画面外に出る。✓ だけでは全容が分からないので
         ここに枚数と全解除を置く（spec §5-1） -->
    <span class="selected">{selectedCount} 枚選択中</span>
    <Button variant="text" onclick={onClearSelection}>全解除</Button>
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

  .selected {
    color: var(--md-sys-color-primary);
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
