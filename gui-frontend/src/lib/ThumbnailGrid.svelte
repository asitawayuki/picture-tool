<script lang="ts">
  import type { ImageEntry } from "./types";

  interface Props {
    images: ImageEntry[];
    selectedPaths: Set<string>;
    /** サムネイルは解像度ごとに別物なので path と maxDimension の両方で引く */
    thumbnailFor: (path: string, maxDimension: number) => string | undefined;
    currentPage: number;
    onToggleSelect: (image: ImageEntry) => void;
    onRequestThumbnail: (path: string, maxDimension: number) => void;
    onPreview: (image: ImageEntry) => void;
    onPageChange: (page: number) => void;
  }

  let { images, selectedPaths, thumbnailFor, currentPage, onToggleSelect, onRequestThumbnail, onPreview, onPageChange }: Props = $props();

  const PAGE_SIZE = 50;
  let columnCount = $state(4);
  let gridElement: HTMLDivElement | undefined = $state();
  // 生の列幅をそのままキャッシュキーにすると 1px の差で別エントリになるため、
  // 64px 刻みに丸めて再利用できるようにする。
  const SIZE_STEP = 64;
  const MIN_SIZE = 96;
  const MAX_SIZE = 512;

  let thumbSize = $derived.by(() => {
    const containerWidth = gridElement?.clientWidth ?? window.innerWidth * 0.5;
    const raw = containerWidth / columnCount;
    const stepped = Math.ceil(raw / SIZE_STEP) * SIZE_STEP;
    return Math.min(MAX_SIZE, Math.max(MIN_SIZE, stepped));
  });

  let pagedImages = $derived(
    images.slice(currentPage * PAGE_SIZE, (currentPage + 1) * PAGE_SIZE)
  );
  let totalPages = $derived(Math.ceil(images.length / PAGE_SIZE));

  /**
   * 画面に入った時点でサムネイルを要求する。
   * 列数を変えて要求解像度が上がった場合は、既に表示済みでも取り直す
   * （そうしないと低解像度のまま引き伸ばされたままになる）。
   */
  function observeThumbnail(node: HTMLElement, params: { path: string; size: number }) {
    let observer: IntersectionObserver | null = null;
    let intersected = false;
    let current = params;

    function start(next: { path: string; size: number }) {
      current = next;
      if (intersected) {
        onRequestThumbnail(next.path, next.size);
        return;
      }
      observer?.disconnect();
      observer = new IntersectionObserver(
        (entries) => {
          if (!entries[0].isIntersecting) return;
          intersected = true;
          onRequestThumbnail(current.path, current.size);
          observer?.disconnect();
          observer = null;
        },
        { rootMargin: "200px" }
      );
      observer.observe(node);
    }

    start(params);

    return {
      update(next: { path: string; size: number }) {
        if (next.path === current.path && next.size === current.size) return;
        if (next.path !== current.path) intersected = false;
        start(next);
      },
      destroy() {
        observer?.disconnect();
      },
    };
  }
</script>

<div class="thumbnail-grid">
  <div class="grid-header">
    <span class="count">{images.length} 枚</span>
    <div class="toolbar-right">
      <div class="size-control">
        <label class="size-label" for="grid-columns">列</label>
        <input
          id="grid-columns"
          type="range"
          min="2"
          max="8"
          bind:value={columnCount}
          class="size-slider"
        />
      </div>
      {#if totalPages > 1}
        <div class="pagination">
          <button
            aria-label="前のページ"
            onclick={() => onPageChange(Math.max(0, currentPage - 1))}
            disabled={currentPage === 0}>←</button>
          <span>{currentPage + 1} / {totalPages}</span>
          <button
            aria-label="次のページ"
            onclick={() => onPageChange(Math.min(totalPages - 1, currentPage + 1))}
            disabled={currentPage >= totalPages - 1}>→</button>
        </div>
      {/if}
    </div>
  </div>

  <div class="grid" bind:this={gridElement} style="grid-template-columns: repeat({columnCount}, 1fr);">
    {#each pagedImages as image (image.path)}
      {@const thumb = thumbnailFor(image.path, thumbSize)}
      <button
        class="grid-item"
        class:selected={selectedPaths.has(image.path)}
        aria-pressed={selectedPaths.has(image.path)}
        onclick={() => onToggleSelect(image)}
        ondblclick={(e) => { e.preventDefault(); onPreview(image); }}
        use:observeThumbnail={{ path: image.path, size: thumbSize }}
      >
        <div class="thumb-wrapper">
          {#if thumb}
            <img src="data:image/jpeg;base64,{thumb}" alt={image.name} />
          {:else}
            <div class="placeholder">📷</div>
          {/if}
          {#if selectedPaths.has(image.path)}
            <span class="check">✓</span>
          {/if}
        </div>
        <span class="filename">{image.name}</span>
      </button>
    {/each}
  </div>
</div>

<style>
  .thumbnail-grid {
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--bg-primary);
    overflow: hidden;
  }

  .grid-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 8px 12px;
    color: var(--text-secondary);
    font-size: 11px;
    border-bottom: 1px solid var(--border-color);
  }

  .pagination {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .pagination button {
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    color: var(--text-primary);
    padding: 2px 8px;
    border-radius: var(--radius-sm);
    cursor: pointer;
  }

  .pagination button:disabled {
    opacity: 0.3;
    cursor: default;
  }

  .grid {
    flex: 1;
    overflow-y: auto;
    padding: 12px;
    display: grid;
    gap: 8px;
    align-content: start;
  }

  .grid-item {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    padding: 4px;
    border: 2px solid transparent;
    border-radius: var(--radius);
    background: none;
    cursor: pointer;
    color: var(--text-primary);
  }

  .grid-item:hover {
    background: var(--bg-hover);
  }

  .grid-item.selected {
    border-color: var(--accent);
  }

  .thumb-wrapper {
    position: relative;
    width: 100%;
    aspect-ratio: 4 / 5;
    border-radius: var(--radius-sm);
    overflow: hidden;
    background: var(--bg-secondary);
  }

  .thumb-wrapper img {
    width: 100%;
    height: 100%;
    object-fit: contain;
  }

  .placeholder {
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 24px;
    color: var(--text-muted);
  }

  .check {
    position: absolute;
    top: 4px;
    right: 4px;
    background: var(--accent);
    color: white;
    border-radius: 50%;
    width: 20px;
    height: 20px;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 11px;
    font-weight: bold;
  }

  .filename {
    font-size: 11px;
    color: var(--text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 100%;
  }

  .toolbar-right {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .size-control {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .size-label {
    font-size: 12px;
  }

  .size-slider {
    width: 80px;
    height: 16px;
    -webkit-appearance: none;
    appearance: none;
    background: transparent;
    cursor: pointer;
    padding: 0;
    margin: 0;
  }

  .size-slider::-webkit-slider-runnable-track {
    height: 3px;
    background: #555;
    border-radius: 2px;
  }

  .size-slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--accent);
    border: none;
    margin-top: -5px;
    cursor: pointer;
  }

  .size-slider::-moz-range-track {
    height: 3px;
    background: #555;
    border-radius: 2px;
  }

  .size-slider::-moz-range-thumb {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--accent);
    border: none;
    cursor: pointer;
  }
</style>
