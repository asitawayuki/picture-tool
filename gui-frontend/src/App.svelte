<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { SvelteSet } from "svelte/reactivity";
  import AppShell from "./lib/shell/AppShell.svelte";
  import { createLayout } from "./lib/shell/layout.svelte";
  import { createTheme } from "./lib/shell/theme.svelte";
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
  import UnsavedChangesDialog from "./lib/UnsavedChangesDialog.svelte";
  import ResultDialog from "./lib/ResultDialog.svelte";
  import Toast from "./lib/Toast.svelte";
  import { toast, describeError } from "./lib/toasts.svelte";
  import { listImages, setUnsavedState } from "./lib/api";
  import { createThumbnailQueue } from "./lib/browser/thumbnailQueue.svelte";
  import GridFilterBar from "./lib/browser/GridFilterBar.svelte";
  import {
    DEFAULT_RATING_PRESET,
    DEFAULT_SORT,
    arrangeImages,
    hiddenSelectedCount,
    ratingFilterOf,
    sortImages,
    type GridSort,
    type RatingPresetId,
  } from "./lib/browser/gridFilter";
  import { createPresetStore } from "./lib/panels/presets.svelte";
  import { createConvertRun } from "./lib/panels/convertRun.svelte";
  import { createFrameDraft } from "./lib/panels/frameDraft.svelte";
  import { createMetadataDraft } from "./lib/panels/metadataDraft.svelte";
  import { createGrantStore } from "./lib/panels/grants.svelte";
  import { isEditableImage } from "./lib/panels/metadataValues";
  import { parentFolder } from "./lib/panels/grants";
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
  let editingPath = $state<string | null>(null);

  // 未保存のまま離れようとしたときに保留する操作。
  // 3 経路（写真の切替 / モードの移動 / フォルダーの変更）をここへ集める
  let pendingLeave = $state<(() => void) | null>(null);

  // 変換モード専用の選択。フォルダーを変えたらクリアする（spec §3-2）
  const selectedPaths = new SvelteSet<string>();

  /**
   * グリッドの絞り込みと並び順。**フォルダーを変えても保つ** ── 「★を付けた
   * 写真を探して回る」ような使い方では、フォルダーごとに打ち直すことになる。
   * 選択は絞り込みで変えない（絞り込みは表示の話）。隠れた選択の枚数は
   * ヘッダーに出す
   */
  let searchQuery = $state("");
  let gridSort = $state<GridSort>({ ...DEFAULT_SORT });
  /** ★の絞り込み。7 択の id を持つ（対応表は `gridFilter.ts`） */
  let ratingPreset = $state<RatingPresetId>(DEFAULT_RATING_PRESET);
  let ratingFilter = $derived(ratingFilterOf(ratingPreset));

  /**
   * グリッドのスクロール位置。**App が持つ**（spec §3-2）。
   * フレームモードでは PhotoGrid が unmount されるので、
   * グリッド内部の state に置くと戻ったときに先頭へ飛ぶ。
   */
  let gridScrollTop = $state(0);
  /** `revealPath` を呼ぶための PhotoGrid インスタンス（Step 6b） */
  let grid = $state<ReturnType<typeof PhotoGrid> | undefined>();

  const layout = createLayout();
  const theme = createTheme();
  const thumbnails = createThumbnailQueue();
  const presets = createPresetStore();
  const convert = createConvertRun();
  // 下書きは一覧を読み書きするのでプリセットストアを渡す
  const frame = createFrameDraft(presets);
  const metadata = createMetadataDraft();
  const grants = createGrantStore();

  // --- 派生状態 ---
  /**
   * 画面に出す一覧。**順序を決めるのはここ 1 箇所だけ。**
   * 下の `nextImagePath`・★の要求・`PhotoGrid` は同じ配列を index で参照する。
   * 2 箇所で順序を決めると index がずれ、そのまま「別の写真を処理する」になる
   */
  let visibleImages = $derived(arrangeImages(images, searchQuery, ratingFilter, gridSort));
  /**
   * 変換対象。**絞り込みでは減らさない** ── 選択は表示と独立しているので、
   * 隠れている分も含めて変換する（その枚数はヘッダーに出す）。
   * 並び順だけは画面に合わせる。進捗に出るファイル名の順が画面と食い違わない
   */
  let selectedImages = $derived(
    sortImages(
      images.filter((img) => selectedPaths.has(img.path)),
      gridSort
    )
  );
  let hiddenSelected = $derived(hiddenSelectedCount(visibleImages, selectedPaths));
  let editingImage = $derived(
    editingPath === null ? null : (images.find((img) => img.path === editingPath) ?? null)
  );
  // 選択（App）と変換の設定（convert）にまたがるのでここで導く
  let canProcess = $derived(
    selectedImages.length > 0 && !convert.processing && convert.outputFolder !== ""
  );

  let editingFolder = $derived(editingPath === null ? "" : parentFolder(editingPath));
  let canWriteEditing = $derived(editingPath !== null && grants.allows(editingPath));
  let editingIsJpeg = $derived(editingImage !== null && isEditableImage(editingImage.name));
  // 「次へ」は**画面に出ている並び**を辿る。絞り込みの外へ飛ぶと、送った先が
  // グリッドに無い（選択枠も付かない）状態になる。編集中の写真が絞り込みで
  // 隠れている場合は index が -1 になり、次が無い扱いになる
  let nextImagePath = $derived.by(() => {
    if (editingPath === null) return null;
    const index = visibleImages.findIndex((img) => img.path === editingPath);
    return index >= 0 && index + 1 < visibleImages.length
      ? visibleImages[index + 1].path
      : null;
  });

  // 編集対象が変わったら読み込む
  $effect(() => {
    // 非 JPEG では読みに行かない。read_image_metadata は readable_jpeg で
    // PNG / WebP を弾くので、無条件に呼ぶと情報モードで PNG をクリックするたびに
    // エラートーストが出る（グリッドは PNG / WebP も並べる）
    const target = editingPath !== null && editingIsJpeg ? editingPath : null;
    // `untrack` で囲む ── `load` はストアの $state を書くので、
    // 囲まないと自分の書き込みで再実行されうる
    untrack(() => {
      metadata.load(target).catch((e) => {
        toast.error(`メタデータを読めませんでした: ${describeError(e)}`);
      });
    });
  });

  // 未保存の有無が変わるたびに Rust へ伝える。Tauri の外（vite dev）では
  // invoke が必ず reject するので、そこだけ握りつぶす。本番で失敗すると
  // Rust 側の unsaved が false のまま取り残され、終了時に未保存のまま
  // 黙って閉じてしまう（このガードが防ぎたいことそのもの）ので、
  // 本番では最低限コンソールに痕跡を残す（利用者に見せるほどではないので toast は出さない）
  $effect(() => {
    const dirty = metadata.isDirty;
    untrack(() => {
      void setUnsavedState(dirty).catch((e) => {
        if (import.meta.env.DEV) return;
        console.error("未保存状態の通知に失敗", e);
      });
    });
  });

  // 絞り込みと並び順が変わったら先頭へ戻す。前のスクロール位置は別の写真を
  // 指しており、そのままだと「絞り込んだのに何も出ない」（一覧より下に居る）。
  // ★を足さないと、一覧が短くなったぶん scrollTop がクランプされて末尾が出る
  //
  // **`$effect.pre` でなければならない。** 仮想化の余白が縮む前に位置を 0 にしないと、
  // ブラウザーが先に scrollTop を新しい末尾へクランプして scroll イベントを撒き、
  // `PhotoGrid` の onscroll がそのクランプ値を書き戻して 0 を潰す
  // （実測: 全体実行の約 50% で「絞り込んだ結果の末尾」が出たまま固定された）
  $effect.pre(() => {
    searchQuery;
    gridSort.key;
    gridSort.desc;
    // **依存は絞り込み設定そのもの。`visibleImages` に張ってはならない** ──
    // 保存 → その場で★を書き換え → グリッドから消える、でもスクロールが飛ぶ
    ratingFilter.min;
    ratingFilter.includeUnrated;
    untrack(() => {
      gridScrollTop = 0;
      // **state だけでは足りない。** scrollTop の state を見て DOM を動かす
      // PhotoGrid 側の effect は、ここでの書き込みで初めて dirty になるため、
      // 余白の更新より後のフラッシュへ回されうる（実測で両方の順が出た）。
      // グリッドが居るなら DOM も同じ turn で 0 にする → `scrollToTop` の説明
      grid?.scrollToTop();
    });
  });

  // --- イベントリスナー ---
  onMount(() => {
    const unsubscribe = convert.subscribeProgress();
    // OS のテーマ追従。Linux では Rust が portal を見ている（gui/src/theme.rs）
    const unwatchTheme = theme.start();
    presets.reload();
    // キャッシュ上限を実測で決める（Task 15 / spec §7-2）ための窓口。
    // import.meta.env.DEV で囲んであるので本番バンドルには残らない
    if (import.meta.env.DEV) {
      (window as unknown as Record<string, unknown>).__thumbnailStats = thumbnails.stats;
    }
    return () => {
      unsubscribe();
      unwatchTheme();
    };
  });

  // --- ハンドラー ---
  // フォルダー連打で古い listImages の応答が新しい一覧を上書きしないようトークンで守る
  let listImagesToken = 0;

  // 未保存のまま離れようとしたときに保留していた操作を実行する
  function guarded(action: () => void) {
    if (metadata.isDirty) {
      pendingLeave = action;
      return;
    }
    action();
  }

  function resumeLeave() {
    const action = pendingLeave;
    pendingLeave = null;
    action?.();
  }

  function handleDiscardAndLeave() {
    metadata.discard();
    resumeLeave();
  }

  async function handleSaveAndLeave() {
    // await をまたぐ前に「いま同意を取ろうとしている」保留アクションを固定する
    // （Task 15 裁定 T3）。`resumeLeave()` のように await 後の pendingLeave を
    // 読み直すと、保存が往復している間に別の pendingLeave へ差し替わっていた場合
    // ユーザーが答えていない新しいダイアログを勝手に解決してしまう
    const action = pendingLeave;
    const didSave = await handleSaveMetadata();
    // **失敗したらダイアログは閉じない。** 閉じてしまうと「移動しなかった」ことが
    // トーストからしか分からず、未保存の変更を抱えたまま元の画面に戻される
    if (!didSave) return;
    pendingLeave = null;
    action?.();
  }

  async function loadFolder(path: string) {
    currentFolder = path;
    gridScrollTop = 0;
    // フォルダーを変えたら最後に触った写真・編集対象は無効
    focusedPath = null;
    editingPath = null;
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

  function handleSelectFolder(path: string) {
    guarded(() => void loadFolder(path));
  }

  function handleVisibleRangeChange(start: number, end: number) {
    thumbnails.setVisibleRange(start, end);
  }

  // クリックは「選択のトグル ＋ focusedPath の移動」を同時に行う（spec §3-2）
  function handleToggleSelect(image: ImageEntry) {
    if (selectedPaths.has(image.path)) selectedPaths.delete(image.path);
    else selectedPaths.add(image.path);
    handleFocus(image);
  }

  function handleFocus(image: ImageEntry) {
    if (mode === "metadata") {
      // 編集中のタイルを再クリックしただけなら何も動かさない。保留する操作が
      // 同じ値の再代入で無意味な上、未保存のまま再クリックするたびにダイアログが
      // 出てしまう（Task 15 裁定 T4 Minor#1）
      if (image.path === editingPath) return;
      // **`focusedPath` の代入もガードの内側に入れる。** 外に出すと、
      // ダイアログでキャンセルしてもグリッドの選択だけが移動する
      guarded(() => {
        focusedPath = image.path;
        editingPath = image.path;
      });
      return;
    }
    // 変換モードのクリックは focusedPath と selectedPaths にしか触らない（spec §3-2）。
    // ここで editingPath を動かすと、変換モードで写真をチェックするたびに
    // 未保存ガードが誤発火する
    focusedPath = image.path;
  }

  function handleModeChange(next: AppMode) {
    // メタデータモードから出るときだけガードする。入るときは失うものが無い
    if (mode === "metadata" && next !== "metadata") {
      guarded(() => applyModeChange(next));
      return;
    }
    applyModeChange(next);
  }

  // メタデータモードへ入ったとき、編集対象が空なら最後に触った 1 枚を採る（spec §3-2）
  function applyModeChange(next: AppMode) {
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

  async function handleGrant() {
    if (editingFolder === "") return;
    try {
      const approved = await grants.request(editingFolder);
      if (!approved) toast.warning("書き込みは許可されませんでした。");
    } catch (e) {
      toast.error(`許可を求められませんでした: ${describeError(e)}`);
    }
  }

  async function handleSaveMetadata(): Promise<boolean> {
    try {
      const didSave = await metadata.save();
      if (didSave) {
        toast.success("保存しました。");
        // ★の出所は `list_images` なので、**保存した 1 枚だけその場で書き換える**。
        // `listImages` を呼び直すとスクロール位置と選択が飛ぶ。
        //
        // **値は保存の応答（＝書いた後のファイルの実際の中身）から採る。**
        // `metadataDraft.save()` は応答を `values` に入れている。送信した下書きの
        // 値を採ると Rust 側の正規化（★の範囲外 → None）と食い違い、
        // グリッドの★がディスクに無い値になる
        const savedPath = metadata.path!;
        const stars = metadata.values.rating === 0 ? null : metadata.values.rating;
        images = images.map((img) =>
          img.path === savedPath ? { ...img, rating: stars } : img
        );
      }
      return didSave;
    } catch (e) {
      toast.error(`保存できませんでした: ${describeError(e)}`);
      return false;
    }
  }

  async function handleSaveAndNext() {
    const next = nextImagePath;
    if (!(await handleSaveMetadata())) return;
    editingPath = next;
    // **グリッドの選択も動かし、スクロールも追従させる。**
    // 単一選択モードの `aria-selected` と太枠は `focusedPath` で決まる
    // （`PhotoGrid.svelte` の `isSelected`）ので、落とすと
    // 「パネルは次の写真・グリッドの枠は前の写真」になる。
    // 送り続けると可視範囲の外へ出るので `revealPath` も呼ぶ（Step 6b）
    if (next !== null) {
      focusedPath = next;
      grid?.revealPath(next);
    }
  }
</script>

<AppShell {mode} onModeChange={handleModeChange} {layout} {theme}>
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
      <FolderTree {currentFolder} onSelectFolder={handleSelectFolder} />
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
        bind:this={grid}
        images={visibleImages}
        unfilteredCount={images.length}
        hiddenSelectedCount={hiddenSelected}
        filters={gridFilters}
        selectionMode={mode === "convert" ? "multi" : "single"}
        {selectedPaths}
        {focusedPath}
        thumbnailFor={thumbnails.get}
        onRequestThumbnail={thumbnails.request}
        onVisibleRangeChange={handleVisibleRangeChange}
        bind:scrollTop={gridScrollTop}
        onToggleSelect={handleToggleSelect}
        onFocus={handleFocus}
        unsavedPath={metadata.isDirty ? editingPath : null}
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
        canWrite={canWriteEditing}
        editable={editingIsJpeg}
        hasNext={nextImagePath !== null}
        onGrant={handleGrant}
        onSave={handleSaveMetadata}
        onSaveAndNext={handleSaveAndNext}
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
<!-- 絞り込みと並び順。状態は App が持ち（`visibleImages` の出所）、
     見た目は PhotoGrid → GridHeader へ snippet で降ろす -->
{#snippet gridFilters()}
  <GridFilterBar bind:query={searchQuery} sort={gridSort} bind:ratingPreset />
{/snippet}

{#snippet collapsedPrimaryAction()}
  {#if mode === "convert"}
    <Button variant="filled" disabled={!canProcess} onclick={handleProcess}>
      {selectedPaths.size} 枚を変換
    </Button>
  {:else if mode === "metadata"}
    <Button
      variant="filled"
      disabled={!canWriteEditing || !editingIsJpeg || !metadata.isDirty || metadata.saving}
      onclick={handleSaveMetadata}>保存</Button>
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

{#if pendingLeave !== null}
  <UnsavedChangesDialog
    name={editingImage?.name ?? ""}
    canSave={canWriteEditing && editingIsJpeg}
    saving={metadata.saving}
    onCancel={() => (pendingLeave = null)}
    onDiscard={handleDiscardAndLeave}
    onSave={handleSaveAndLeave}
  />
{/if}

<Toast />
