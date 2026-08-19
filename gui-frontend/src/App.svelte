<script lang="ts">
  import { onMount } from "svelte";
  import FolderTree from "./lib/FolderTree.svelte";
  import ThumbnailGrid from "./lib/ThumbnailGrid.svelte";
  import SelectionList from "./lib/SelectionList.svelte";
  import SettingsPanel from "./lib/SettingsPanel.svelte";
  import ProgressOverlay from "./lib/ProgressOverlay.svelte";
  import ImagePreview from "./lib/ImagePreview.svelte";
  import ExifFrameSettings from "./lib/ExifFrameSettings.svelte";
  import Dialog from "./lib/ui/Dialog.svelte";
  import Button from "./lib/ui/Button.svelte";
  import ResultDialog from "./lib/ResultDialog.svelte";
  import Toast from "./lib/Toast.svelte";
  import { toast, describeError } from "./lib/toasts.svelte";
  import { listImages, pickOutputFolder } from "./lib/api";
  import { createThumbnailQueue } from "./lib/browser/thumbnailQueue.svelte";
  import { createPresetStore } from "./lib/panels/presets.svelte";
  import { createConvertRun } from "./lib/panels/convertRun.svelte";
  import type { ImageEntry, ProcessingConfig } from "./lib/types";

  // --- 状態 ---
  let images = $state<ImageEntry[]>([]);
  let selectedImages = $state<ImageEntry[]>([]);
  let outputFolder = $state("");
  let config = $state<ProcessingConfig>({
    mode: "crop",
    bg_color: "white",
    quality: 90,
    max_size_mb: 8,
    delete_originals: false,
    max_width: null,
  });

  const thumbnails = createThumbnailQueue();
  const presets = createPresetStore();
  const convert = createConvertRun();

  // --- Exifフレーム状態 ---
  let exifFrameEnabled = $state(false);
  let showExifFrameSettings = $state(false);

  const PAGE_SIZE = 50;
  let currentPage = $state(0);

  let previewImage = $state<ImageEntry | null>(null);

  function handlePreview(image: ImageEntry) {
    previewImage = image;
  }

  function handleClosePreview() {
    previewImage = null;
  }

  function handleNavigatePreview(image: ImageEntry) {
    const idx = images.findIndex((img) => img.path === image.path);
    if (idx >= 0) {
      const targetPage = Math.floor(idx / PAGE_SIZE);
      if (targetPage !== currentPage) {
        currentPage = targetPage;
      }
    }
    previewImage = image;
  }

  // --- 派生状態 ---
  let selectedPaths = $derived(new Set(selectedImages.map((img) => img.path)));
  let canProcess = $derived(
    selectedImages.length > 0 && !convert.processing && outputFolder !== ""
  );

  // --- イベントリスナー ---
  onMount(() => {
    const unsubscribe = convert.subscribeProgress();
    presets.reload();
    return unsubscribe;
  });

  // --- ハンドラー ---
  let currentFolder = $state("");
  // フォルダー連打で古い listImages の応答が新しい一覧を上書きしないようトークンで守る
  let listImagesToken = 0;

  async function handleSelectFolder(path: string) {
    currentFolder = path;
    currentPage = 0;
    const token = ++listImagesToken;
    try {
      const result = await listImages(path);
      if (token !== listImagesToken) return;
      images = result;
    } catch (e) {
      if (token !== listImagesToken) return;
      images = [];
      toast.error(`フォルダーを開けませんでした: ${describeError(e)}`);
    }
  }

  function handleToggleSelect(image: ImageEntry) {
    const idx = selectedImages.findIndex((img) => img.path === image.path);
    if (idx >= 0) {
      selectedImages = selectedImages.filter((_, i) => i !== idx);
    } else {
      selectedImages = [...selectedImages, image];
    }
  }

  function handleRemove(image: ImageEntry) {
    selectedImages = selectedImages.filter((img) => img.path !== image.path);
  }

  async function handlePickOutputFolder() {
    try {
      // ダイアログは Rust 側が開く。ここで選ばれたフォルダーだけが
      // バックエンドの書き込み許可対象になる（S6-H8）。
      const selected = await pickOutputFolder(currentFolder || undefined);
      if (selected) {
        outputFolder = selected;
      }
    } catch (e) {
      toast.error(`出力先の選択に失敗しました: ${describeError(e)}`);
    }
  }

  // --- 変換実行 ---
  let showDeleteConfirm = $state(false);

  function handleProcess() {
    if (!canProcess) return;
    // 元ファイルの一括削除は取り消せないため必ず確認を挟む
    if (config.delete_originals) {
      showDeleteConfirm = true;
      return;
    }
    runProcess();
  }

  function runProcess() {
    showDeleteConfirm = false;
    const efConfig =
      config.mode === "pad" && exifFrameEnabled ? presets.active : null;
    convert.run(selectedImages, outputFolder, config, efConfig);
  }
</script>

<div class="app">
  <div class="left-panel">
    <FolderTree onSelectFolder={handleSelectFolder} />
  </div>

  <div class="center-panel">
    <ThumbnailGrid
      {images}
      {selectedPaths}
      thumbnailFor={thumbnails.get}
      {currentPage}
      onToggleSelect={handleToggleSelect}
      onRequestThumbnail={thumbnails.request}
      onPreview={handlePreview}
      onPageChange={(page) => (currentPage = page)}
    />
  </div>

  <div class="right-panel">
    <SelectionList
      {selectedImages}
      thumbnailFor={thumbnails.get}
      onRemove={handleRemove}
      onRequestThumbnail={thumbnails.request}
      onPreview={handlePreview}
    />
    <SettingsPanel
      bind:config
      {outputFolder}
      {canProcess}
      onPickOutputFolder={handlePickOutputFolder}
      onProcess={handleProcess}
      {exifFrameEnabled}
      selectedPresetName={presets.selectedName}
      presets={presets.presets}
      onExifFrameEnabledChange={(enabled) => (exifFrameEnabled = enabled)}
      onPresetChange={(name) => (presets.selectedName = name)}
      onOpenExifSettings={() => (showExifFrameSettings = true)}
    />
  </div>
</div>

{#if previewImage}
  <ImagePreview
    image={previewImage}
    {images}
    {selectedPaths}
    onToggleSelect={handleToggleSelect}
    onClose={handleClosePreview}
    onNavigate={handleNavigatePreview}
  />
{/if}

<ProgressOverlay progress={convert.progress} onCancel={convert.cancel} />

{#if showExifFrameSettings}
  <ExifFrameSettings
    presets={presets.presets}
    selectedPresetName={presets.selectedName}
    previewImagePath={selectedImages[0]?.path ?? null}
    bgColor={config.bg_color}
    onClose={() => (showExifFrameSettings = false)}
    onSave={async (p) => {
      if (await presets.save(p)) showExifFrameSettings = false;
    }}
    onDelete={presets.remove}
  />
{/if}

{#if showDeleteConfirm}
  <!-- 破壊的操作なので alertdialog にし、初期フォーカスはキャンセル側に置く -->
  <Dialog
    title="元ファイルを削除します"
    danger
    initialFocus="footer button"
    onClose={() => (showDeleteConfirm = false)}
  >
    <p>変換に成功した {selectedImages.length} 枚の元ファイルを削除します。</p>
    <p class="dialog-detail">削除したファイルはゴミ箱に入らず、元に戻せません。</p>
    {#snippet actions()}
      <Button variant="text" onclick={() => (showDeleteConfirm = false)}>キャンセル</Button>
      <Button variant="filled" danger onclick={runProcess}>削除して変換</Button>
    {/snippet}
  </Dialog>
{/if}

{#if convert.result}
  <ResultDialog
    requested={convert.result.requested}
    response={convert.result.response}
    cancelled={convert.result.cancelled}
    onClose={convert.dismissResult}
  />
{/if}

<Toast />

<style>
  .app {
    display: flex;
    height: 100vh;
    overflow: hidden;
  }

  .left-panel {
    width: 220px;
    min-width: 180px;
    border-right: 1px solid var(--md-sys-color-outline-variant);
    overflow: hidden;
  }

  .center-panel {
    flex: 1;
    overflow: hidden;
  }

  .right-panel {
    width: 240px;
    min-width: 200px;
    border-left: 1px solid var(--md-sys-color-outline-variant);
    background: var(--md-sys-color-surface-container-low);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .dialog-detail {
    color: var(--md-sys-color-on-surface-variant);
    font: var(--md-sys-typescale-body-sm);
  }
</style>
