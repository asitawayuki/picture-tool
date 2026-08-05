<script lang="ts">
  import type { ImageEntry } from "./types";

  interface Props {
    selectedImages: ImageEntry[];
    thumbnailFor: (path: string, maxDimension: number) => string | undefined;
    onRemove: (image: ImageEntry) => void;
    onRequestThumbnail: (path: string, maxDimension: number) => void;
    onPreview: (image: ImageEntry) => void;
  }

  let { selectedImages, thumbnailFor, onRemove, onRequestThumbnail, onPreview }: Props = $props();

  const THUMB_SIZE = 200;

  // キャッシュを読むと「サムネイル1枚届くたびに選択済み全件を走査」になるため、
  // 依頼済みかどうかはこのコンポーネント側の非リアクティブな Set で覚える。
  // 重複要求は onRequestThumbnail 側でも弾かれる。
  const requestedPaths = new Set<string>();

  $effect(() => {
    for (const img of selectedImages) {
      if (requestedPaths.has(img.path)) continue;
      requestedPaths.add(img.path);
      onRequestThumbnail(img.path, THUMB_SIZE);
    }
  });
</script>

<div class="selection-list">
  <div class="header">選択済み ({selectedImages.length})</div>
  <div class="list">
    {#each selectedImages as image (image.path)}
      {@const thumb = thumbnailFor(image.path, THUMB_SIZE)}
      <div class="item">
        <button
          class="open"
          aria-label="{image.name} をプレビュー"
          ondblclick={() => onPreview(image)}
          onkeydown={(e) => { if (e.key === "Enter") onPreview(image); }}
        >
          <span class="thumb">
            {#if thumb}
              <img src="data:image/jpeg;base64,{thumb}" alt="" />
            {:else}
              <span class="thumb-placeholder" aria-hidden="true">📷</span>
            {/if}
          </span>
          <span class="info">
            <span class="name">{image.name}</span>
            <span class="meta">{image.width}×{image.height}</span>
          </span>
        </button>
        <button class="remove" aria-label="{image.name} を選択から外す" onclick={() => onRemove(image)}>×</button>
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
    gap: 4px;
    padding: 6px;
    background: var(--accent-bg);
    border-radius: var(--radius);
  }

  .open {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: 1;
    min-width: 0;
    background: none;
    border: none;
    padding: 0;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .thumb {
    display: block;
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
    display: block;
    flex: 1;
    min-width: 0;
  }

  .name {
    display: block;
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .meta {
    display: block;
    font-size: 11px;
    color: var(--text-secondary);
  }

  .remove {
    flex-shrink: 0;
    background: none;
    border: none;
    color: var(--text-secondary);
    font-size: 16px;
    cursor: pointer;
    padding: 4px;
    line-height: 1;
  }

  .remove:hover {
    color: var(--danger);
  }
</style>
