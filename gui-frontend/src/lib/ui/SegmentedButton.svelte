<!-- value をジェネリックにしないと bind:value={config.mode}
     （"crop"|"pad"|"quality"）が svelte-check で落ちる -->
<script lang="ts" generics="T extends string">
  interface Props {
    value: T;
    /** グループのラベル。可視ラベルは親側が置く前提で aria-label に使う */
    label: string;
    options: { value: T; label: string; icon?: string }[];
    disabled?: boolean;
  }

  let { value = $bindable(), label, options, disabled = false }: Props = $props();

  /**
   * ネイティブの radio を隠して重ねる。
   * button + aria-pressed で組むと、矢印キーでの移動とグループの
   * 意味論を自前で作り直すことになる（ブラウザが radio に対して既にやっている）。
   */
  const groupName = $props.id();
</script>

<div class="segmented" role="radiogroup" aria-label={label}>
  {#each options as option (option.value)}
    <label class="segment state-layer" class:selected={value === option.value}>
      <input
        type="radio"
        name={groupName}
        value={option.value}
        bind:group={value}
        {disabled}
      />
      {#if option.icon}<span class="icon" aria-hidden="true">{option.icon}</span>{/if}
      <span class="text">{option.label}</span>
    </label>
  {/each}
</div>

<style>
  .segmented {
    display: flex;
    border: 1px solid var(--md-sys-color-outline);
    border-radius: var(--md-sys-shape-corner-full);
    overflow: hidden;
  }

  .segment {
    flex: 1;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-1);
    min-height: 36px;
    padding: 0 var(--space-3);
    cursor: pointer;
    font: var(--md-sys-typescale-label-lg);
    letter-spacing: var(--md-sys-typescale-label-lg-tracking);
    color: var(--md-sys-color-on-surface);
    white-space: nowrap;
  }

  .segment + .segment {
    border-left: 1px solid var(--md-sys-color-outline);
  }

  .segment.selected {
    background: var(--md-sys-color-primary-container);
    color: var(--md-sys-color-on-primary-container);
  }

  input {
    position: absolute;
    opacity: 0;
    pointer-events: none;
  }

  input:focus-visible ~ .text,
  input:focus-visible ~ .icon {
    outline: var(--md-sys-state-focus-ring);
    outline-offset: var(--md-sys-state-focus-ring-offset);
  }

  input:disabled ~ .text {
    opacity: 0.38;
  }
</style>
