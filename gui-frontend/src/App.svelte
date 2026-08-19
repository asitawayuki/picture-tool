<script lang="ts">
  import { onMount } from "svelte";
  import { SvelteSet } from "svelte/reactivity";
  import AppShell from "./lib/shell/AppShell.svelte";
  import { createLayout } from "./lib/shell/layout.svelte";
  import type { AppMode } from "./lib/shell/modes";
  import FolderTree from "./lib/browser/FolderTree.svelte";
  import PhotoGrid from "./lib/browser/PhotoGrid.svelte";
  import ConvertPanel from "./lib/panels/ConvertPanel.svelte";
  import ProgressOverlay from "./lib/ProgressOverlay.svelte";
  import PhotoViewer from "./lib/browser/PhotoViewer.svelte";
  import PresetList from "./lib/panels/PresetList.svelte";
  import FramePreview from "./lib/panels/FramePreview.svelte";
  import FramePanel from "./lib/panels/FramePanel.svelte";
  import Card from "./lib/ui/Card.svelte";
  import Dialog from "./lib/ui/Dialog.svelte";
  import Button from "./lib/ui/Button.svelte";
  import ResultDialog from "./lib/ResultDialog.svelte";
  import Toast from "./lib/Toast.svelte";
  import { toast, describeError } from "./lib/toasts.svelte";
  import { listAvailableFonts, listImages, pickOutputFolder } from "./lib/api";
  import { createThumbnailQueue } from "./lib/browser/thumbnailQueue.svelte";
  import { createPresetStore } from "./lib/panels/presets.svelte";
  import { createConvertRun } from "./lib/panels/convertRun.svelte";
  import { createFrameDraft } from "./lib/panels/frameDraft.svelte";
  import type { FontInfo, ImageEntry, ProcessingConfig } from "./lib/types";

  // --- 状態 ---
  let mode = $state<AppMode>("convert");

  // 全モードで共有。rail の切替では破棄しない
  let currentFolder = $state("");
  let images = $state<ImageEntry[]>([]);

  // 最後にクリックした 1 枚。フレームの見本写真の出所（spec §3-2）
  let focusedPath = $state<string | null>(null);

  // メタデータの編集対象。未保存ガードはこれの変更にだけ掛かる。
  // 本刷新では読むだけで、ガードの配線は次工程（spec §5-2）
  let editingPath = $state<string | null>(null);

  const layout = createLayout();

  // 変換モード専用の選択。フォルダーを変えたらクリアする（spec §3-2）
  const selectedPaths = new SvelteSet<string>();

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
  const frame = createFrameDraft();

  // --- Exifフレーム状態 ---
  let exifFrameEnabled = $state(false);
  let fonts = $state<FontInfo[]>([]);

  let previewImage = $state<ImageEntry | null>(null);

  /**
   * グリッドのスクロール位置。**App が持つ**（spec §3-2）。
   * フレームモードでは PhotoGrid が unmount されるので、
   * グリッド内部の state に置くと戻ったときに先頭へ飛ぶ。
   */
  let gridScrollTop = $state(0);

  function handlePreview(image: ImageEntry) {
    previewImage = image;
  }

  function handleClosePreview() {
    previewImage = null;
  }

  // --- 派生状態 ---
  let selectedImages = $derived(images.filter((img) => selectedPaths.has(img.path)));
  let canProcess = $derived(
    selectedImages.length > 0 && !convert.processing && outputFolder !== ""
  );

  // --- イベントリスナー ---
  onMount(() => {
    const unsubscribe = convert.subscribeProgress();
    presets.reload();
    listAvailableFonts()
      .then((f) => (fonts = f))
      .catch((e) => toast.error(`フォント一覧の取得に失敗しました: ${describeError(e)}`));
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
      frame.select(presets.selectedName, presets.presets);
    }
  }

  // --- フレームのプリセット操作 ---
  async function handleSaveFrame() {
    const snap = frame.snapshot();
    // 改名なら「新しい名前で保存 → 旧名を削除」。それ以外は普通の保存
    const from = frame.renamedFrom;
    const ok = from ? await presets.rename(from, snap) : await presets.save(snap);
    // editingName を保存後の名前へ合わせ直す。これをしないと、改名の直後に
    // もう一度保存したときに renamedFrom が消えた旧名を指し、
    // 存在しないプリセットを削除しようとする
    if (ok) frame.select(snap.name, presets.presets);
  }

  async function handleDeletePreset(name: string) {
    await presets.remove(name);
    // 消したのが編集中のものなら、下書きがディスク上に無い実体を指したままになる。
    // そのまま保存すると消したはずのプリセットが復活するので選び直す
    if (frame.editingName === name) frame.select(presets.selectedName, presets.presets);
  }

  function handleClearSelection() {
    selectedPaths.clear();
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

<AppShell {mode} onModeChange={handleModeChange} {layout}>
  {#snippet left()}
    {#if mode === "frame"}
      <PresetList
        presets={presets.presets}
        editingName={frame.editingName}
        onSelect={(name) => frame.select(name, presets.presets)}
        onRename={frame.rename}
        onCreate={() => frame.createNew(presets.presets)}
        onDelete={handleDeletePreset}
      />
    {:else}
      <FolderTree onSelectFolder={handleSelectFolder} />
    {/if}
  {/snippet}

  {#snippet center()}
    {#if mode === "frame"}
      <FramePreview config={frame.draft} bgColor={config.bg_color} imagePath={focusedPath} />
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
      onPreview={handlePreview}
      selectedCount={selectedPaths.size}
      rightPanelCollapsed={layout.rightPanelCollapsed}
      onToggleRightPanel={() =>
        (layout.rightPanelCollapsed = !layout.rightPanelCollapsed)}
      onClearSelection={handleClearSelection}
      primaryAction={collapsedPrimaryAction}
    />
    {/if}
  {/snippet}

  {#snippet right()}
    {#if mode === "convert"}
      <ConvertPanel
        bind:config
        {outputFolder}
        selectedCount={selectedPaths.size}
        {canProcess}
        bind:exifFrameEnabled
        presetNames={presets.presets.map((p) => p.name)}
        bind:selectedPresetName={presets.selectedName}
        onPickOutputFolder={handlePickOutputFolder}
        onProcess={handleProcess}
        onEditFrame={() => handleModeChange("frame")}
      />
    {:else if mode === "metadata"}
      <div class="placeholder">
        <Card level={1} title="メタデータ">
          <p>Task 17（段階 8）で実装する。</p>
        </Card>
      </div>
    {:else if mode === "frame"}
      <!-- frame.draft は getter しか持たず、型も ExifFrameConfig | null なので
           bind:config は使えない。{@const} で絞ってから非 bind: で渡す。
           FramePanel 側は $bindable() で受けてプロパティを直接書き換える
           （$state のプロキシなので親まで届く） -->
      {@const draft = frame.draft}
      {#if draft}
        <FramePanel
          config={draft}
          bind:bgColor={config.bg_color}
          {fonts}
          isNew={frame.isNew}
          isRenamed={frame.isRenamed}
          nameConflict={frame.nameConflict}
          canSave={frame.canSave}
          canDelete={frame.canDelete}
          sampleName={images.find((img) => img.path === focusedPath)?.name ?? null}
          onSave={handleSaveFrame}
          onDelete={() => handleDeletePreset(frame.editingName)}
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

{#if previewImage}
  <PhotoViewer
    image={previewImage}
    {images}
    selectionMode={mode === "convert" ? "multi" : "single"}
    {selectedPaths}
    thumbnailFor={thumbnails.get}
    onRequestThumbnail={thumbnails.request}
    onToggleSelect={handleToggleSelect}
    onClose={handleClosePreview}
    onNavigate={(img) => {
      previewImage = img;
      handleFocus(img);
    }}
  />
{/if}

<ProgressOverlay progress={convert.progress} onCancel={convert.cancel} />

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
  .placeholder {
    padding: var(--space-4);
  }

  .dialog-detail {
    color: var(--md-sys-color-on-surface-variant);
    font: var(--md-sys-typescale-body-sm);
  }
</style>
