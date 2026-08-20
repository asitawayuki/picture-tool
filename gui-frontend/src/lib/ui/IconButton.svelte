<script lang="ts">
  interface Props {
    variant?: "standard" | "filled";
    /** トグルとして使う。true のとき aria-pressed を出す */
    toggle?: boolean;
    pressed?: boolean;
    /** アイコンしか出ないので必須。aria-label と title の両方に使う */
    label: string;
    icon: string;
    disabled?: boolean;
    onclick?: (event: MouseEvent) => void;
  }

  let {
    variant = "standard",
    toggle = false,
    pressed = false,
    label,
    icon,
    disabled = false,
    onclick,
  }: Props = $props();
</script>

<button
  class="icon-btn state-layer {variant}"
  class:on={toggle && pressed}
  aria-label={label}
  aria-pressed={toggle ? pressed : undefined}
  title={label}
  {disabled}
  {onclick}
>
  <span aria-hidden="true">{icon}</span>
</button>

<style>
  .icon-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 40px;
    height: 40px;
    flex-shrink: 0;
    border: none;
    border-radius: var(--md-sys-shape-corner-full);
    font-size: 18px;
    line-height: 1;
    cursor: pointer;
  }

  .icon-btn:disabled {
    cursor: default;
    opacity: 0.38;
  }

  .standard {
    background: transparent;
    color: var(--md-sys-color-on-surface-variant);
  }

  .standard.on {
    color: var(--md-sys-color-primary);
  }

  .filled {
    background: var(--md-sys-color-surface-container-high);
    color: var(--md-sys-color-on-surface-variant);
  }

  .filled.on {
    background: var(--md-sys-color-primary);
    color: var(--md-sys-color-on-primary);
  }
</style>
