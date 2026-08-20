<script lang="ts">
  interface Props {
    checked: boolean;
    label: string;
    /** 不可逆な操作のトグル（元ファイル削除）。on のとき error ロールで塗る */
    danger?: boolean;
    disabled?: boolean;
    onchange?: () => void;
  }

  let {
    checked = $bindable(),
    label,
    danger = false,
    disabled = false,
    onchange,
  }: Props = $props();
</script>

<label class="switch" class:disabled>
  <input type="checkbox" bind:checked {disabled} onchange={() => onchange?.()} />
  <span class="track state-layer" class:danger>
    <span class="thumb"></span>
  </span>
  <span class="text">{label}</span>
</label>

<style>
  .switch {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    cursor: pointer;
    font: var(--md-sys-typescale-body-md);
    color: var(--md-sys-color-on-surface);
  }

  .switch.disabled {
    cursor: default;
    opacity: 0.38;
  }

  /* ネイティブの checkbox は消さずに透明にして重ねる。
     消すとキーボード操作とフォームの意味論を自前で作り直すことになる。 */
  input {
    position: absolute;
    width: 52px;
    height: 32px;
    margin: 0;
    opacity: 0;
    cursor: inherit;
  }

  .track {
    position: relative;
    flex-shrink: 0;
    width: 52px;
    height: 32px;
    border-radius: var(--md-sys-shape-corner-full);
    background: var(--md-sys-color-surface-container-highest);
    border: 2px solid var(--md-sys-color-outline);
    color: var(--md-sys-color-on-surface-variant);
    transition: background var(--md-sys-motion-duration-short)
      var(--md-sys-motion-easing-standard);
  }

  .thumb {
    position: absolute;
    top: 50%;
    left: 6px;
    width: 16px;
    height: 16px;
    transform: translateY(-50%);
    border-radius: var(--md-sys-shape-corner-full);
    background: var(--md-sys-color-outline);
    transition: left var(--md-sys-motion-duration-short)
        var(--md-sys-motion-easing-standard),
      width var(--md-sys-motion-duration-short) var(--md-sys-motion-easing-standard),
      height var(--md-sys-motion-duration-short) var(--md-sys-motion-easing-standard),
      background var(--md-sys-motion-duration-short)
        var(--md-sys-motion-easing-standard);
  }

  input:checked ~ .track {
    background: var(--md-sys-color-primary);
    border-color: var(--md-sys-color-primary);
    color: var(--md-sys-color-on-primary);
  }

  input:checked ~ .track.danger {
    background: var(--md-sys-color-error);
    border-color: var(--md-sys-color-error);
    color: var(--md-sys-color-on-error);
  }

  input:checked ~ .track .thumb {
    left: 26px;
    width: 24px;
    height: 24px;
    background: var(--md-sys-color-on-primary);
  }

  input:checked ~ .track.danger .thumb {
    background: var(--md-sys-color-on-error);
  }

  input:focus-visible ~ .track {
    outline: var(--md-sys-state-focus-ring);
    outline-offset: var(--md-sys-state-focus-ring-offset);
  }
</style>
