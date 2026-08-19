<script lang="ts">
  import Button from "./lib/ui/Button.svelte";
  import IconButton from "./lib/ui/IconButton.svelte";
  import Card from "./lib/ui/Card.svelte";
  import TextField from "./lib/ui/TextField.svelte";
  import Switch from "./lib/ui/Switch.svelte";
  import Slider from "./lib/ui/Slider.svelte";
  import Select from "./lib/ui/Select.svelte";
  import SegmentedButton from "./lib/ui/SegmentedButton.svelte";
  import Rating from "./lib/ui/Rating.svelte";
  import LinearProgress from "./lib/ui/LinearProgress.svelte";
  import Dialog from "./lib/ui/Dialog.svelte";

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

  let text = $state("キャプション");
  let comment = $state("複数行のコメント\n2 行目");
  let numeric = $state<number | null>(1080);
  let toggleOn = $state(true);
  let dangerOn = $state(false);
  let quality = $state(90);
  let selected = $state("crop");
  let bg = $state("white");
  let font = $state("");
  let rating = $state(3);
  let dialogOpen = $state(false);
  let dangerDialogOpen = $state(false);
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

  <section class="specimen" data-specimen="TextField">
    <h2>TextField</h2>
    <div class="row grid">
      <TextField bind:value={text} label="タイトル" placeholder="未設定" />
      <TextField bind:value={numeric} label="出力幅の上限" type="number" suffix="px"
        min={4} max={20000} normalize={(v) => Math.floor(Math.min(Math.max(v, 4), 20000) / 4) * 4}
        hint="4 の倍数へ切り捨てる" />
      <TextField bind:value={text} label="エラー状態" error="値が範囲外です" />
      <TextField bind:value={text} label="無効" disabled />
      <TextField bind:value={comment} label="コメント" multiline rows={4} />
    </div>
  </section>

  <section class="specimen" data-specimen="Switch">
    <h2>Switch</h2>
    <div class="row">
      <Switch bind:checked={toggleOn} label="Exif フレーム" />
      <Switch bind:checked={dangerOn} label="元ファイルを削除" danger />
      <Switch bind:checked={toggleOn} label="無効" disabled />
    </div>
  </section>

  <section class="specimen" data-specimen="Slider">
    <h2>Slider</h2>
    <div class="row grid">
      <Slider bind:value={quality} label="品質" min={1} max={100} suffix="%" />
      <Slider bind:value={quality} label="無効" min={1} max={100} disabled />
    </div>
  </section>

  <section class="specimen" data-specimen="Select">
    <h2>Select</h2>
    <div class="row grid">
      <Select bind:value={font} label="フォント"
        options={[{ value: "", label: "同梱フォント" }, { value: "/a.ttf", label: "Noto Sans JP" }]} />
      <Select bind:value={font} label="無効" disabled
        options={[{ value: "", label: "同梱フォント" }]} />
    </div>
  </section>

  <section class="specimen" data-specimen="SegmentedButton">
    <h2>SegmentedButton</h2>
    <div class="row grid">
      <SegmentedButton bind:value={selected} label="変換モード"
        options={[
          { value: "crop", label: "Crop" },
          { value: "pad", label: "Pad" },
          { value: "quality", label: "Quality" },
        ]} />
      <SegmentedButton bind:value={bg} label="背景色"
        options={[{ value: "white", label: "白" }, { value: "black", label: "黒" }]} />
    </div>
  </section>

  <section class="specimen" data-specimen="Rating">
    <h2>Rating</h2>
    <div class="row">
      <Rating bind:value={rating} />
      <Rating value={4} readonly />
      <Rating value={0} disabled />
      <span>value = {rating}</span>
    </div>
  </section>

  <section class="specimen" data-specimen="LinearProgress">
    <h2>LinearProgress</h2>
    <div class="row grid">
      <LinearProgress value={0} />
      <LinearProgress value={40} />
      <LinearProgress value={100} />
      <LinearProgress />
    </div>
  </section>

  <section class="specimen" data-specimen="Dialog">
    <h2>Dialog</h2>
    <div class="row">
      <Button onclick={() => (dialogOpen = true)}>通常のダイアログ</Button>
      <Button variant="filled" danger onclick={() => (dangerDialogOpen = true)}>
        危険なダイアログ
      </Button>
    </div>
  </section>
</div>

{#if dialogOpen}
  <Dialog title="変換結果" onClose={() => (dialogOpen = false)}>
    <p>本文。Card を並べて中身を作る。</p>
    {#snippet actions()}
      <Button variant="text" onclick={() => (dialogOpen = false)}>閉じる</Button>
    {/snippet}
  </Dialog>
{/if}

{#if dangerDialogOpen}
  <Dialog
    title="元ファイルを削除します"
    danger
    initialFocus="footer button"
    onClose={() => (dangerDialogOpen = false)}
  >
    <p>削除したファイルはゴミ箱に入らず、元に戻せません。</p>
    {#snippet actions()}
      <Button variant="text" onclick={() => (dangerDialogOpen = false)}>キャンセル</Button>
      <Button variant="filled" danger onclick={() => (dangerDialogOpen = false)}>
        削除して変換
      </Button>
    {/snippet}
  </Dialog>
{/if}

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

  .row.grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    align-items: start;
  }

  .row.block {
    display: block;
    max-width: 320px;
  }
</style>
