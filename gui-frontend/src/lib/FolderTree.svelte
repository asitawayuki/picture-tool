<script lang="ts">
  import { onMount } from "svelte";
  import { listDirectory, listDrives, loadFavorites, saveFavorites } from "./api";
  import { toast, describeError } from "./toasts.svelte";
  import type { FileEntry } from "./types";

  interface Props {
    onSelectFolder: (path: string) => void;
  }

  let { onSelectFolder }: Props = $props();

  interface TreeNode {
    entry: FileEntry;
    children: TreeNode[] | null;
    expanded: boolean;
    loading: boolean;
    /** 読み込みに失敗した理由。null なら失敗していない。 */
    error: string | null;
  }

  function makeNode(entry: FileEntry): TreeNode {
    return { entry, children: null, expanded: false, loading: false, error: null };
  }

  let roots = $state<TreeNode[]>([]);
  let selectedPath = $state("");
  let favorites = $state<string[]>([]);

  let favoriteNodes = $state<TreeNode[]>([]);

  function buildFavoriteNodes(paths: string[]): TreeNode[] {
    return paths.map((path) =>
      makeNode({ name: getFolderName(path), path, is_dir: true, is_image: false })
    );
  }

  async function initFavorites() {
    const saved = await loadFavorites();
    favorites = saved;
    favoriteNodes = buildFavoriteNodes(saved);
  }

  async function toggleFavorite(path: string) {
    const wasFavorite = favorites.includes(path);
    const previous = favorites;
    favorites = wasFavorite ? favorites.filter((f) => f !== path) : [...favorites, path];
    favoriteNodes = buildFavoriteNodes(favorites);
    try {
      await saveFavorites(favorites);
    } catch (e) {
      // 保存できなかったら画面上の状態も戻す（次回起動で消える方が分かりにくい）
      favorites = previous;
      favoriteNodes = buildFavoriteNodes(favorites);
      toast.error(`お気に入りを保存できませんでした: ${describeError(e)}`);
    }
  }

  async function loadRoots() {
    try {
      const drives = await listDrives();
      roots = drives.map((drive) =>
        makeNode({ name: drive, path: drive, is_dir: true, is_image: false })
      );
      if (roots.length > 0) {
        await expandNode(roots[0]);
      }
    } catch (e) {
      toast.error(`ドライブ一覧を取得できませんでした: ${describeError(e)}`);
    }
  }

  async function expandNode(node: TreeNode) {
    if (!node.entry.is_dir) return;

    if (node.children === null) {
      node.loading = true;
      node.error = null;
      try {
        const entries = await listDirectory(node.entry.path);
        node.children = entries.filter((e) => e.is_dir).map(makeNode);
      } catch (e) {
        // 「空のフォルダー」と見分けが付かなくなるため、失敗はノードに残して表示する
        node.children = [];
        node.error = describeError(e);
        toast.error(`${node.entry.name} を開けませんでした: ${node.error}`);
      } finally {
        node.loading = false;
      }
    }

    node.expanded = true;
  }

  function toggleNode(node: TreeNode) {
    if (node.expanded) {
      node.expanded = false;
      return;
    }
    // 前回失敗したノードは再度開いた時にやり直す
    if (node.error !== null) node.children = null;
    expandNode(node);
  }

  function selectFolder(node: TreeNode) {
    selectedPath = node.entry.path;
    onSelectFolder(node.entry.path);
    if (!node.expanded) toggleNode(node);
  }

  function getFolderName(path: string): string {
    const parts = path.replace(/[/\\]+$/, "").split(/[/\\]/);
    return parts[parts.length - 1] || path;
  }

  onMount(() => {
    loadRoots();
    initFavorites().catch((e) => {
      toast.error(`お気に入りの読み込みに失敗しました: ${describeError(e)}`);
    });
  });
</script>

<div class="folder-tree">
  {#if favorites.length > 0}
    <div class="section-header">⭐ お気に入り</div>
    <div class="favorites">
      {#each favoriteNodes as node}
        {@render treeNode(node, 0)}
      {/each}
    </div>
  {/if}

  <div class="section-header">💾 ドライブ</div>
  <div class="tree-content">
    {#each roots as node}
      {@render treeNode(node, 0)}
    {/each}
  </div>
</div>

{#snippet treeNode(node: TreeNode, depth: number)}
  {@const isFavorite = favorites.includes(node.entry.path)}
  <div class="tree-row state-layer" class:selected={selectedPath === node.entry.path}>
    <button
      class="tree-item"
      style="padding-left: {12 + depth * 16}px"
      aria-expanded={node.expanded}
      title={node.entry.path}
      onclick={() => selectFolder(node)}
    >
      <span class="icon" aria-hidden="true">
        {#if node.loading}
          ⏳
        {:else if node.error}
          ⚠
        {:else if node.expanded}
          📂
        {:else}
          📁
        {/if}
      </span>
      <span class="name">{node.entry.name}</span>
    </button>
    <!-- 右クリックだけだとキーボードから到達できないため、常設のトグルにする -->
    <button
      class="fav-toggle"
      class:active={isFavorite}
      aria-pressed={isFavorite}
      aria-label="{node.entry.name} を{isFavorite ? 'お気に入りから削除' : 'お気に入りに追加'}"
      onclick={() => toggleFavorite(node.entry.path)}
    >
      {isFavorite ? "★" : "☆"}
    </button>
  </div>

  {#if node.error}
    <p class="tree-error" style="padding-left: {12 + depth * 16 + 20}px">{node.error}</p>
  {/if}

  {#if node.expanded && node.children}
    {#each node.children as child (child.entry.path)}
      {@render treeNode(child, depth + 1)}
    {/each}
  {/if}
{/snippet}

<style>
  .folder-tree {
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--md-sys-color-surface-container-low);
    overflow: hidden;
  }

  .section-header {
    padding: var(--space-2) var(--space-3);
    color: var(--md-sys-color-on-surface-variant);
    font: var(--md-sys-typescale-body-sm);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    border-bottom: 1px solid var(--md-sys-color-outline-variant);
  }

  .favorites {
    border-bottom: 1px solid var(--md-sys-color-outline-variant);
    max-height: 40vh;
    overflow-y: auto;
  }

  .tree-content {
    flex: 1;
    overflow-y: auto;
    padding: var(--space-1) 0;
  }

  /* hover / pressed は .state-layer（tokens.css）が ::after で供給する。
     ::after は border-radius: inherit なので、状態レイヤーは選択中の pill と
     同じ形になる必要がある。したがって .tree-item ではなく行そのものに付ける。 */
  .tree-row {
    display: flex;
    align-items: center;
    border-radius: var(--md-sys-shape-corner-full);
  }

  .tree-row.selected {
    background: var(--md-sys-color-primary-container);
    color: var(--md-sys-color-on-primary-container);
  }

  .tree-row.selected .tree-item {
    color: var(--md-sys-color-on-primary-container);
  }

  .tree-item {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex: 1;
    min-width: 0;
    padding: var(--space-1) var(--space-3);
    border: none;
    background: none;
    color: var(--md-sys-color-on-surface);
    font: var(--md-sys-typescale-body-md);
    cursor: pointer;
    text-align: left;
  }

  .fav-toggle {
    flex-shrink: 0;
    background: none;
    border: none;
    color: var(--md-sys-color-on-surface-variant);
    font: var(--md-sys-typescale-body-md);
    line-height: 1;
    padding: var(--space-1) var(--space-2);
    cursor: pointer;
    /* 常時表示だと視覚的に騒がしいので、ホバー・フォーカス・登録済みのみ見せる */
    opacity: 0;
  }

  .tree-row:hover .fav-toggle,
  .fav-toggle:focus-visible,
  .fav-toggle.active {
    opacity: 1;
  }

  /* spec §1-1 は warning ロールを定義しないので、登録済みの★は primary で示す */
  .fav-toggle.active {
    color: var(--md-sys-color-primary);
  }

  .tree-error {
    margin: 0 var(--space-3) var(--space-1);
    font: var(--md-sys-typescale-body-sm);
    color: var(--md-sys-color-error);
    overflow-wrap: anywhere;
  }

  .icon {
    flex-shrink: 0;
    font-size: 14px;
  }

  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
