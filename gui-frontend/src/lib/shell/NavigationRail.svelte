<script lang="ts">
  import { MODES, type AppMode } from "./modes";

  interface Props {
    mode: AppMode;
    onModeChange: (mode: AppMode) => void;
  }

  let { mode, onModeChange }: Props = $props();
</script>

<nav class="rail" aria-label="モード">
  {#each MODES as destination (destination.value)}
    {@const selected = mode === destination.value}
    <button
      class="destination"
      class:selected
      type="button"
      aria-current={selected ? "page" : undefined}
      onclick={() => onModeChange(destination.value)}
    >
      <span class="indicator state-layer" aria-hidden="true">{destination.icon}</span>
      <span class="label">{destination.label}</span>
    </button>
  {/each}
</nav>

<style>
  .rail {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-3);
    /* rail 幅は 80px 固定（spec §3-1）。ここだけは構造的な寸法 */
    width: 80px;
    flex-shrink: 0;
    padding: var(--space-3) 0;
    background: var(--md-sys-color-surface);
    border-right: 1px solid var(--md-sys-color-outline-variant);
  }

  .destination {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-1);
    width: 100%;
    padding: 0;
    background: none;
    border: none;
    cursor: pointer;
    color: var(--md-sys-color-on-surface-variant);
  }

  .destination.selected {
    color: var(--md-sys-color-on-surface);
  }

  /* 選択インジケータは pill 形（spec §3-3） */
  .indicator {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 56px;
    height: 32px;
    border-radius: var(--md-sys-shape-corner-full);
    font-size: 18px;
    line-height: 1;
    transition: background var(--md-sys-motion-duration-short)
      var(--md-sys-motion-easing-standard);
  }

  .destination.selected .indicator {
    background: var(--md-sys-color-primary-container);
    color: var(--md-sys-color-on-primary-container);
  }

  .label {
    font: var(--md-sys-typescale-body-sm);
    white-space: nowrap;
  }
</style>
