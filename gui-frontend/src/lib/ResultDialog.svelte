<script lang="ts">
  import { focusTrap } from "./focusTrap";
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

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="overlay">
  <div
    class="dialog"
    role="dialog"
    aria-modal="true"
    aria-labelledby="result-title"
    tabindex="-1"
    use:focusTrap
  >
    <header>
      <h2 id="result-title">変換結果</h2>
      <button class="close-btn" aria-label="閉じる" onclick={onClose}>✕</button>
    </header>

    <div class="summary">
      <div class="stat">
        <span class="value success">{results.length}</span>
        <span class="key">成功</span>
      </div>
      <div class="stat">
        <span class="value" class:danger={failed.length > 0}>{failed.length}</span>
        <span class="key">失敗</span>
      </div>
      {#if unprocessed.length > 0}
        <div class="stat">
          <span class="value">{unprocessed.length}</span>
          <span class="key">{cancelled ? "未処理" : "不明"}</span>
        </div>
      {/if}
      <div class="stat">
        <span class="value">{requested.length}</span>
        <span class="key">対象</span>
      </div>
    </div>

    <div class="body">
      {#if !hasIssues}
        <p class="all-ok">すべて正常に変換しました。</p>
      {/if}

      {#if failed.length > 0}
        <section>
          <h3 class="danger">変換できなかったファイル ({failed.length})</h3>
          <ul>
            {#each failed as f (f.path)}
              <li>{f.name} — {f.error}</li>
            {/each}
          </ul>
        </section>
      {/if}

      {#if unprocessed.length > 0}
        <section>
          <h3 class="warning">
            {cancelled ? "キャンセルにより未処理" : "結果が返らなかったファイル"} ({unprocessed.length})
          </h3>
          <ul>
            {#each unprocessed as img (img.path)}
              <li>{img.name}</li>
            {/each}
          </ul>
        </section>
      {/if}

      {#if oversized.length > 0}
        <section>
          <h3 class="warning">最大サイズに収まらなかったファイル ({oversized.length})</h3>
          <p class="note">品質を下限まで下げても指定サイズを超えています。</p>
          <ul>
            {#each oversized as r (r.input_path)}
              <li>{baseName(r.input_path)} — {r.final_size_mb.toFixed(2)}MB</li>
            {/each}
          </ul>
        </section>
      {/if}

      {#if warnings.length > 0}
        <section>
          <h3 class="warning">警告 ({warnings.length})</h3>
          <ul>
            {#each warnings as w, i (i)}
              <li>{w.file ? `${w.file} — ${w.message}` : w.message}</li>
            {/each}
          </ul>
        </section>
      {/if}
    </div>

    <footer>
      <button class="btn-close" onclick={onClose}>閉じる</button>
    </footer>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.6);
    z-index: 500;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .dialog {
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--radius);
    width: 90vw;
    max-width: 520px;
    max-height: 80vh;
    display: flex;
    flex-direction: column;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.5);
  }

  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 14px 18px;
    border-bottom: 1px solid var(--border-color);
  }

  h2 {
    margin: 0;
    font-size: 15px;
    color: var(--text-primary);
  }

  .close-btn {
    background: var(--bg-hover);
    border: none;
    color: var(--text-secondary);
    width: 26px;
    height: 26px;
    border-radius: var(--radius-sm);
    cursor: pointer;
    font-size: 12px;
  }

  .close-btn:hover {
    color: var(--text-primary);
  }

  .summary {
    display: flex;
    gap: 24px;
    padding: 16px 18px;
    border-bottom: 1px solid var(--border-color);
  }

  .stat {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .stat .value {
    font-size: 22px;
    font-weight: 600;
    color: var(--text-primary);
    line-height: 1.1;
  }

  .stat .value.success {
    color: var(--success);
  }

  .stat .value.danger {
    color: var(--danger);
  }

  .stat .key {
    font-size: 11px;
    color: var(--text-secondary);
  }

  .body {
    flex: 1;
    overflow-y: auto;
    padding: 14px 18px;
    min-height: 0;
  }

  .all-ok {
    margin: 0;
    font-size: 13px;
    color: var(--text-secondary);
  }

  section {
    margin-bottom: 16px;
  }

  section:last-child {
    margin-bottom: 0;
  }

  h3 {
    margin: 0 0 6px;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-primary);
  }

  h3.danger {
    color: var(--danger);
  }

  h3.warning {
    color: var(--warning);
  }

  .note {
    margin: 0 0 6px;
    font-size: 12px;
    color: var(--text-secondary);
  }

  ul {
    margin: 0;
    padding-left: 18px;
    font-size: 12px;
    line-height: 1.7;
    color: var(--text-secondary);
  }

  li {
    overflow-wrap: anywhere;
  }

  footer {
    display: flex;
    justify-content: flex-end;
    padding: 12px 18px;
    border-top: 1px solid var(--border-color);
  }

  .btn-close {
    background: var(--accent);
    border: none;
    color: #fff;
    padding: 7px 20px;
    border-radius: var(--radius);
    cursor: pointer;
    font-size: 13px;
  }

  .btn-close:hover {
    background: var(--accent-hover);
  }
</style>
