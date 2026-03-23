<script lang="ts">
  import type { ImageEntry } from "./types";

  interface Props {
    images: ImageEntry[];
    selectedPaths: Set<string>;
    thumbnailCache: Map<string, string>;
    onToggleSelect: (image: ImageEntry) => void;
    onRequestThumbnail: (path: string) => void;
  }

  let { images, selectedPaths, thumbnailCache, onToggleSelect, onRequestThumbnail }: Props = $props();

  const PAGE_SIZE = 50;
  let currentPage = $state(0);
  let columnCount = $state(4);

  let pagedImages = $derived(
    images.slice(currentPage * PAGE_SIZE, (currentPage + 1) * PAGE_SIZE)
  );
  let totalPages = $derived(Math.ceil(images.length / PAGE_SIZE));

  $effect(() => {
    images;
    currentPage = 0;
  });

  function observeThumbnail(node: HTMLElement, path: string) {
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries[0].isIntersecting) {
          onRequestThumbnail(path);
          observer.disconnect();
        }
      },
      { rootMargin: "200px" }
    );
    observer.observe(node);
    return {
      destroy() {
        observer.disconnect();
      },
    };
  }
</script>

<div class="thumbnail-grid">
  <div class="grid-header">
    <span class="count">{images.length} 枚</span>
    <div class="toolbar-right">
      <div class="size-control">
        <span class="size-label">🖼</span>
        <input
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
            onclick={() => (currentPage = Math.max(0, currentPage - 1))}
            disabled={currentPage === 0}>←</button>
          <span>{currentPage + 1} / {totalPages}</span>
          <button
            onclick={() => (currentPage = Math.min(totalPages - 1, currentPage + 1))}
            disabled={currentPage >= totalPages - 1}>→</button>
        </div>
      {/if}
    </div>
  </div>

  <div class="grid" style="grid-template-columns: repeat({columnCount}, 1fr);">
    {#each pagedImages as image (image.path)}
      <button
        class="grid-item"
        class:selected={selectedPaths.has(image.path)}
        onclick={() => onToggleSelect(image)}
        use:observeThumbnail={image.path}
      >
        <div class="thumb-wrapper">
          {#if thumbnailCache.has(image.path)}
            <img
              src="data:image/jpeg;base64,{thumbnailCache.get(image.path)}"
              alt={image.name}
            />
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
    object-fit: cover;
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
    font-size: 10px;
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

  .size-slider::-webkit-slider-track {
    height: 3px;
    background: var(--border-color);
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
    background: var(--border-color);
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
