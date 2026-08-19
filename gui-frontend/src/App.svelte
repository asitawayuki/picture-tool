<script lang="ts">
  import { onMount } from "svelte";
  import { SvelteSet } from "svelte/reactivity";
  import AppShell from "./lib/shell/AppShell.svelte";
  import { createLayout } from "./lib/shell/layout.svelte";
  import type { AppMode } from "./lib/shell/modes";
  import FolderTree from "./lib/browser/FolderTree.svelte";
  import PhotoGrid from "./lib/browser/PhotoGrid.svelte";
  import ConvertPanel from "./lib/panels/ConvertPanel.svelte";
  import MetadataPanel from "./lib/panels/MetadataPanel.svelte";
  import PresetList from "./lib/panels/PresetList.svelte";
  import FramePreview from "./lib/panels/FramePreview.svelte";
  import FramePanel from "./lib/panels/FramePanel.svelte";
  import Button from "./lib/ui/Button.svelte";
  import ProgressOverlay from "./lib/ProgressOverlay.svelte";
  import DeleteOriginalsDialog from "./lib/DeleteOriginalsDialog.svelte";
  import ResultDialog from "./lib/ResultDialog.svelte";
  import Toast from "./lib/Toast.svelte";
  import { toast, describeError } from "./lib/toasts.svelte";
  import { listImages } from "./lib/api";
  import { createThumbnailQueue } from "./lib/browser/thumbnailQueue.svelte";
  import { createPresetStore } from "./lib/panels/presets.svelte";
  import { createConvertRun } from "./lib/panels/convertRun.svelte";
  import { createFrameDraft } from "./lib/panels/frameDraft.svelte";
  import { createMetadataDraft } from "./lib/panels/metadataDraft.svelte";
  import type { ImageEntry } from "./lib/types";

  // --- 状態 ---
  // App が持つのは「モード」「フォルダー」「選択」「フォーカス」の 4 状態と
  // パネルの差し替えだけ（spec §3-5）。それ以外は下のストアとパネルが持つ。
  let mode = $state<AppMode>("convert");

  // 全モードで共有。rail の切替では破棄しない
  let currentFolder = $state("");
  let images = $state<ImageEntry[]>([]);

  // 最後にクリックした 1 枚。フレームの見本写真の出所（spec §3-2）
  let focusedPath = $state<string | null>(null);

  // メタデータの編集対象。未保存ガードはこれの変更にだけ掛かる。
  // 本刷新では読むだけで、ガードの配線は次工程（spec §5-2）
  let editingPath = $state<string | null>(null);

  // 変換モード専用の選択。フォルダーを変えたらクリアする（spec §3-2）
  const selectedPaths = new SvelteSet<string>();

  /**
   * グリッドのスクロール位置。**App が持つ**（spec §3-2）。
   * フレームモードでは PhotoGrid が unmount されるので、
   * グリッド内部の state に置くと戻ったときに先頭へ飛ぶ。
   */
  let gridScrollTop = $state(0);

  const layout = createLayout();
  const thumbnails = createThumbnailQueue();
  const presets = createPresetStore();
  const convert = createConvertRun();
  // 下書きは一覧を読み書きするのでプリセットストアを渡す
  const frame = createFrameDraft(presets);
  const metadata = createMetadataDraft();

  // --- 派生状態 ---
  let selectedImages = $derived(images.filter((img) => selectedPaths.has(img.path)));
  let editingImage = $derived(
    editingPath === null ? null : (images.find((img) => img.path === editingPath) ?? null)
  );
  // 選択（App）と変換の設定（convert）にまたがるのでここで導く
  let canProcess = $derived(
    selectedImages.length > 0 && !convert.processing && convert.outputFolder !== ""
  );

  // 編集対象が変わったら下書きを入れ替える。次工程ではここが
  // read_image_metadata の呼び出し口になる（spec §5-2）
  $effect(() => {
    metadata.load(editingPath);
  });

  // --- イベントリスナー ---
  onMount(() => {
    const unsubscribe = convert.subscribeProgress();
    presets.reload();
    // キャッシュ上限を実測で決める（Task 15 / spec §7-2）ための窓口。
    // import.meta.env.DEV で囲んであるので本番バンドルには残らない
    if (import.meta.env.DEV) {
      (window as unknown as Record<string, unknown>).__thumbnailStats = thumbnails.stats;
    }
    return unsubscribe;
  });

  // --- ハンドラー ---
  // フォルダー連打で古い listImages の応答が新しい一覧を上書きしないようトークンで守る
  let listImagesToken = 0;

  async function handleSelectFolder(path: string) {
    currentFolder = path;
    gridScrollTop = 0;
    // フォルダーを変えたら最後に触った写真は無効
    focusedPath = null;
    // 選択は常に現在のフォルダー内に閉じる。SelectionList を廃止したので
    // 画面外の選択を可視化・解除する窓口がもう無い（spec §3-2 / §5-1）
    selectedPaths.clear();
    // 初回 1 画面分の目安。正確な可視枚数は PhotoGrid が出すが、
    // ここでは「上から順に流す枚数」の見積もりで足りる
    thumbnails.resetForFolder(30);
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

  // クリックは「選択のトグル ＋ focusedPath の移動」を同時に行う（spec §3-2）
  function handleToggleSelect(image: ImageEntry) {
    if (selectedPaths.has(image.path)) selectedPaths.delete(image.path);
    else selectedPaths.add(image.path);
    handleFocus(image);
  }

  function handleFocus(image: ImageEntry) {
    focusedPath = image.path;
    // 変換モードのクリックは focusedPath と selectedPaths にしか触らない。
    // ここで editingPath を動かすと、変換モードで写真をチェックするたびに
    // 未保存ガードが誤発火する
    if (mode === "metadata") editingPath = image.path;
  }

  // メタデータモードへ入ったとき、編集対象が空なら最後に触った 1 枚を採る（spec §3-2）
  function handleModeChange(next: AppMode) {
    mode = next;
    if (next === "metadata" && editingPath === null) editingPath = focusedPath;
    // 下書きはモードを跨いで保つ。null のときだけ、いま選ばれているプリセットから起こす
    if (next === "frame" && frame.draft === null) {
      frame.select(presets.selectedName);
    }
  }

  function handleProcess() {
    if (canProcess) convert.request(selectedImages, presets.active);
  }
</script>

<AppShell {mode} onModeChange={handleModeChange} {layout}>
  {#snippet left()}
    {#if mode === "frame"}
      <PresetList
        presets={presets.presets}
        editingName={frame.editingName}
        onSelect={frame.select}
        onRename={frame.rename}
        onCreate={frame.createNew}
        onDelete={frame.remove}
      />
    {:else}
      <FolderTree onSelectFolder={handleSelectFolder} />
    {/if}
  {/snippet}

  {#snippet center()}
    {#if mode === "frame"}
      <FramePreview
        config={frame.draft}
        bgColor={convert.config.bg_color}
        imagePath={focusedPath}
      />
    {:else}
      <PhotoGrid
        {images}
        selectionMode={mode === "convert" ? "multi" : "single"}
        {selectedPaths}
        {focusedPath}
        thumbnailFor={thumbnails.get}
        onRequestThumbnail={thumbnails.request}
        onVisibleRangeChange={thumbnails.setVisibleRange}
        bind:scrollTop={gridScrollTop}
        onToggleSelect={handleToggleSelect}
        onFocus={handleFocus}
        selectedCount={selectedPaths.size}
        rightPanelCollapsed={layout.rightPanelCollapsed}
        onToggleRightPanel={() =>
          (layout.rightPanelCollapsed = !layout.rightPanelCollapsed)}
        onClearSelection={() => selectedPaths.clear()}
        primaryAction={collapsedPrimaryAction}
      />
    {/if}
  {/snippet}

  {#snippet right()}
    {#if mode === "convert"}
      <!-- convert.config は getter しか持たないので bind: は使えない。
           ConvertPanel はプロパティを直接書き換える（$state のプロキシなので
           ストアまで届く）。値そのものを差し替える exifFrameEnabled だけ
           setter を持たせて bind: にしてある -->
      <ConvertPanel
        config={convert.config}
        outputFolder={convert.outputFolder}
        selectedCount={selectedPaths.size}
        {canProcess}
        bind:exifFrameEnabled={convert.exifFrameEnabled}
        presetNames={presets.presets.map((p) => p.name)}
        bind:selectedPresetName={presets.selectedName}
        onPickOutputFolder={() => convert.pickOutput(currentFolder)}
        onProcess={handleProcess}
        onEditFrame={() => handleModeChange("frame")}
      />
    {:else if mode === "metadata"}
      <MetadataPanel
        image={editingImage}
        draft={metadata}
        thumbnailFor={thumbnails.get}
        onRequestThumbnail={thumbnails.request}
      />
    {:else if mode === "frame"}
      <!-- frame.draft は getter しか持たず、型も ExifFrameConfig | null なので
           bind:config は使えない。{@const} で絞ってから非 bind: で渡す。
           FramePanel 側は $bindable() で受けてプロパティを直接書き換える
           （$state のプロキシなので親まで届く） -->
      {@const draft = frame.draft}
      {#if draft}
        <FramePanel
          config={draft}
          bind:bgColor={convert.config.bg_color}
          isNew={frame.isNew}
          isRenamed={frame.isRenamed}
          nameConflict={frame.nameConflict}
          canSave={frame.canSave}
          canDelete={frame.canDelete}
          sampleName={images.find((img) => img.path === focusedPath)?.name ?? null}
          onSave={frame.save}
          onDelete={() => frame.remove(frame.editingName)}
          onPickSample={() => handleModeChange("convert")}
        />
      {/if}
    {/if}
  {/snippet}
</AppShell>

<!-- 右パネルを畳んでいる間、主導線はグリッドヘッダーへ移る（spec §3-1）。
     畳むボタンがパネルの中にあると、畳んだ瞬間に開くボタンごと消える -->
{#snippet collapsedPrimaryAction()}
  {#if mode === "convert"}
    <Button variant="filled" disabled={!canProcess} onclick={handleProcess}>
      {selectedPaths.size} 枚を変換
    </Button>
  {:else if mode === "metadata"}
    <!-- メタデータの保存は次工程で配線する（spec §5-2）。
         畳んでいる間に主導線が消えないよう、場所だけ先に確保しておく -->
    <Button variant="filled" disabled>保存</Button>
  {/if}
{/snippet}

<ProgressOverlay progress={convert.progress} onCancel={convert.cancel} />

{#if convert.confirming !== null}
  <DeleteOriginalsDialog
    count={convert.confirming}
    onCancel={convert.dismissConfirm}
    onConfirm={convert.confirm}
  />
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
