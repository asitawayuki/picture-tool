<script lang="ts">
  import { getFullImage, getExifInfo } from "../api";
  import { focusTrap } from "../focusTrap";
  import { toast, describeError } from "../toasts.svelte";
  import type { RequestKind } from "./requestQueue";
  import type { ImageEntry, ExifInfo } from "../types";

  interface Props {
    image: ImageEntry;
    images: ImageEntry[];
    /** multi: 変換モード（選択できる） / single: メタデータモード（選択の概念が無い） */
    selectionMode: "multi" | "single";
    selectedPaths: Set<string>;
    thumbnailFor: (path: string, size: number) => string | undefined;
    onRequestThumbnail: (
      path: string,
      size: number,
      kind: RequestKind,
      index: number
    ) => void;
    onToggleSelect: (image: ImageEntry) => void;
    onClose: () => void;
    onNavigate: (image: ImageEntry) => void;
  }

  let {
    image,
    images,
    selectionMode,
    selectedPaths,
    thumbnailFor,
    onRequestThumbnail,
    onToggleSelect,
    onClose,
    onNavigate,
  }: Props = $props();

  let fullImageData = $state<string | null>(null);
  let loading = $state(false);
  let exifInfo = $state<ExifInfo | null>(null);
  let imageElement: HTMLImageElement | undefined = $state();

  /** フィルムストリップの高さ。4:5 なので幅はこの 0.8 倍 */
  const STRIP_THUMB = 96;
  /** 現在位置の前後どれだけを要求するか。全部要求すると 3,000 枚分の IPC が走る */
  const STRIP_WINDOW = 20;

  let stripElement: HTMLDivElement | undefined = $state();
  /** 下端（情報バー＋ストリップ）の実高。写真の最大高をここから決める */
  let bottomHeight = $state(0);

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

  // 矢印キーの高速ナビで、古い画像／EXIF の応答が新しい表示を上書きしないよう
  // リクエストごとにトークンを振り、最新のものだけを state に反映する。
  let loadToken = 0;
  let imageErrorReported = false;

  $effect(() => {
    const path = image.path;
    const token = ++loadToken;
    loadFullImage(path, token);
    loadExifInfo(path, token);
  });

  $effect(() => {
    void image.path;
    resetZoom();
  });

  async function loadFullImage(path: string, token: number) {
    loading = true;
    fullImageData = null;
    try {
      const maxW = Math.min(window.innerWidth - 80, 2560);
      const maxH = Math.min(window.innerHeight - 120, 1600);
      const data = await getFullImage(path, maxW, maxH);
      if (token !== loadToken) return;
      fullImageData = data;
    } catch (e) {
      if (token !== loadToken) return;
      // 連続ナビで壊れた画像が並ぶとトーストで埋まるため最初の1件だけ通知する
      if (!imageErrorReported) {
        imageErrorReported = true;
        toast.error(`画像を表示できません: ${describeError(e)}`);
      }
    } finally {
      if (token === loadToken) loading = false;
    }
  }

  async function loadExifInfo(path: string, token: number) {
    exifInfo = null;
    try {
      const info = await getExifInfo(path);
      if (token !== loadToken) return;
      exifInfo = info;
    } catch {
      // EXIF は無くても表示は成立するので通知しない
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
        // メタデータモードに「選択」は無い（spec §3-2）
        if (selectionMode !== "multi") return;
        // ボタンにフォーカスがある時はボタン既定の動作に任せる（二重トグル防止）
        if ((e.target as HTMLElement | null)?.tagName === "BUTTON") return;
        e.preventDefault();
        onToggleSelect(image);
        break;
    }
  }

  function handleBackdropClick() {
    if (zoomed) {
      resetZoom();
    } else {
      onClose();
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

  // 現在位置の前後だけを pinned で要求する。グリッドの可視範囲に入らないので
  // discardable にすると捨てられて埋まらない（spec §4-2）
  $effect(() => {
    const from = Math.max(0, currentIndex - STRIP_WINDOW);
    const to = Math.min(images.length - 1, currentIndex + STRIP_WINDOW);
    for (let i = from; i <= to; i++) {
      onRequestThumbnail(images[i].path, STRIP_THUMB, "pinned", -1);
    }
  });

  // 送るたびに現在位置をストリップの中央へ寄せる。
  // ストリップ内にフォーカスがあるときは、フォーカスも一緒に運ぶ
  // （roving tabindex。PhotoGrid と同じ理由。tabindex の出し分けだけでは
  //  DOM のフォーカスが前の枠に取り残される）
  $effect(() => {
    void image.path;
    const current = stripElement?.querySelector<HTMLElement>('[aria-current="true"]');
    if (!current) return;
    current.scrollIntoView({ block: "nearest", inline: "center" });
    const active = document.activeElement;
    if (
      active instanceof HTMLElement &&
      stripElement?.contains(active) &&
      active !== current
    ) {
      current.focus({ preventScroll: true });
    }
  });
</script>

<svelte:window
  onkeydown={handleKeydown}
  onmousemove={handleMouseMove}
  onmouseup={handleMouseUp}
/>

<div
  class="preview-overlay"
  style:--preview-max-h="calc(100vh - {bottomHeight + 32}px)"
  style:padding-bottom="{bottomHeight}px"
  role="dialog"
  aria-modal="true"
  aria-label="画像プレビュー"
  tabindex="-1"
  use:focusTrap
>
  <!-- 余白クリックで閉じるための背景。ダイアログ本体にクリックハンドラーを
       付けるとキーボード操作を持たない対話要素になるため分離する。
       キーボードからは Escape で閉じられる。 -->
  <div class="backdrop" role="presentation" onclick={handleBackdropClick}></div>

  {#if selectionMode === "multi"}
    <button
      class="select-btn state-layer"
      class:selected={isSelected}
      onclick={() => onToggleSelect(image)}
    >
      {#if isSelected}
        <span>✓ 選択済み</span>
      {:else}
        <span>○ 選択する</span>
      {/if}
    </button>
  {/if}

  <button class="close-btn state-layer" aria-label="閉じる" onclick={onClose}>✕</button>

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
    <button class="nav-btn nav-prev state-layer" aria-label="前の写真" onclick={goPrev}>‹</button>
  {/if}
  {#if hasNext}
    <button class="nav-btn nav-next state-layer" aria-label="次の写真" onclick={goNext}>›</button>
  {/if}

  <div class="image-container" class:zoomed>
    {#if loading}
      <div class="loading">読み込み中...</div>
    {:else if fullImageData}
      <!-- ズーム操作はポインタ専用の補助機能。画像そのものに手を付けず、
           前面の操作面でマウスイベントを受ける（キーボードからは Escape で解除、
           矢印キーで前後移動できる）。 -->
      {#if zoomed}
        <div class="zoom-surface" role="presentation" onclick={handleZoomedClick}>
          <img
            bind:this={imageElement}
            src="data:image/jpeg;base64,{fullImageData}"
            alt={image.name}
            class="preview-image zoomed"
            style="transform-origin: 0 0; transform: {zoomTransform};"
          />
        </div>
      {:else}
        <div class="zoom-surface" role="presentation" onmousedown={handleImageMouseDown}>
          <img
            bind:this={imageElement}
            src="data:image/jpeg;base64,{fullImageData}"
            alt={image.name}
            class="preview-image"
          />
          {#if selectionRect}
            <div
              class="selection-rect"
              style="left: {selectionRect.x}px; top: {selectionRect.y}px; width: {selectionRect.w}px; height: {selectionRect.h}px;"
            ></div>
          {/if}
        </div>
      {/if}
    {/if}
  </div>

  <!-- 情報バーとフィルムストリップは 1 つの箱に積む。どちらも bottom: 0 の
       絶対配置にすると重なるため。高さを測って写真の最大高に反映する -->
  <div class="bottom" bind:clientHeight={bottomHeight}>
    <div class="info-bar">
      <span>{image.name}</span>
      <span>
        {image.width} × {image.height} · {formatSize(image.size_bytes)}{#if exifInfo?.date_taken} · {exifInfo.date_taken}{/if}
      </span>
      <span class="position">{currentIndex + 1} / {images.length}</span>
    </div>

    <!-- role="list" は付けない。付けると子に role="listitem" が要り、
         それは button ロールを上書きしてしまう（「押せるもの」として
         支援技術に伝わらなくなる）。枚数と位置は各ボタンの aria-label が持つ -->
    <div class="strip" aria-label="フィルムストリップ" bind:this={stripElement}>
      {#each images as item, index (item.path)}
        {@const thumb = thumbnailFor(item.path, STRIP_THUMB)}
        {@const current = item.path === image.path}
        <button
          class="frame state-layer"
          class:current
          class:selected={selectionMode === "multi" && selectedPaths.has(item.path)}
          type="button"
          aria-current={current}
          aria-label="{index + 1} 枚目 {item.name}"
          tabindex={current ? 0 : -1}
          onclick={() => onNavigate(item)}
        >
          {#if thumb}
            <img src="data:image/jpeg;base64,{thumb}" alt="" />
          {/if}
        </button>
      {/each}
    </div>
  </div>
</div>

<style>
  /* 写真を見るための面なので、下地は常に scrim（黒）で暗くする。
     その上に浮く操作は「地の色を持つチップ」にしてある ── ライト／ダークの
     どちらでも読めるのは inverse-surface / inverse-on-surface の対だけで、
     scrim の上に素の文字を置くとダークで沈む */
  .preview-overlay {
    position: fixed;
    inset: 0;
    z-index: 200;
    display: flex;
    align-items: center;
    justify-content: center;
    /* 下端の箱の高さぶんを空ける。写真は「残りの領域」の中央に来る。
       これが無いと画面全体の中央に置かれ、下半分が箱の裏に隠れる */
    box-sizing: border-box;
  }

  /* 塗りと「余白クリックで閉じる」を 1 要素が持つ。
     Dialog と同じく color-mix を使わずに不透明度を持たせる */
  .backdrop {
    position: absolute;
    inset: 0;
    z-index: 0;
    background: var(--md-sys-color-scrim);
    opacity: 0.92;
  }

  .select-btn,
  .close-btn,
  .nav-btn,
  .loading,
  .exif-overlay {
    background: var(--md-sys-color-inverse-surface);
    color: var(--md-sys-color-inverse-on-surface);
    border: none;
  }

  .select-btn {
    position: absolute;
    top: var(--space-4);
    left: var(--space-4);
    padding: var(--space-2) var(--space-3);
    border-radius: var(--md-sys-shape-corner-full);
    font: var(--md-sys-typescale-label-lg);
    cursor: pointer;
    z-index: 210;
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .select-btn.selected {
    background: var(--md-sys-color-primary);
    color: var(--md-sys-color-on-primary);
  }

  .close-btn {
    position: absolute;
    top: var(--space-4);
    right: var(--space-4);
    width: 40px;
    height: 40px;
    border-radius: var(--md-sys-shape-corner-full);
    font-size: 20px;
    line-height: 1;
    cursor: pointer;
    z-index: 210;
  }

  .nav-btn {
    position: absolute;
    top: 50%;
    transform: translateY(-50%);
    width: 48px;
    height: 48px;
    border-radius: var(--md-sys-shape-corner-full);
    font-size: 32px;
    line-height: 1;
    cursor: pointer;
    z-index: 210;
    padding: 0;
  }

  .nav-prev {
    left: var(--space-3);
  }

  .nav-next {
    right: var(--space-3);
  }

  .image-container {
    position: relative;
    z-index: 1;
    max-width: calc(100vw - 120px);
    /* 下端（情報バー＋フィルムストリップ）の実測高だけ空ける。
       固定値にすると、ストリップの高さを変えたときに写真が隠れる */
    max-height: var(--preview-max-h);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .image-container.zoomed {
    overflow: hidden;
  }

  /* 画像の描画ボックスにぴったり重なるよう縮小し、選択矩形の基準にもする */
  .zoom-surface {
    position: relative;
    display: flex;
    max-width: 100%;
    max-height: var(--preview-max-h);
  }

  .preview-image {
    max-width: 100%;
    max-height: var(--preview-max-h);
    object-fit: contain;
    border-radius: var(--md-sys-shape-corner-xs);
    cursor: crosshair;
    user-select: none;
    -webkit-user-drag: none;
  }

  .preview-image.zoomed {
    cursor: zoom-out;
  }

  .selection-rect {
    position: absolute;
    border: 2px dashed var(--md-sys-color-primary);
    pointer-events: none;
  }

  /* 内側の薄い塗りと外側の暗転。不透明度を疑似要素側に持たせることで、
     破線そのものは実線の primary のまま残る（要素に opacity を掛けると
     枠まで薄くなって見えなくなる） */
  .selection-rect::before,
  .selection-rect::after {
    content: "";
    position: absolute;
    inset: 0;
  }

  .selection-rect::before {
    background: var(--md-sys-color-primary-container);
    opacity: 0.2;
  }

  .selection-rect::after {
    box-shadow: 0 0 0 9999px var(--md-sys-color-scrim);
    opacity: 0.4;
  }

  .loading {
    padding: var(--space-2) var(--space-4);
    border-radius: var(--md-sys-shape-corner-full);
    font: var(--md-sys-typescale-body-md);
  }

  .exif-overlay {
    position: absolute;
    top: 68px;
    left: var(--space-4);
    z-index: 210;
    padding: var(--space-2) var(--space-3);
    border-radius: var(--md-sys-shape-corner-sm);
    pointer-events: none;
  }

  .exif-line {
    font: var(--md-sys-typescale-body-sm);
  }

  /* 情報バーとフィルムストリップの箱。写真の下地（scrim）ではなく
     アプリの面色を持たせる ── ここは写真ではなく操作の領域 */
  .bottom {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    z-index: 210;
    background: var(--md-sys-color-surface-container);
    border-top: 1px solid var(--md-sys-color-outline-variant);
  }

  .info-bar {
    display: flex;
    justify-content: space-between;
    gap: var(--space-4);
    padding: var(--space-2) var(--space-4);
    font: var(--md-sys-typescale-body-sm);
    color: var(--md-sys-color-on-surface-variant);
  }

  .position {
    font-variant-numeric: tabular-nums;
  }

  .strip {
    display: flex;
    gap: var(--space-1);
    overflow-x: auto;
    padding: 0 var(--space-3) var(--space-2);
  }

  .frame {
    flex-shrink: 0;
    width: 77px; /* 96 * 4/5 */
    height: 96px;
    padding: 0;
    border: 2px solid transparent;
    border-radius: var(--md-sys-shape-corner-xs);
    background: var(--md-sys-color-surface-container-high);
    cursor: pointer;
    overflow: hidden;
  }

  .frame.selected {
    border-color: var(--md-sys-color-primary-container);
  }

  .frame.current {
    border-color: var(--md-sys-color-primary);
  }

  .frame img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
</style>
