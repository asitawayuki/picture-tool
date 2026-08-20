<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    variant?: "filled" | "tonal" | "outlined" | "text";
    /** 破壊的操作。primary ロールを error ロールに振り替える */
    danger?: boolean;
    disabled?: boolean;
    /** ラベルの左に置く記号。装飾なので aria-hidden にする */
    icon?: string;
    /** 幅を親いっぱいにする（パネル最下部の主ボタンなど） */
    full?: boolean;
    type?: "button" | "submit";
    onclick?: (event: MouseEvent) => void;
    children: Snippet;
  }

  let {
    variant = "filled",
    danger = false,
    disabled = false,
    icon,
    full = false,
    type = "button",
    onclick,
    children,
  }: Props = $props();
</script>

<button
  class="btn state-layer {variant}"
  class:danger
  class:full
  {type}
  {disabled}
  {onclick}
>
  {#if icon}<span class="icon" aria-hidden="true">{icon}</span>{/if}
  {@render children()}
</button>

<style>
  /* hover / focus / pressed は .state-layer（tokens.css）が ::after で供給する。
     ここで background を書き換えないこと。 */
  .btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-2);
    min-height: 40px;
    padding: 0 var(--space-5);
    border: none;
    border-radius: var(--md-sys-shape-corner-full);
    font: var(--md-sys-typescale-label-lg);
    letter-spacing: var(--md-sys-typescale-label-lg-tracking);
    cursor: pointer;
    white-space: nowrap;
  }

  .btn.full {
    width: 100%;
  }

  .btn:disabled {
    cursor: default;
    opacity: 0.38;
  }

  .filled {
    background: var(--md-sys-color-primary);
    color: var(--md-sys-color-on-primary);
  }

  .filled.danger {
    background: var(--md-sys-color-error);
    color: var(--md-sys-color-on-error);
  }

  .tonal {
    background: var(--md-sys-color-primary-container);
    color: var(--md-sys-color-on-primary-container);
  }

  .tonal.danger {
    background: var(--md-sys-color-error-container);
    color: var(--md-sys-color-on-error-container);
  }

  .outlined {
    background: transparent;
    color: var(--md-sys-color-primary);
    border: 1px solid var(--md-sys-color-outline);
  }

  .outlined.danger {
    color: var(--md-sys-color-error);
  }

  .text {
    background: transparent;
    color: var(--md-sys-color-primary);
    padding: 0 var(--space-3);
  }

  .text.danger {
    color: var(--md-sys-color-error);
  }

  .icon {
    font-size: 16px;
    line-height: 1;
  }
</style>
