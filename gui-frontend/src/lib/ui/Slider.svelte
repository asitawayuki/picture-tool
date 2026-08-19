<script lang="ts">
  interface Props {
    value: number;
    label: string;
    min: number;
    max: number;
    step?: number;
    /** 現在値の後ろに出す単位（"%" / "px" など） */
    suffix?: string;
    /** 値の見せ方を変えたいとき（フレーム文字サイズの 0.025 → "2.5"） */
    format?: (value: number) => string;
    disabled?: boolean;
  }

  let {
    value = $bindable(),
    label,
    min,
    max,
    step = 1,
    suffix = "",
    format,
    disabled = false,
  }: Props = $props();

  const id = $props.id();
  let display = $derived((format ? format(value) : String(value)) + suffix);
</script>

<div class="slider">
  <div class="head">
    <label for={id}>{label}</label>
    <span class="value">{display}</span>
  </div>
  <input {id} type="range" {min} {max} {step} {disabled} bind:value />
</div>

<style>
  .slider {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .head {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: var(--space-2);
    font: var(--md-sys-typescale-body-sm);
    color: var(--md-sys-color-on-surface-variant);
  }

  .value {
    color: var(--md-sys-color-on-surface);
    font-variant-numeric: tabular-nums;
  }

  input {
    width: 100%;
    height: 20px;
    margin: 0;
    padding: 0;
    background: transparent;
    -webkit-appearance: none;
    appearance: none;
    cursor: pointer;
  }

  input:disabled {
    cursor: default;
    opacity: 0.38;
  }

  /* WebKit / Blink（WebKitGTK も含む） */
  input::-webkit-slider-runnable-track {
    height: 4px;
    border-radius: var(--md-sys-shape-corner-full);
    background: var(--md-sys-color-surface-container-highest);
  }

  input::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: 16px;
    height: 16px;
    margin-top: -6px;
    border: none;
    border-radius: var(--md-sys-shape-corner-full);
    background: var(--md-sys-color-primary);
  }

  /* Gecko（開発時のブラウザ差を埋めるために残す） */
  input::-moz-range-track {
    height: 4px;
    border-radius: var(--md-sys-shape-corner-full);
    background: var(--md-sys-color-surface-container-highest);
  }

  input::-moz-range-thumb {
    width: 16px;
    height: 16px;
    border: none;
    border-radius: var(--md-sys-shape-corner-full);
    background: var(--md-sys-color-primary);
  }
</style>
