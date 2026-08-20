<script lang="ts">
  interface Props {
    /** null（既定）で indeterminate */
    value?: number | null;
    max?: number;
    label?: string;
  }

  let { value = null, max = 100, label = "進捗" }: Props = $props();

  let percent = $derived(
    value === null || max <= 0 ? 0 : Math.min(100, Math.max(0, (value / max) * 100))
  );
</script>

<div
  class="track"
  role="progressbar"
  aria-label={label}
  aria-valuemin={value === null ? undefined : 0}
  aria-valuemax={value === null ? undefined : max}
  aria-valuenow={value === null ? undefined : value}
>
  {#if value === null}
    <div class="bar indeterminate"></div>
  {:else}
    <div class="bar" style="width: {percent}%"></div>
  {/if}
</div>

<style>
  .track {
    position: relative;
    width: 100%;
    height: 4px;
    overflow: hidden;
    border-radius: var(--md-sys-shape-corner-full);
    background: var(--md-sys-color-surface-container-highest);
  }

  .bar {
    height: 100%;
    border-radius: inherit;
    background: var(--md-sys-color-primary);
    transition: width var(--md-sys-motion-duration-medium)
      var(--md-sys-motion-easing-standard);
  }

  .bar.indeterminate {
    position: absolute;
    inset-block: 0;
    width: 40%;
    animation: slide 1.4s var(--md-sys-motion-easing-standard) infinite;
  }

  @keyframes slide {
    from {
      left: -40%;
    }
    to {
      left: 100%;
    }
  }
</style>
