<script lang="ts">
  import IconButton from "../ui/IconButton.svelte";
  import type { ExifFrameConfig } from "../types";
  import { BUNDLED_PRESET_NAME } from "./frameDraft.svelte";

  interface Props {
    presets: ExifFrameConfig[];
    editingName: string;
    onSelect: (name: string) => void;
    /** ダブルクリックでの改名。名前を変えるだけで、旧名の削除は保存時 */
    onRename: (name: string) => void;
    onCreate: () => void;
    onDelete: (name: string) => void;
  }

  let { presets, editingName, onSelect, onRename, onCreate, onDelete }: Props = $props();

  let renaming = $state<string | null>(null);
  let renameValue = $state("");

  function startRename(name: string) {
    if (name === BUNDLED_PRESET_NAME) return; // 組み込みは改名できない
    renaming = name;
    renameValue = name;
  }

  /**
   * 確定は Enter と blur の両方から来る。Enter で確定した直後に入力欄が消えると
   * ブラウザによっては blur も飛ぶので、二重確定と「Escape で消した直後の
   * blur が確定してしまう」経路を `renaming` の状態で塞ぐ。
   */
  function commitRename() {
    if (renaming === null) return;
    const next = renameValue.trim();
    renaming = null;
    if (next.length > 0) onRename(next);
  }
</script>

<div class="preset-list">
  <div class="head">
    <span>プリセット</span>
    <IconButton label="新規プリセット" icon="＋" onclick={onCreate} />
  </div>

  <ul>
    {#each presets as preset (preset.name)}
      <li>
        {#if renaming === preset.name}
          <!-- svelte-ignore a11y_autofocus -->
          <input
            class="rename"
            autofocus
            aria-label="プリセット名"
            bind:value={renameValue}
            onblur={commitRename}
            onkeydown={(e) => {
              if (e.key === "Enter") commitRename();
              if (e.key === "Escape") renaming = null;
            }}
          />
        {:else}
          <button
            class="item state-layer"
            class:active={preset.name === editingName}
            type="button"
            aria-current={preset.name === editingName}
            onclick={() => onSelect(preset.name)}
            ondblclick={() => startRename(preset.name)}
          >
            {preset.name}
          </button>
          {#if preset.name !== BUNDLED_PRESET_NAME}
            <IconButton
              label="{preset.name} を削除"
              icon="🗑"
              onclick={() => onDelete(preset.name)}
            />
          {/if}
        {/if}
      </li>
    {/each}
  </ul>

  <p class="hint">項目をダブルクリックで改名できます。</p>
</div>

<style>
  .preset-list {
    display: flex;
    flex-direction: column;
    height: 100%;
    padding: var(--space-3);
    gap: var(--space-2);
  }

  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font: var(--md-sys-typescale-title-sm);
    color: var(--md-sys-color-on-surface-variant);
  }

  ul {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  li {
    display: flex;
    align-items: center;
    gap: var(--space-1);
  }

  .item {
    flex: 1;
    min-width: 0;
    text-align: left;
    padding: var(--space-2) var(--space-3);
    border: none;
    border-radius: var(--md-sys-shape-corner-full);
    background: none;
    color: var(--md-sys-color-on-surface);
    font: var(--md-sys-typescale-body-md);
    cursor: pointer;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .item.active {
    background: var(--md-sys-color-primary-container);
    color: var(--md-sys-color-on-primary-container);
  }

  .rename {
    flex: 1;
    min-width: 0;
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--md-sys-color-primary);
    border-radius: var(--md-sys-shape-corner-sm);
    background: var(--md-sys-color-surface-container-highest);
    color: var(--md-sys-color-on-surface);
    font: var(--md-sys-typescale-body-md);
  }

  .hint {
    margin: 0;
    font: var(--md-sys-typescale-body-sm);
    color: var(--md-sys-color-on-surface-variant);
  }
</style>
