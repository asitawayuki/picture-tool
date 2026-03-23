<script lang="ts">
  import { getFullImage } from "./api";
  import type { ImageEntry } from "./types";

  interface Props {
    image: ImageEntry;
    images: ImageEntry[];
    selectedPaths: Set<string>;
    onToggleSelect: (image: ImageEntry) => void;
    onClose: () => void;
    onNavigate: (image: ImageEntry) => void;
  }

  let { image, images, selectedPaths, onToggleSelect, onClose, onNavigate }: Props = $props();

  let fullImageData = $state<string | null>(null);
  let loading = $state(false);

  let currentIndex = $derived(images.findIndex((img) => img.path === image.path));
  let hasPrev = $derived(currentIndex > 0);
  let hasNext = $derived(currentIndex < images.length - 1);
  let isSelected = $derived(selectedPaths.has(image.path));

  $effect(() => {
    loadFullImage(image.path);
  });

  async function loadFullImage(path: string) {
    loading = true;
    fullImageData = null;
    try {
      const maxW = Math.min(window.innerWidth - 80, 2560);
      const maxH = Math.min(window.innerHeight - 120, 1600);
      fullImageData = await getFullImage(path, maxW, maxH);
    } catch (e) {
      console.error("Failed to load full image:", e);
    } finally {
      loading = false;
    }
  }

  function goPrev() {
    if (hasPrev) onNavigate(images[currentIndex - 1]);
  }

  function goNext() {
    if (hasNext) onNavigate(images[currentIndex + 1]);
  }

  function handleKeydown(e: KeyboardEvent) {
    switch (e.key) {
      case "ArrowLeft":
        e.preventDefault();
        goPrev();
        break;
      case "ArrowRight":
        e.preventDefault();
        goNext();
        break;
      case "Escape":
        e.preventDefault();
        onClose();
        break;
      case " ":
        e.preventDefault();
        onToggleSelect(image);
        break;
    }
  }

  function handleOverlayClick(e: MouseEvent) {
    if ((e.target as HTMLElement).classList.contains("preview-overlay")) {
      onClose();
    }
  }

  function formatSize(bytes: number): string {
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)}KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)}MB`;
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div
  class="preview-overlay"
  role="dialog"
  aria-modal="true"
  onclick={handleOverlayClick}
>
  <button
    class="select-btn"
    class:selected={isSelected}
    onclick={() => onToggleSelect(image)}
  >
    {#if isSelected}
      <span>✓ 選択済み</span>
    {:else}
      <span>○ 選択する</span>
    {/if}
  </button>

  <button class="close-btn" onclick={onClose}>✕</button>

  {#if hasPrev}
    <button class="nav-btn nav-prev" onclick={goPrev}>‹</button>
  {/if}
  {#if hasNext}
    <button class="nav-btn nav-next" onclick={goNext}>›</button>
  {/if}

  <div class="image-container">
    {#if loading}
      <div class="loading">読み込み中...</div>
    {:else if fullImageData}
      <img
        src="data:image/jpeg;base64,{fullImageData}"
        alt={image.name}
        class="preview-image"
      />
    {/if}
  </div>

  <div class="info-bar">
    <span>{image.name}</span>
    <span>{image.width} × {image.height} · {formatSize(image.size_bytes)}</span>
    <span>{currentIndex + 1} / {images.length}</span>
  </div>
</div>

<style>
  .preview-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.92);
    z-index: 200;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .select-btn {
    position: absolute;
    top: 16px;
    left: 16px;
    padding: 6px 14px;
    border-radius: 6px;
    font-size: 13px;
    cursor: pointer;
    z-index: 210;
    display: flex;
    align-items: center;
    gap: 6px;
    background: transparent;
    border: 1px solid rgba(255, 255, 255, 0.3);
    color: rgba(255, 255, 255, 0.7);
  }

  .select-btn.selected {
    background: var(--accent);
    border-color: var(--accent);
    color: white;
  }

  .select-btn:hover {
    background: var(--accent-hover);
    border-color: var(--accent-hover);
    color: white;
  }

  .close-btn {
    position: absolute;
    top: 16px;
    right: 16px;
    background: none;
    border: none;
    color: rgba(255, 255, 255, 0.7);
    font-size: 24px;
    cursor: pointer;
    z-index: 210;
    padding: 4px 8px;
  }

  .close-btn:hover {
    color: white;
  }

  .nav-btn {
    position: absolute;
    top: 50%;
    transform: translateY(-50%);
    background: none;
    border: none;
    color: rgba(255, 255, 255, 0.5);
    font-size: 48px;
    cursor: pointer;
    z-index: 210;
    padding: 16px;
    line-height: 1;
  }

  .nav-btn:hover {
    color: white;
  }

  .nav-prev {
    left: 8px;
  }

  .nav-next {
    right: 8px;
  }

  .image-container {
    max-width: calc(100vw - 120px);
    max-height: calc(100vh - 100px);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .preview-image {
    max-width: 100%;
    max-height: calc(100vh - 100px);
    object-fit: contain;
    border-radius: 4px;
  }

  .loading {
    color: rgba(255, 255, 255, 0.5);
    font-size: 16px;
  }

  .info-bar {
    position: absolute;
    bottom: 0;
    left: 0;
    right: 0;
    padding: 10px 20px;
    background: rgba(0, 0, 0, 0.6);
    display: flex;
    justify-content: space-between;
    color: rgba(255, 255, 255, 0.6);
    font-size: 12px;
    z-index: 210;
  }
</style>
