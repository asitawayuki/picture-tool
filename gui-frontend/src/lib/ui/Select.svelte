<!-- SegmentedButton と同じ理由でジェネリック。
     bind:value={config.position}（ExifPosition）を受けられるようにする -->
<script lang="ts" generics="T extends string">
  interface Props {
    value: T;
    label: string;
    options: { value: T; label: string }[];
    disabled?: boolean;
    onchange?: () => void;
  }

  let {
    value = $bindable(),
    label,
    options,
    disabled = false,
    onchange,
  }: Props = $props();

  const id = $props.id();
</script>

<div class="select">
  <label for={id}>{label}</label>
  <div class="control">
    <select {id} {disabled} bind:value onchange={() => onchange?.()}>
      {#each options as option (option.value)}
        <option value={option.value}>{option.label}</option>
      {/each}
    </select>
    <span class="arrow" aria-hidden="true">▾</span>
  </div>
</div>

<style>
  .select {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  label {
    font: var(--md-sys-typescale-body-sm);
    color: var(--md-sys-color-on-surface-variant);
  }

  .control {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: 0 var(--space-3);
    background: var(--md-sys-color-surface-container-highest);
    border: 1px solid var(--md-sys-color-outline);
    border-radius: var(--md-sys-shape-corner-sm);
  }

  .control:focus-within {
    outline: var(--md-sys-state-focus-ring);
    outline-offset: var(--md-sys-state-focus-ring-offset);
    border-color: var(--md-sys-color-primary);
  }

  select {
    flex: 1;
    min-width: 0;
    padding: var(--space-2) 0;
    background: none;
    border: none;
    color: var(--md-sys-color-on-surface);
    font: var(--md-sys-typescale-body-md);
    -webkit-appearance: none;
    appearance: none;
  }

  select:focus {
    outline: none;
  }

  select:disabled {
    opacity: 0.38;
  }

  .arrow {
    flex-shrink: 0;
    color: var(--md-sys-color-on-surface-variant);
  }
</style>
