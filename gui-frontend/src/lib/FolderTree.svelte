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
  <div class="tree-row" class:selected={selectedPath === node.entry.path}>
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
    background: var(--bg-secondary);
    overflow: hidden;
  }

  .section-header {
    padding: 8px 12px;
    color: var(--text-secondary);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    border-bottom: 1px solid var(--border-color);
  }

  .favorites {
    border-bottom: 1px solid var(--border-color);
    max-height: 40vh;
    overflow-y: auto;
  }

  .tree-content {
    flex: 1;
    overflow-y: auto;
    padding: 4px 0;
  }

  .tree-row {
    display: flex;
    align-items: center;
  }

  .tree-row:hover {
    background: var(--bg-hover);
  }

  .tree-row.selected {
    background: var(--accent-bg);
  }

  .tree-row.selected .tree-item {
    color: var(--accent);
  }

  .tree-item {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: 1;
    min-width: 0;
    padding: 4px 12px;
    border: none;
    background: none;
    color: var(--text-primary);
    font-size: 13px;
    cursor: pointer;
    text-align: left;
  }

  .fav-toggle {
    flex-shrink: 0;
    background: none;
    border: none;
    color: var(--text-muted);
    font-size: 13px;
    line-height: 1;
    padding: 4px 8px;
    cursor: pointer;
    /* 常時表示だと視覚的に騒がしいので、ホバー・フォーカス・登録済みのみ見せる */
    opacity: 0;
  }

  .tree-row:hover .fav-toggle,
  .fav-toggle:focus-visible,
  .fav-toggle.active {
    opacity: 1;
  }

  .fav-toggle.active {
    color: var(--warning);
  }

  .tree-error {
    margin: 0 12px 4px;
    font-size: 11px;
    line-height: 1.4;
    color: var(--danger);
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
