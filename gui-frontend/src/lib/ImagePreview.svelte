<script lang="ts">
  import { getFullImage, getExifInfo } from "./api";
  import type { ImageEntry, ExifInfo } from "./types";

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
  let exifInfo = $state<ExifInfo | null>(null);
  let imageElement: HTMLImageElement | undefined = $state();

  // ズーム状態
  let zoomed = $state(false);
  let zoomTransform = $state("");
  let selecting = $state(false);
  let selStart = $state({ x: 0, y: 0 });
  let selEnd = $state({ x: 0, y: 0 });

  let currentIndex = $derived(images.findIndex((img) => img.path === image.path));
  let hasPrev = $derived(currentIndex > 0);
  let hasNext = $derived(currentIndex < images.length - 1);
  let isSelected = $derived(selectedPaths.has(image.path));

  // 選択矩形（画像要素相対座標）
  let selectionRect = $derived.by(() => {
    if (!selecting) return null;
    const x = Math.min(selStart.x, selEnd.x);
    const y = Math.min(selStart.y, selEnd.y);
    const w = Math.abs(selEnd.x - selStart.x);
    const h = Math.abs(selEnd.y - selStart.y);
    if (w < 2 && h < 2) return null;
    return { x, y, w, h };
  });

  $effect(() => {
    loadFullImage(image.path);
    loadExifInfo(image.path);
  });

  $effect(() => {
    void image.path;
    resetZoom();
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

  async function loadExifInfo(path: string) {
    exifInfo = null;
    try {
      exifInfo = await getExifInfo(path);
    } catch (e) {
      console.error("Failed to load EXIF info:", e);
    }
  }

  function formatExifLine1(info: ExifInfo): string | null {
    const parts: string[] = [];
    const camera = [info.camera_make, info.camera_model].filter(Boolean).join(" ");
    if (camera) parts.push(camera);
    if (info.lens_model) parts.push(info.lens_model);
    return parts.length > 0 ? parts.join(" | ") : null;
  }

  function formatExifLine2(info: ExifInfo): string | null {
    const parts: string[] = [];
    if (info.focal_length) parts.push(info.focal_length);
    if (info.f_number) parts.push(info.f_number);
    if (info.shutter_speed) parts.push(info.shutter_speed);
    if (info.iso != null) parts.push(`ISO ${info.iso}`);
    return parts.length > 0 ? parts.join("  ") : null;
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
        if (zoomed) {
          resetZoom();
        } else {
          onClose();
        }
        break;
      case " ":
        e.preventDefault();
        onToggleSelect(image);
        break;
    }
  }

  function handleOverlayClick(e: MouseEvent) {
    if ((e.target as HTMLElement).classList.contains("preview-overlay")) {
      if (zoomed) {
        resetZoom();
      } else {
        onClose();
      }
    }
  }

  function formatSize(bytes: number): string {
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)}KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)}MB`;
  }

  function resetZoom() {
    zoomed = false;
    zoomTransform = "";
    selecting = false;
  }

  function handleImageMouseDown(e: MouseEvent) {
    if (zoomed || !imageElement) return;
    e.preventDefault();
    const rect = imageElement.getBoundingClientRect();
    const pos = { x: e.clientX - rect.left, y: e.clientY - rect.top };
    selStart = pos;
    selEnd = { ...pos };
    selecting = true;
  }

  function handleMouseMove(e: MouseEvent) {
    if (!selecting || !imageElement) return;
    const rect = imageElement.getBoundingClientRect();
    selEnd = {
      x: Math.max(0, Math.min(e.clientX - rect.left, rect.width)),
      y: Math.max(0, Math.min(e.clientY - rect.top, rect.height)),
    };
  }

  function handleMouseUp(_e: MouseEvent) {
    if (!selecting || !imageElement) return;
    selecting = false;

    const rect = imageElement.getBoundingClientRect();
    const sw = Math.abs(selEnd.x - selStart.x);
    const sh = Math.abs(selEnd.y - selStart.y);

    // ドラッグ距離が小さすぎる場合は無視
    if (sw < 15 || sh < 15) return;

    const sx = Math.min(selStart.x, selEnd.x);
    const sy = Math.min(selStart.y, selEnd.y);

    // コンテナサイズ = 画像の描画サイズ
    const containerW = rect.width;
    const containerH = rect.height;
    const scale = Math.min(containerW / sw, containerH / sh);

    // 選択領域の中心が画面中央に来るように移動
    const selCenterX = sx + sw / 2;
    const selCenterY = sy + sh / 2;
    const tx = containerW / 2 - selCenterX * scale;
    const ty = containerH / 2 - selCenterY * scale;

    zoomTransform = `translate(${tx}px, ${ty}px) scale(${scale})`;
    zoomed = true;
  }

  function handleZoomedClick(e: MouseEvent) {
    e.stopPropagation();
    resetZoom();
  }
</script>

<svelte:window
  onkeydown={handleKeydown}
  onmousemove={handleMouseMove}
  onmouseup={handleMouseUp}
/>

<div
  class="preview-overlay"
  role="dialog"
  aria-modal="true"
  aria-label="画像プレビュー"
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

  {#if exifInfo && (formatExifLine1(exifInfo) || formatExifLine2(exifInfo))}
    <div class="exif-overlay">
      {#if formatExifLine1(exifInfo)}
        <div class="exif-line">{formatExifLine1(exifInfo)}</div>
      {/if}
      {#if formatExifLine2(exifInfo)}
        <div class="exif-line">{formatExifLine2(exifInfo)}</div>
      {/if}
    </div>
  {/if}

  {#if hasPrev}
    <button class="nav-btn nav-prev" onclick={goPrev}>‹</button>
  {/if}
  {#if hasNext}
    <button class="nav-btn nav-next" onclick={goNext}>›</button>
  {/if}

  <div class="image-container" class:zoomed>
    {#if loading}
      <div class="loading">読み込み中...</div>
    {:else if fullImageData}
      {#if zoomed}
        <!-- ズーム時: クリックでリセット -->
        <img
          bind:this={imageElement}
          src="data:image/jpeg;base64,{fullImageData}"
          alt={image.name}
          class="preview-image zoomed"
          style="transform-origin: 0 0; transform: {zoomTransform};"
          onclick={handleZoomedClick}
        />
      {:else}
        <!-- 通常時: ドラッグで範囲選択 -->
        <img
          bind:this={imageElement}
          src="data:image/jpeg;base64,{fullImageData}"
          alt={image.name}
          class="preview-image"
          onmousedown={handleImageMouseDown}
        />
        {#if selectionRect}
          <div
            class="selection-rect"
            style="left: {selectionRect.x}px; top: {selectionRect.y}px; width: {selectionRect.w}px; height: {selectionRect.h}px;"
          ></div>
        {/if}
      {/if}
    {/if}
  </div>

  <div class="info-bar">
    <span>{image.name}</span>
    <span>
      {image.width} × {image.height} · {formatSize(image.size_bytes)}{#if exifInfo?.date_taken} · {exifInfo.date_taken}{/if}
    </span>
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
    position: relative;
    max-width: calc(100vw - 120px);
    max-height: calc(100vh - 100px);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .image-container.zoomed {
    overflow: hidden;
  }

  .preview-image {
    max-width: 100%;
    max-height: calc(100vh - 100px);
    object-fit: contain;
    border-radius: 4px;
    cursor: crosshair;
    user-select: none;
    -webkit-user-drag: none;
  }

  .preview-image.zoomed {
    cursor: zoom-out;
  }

  .selection-rect {
    position: absolute;
    border: 2px dashed rgba(255, 255, 255, 0.8);
    background: rgba(255, 255, 255, 0.08);
    pointer-events: none;
    box-shadow: 0 0 0 9999px rgba(0, 0, 0, 0.4);
  }

  .loading {
    color: rgba(255, 255, 255, 0.5);
    font-size: 16px;
  }

  .exif-overlay {
    position: absolute;
    top: 56px;
    left: 16px;
    z-index: 210;
    pointer-events: none;
  }

  .exif-line {
    color: rgba(255, 255, 255, 0.85);
    font-size: 12px;
    line-height: 1.5;
    text-shadow: 0 1px 3px rgba(0, 0, 0, 0.8), 0 0 6px rgba(0, 0, 0, 0.5);
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
