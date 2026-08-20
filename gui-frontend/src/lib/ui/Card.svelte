<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    /** elevation の段。面の明度差が主、影が従（spec §1-5） */
    level?: 0 | 1 | 2 | 3;
    /** 既定は --space-4。行の並びなど余白を持たせたくない用途で "0" を渡す */
    padding?: string;
    /** 付けると title-sm の見出しが出る。パネル内のグループ分け用 */
    title?: string;
    children: Snippet;
  }

  let { level = 1, padding = "var(--space-4)", title, children }: Props = $props();
</script>

<section class="card level-{level}" style="padding: {padding};">
  {#if title}
    <h3 class="card-title">{title}</h3>
  {/if}
  {@render children()}
</section>

<style>
  .card {
    border-radius: var(--md-sys-shape-corner-md);
    color: var(--md-sys-color-on-surface);
    font: var(--md-sys-typescale-body-md);
  }

  .level-0 {
    background: var(--md-sys-elevation-surface-0);
    box-shadow: var(--md-sys-elevation-shadow-0);
  }

  .level-1 {
    background: var(--md-sys-elevation-surface-1);
    box-shadow: var(--md-sys-elevation-shadow-1);
  }

  .level-2 {
    background: var(--md-sys-elevation-surface-2);
    box-shadow: var(--md-sys-elevation-shadow-2);
  }

  .level-3 {
    background: var(--md-sys-elevation-surface-3);
    box-shadow: var(--md-sys-elevation-shadow-3);
  }

  .card-title {
    margin: 0 0 var(--space-3);
    font: var(--md-sys-typescale-title-sm);
    color: var(--md-sys-color-on-surface-variant);
  }
</style>
