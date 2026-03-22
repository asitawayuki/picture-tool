<script lang="ts">
  import type { ProgressPayload } from "./types";

  interface Props {
    progress: ProgressPayload | null;
    onCancel: () => void;
  }

  let { progress, onCancel }: Props = $props();

  let percentage = $derived(
    progress ? Math.round((progress.current / progress.total) * 100) : 0
  );
</script>

{#if progress}
  <div class="overlay">
    <div class="modal">
      <h3>変換中...</h3>
      <div class="progress-bar">
        <div class="progress-fill" style="width: {percentage}%"></div>
      </div>
      <div class="info">
        <span>{progress.current} / {progress.total}</span>
        <span>{progress.file_name}</span>
      </div>
      <button class="cancel-btn" onclick={onCancel}>キャンセル</button>
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.7);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
  }

  .modal {
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: 12px;
    padding: 32px;
    min-width: 400px;
    text-align: center;
  }

  h3 {
    margin-bottom: 20px;
    font-size: 18px;
  }

  .progress-bar {
    height: 8px;
    background: var(--bg-primary);
    border-radius: 4px;
    overflow: hidden;
    margin-bottom: 12px;
  }

  .progress-fill {
    height: 100%;
    background: var(--accent);
    border-radius: 4px;
    transition: width 0.3s ease;
  }

  .info {
    display: flex;
    justify-content: space-between;
    font-size: 12px;
    color: var(--text-secondary);
    margin-bottom: 20px;
  }

  .cancel-btn {
    padding: 8px 24px;
    background: none;
    border: 1px solid var(--border-color);
    color: var(--text-primary);
    border-radius: var(--radius);
    cursor: pointer;
    font-size: 13px;
  }

  .cancel-btn:hover {
    border-color: var(--danger);
    color: var(--danger);
  }
</style>
