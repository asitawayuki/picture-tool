<script lang="ts">
  import { listDirectory, listDrives } from "./api";
  import { load } from "@tauri-apps/plugin-store";
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
  }

  let roots = $state<TreeNode[]>([]);
  let selectedPath = $state("");
  let favorites = $state<string[]>([]);

  let contextMenu = $state<{ x: number; y: number; path: string; isFavorite: boolean } | null>(null);

  let store: Awaited<ReturnType<typeof load>> | null = null;

  async function initStore() {
    store = await load("favorites.json", { autoSave: false });
    const saved = await store.get<string[]>("favorites");
    if (saved) {
      favorites = saved;
    }
  }

  async function saveFavorites() {
    if (store) {
      await store.set("favorites", favorites);
      await store.save();
    }
  }

  async function addFavorite(path: string) {
    if (!favorites.includes(path)) {
      favorites = [...favorites, path];
      await saveFavorites();
    }
  }

  async function removeFavorite(path: string) {
    favorites = favorites.filter((f) => f !== path);
    await saveFavorites();
  }

  function handleContextMenu(e: MouseEvent, path: string, isFavorite: boolean) {
    e.preventDefault();
    contextMenu = { x: e.clientX, y: e.clientY, path, isFavorite };
  }

  function closeContextMenu() {
    contextMenu = null;
  }

  async function handleContextAction() {
    if (!contextMenu) return;
    if (contextMenu.isFavorite) {
      await removeFavorite(contextMenu.path);
    } else {
      await addFavorite(contextMenu.path);
    }
    closeContextMenu();
  }

  async function loadRoots() {
    const drives = await listDrives();
    roots = drives.map((drive) => ({
      entry: { name: drive, path: drive, is_dir: true, is_image: false },
      children: null,
      expanded: false,
      loading: false,
    }));

    if (roots.length > 0) {
      await toggleNode(roots[0]);
    }
  }

  async function toggleNode(node: TreeNode) {
    if (!node.entry.is_dir) return;

    if (node.expanded) {
      node.expanded = false;
      return;
    }

    if (node.children === null) {
      node.loading = true;
      try {
        const entries = await listDirectory(node.entry.path);
        node.children = entries
          .filter((e) => e.is_dir)
          .map((entry) => ({
            entry,
            children: null,
            expanded: false,
            loading: false,
          }));
      } catch (e) {
        node.children = [];
      }
      node.loading = false;
    }

    node.expanded = true;
  }

  function selectFolder(node: TreeNode) {
    selectedPath = node.entry.path;
    onSelectFolder(node.entry.path);
    toggleNode(node);
  }

  function selectFavorite(path: string) {
    selectedPath = path;
    onSelectFolder(path);
  }

  function getFolderName(path: string): string {
    const parts = path.replace(/[/\\]+$/, "").split(/[/\\]/);
    return parts[parts.length - 1] || path;
  }

  $effect(() => {
    loadRoots();
    initStore();
  });
</script>

<svelte:window onclick={closeContextMenu} />

<div class="folder-tree">
  {#if favorites.length > 0}
    <div class="section-header">⭐ お気に入り</div>
    <div class="favorites">
      {#each favorites as fav}
        <button
          class="tree-item"
          class:selected={selectedPath === fav}
          onclick={() => selectFavorite(fav)}
          oncontextmenu={(e) => handleContextMenu(e, fav, true)}
          title={fav}
        >
          <span class="icon">📁</span>
          <span class="name">{getFolderName(fav)}</span>
        </button>
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
  <button
    class="tree-item"
    class:selected={selectedPath === node.entry.path}
    style="padding-left: {12 + depth * 16}px"
    onclick={() => selectFolder(node)}
    oncontextmenu={(e) => handleContextMenu(e, node.entry.path, favorites.includes(node.entry.path))}
  >
    <span class="icon">
      {#if node.loading}
        ⏳
      {:else if node.expanded}
        📂
      {:else}
        📁
      {/if}
    </span>
    <span class="name">{node.entry.name}</span>
  </button>

  {#if node.expanded && node.children}
    {#each node.children as child}
      {@render treeNode(child, depth + 1)}
    {/each}
  {/if}
{/snippet}

{#if contextMenu}
  <div
    class="context-menu"
    style="left: {contextMenu.x}px; top: {contextMenu.y}px"
  >
    <button class="context-item" onclick={handleContextAction}>
      {#if contextMenu.isFavorite}
        ✕ お気に入りから削除
      {:else}
        ⭐ お気に入りに追加
      {/if}
    </button>
  </div>
{/if}

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
  }

  .tree-content {
    flex: 1;
    overflow-y: auto;
    padding: 4px 0;
  }

  .tree-item {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 4px 12px;
    border: none;
    background: none;
    color: var(--text-primary);
    font-size: 13px;
    cursor: pointer;
    text-align: left;
  }

  .tree-item:hover {
    background: var(--bg-hover);
  }

  .tree-item.selected {
    background: var(--accent-bg);
    color: var(--accent);
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

  .context-menu {
    position: fixed;
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--radius);
    padding: 4px 0;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.4);
    z-index: 300;
    min-width: 180px;
  }

  .context-item {
    display: block;
    width: 100%;
    padding: 8px 14px;
    border: none;
    background: none;
    color: var(--text-primary);
    font-size: 13px;
    cursor: pointer;
    text-align: left;
  }

  .context-item:hover {
    background: var(--bg-hover);
  }
</style>
