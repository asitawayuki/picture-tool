<script lang="ts">
  import { listDirectory, listDrives } from "./api";
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

  async function loadRoots() {
    const drives = await listDrives();
    roots = drives.map((drive) => ({
      entry: { name: drive, path: drive, is_dir: true, is_image: false },
      children: null,
      expanded: false,
      loading: false,
    }));

    // 最初のドライブを自動展開
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

  $effect(() => {
    loadRoots();
  });
</script>

<div class="folder-tree">
  <div class="header">フォルダー</div>
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

<style>
  .folder-tree {
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--bg-secondary);
    overflow: hidden;
  }

  .header {
    padding: 12px;
    color: var(--text-secondary);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 1px;
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
</style>
