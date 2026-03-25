<script lang="ts">
  import type { ProcessingConfig } from "./types";

  interface Props {
    config: ProcessingConfig;
    outputFolder: string;
    canProcess: boolean;
    currentFolder: string;
    onPickOutputFolder: () => void;
    onProcess: () => void;
  }

  let { config = $bindable(), outputFolder, canProcess, currentFolder, onPickOutputFolder, onProcess }: Props = $props();
</script>

<div class="settings-panel">
  <div class="header">設定</div>
  <div class="settings">
    <label class="field">
      <span class="label">モード</span>
      <select bind:value={config.mode}>
        <option value="crop">Crop (中央クロップ)</option>
        <option value="pad">Pad (パディング)</option>
        <option value="quality">Quality (サイズのみ)</option>
      </select>
    </label>

    {#if config.mode === "pad"}
      <label class="field">
        <span class="label">背景色</span>
        <select bind:value={config.bg_color}>
          <option value="white">白</option>
          <option value="black">黒</option>
        </select>
      </label>
    {/if}

    <label class="field">
      <span class="label">品質: {config.quality}%</span>
      <input type="range" min="1" max="100" bind:value={config.quality} />
    </label>

    <label class="field">
      <span class="label">最大サイズ: {config.max_size_mb}MB</span>
      <input type="range" min="1" max="50" bind:value={config.max_size_mb} />
    </label>

    <div class="field">
      <span class="label">出力先</span>
      <button class="folder-btn" onclick={onPickOutputFolder}>
        {outputFolder || "フォルダーを選択..."}
      </button>
    </div>

    <label class="checkbox">
      <input type="checkbox" bind:checked={config.delete_originals} />
      <span>元ファイルを削除</span>
    </label>
  </div>

  <div class="action">
    <button class="process-btn" disabled={!canProcess} onclick={onProcess}>
      変換実行 →
    </button>
  </div>
</div>

<style>
  .settings-panel {
    display: flex;
    flex-direction: column;
    border-top: 1px solid var(--border-color);
  }

  .header {
    padding: 12px;
    color: var(--text-secondary);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 1px;
  }

  .settings {
    padding: 0 12px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .label {
    font-size: 12px;
    color: var(--text-secondary);
  }

  select {
    width: 100%;
    background: var(--bg-primary);
    border: 1px solid var(--border-color);
    color: var(--text-primary);
    padding: 4px 8px;
    border-radius: var(--radius-sm);
    font-size: 12px;
  }

  input[type="range"] {
    width: 100%;
    height: 20px;
    -webkit-appearance: none;
    appearance: none;
    background: transparent;
    cursor: pointer;
    padding: 0;
    margin: 0;
  }

  input[type="range"]::-webkit-slider-runnable-track {
    height: 4px;
    background: #555;
    border-radius: 2px;
    border: 1px solid var(--border-color);
  }

  input[type="range"]::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: var(--accent);
    border: none;
    margin-top: -6px;
    cursor: pointer;
  }

  input[type="range"]::-webkit-slider-thumb:hover {
    background: var(--accent-hover);
  }

  input[type="range"]::-moz-range-track {
    height: 4px;
    background: #555;
    border-radius: 2px;
    border: 1px solid var(--border-color);
  }

  input[type="range"]::-moz-range-thumb {
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: var(--accent);
    border: none;
    cursor: pointer;
  }

  .folder-btn {
    width: 100%;
    padding: 6px 8px;
    background: var(--bg-primary);
    border: 1px solid var(--border-color);
    color: var(--text-secondary);
    border-radius: var(--radius-sm);
    cursor: pointer;
    font-size: 11px;
    text-align: left;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .folder-btn:hover {
    border-color: var(--accent);
  }

  .checkbox {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    color: var(--text-secondary);
    cursor: pointer;
  }

  .action {
    padding: 12px;
  }

  .process-btn {
    width: 100%;
    padding: 10px;
    background: var(--accent);
    color: white;
    border: none;
    border-radius: var(--radius);
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
  }

  .process-btn:hover:not(:disabled) {
    background: var(--accent-hover);
  }

  .process-btn:disabled {
    opacity: 0.4;
    cursor: default;
  }
</style>
