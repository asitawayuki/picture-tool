<script lang="ts">
  import Button from "./ui/Button.svelte";
  import Card from "./ui/Card.svelte";
  import Dialog from "./ui/Dialog.svelte";
  import type { ImageEntry, ProcessBatchResponse } from "./types";

  interface Props {
    /** 変換を依頼した画像。キャンセル分は results にも failures にも現れない */
    requested: ImageEntry[];
    response: ProcessBatchResponse;
    /** 利用者がキャンセルした場合、未処理分は「失敗」ではないので区別する */
    cancelled: boolean;
    onClose: () => void;
  }

  let { requested, response, cancelled, onClose }: Props = $props();

  function baseName(path: string): string {
    const parts = path.replace(/[/\\]+$/, "").split(/[/\\]/);
    return parts[parts.length - 1] || path;
  }

  let results = $derived(response.results);

  /**
   * 失敗はバックエンドが理由付きで返す。
   * 依頼したのに成功にも失敗にも現れないものはキャンセルによる未処理。
   */
  let failed = $derived(
    response.failures.map((f) => ({ name: baseName(f.input_path), path: f.input_path, error: f.error }))
  );
  let accountedPaths = $derived(
    new Set([...results.map((r) => r.input_path), ...response.failures.map((f) => f.input_path)])
  );
  let unprocessed = $derived(requested.filter((img) => !accountedPaths.has(img.path)));

  /** 品質を下限まで下げても最大サイズを満たせなかったもの */
  let oversized = $derived(results.filter((r) => r.size_limit_exceeded));

  /** core が ProcessResult.warnings に積んだ事象（これまで GUI で捨てられていた） */
  let warnings = $derived([
    ...response.warnings.map((message) => ({ file: "", message })),
    ...results.flatMap((r) =>
      r.warnings.map((message) => ({ file: baseName(r.input_path), message }))
    ),
  ]);

  let hasIssues = $derived(
    failed.length > 0 || unprocessed.length > 0 || oversized.length > 0 || warnings.length > 0
  );
</script>

<Dialog title="変換結果" onClose={onClose}>
  <div class="summary">
    <Card level={2} padding="var(--space-3)">
      <span class="value">{results.length}</span>
      <span class="key">成功</span>
    </Card>
    <Card level={2} padding="var(--space-3)">
      <span class="value" class:danger={failed.length > 0}>{failed.length}</span>
      <span class="key">失敗</span>
    </Card>
    {#if unprocessed.length > 0}
      <Card level={2} padding="var(--space-3)">
        <span class="value">{unprocessed.length}</span>
        <span class="key">{cancelled ? "未処理" : "不明"}</span>
      </Card>
    {/if}
    <Card level={2} padding="var(--space-3)">
      <span class="value">{requested.length}</span>
      <span class="key">対象</span>
    </Card>
  </div>

  {#if !hasIssues}
    <p class="all-ok">すべて正常に変換しました。</p>
  {/if}

  <!-- Card は自分の外側の余白を持たない（padding だけ）。積むと角丸が
       噛み合って 1 枚に見えるので、間隔はここで持つ -->
  <div class="sections">
    {#if failed.length > 0}
      <Card level={1} title="変換できなかったファイル ({failed.length})">
        <ul>
          {#each failed as f (f.path)}
            <li>{f.name} — {f.error}</li>
          {/each}
        </ul>
      </Card>
    {/if}

    {#if unprocessed.length > 0}
      <Card
        level={1}
        title="{cancelled ? 'キャンセルにより未処理' : '結果が返らなかったファイル'} ({unprocessed.length})"
      >
        <ul>
          {#each unprocessed as img (img.path)}
            <li>{img.name}</li>
          {/each}
        </ul>
      </Card>
    {/if}

    {#if oversized.length > 0}
      <Card level={1} title="最大サイズに収まらなかったファイル ({oversized.length})">
        <p class="note">品質を下限まで下げても指定サイズを超えています。</p>
        <ul>
          {#each oversized as r (r.input_path)}
            <li>{baseName(r.input_path)} — {r.final_size_mb.toFixed(2)}MB</li>
          {/each}
        </ul>
      </Card>
    {/if}

    {#if warnings.length > 0}
      <Card level={1} title="警告 ({warnings.length})">
        <ul>
          {#each warnings as w, i (i)}
            <li>{w.file ? `${w.file} — ` : ""}{w.message}</li>
          {/each}
        </ul>
      </Card>
    {/if}
  </div>

  {#snippet actions()}
    <Button variant="filled" onclick={onClose}>閉じる</Button>
  {/snippet}
</Dialog>

<style>
  .summary {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(88px, 1fr));
    gap: var(--space-2);
    margin-bottom: var(--space-4);
    text-align: center;
  }

  .value {
    display: block;
    font: var(--md-sys-typescale-title-md);
    font-variant-numeric: tabular-nums;
  }

  .value.danger {
    color: var(--md-sys-color-error);
  }

  .key {
    display: block;
    font: var(--md-sys-typescale-body-sm);
    color: var(--md-sys-color-on-surface-variant);
  }

  .all-ok {
    margin: 0 0 var(--space-4);
  }

  .sections {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  ul {
    margin: 0;
    padding-left: var(--space-5);
  }

  li {
    font: var(--md-sys-typescale-body-sm);
    line-height: 1.7;
    overflow-wrap: anywhere;
  }

  .note {
    margin: 0 0 var(--space-2);
    font: var(--md-sys-typescale-body-sm);
    color: var(--md-sys-color-on-surface-variant);
  }
</style>
