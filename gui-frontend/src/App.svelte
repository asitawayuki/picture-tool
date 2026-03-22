<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { open } from "@tauri-apps/plugin-dialog";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import FolderTree from "./lib/FolderTree.svelte";
  import ThumbnailGrid from "./lib/ThumbnailGrid.svelte";
  import SelectionList from "./lib/SelectionList.svelte";
  import SettingsPanel from "./lib/SettingsPanel.svelte";
  import ProgressOverlay from "./lib/ProgressOverlay.svelte";
  import { listImages, processImages, cancelProcessing } from "./lib/api";
  import type { ImageEntry, ProcessingConfig, ProgressPayload } from "./lib/types";

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
  let thumbnailCache = $state<Map<string, string>>(new Map());

  // --- 派生状態 ---
  let selectedPaths = $derived(new Set(selectedImages.map((img) => img.path)));
  let canProcess = $derived(
    selectedImages.length > 0 && !processing && outputFolder !== ""
  );

  // --- イベントリスナー ---
  let unlisten: UnlistenFn | null = $state(null);

  $effect(() => {
    listen<ProgressPayload>("processing-progress", (event) => {
      progress = event.payload;
    }).then((fn) => {
      unlisten = fn;
    });

    return () => {
      unlisten?.();
    };
  });

  // --- ハンドラー ---
  async function handleSelectFolder(path: string) {
    try {
      images = await listImages(path);
    } catch (e) {
      console.error("Failed to list images:", e);
      images = [];
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
    const selected = await open({ directory: true, multiple: false });
    if (selected) {
      outputFolder = selected as string;
    }
  }

  async function handleProcess() {
    if (!canProcess) return;
    processing = true;
    progress = { current: 0, total: selectedImages.length, file_name: "" };

    try {
      const files = selectedImages.map((img) => img.path);
      const results = await processImages(files, outputFolder, config);
      alert(`完了: ${results.length}/${selectedImages.length} 枚を変換しました`);
    } catch (e) {
      alert(`エラー: ${e}`);
    } finally {
      processing = false;
      progress = null;
    }
  }

  async function handleCancel() {
    await cancelProcessing();
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
      onToggleSelect={handleToggleSelect}
    />
  </div>

  <div class="right-panel">
    <SelectionList
      {selectedImages}
      {thumbnailCache}
      onRemove={handleRemove}
    />
    <SettingsPanel
      bind:config
      {outputFolder}
      {canProcess}
      onPickOutputFolder={handlePickOutputFolder}
      onProcess={handleProcess}
    />
  </div>
</div>

<ProgressOverlay {progress} onCancel={handleCancel} />

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
