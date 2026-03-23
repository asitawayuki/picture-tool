<script lang="ts">
  import type { ImageEntry } from "./types";

  interface Props {
    selectedImages: ImageEntry[];
    thumbnailCache: Map<string, string>;
    onRemove: (image: ImageEntry) => void;
    onRequestThumbnail: (path: string) => void;
    onPreview: (image: ImageEntry) => void;
  }

  let { selectedImages, thumbnailCache, onRemove, onRequestThumbnail, onPreview }: Props = $props();

  $effect(() => {
    for (const img of selectedImages) {
      if (!thumbnailCache.has(img.path)) {
        onRequestThumbnail(img.path);
      }
    }
  });
</script>

<div class="selection-list">
  <div class="header">選択済み ({selectedImages.length})</div>
  <div class="list">
    {#each selectedImages as image (image.path)}
      <div class="item" ondblclick={() => onPreview(image)}>
        <div class="thumb">
          {#if thumbnailCache.has(image.path)}
            <img
              src="data:image/jpeg;base64,{thumbnailCache.get(image.path)}"
              alt={image.name}
            />
          {:else}
            <div class="thumb-placeholder">📷</div>
          {/if}
        </div>
        <div class="info">
          <div class="name">{image.name}</div>
          <div class="meta">{image.width}×{image.height}</div>
        </div>
        <button class="remove" onclick={() => onRemove(image)}>×</button>
      </div>
    {/each}
  </div>
</div>

<style>
  .selection-list {
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .header {
    padding: 12px;
    color: var(--text-secondary);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 1px;
    border-bottom: 1px solid var(--border-color);
  }

  .list {
    flex: 1;
    overflow-y: auto;
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px;
    background: var(--accent-bg);
    border-radius: var(--radius);
  }

  .thumb {
    width: 40px;
    height: 50px;
    flex-shrink: 0;
    border-radius: var(--radius-sm);
    overflow: hidden;
    background: var(--bg-primary);
  }

  .thumb img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .thumb-placeholder {
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 16px;
  }

  .info {
    flex: 1;
    min-width: 0;
  }

  .name {
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .meta {
    font-size: 10px;
    color: var(--text-muted);
  }

  .remove {
    background: none;
    border: none;
    color: var(--text-muted);
    font-size: 16px;
    cursor: pointer;
    padding: 4px;
    line-height: 1;
  }

  .remove:hover {
    color: var(--danger);
  }
</style>
