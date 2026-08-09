<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { SvelteMap } from "svelte/reactivity";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import FolderTree from "./lib/FolderTree.svelte";
  import ThumbnailGrid from "./lib/ThumbnailGrid.svelte";
  import SelectionList from "./lib/SelectionList.svelte";
  import SettingsPanel from "./lib/SettingsPanel.svelte";
  import ProgressOverlay from "./lib/ProgressOverlay.svelte";
  import ImagePreview from "./lib/ImagePreview.svelte";
  import ExifFrameSettings from "./lib/ExifFrameSettings.svelte";
  import ConfirmDialog from "./lib/ConfirmDialog.svelte";
  import ResultDialog from "./lib/ResultDialog.svelte";
  import Toast from "./lib/Toast.svelte";
  import { toast, describeError } from "./lib/toasts.svelte";
  import { listImages, processImages, cancelProcessing, getThumbnail, listPresets, savePreset, deletePreset, pickOutputFolder } from "./lib/api";
  import type { ImageEntry, ProcessingConfig, ProcessBatchResponse, ProgressPayload, ExifFrameConfig } from "./lib/types";

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
  });
  let processing = $state(false);
  let progress = $state<ProgressPayload | null>(null);

  // サムネイルは解像度ごとに別物なので `path:maxDimension` をキーにする。
  // path だけで持つと列数を変えても再取得されず、低解像度が引き伸ばされる。
  let thumbnailCache = new SvelteMap<string, string>();

  function thumbnailKey(path: string, maxDimension: number): string {
    return `${path}:${maxDimension}`;
  }

  function thumbnailFor(path: string, maxDimension: number): string | undefined {
    return thumbnailCache.get(thumbnailKey(path, maxDimension));
  }

  // --- Exifフレーム状態 ---
  // プリセット一覧は App が唯一の保持者。ExifFrameSettings は props で受け取る。
  let exifFrameEnabled = $state(false);
  let selectedPresetName = $state("default");
  let exifFramePresets = $state<ExifFrameConfig[]>([]);
  let showExifFrameSettings = $state(false);

  async function reloadPresets() {
    try {
      exifFramePresets = await listPresets();
      if (!exifFramePresets.some((p) => p.name === selectedPresetName)) {
        selectedPresetName = exifFramePresets[0]?.name ?? "default";
      }
    } catch (e) {
      toast.error(`プリセットの読み込みに失敗しました: ${describeError(e)}`);
    }
  }

  let activeExifFrameConfig = $derived(
    exifFramePresets.find((p) => p.name === selectedPresetName) ?? null
  );

  // --- サムネイルロード（並列制限キュー） ---
  let activeRequests = 0;
  const MAX_CONCURRENT = 3;
  const pendingQueue: { path: string; maxDimension: number }[] = [];
  // 同一キーの失敗を繰り返し再要求しないための記録
  const failedThumbnails = new Set<string>();
  let thumbnailErrorReported = false;

  function processQueue() {
    while (activeRequests < MAX_CONCURRENT && pendingQueue.length > 0) {
      const { path, maxDimension } = pendingQueue.shift()!;
      const key = thumbnailKey(path, maxDimension);
      if (thumbnailCache.has(key)) continue;
      activeRequests++;
      getThumbnail(path, maxDimension)
        .then((base64) => {
          thumbnailCache.set(key, base64);
        })
        .catch((e) => {
          failedThumbnails.add(key);
          // 1枚ごとにトーストを出すと壊れたフォルダーで埋め尽くされるため最初の1件だけ通知する
          if (!thumbnailErrorReported) {
            thumbnailErrorReported = true;
            toast.error(`サムネイルを生成できない画像があります: ${describeError(e)}`);
          }
        })
        .finally(() => {
          activeRequests--;
          processQueue();
        });
    }
  }

  function handleRequestThumbnail(path: string, maxDimension: number) {
    const key = thumbnailKey(path, maxDimension);
    if (thumbnailCache.has(key) || failedThumbnails.has(key)) return;
    if (!pendingQueue.some((item) => item.path === path && item.maxDimension === maxDimension)) {
      pendingQueue.push({ path, maxDimension });
    }
    processQueue();
  }

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
    selectedImages.length > 0 && !processing && outputFolder !== ""
  );

  // --- イベントリスナー ---
  onMount(() => {
    let unlisten: UnlistenFn | null = null;
    let cancelled = false;

    listen<ProgressPayload>("processing-progress", (event) => {
      progress = event.payload;
    })
      .then((fn) => {
        if (cancelled) {
          fn();
        } else {
          unlisten = fn;
        }
      })
      .catch((e) => {
        toast.error(`進捗の購読に失敗しました: ${describeError(e)}`);
      });

    reloadPresets();

    return () => {
      cancelled = true;
      unlisten?.();
    };
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
  let batchResponse = $state<ProcessBatchResponse | null>(null);
  let batchRequested = $state<ImageEntry[]>([]);
  let batchCancelled = $state(false);
  let cancelRequested = false;

  function handleProcess() {
    if (!canProcess) return;
    // 元ファイルの一括削除は取り消せないため必ず確認を挟む
    if (config.delete_originals) {
      showDeleteConfirm = true;
      return;
    }
    runProcess();
  }

  async function runProcess() {
    showDeleteConfirm = false;
    if (!canProcess) return;

    const requested = selectedImages;
    processing = true;
    cancelRequested = false;
    progress = { current: 0, total: requested.length, file_name: "" };

    try {
      const files = requested.map((img) => img.path);
      const efConfig = config.mode === "pad" && exifFrameEnabled ? activeExifFrameConfig : null;
      const response = await processImages(files, outputFolder, config, efConfig);
      batchRequested = requested;
      batchResponse = response;
      batchCancelled = cancelRequested;
    } catch (e) {
      toast.error(`変換に失敗しました: ${describeError(e)}`);
    } finally {
      processing = false;
      progress = null;
    }
  }

  async function handleCancel() {
    try {
      cancelRequested = true;
      await cancelProcessing();
    } catch (e) {
      toast.error(`キャンセルに失敗しました: ${describeError(e)}`);
    }
  }

  async function handleSavePreset(preset: ExifFrameConfig) {
    try {
      await savePreset(preset);
      selectedPresetName = preset.name;
      await reloadPresets();
      showExifFrameSettings = false;
      toast.success(`プリセット「${preset.name}」を保存しました`);
    } catch (e) {
      toast.error(`プリセットの保存に失敗しました: ${describeError(e)}`);
    }
  }

  async function handleDeletePreset(name: string) {
    try {
      await deletePreset(name);
      await reloadPresets();
      toast.success(`プリセット「${name}」を削除しました`);
    } catch (e) {
      toast.error(`プリセットの削除に失敗しました: ${describeError(e)}`);
    }
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
      {thumbnailFor}
      {currentPage}
      onToggleSelect={handleToggleSelect}
      onRequestThumbnail={handleRequestThumbnail}
      onPreview={handlePreview}
      onPageChange={(page) => (currentPage = page)}
    />
  </div>

  <div class="right-panel">
    <SelectionList
      {selectedImages}
      {thumbnailFor}
      onRemove={handleRemove}
      onRequestThumbnail={handleRequestThumbnail}
      onPreview={handlePreview}
    />
    <SettingsPanel
      bind:config
      {outputFolder}
      {canProcess}
      onPickOutputFolder={handlePickOutputFolder}
      onProcess={handleProcess}
      {exifFrameEnabled}
      {selectedPresetName}
      presets={exifFramePresets}
      onExifFrameEnabledChange={(enabled) => (exifFrameEnabled = enabled)}
      onPresetChange={(name) => (selectedPresetName = name)}
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

<ProgressOverlay {progress} onCancel={handleCancel} />

{#if showExifFrameSettings}
  <ExifFrameSettings
    presets={exifFramePresets}
    {selectedPresetName}
    previewImagePath={selectedImages[0]?.path ?? null}
    bgColor={config.bg_color}
    onClose={() => (showExifFrameSettings = false)}
    onSave={handleSavePreset}
    onDelete={handleDeletePreset}
  />
{/if}

{#if showDeleteConfirm}
  <ConfirmDialog
    title="元ファイルを削除します"
    message={`変換に成功した ${selectedImages.length} 枚の元ファイルを削除します。`}
    detail="削除したファイルはゴミ箱に入らず、元に戻せません。"
    confirmLabel="削除して変換"
    danger
    onConfirm={runProcess}
    onCancel={() => (showDeleteConfirm = false)}
  />
{/if}

{#if batchResponse}
  <ResultDialog
    requested={batchRequested}
    response={batchResponse}
    cancelled={batchCancelled}
    onClose={() => (batchResponse = null)}
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
    border-right: 1px solid var(--border-color);
    overflow: hidden;
  }

  .center-panel {
    flex: 1;
    overflow: hidden;
  }

  .right-panel {
    width: 240px;
    min-width: 200px;
    border-left: 1px solid var(--border-color);
    background: var(--bg-secondary);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
</style>
