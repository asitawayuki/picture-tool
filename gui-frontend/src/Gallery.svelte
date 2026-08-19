<script lang="ts">
  import Button from "./lib/ui/Button.svelte";
  import IconButton from "./lib/ui/IconButton.svelte";
  import Card from "./lib/ui/Card.svelte";

  /**
   * ここだけは data-theme を手で切り替える。
   * アプリ本体には手動切替を入れない（spec §1-7）が、ギャラリーは dev 専用で
   * ビルド成果物に入らないため、明暗を並べて見るためにこの経路を使う。
   * tokens.css の 4 ブロック構造が両方向に効くことの確認も兼ねる。
   */
  let theme = $state<"system" | "light" | "dark">("system");

  $effect(() => {
    if (theme === "system") delete document.documentElement.dataset.theme;
    else document.documentElement.dataset.theme = theme;
  });

  /** each の式に `as const` を直接書くと Svelte の each 構文の `as` と衝突する。
      配列はスクリプト側で定義しておく。 */
  const THEMES = ["system", "light", "dark"] as const;
  const LEVELS = [0, 1, 2, 3] as const;

  let toggled = $state(false);
</script>

<div class="gallery">
  <header>
    <h1>部品ギャラリー</h1>
    <div class="theme-switch" role="group" aria-label="テーマ">
      {#each THEMES as t (t)}
        <button class:active={theme === t} onclick={() => (theme = t)}>{t}</button>
      {/each}
    </div>
  </header>

  <section class="specimen" data-specimen="Button">
    <h2>Button</h2>
    <div class="row">
      <Button variant="filled">filled</Button>
      <Button variant="tonal">tonal</Button>
      <Button variant="outlined">outlined</Button>
      <Button variant="text">text</Button>
    </div>
    <div class="row">
      <Button variant="filled" danger>filled danger</Button>
      <Button variant="tonal" danger>tonal danger</Button>
      <Button variant="outlined" danger>outlined danger</Button>
      <Button variant="text" danger>text danger</Button>
    </div>
    <div class="row">
      <Button variant="filled" disabled>filled disabled</Button>
      <Button variant="outlined" disabled>outlined disabled</Button>
      <Button variant="filled" icon="▶">icon 付き</Button>
    </div>
    <div class="row block">
      <Button variant="filled" full>full width</Button>
    </div>
  </section>

  <section class="specimen" data-specimen="IconButton">
    <h2>IconButton</h2>
    <div class="row">
      <IconButton label="閉じる" icon="✕" />
      <IconButton variant="filled" label="削除" icon="🗑" />
      <IconButton label="切替" icon="★" toggle pressed={toggled}
        onclick={() => (toggled = !toggled)} />
      <IconButton variant="filled" label="切替（塗り）" icon="★" toggle pressed={toggled}
        onclick={() => (toggled = !toggled)} />
      <IconButton label="無効" icon="✕" disabled />
    </div>
  </section>

  <section class="specimen" data-specimen="Card">
    <h2>Card</h2>
    <div class="row">
      {#each LEVELS as level (level)}
        <Card {level} title="level {level}">
          <p>面の明度差が主、影が従。</p>
        </Card>
      {/each}
    </div>
  </section>
</div>

<style>
  .gallery {
    min-height: 100vh;
    padding: var(--space-5);
    background: var(--md-sys-color-surface);
    color: var(--md-sys-color-on-surface);
    font: var(--md-sys-typescale-body-md);
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: var(--space-5);
  }

  h1 {
    font: var(--md-sys-typescale-title-md);
    margin: 0;
  }

  h2 {
    font: var(--md-sys-typescale-title-sm);
    color: var(--md-sys-color-on-surface-variant);
    margin: 0 0 var(--space-3);
  }

  .theme-switch button {
    border: 1px solid var(--md-sys-color-outline);
    background: transparent;
    color: var(--md-sys-color-on-surface);
    padding: var(--space-1) var(--space-3);
    cursor: pointer;
    font: var(--md-sys-typescale-body-sm);
  }

  .theme-switch button.active {
    background: var(--md-sys-color-primary);
    color: var(--md-sys-color-on-primary);
  }

  .specimen {
    margin-bottom: var(--space-6);
  }

  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-3);
    margin-bottom: var(--space-3);
  }

  .row.block {
    display: block;
    max-width: 320px;
  }
</style>
