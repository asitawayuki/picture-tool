<script lang="ts">
  import type { Snippet } from "svelte";
  import GridHeader from "./GridHeader.svelte";
  import PhotoViewer from "./PhotoViewer.svelte";
  import Slider from "../ui/Slider.svelte";
  import {
    GRID_GAP,
    GRID_PADDING,
    computeGridMetrics,
    computeVisibleRange,
  } from "./gridMetrics";
  import type { RequestKind } from "./requestQueue";
  import type { ImageEntry } from "../types";

  interface Props {
    images: ImageEntry[];
    /** multi: 変換モード（複数チェック） / single: メタデータモード（単一フォーカス） */
    selectionMode: "multi" | "single";
    selectedPaths: Set<string>;
    focusedPath: string | null;
    thumbnailFor: (path: string, size: number) => string | undefined;
    onRequestThumbnail: (
      path: string,
      size: number,
      kind: RequestKind,
      index: number
    ) => void;
    onVisibleRangeChange: (start: number, end: number) => void;
    onToggleSelect: (image: ImageEntry) => void;
    onFocus: (image: ImageEntry) => void;
    selectedCount: number;
    rightPanelCollapsed: boolean;
    onToggleRightPanel: () => void;
    onClearSelection: () => void;
    primaryAction?: Snippet;
    /**
     * スクロール位置。**親が持つ**（spec §3-2「rail の切替では破棄しない
     * ── スクロール位置も保つ」）。フレームモードでは PhotoGrid 自体が
     * unmount されるので、内部 state のままだと戻ったときに先頭へ飛ぶ。
     */
    scrollTop: number;
  }

  let {
    images,
    selectionMode,
    selectedPaths,
    focusedPath,
    thumbnailFor,
    onRequestThumbnail,
    onVisibleRangeChange,
    onToggleSelect,
    onFocus,
    selectedCount,
    rightPanelCollapsed,
    onToggleRightPanel,
    onClearSelection,
    primaryAction,
    scrollTop = $bindable(),
  }: Props = $props();

  /** タイルの目標幅。既定 200px は「サムネイルが小さい」への回答（spec §4-1） */
  let targetTileWidth = $state(200);

  /**
   * 全画面プレビューの対象。**グリッドが持つ**。
   * プレビューを開けるのはグリッドからだけで、必要な props
   * （一覧・選択・サムネイル）はすべてここに揃っている。
   */
  let previewImage = $state<ImageEntry | null>(null);

  function openPreview(image: ImageEntry) {
    previewImage = image;
  }

  /** スクロールする箱。仮想化の余白は持たない */
  let scroller: HTMLDivElement | undefined = $state();
  /** role="listbox" の箱。仮想化の余白と列指定はこちらに付く */
  let listbox: HTMLDivElement | undefined = $state();
  let containerWidth = $state(0);
  let viewportHeight = $state(0);

  // mount 時に親が持っている位置へ戻す。以降は onscroll が親へ書き戻す
  $effect(() => {
    if (scroller && scroller.scrollTop !== scrollTop) scroller.scrollTop = scrollTop;
  });

  let metrics = $derived(
    computeGridMetrics(containerWidth, targetTileWidth, images.length)
  );
  let range = $derived(
    computeVisibleRange(metrics, scrollTop, viewportHeight, images.length)
  );
  let visible = $derived(
    range.endIndex < range.startIndex
      ? []
      : images.slice(range.startIndex, range.endIndex + 1)
  );

  let focusedIndex = $derived(
    focusedPath === null ? -1 : images.findIndex((img) => img.path === focusedPath)
  );

  /**
   * roving tabindex。可視の 1 枚だけ tabindex="0"。
   * フォーカス中のタイルが描画範囲の外なら、範囲の先頭を代役にする。
   */
  let rovingIndex = $derived(
    focusedIndex >= range.startIndex && focusedIndex <= range.endIndex
      ? focusedIndex
      : range.startIndex
  );

  // 可視範囲が変わるたびにキューへ通知する。可視範囲の持ち主はグリッド側
  // であり、キューがスクロール状態を二重に持つ理由が無い（spec §4-2）
  $effect(() => {
    onVisibleRangeChange(range.startIndex, range.endIndex);
  });

  // 描いているタイルの分だけ要求する。仮想スクロールが可視範囲を持っているので
  // IntersectionObserver は要らない
  $effect(() => {
    const size = metrics.thumbnailSize;
    const start = range.startIndex;
    visible.forEach((image, offset) => {
      onRequestThumbnail(image.path, size, "discardable", start + offset);
    });
  });

  /**
   * グリッド内にフォーカスがあったかを、**DOM が入れ替わる前に**捕まえる。
   *
   * 仮想化でフォーカス中のタイルが取り除かれると、その瞬間に
   * `document.activeElement` は `body` に落ちる。`$effect`（DOM 更新の後）で
   * 見ても「元からグリッドの外にあった」と区別できないため、
   * 退避の判断ができない。`$effect.pre` は DOM 更新の前に走る。
   */
  let focusInside = false;
  $effect.pre(() => {
    void range.startIndex;
    void range.endIndex;
    void rovingIndex;
    const active = document.activeElement;
    focusInside = active instanceof HTMLElement && !!scroller?.contains(active);
  });

  /**
   * roving tabindex の実体（spec §4-1）。**tabindex の出し分けだけでは
   * DOM のフォーカスは動かない**ので、ここで実際に移す。
   *
   * - グリッド内にフォーカスがあったときだけ動かす（外にあるなら奪わない）
   * - `rovingIndex` のタイルが描画範囲にあればそこへ移す
   * - 仮想化で消えていればコンテナへ退避させる（＝従来の退避処理）
   *
   * 退避と移動を別々の `$effect` に分けると、同じ 1 回の範囲変化に対して
   * 両方が走って互いのフォーカスを奪い合うので、1 本にまとめる。
   */
  $effect(() => {
    const target = rovingIndex;
    // 範囲が動いてもタイルの入れ替わりを拾えるよう、明示的に依存させる
    void range.startIndex;
    void range.endIndex;
    if (!scroller || !focusInside) return;
    const tile = scroller.querySelector<HTMLElement>(`[data-index="${target}"]`);
    if (tile) {
      // preventScroll: スクロール位置は scrollIndexIntoView が決める。
      // ブラウザ既定のスクロールが入ると仮想化の行位置と食い違う
      if (tile !== document.activeElement) tile.focus({ preventScroll: true });
    } else if (listbox && document.activeElement !== listbox) {
      listbox.focus();
    }
  });

  function activate(image: ImageEntry) {
    if (selectionMode === "multi") {
      // 選択のトグル ＋ focusedPath の移動を同時に行う（spec §3-2）
      onToggleSelect(image);
    } else {
      onFocus(image);
    }
  }

  function moveFocus(delta: number) {
    if (images.length === 0) return;
    const from = focusedIndex < 0 ? range.startIndex : focusedIndex;
    const next = Math.min(images.length - 1, Math.max(0, from + delta));
    onFocus(images[next]);
    scrollIndexIntoView(next);
  }

  function scrollIndexIntoView(index: number) {
    if (!scroller || metrics.rowHeight <= 0) return;
    const row = Math.floor(index / metrics.columns);
    const top = row * metrics.rowHeight;
    const bottom = top + metrics.rowHeight;
    if (top < scroller.scrollTop) scroller.scrollTop = top;
    else if (bottom > scroller.scrollTop + viewportHeight) {
      scroller.scrollTop = bottom - viewportHeight;
    }
  }

  function handleKeydown(event: KeyboardEvent) {
    const current = focusedIndex < 0 ? null : images[focusedIndex];
    switch (event.key) {
      case "ArrowRight":
        event.preventDefault();
        moveFocus(1);
        break;
      case "ArrowLeft":
        event.preventDefault();
        moveFocus(-1);
        break;
      case "ArrowDown":
        event.preventDefault();
        moveFocus(metrics.columns);
        break;
      case "ArrowUp":
        event.preventDefault();
        moveFocus(-metrics.columns);
        break;
      case "Home":
        event.preventDefault();
        moveFocus(-images.length);
        break;
      case "End":
        event.preventDefault();
        moveFocus(images.length);
        break;
      case " ":
        // Space はクリックと同じ（spec §4-1）
        event.preventDefault();
        if (current) activate(current);
        break;
      case "Enter":
        // Enter は全画面プレビュー。現行と変わる点
        event.preventDefault();
        if (current) openPreview(current);
        break;
    }
  }

  function isSelected(image: ImageEntry): boolean {
    return selectionMode === "multi"
      ? selectedPaths.has(image.path)
      : focusedPath === image.path;
  }
</script>

<div class="photo-grid">
  <GridHeader
    totalCount={images.length}
    {selectedCount}
    {selectionMode}
    {rightPanelCollapsed}
    {onToggleRightPanel}
    {onClearSelection}
    {primaryAction}
  >
    {#snippet controls()}
      <div class="size">
        <Slider
          bind:value={targetTileWidth}
          label="サイズ"
          min={96}
          max={512}
          step={8}
          suffix="px"
        />
      </div>
    {/snippet}
  </GridHeader>

  <!-- 可視高はこの外枠で測る。**スクローラー自身の clientHeight を bind しては
       ならない** ── 仮想化の余白（padding-top / padding-bottom）はスクローラーに
       付くので、測る対象と変える対象が同じになり、
       「余白が伸びる → 測り直す → 描画範囲が変わる → 余白が縮む」の
       帰還路ができてフレームごとに振動する（実測: 可視範囲が
       [0,14] と [0,2993] を延々と往復し、毎回 3,000 件の要求を捨てる）。
       外枠には余白が付かないので高さが動かない -->
  <!-- **スクロールする箱と listbox を分ける。**
       仮想化の余白（padding-top / padding-bottom）はスクロールする箱には
       付けられない ── padding は要素自身の padding box を膨らませるので、
       `overflow-y: auto` の要素に 34 万 px の padding を付けると、
       その要素自体が 34 万 px の高さになってスクロールしなくなる
       （実測: `clientHeight` も 340,977px になり、可視高の測定も壊れる）。
       余白は内側の listbox に付け、スクロールと可視高は外側の箱が持つ。
       spec §4-1 の「スペーサー要素を listbox の直接の子にしてはならない」は
       これで満たされている（listbox の子は option だけ）。 -->
  <div
    class="scroller"
    bind:this={scroller}
    bind:clientHeight={viewportHeight}
    onscroll={(e) => (scrollTop = e.currentTarget.scrollTop)}
  >
    <div
      class="grid"
      role="listbox"
      aria-label="写真"
      aria-multiselectable={selectionMode === "multi"}
      tabindex="-1"
      bind:this={listbox}
      bind:clientWidth={containerWidth}
      onkeydown={handleKeydown}
      style:grid-template-columns="repeat({metrics.columns}, 1fr)"
      style:padding-top="{GRID_PADDING + range.paddingTop}px"
      style:padding-bottom="{GRID_PADDING + range.paddingBottom}px"
    >
      {#each visible as image, offset (image.path)}
        {@const index = range.startIndex + offset}
        {@const thumb = thumbnailFor(image.path, metrics.thumbnailSize)}
        <!-- キーボードは listbox（親）側の onkeydown が composite widget として
             一括で受ける。ARIA の listbox/option ではそれが正しい形で、
             option ごとに handler を重ねると同じキーが二重に走る。
             svelte は role="option" を対話的と見なさないので誤検出になる -->
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div
          class="tile state-layer"
          class:selected={isSelected(image)}
          class:focused={focusedPath === image.path}
          role="option"
          aria-selected={isSelected(image)}
          aria-setsize={images.length}
          aria-posinset={index + 1}
          aria-label={image.name}
          tabindex={index === rovingIndex ? 0 : -1}
          data-index={index}
          onclick={(e) => {
            // tabindex="-1" の要素はクリックでフォーカスされるが、エンジンによって
            // 挙動が違う（出荷先は WebKitGTK）。上の $effect が「グリッド内に
            // フォーカスがある」を前提にしているので、ここで確実に入れておく
            e.currentTarget.focus({ preventScroll: true });
            activate(image);
          }}
          ondblclick={(e) => {
            e.preventDefault();
            openPreview(image);
          }}
        >
          <div class="thumb">
            {#if thumb}
              <img src="data:image/jpeg;base64,{thumb}" alt="" />
            {:else}
              <div class="placeholder" aria-hidden="true">📷</div>
            {/if}
            {#if selectionMode === "multi" && selectedPaths.has(image.path)}
              <span class="check" aria-hidden="true">✓</span>
            {/if}
          </div>
          <span class="filename">{image.name}</span>
        </div>
      {/each}
    </div>
  </div>
</div>

{#if previewImage}
  <PhotoViewer
    image={previewImage}
    {images}
    {selectionMode}
    {selectedPaths}
    {thumbnailFor}
    {onRequestThumbnail}
    {onToggleSelect}
    onClose={() => (previewImage = null)}
    onNavigate={(img) => {
      previewImage = img;
      onFocus(img);
    }}
  />
{/if}

<style>
  .photo-grid {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
    background: var(--md-sys-color-surface);
  }

  .size {
    width: 140px;
  }

  /* スクロールする箱。**ここに padding を付けないこと**（上のコメント） */
  .scroller {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }

  /* gap と左右 padding は gridMetrics.ts の GRID_GAP / GRID_PADDING と
     同じ値であることが行位置の前提。片方だけ変えないこと */
  .grid {
    display: grid;
    gap: var(--space-2);
    align-content: start;
    padding-left: var(--space-3);
    padding-right: var(--space-3);
  }

  .tile {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-1);
    padding: var(--space-1);
    border-radius: var(--md-sys-shape-corner-sm);
    cursor: pointer;
    color: var(--md-sys-color-on-surface);
    /* 選択とフォーカスの枠がタイル幅を変えないよう、常に同じ太さの枠を持つ */
    border: 2px solid transparent;
  }

  .tile.selected {
    border-color: var(--md-sys-color-primary);
  }

  /* メタデータモードのフォーカスは太いアウトラインで示す（spec §4-3） */
  .tile.focused {
    border-color: var(--md-sys-color-primary);
    box-shadow: var(--md-sys-elevation-shadow-2);
  }

  .thumb {
    position: relative;
    width: 100%;
    aspect-ratio: 4 / 5;
    overflow: hidden;
    border-radius: var(--md-sys-shape-corner-sm);
    background: var(--md-sys-color-surface-container-high);
  }

  .thumb img {
    width: 100%;
    height: 100%;
    object-fit: contain;
  }

  .placeholder {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 100%;
    height: 100%;
    color: var(--md-sys-color-on-surface-variant);
    font-size: 24px;
  }

  /* サムネイルの選択チェックは PhotoGrid のローカル実装（spec §2）。
     写真の上に乗る円形マークで、汎用の部品にする理由が無い */
  .check {
    position: absolute;
    top: var(--space-1);
    right: var(--space-1);
    display: flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    border-radius: var(--md-sys-shape-corner-full);
    background: var(--md-sys-color-primary);
    color: var(--md-sys-color-on-primary);
    font: var(--md-sys-typescale-body-sm);
    font-weight: 700;
  }

  .filename {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font: var(--md-sys-typescale-body-sm);
    color: var(--md-sys-color-on-surface-variant);
  }
</style>
