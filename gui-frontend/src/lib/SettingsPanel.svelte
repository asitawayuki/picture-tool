<script lang="ts">
  import type { ProcessingConfig, ExifFrameConfig } from "./types";

  interface Props {
    config: ProcessingConfig;
    outputFolder: string;
    canProcess: boolean;
    onPickOutputFolder: () => void;
    onProcess: () => void;
    exifFrameEnabled: boolean;
    selectedPresetName: string;
    presets: ExifFrameConfig[];
    onExifFrameEnabledChange: (enabled: boolean) => void;
    onPresetChange: (name: string) => void;
    onOpenExifSettings: () => void;
  }

  let {
    config = $bindable(),
    outputFolder,
    canProcess,
    onPickOutputFolder,
    onProcess,
    exifFrameEnabled,
    selectedPresetName,
    presets,
    onExifFrameEnabledChange,
    onPresetChange,
    onOpenExifSettings,
  }: Props = $props();

  const MAX_WIDTH_MIN = 4;
  const MAX_WIDTH_MAX = 20000;

  // トグルを on にしたときに入れる値。off にしても直前の値を覚えておく。
  let lastMaxWidth = $state(1080);

  // 確定サイズ表示。テンプレート内で null 絞り込みに頼らないよう $derived で持つ。
  let maxWidthLabel = $derived(
    config.max_width === null ? "" : `${config.max_width}×${(config.max_width * 5) / 4}`
  );

  /**
   * 4 の倍数へ切り捨てる（Rust 側 `target_canvas` と同じ丸め方向）。
   * 切り上げると指定値を超えてしまい、「上限」という機能の目的を果たさない。
   */
  function snapWidth(value: number): number {
    const clamped = Math.min(Math.max(value, MAX_WIDTH_MIN), MAX_WIDTH_MAX);
    return Math.floor(clamped / 4) * 4;
  }

  function toggleMaxWidth(enabled: boolean) {
    config.max_width = enabled ? lastMaxWidth : null;
  }

  /**
   * 入力確定時に値をスナップする。`step="4"` はスピナーと HTML バリデーションにしか
   * 効かず、1002 を直接入力・貼り付けできてしまうため予防にならない。
   *
   * DOM の value も明示的に書き戻す。スナップ結果が現在の状態と同じ値のとき
   * （例: 1000 のときに 1002 を入力）は state が変化せず再描画されないので、
   * 表示だけ 1002 のまま残ってしまう。
   */
  function commitMaxWidth(input: HTMLInputElement) {
    const raw = input.value.trim();
    const parsed = Number(raw);
    if (raw !== "" && Number.isFinite(parsed)) {
      lastMaxWidth = snapWidth(parsed);
    }
    config.max_width = lastMaxWidth;
    input.value = String(lastMaxWidth);
  }
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
      <label class="checkbox">
        <input
          type="checkbox"
          checked={config.max_width !== null}
          disabled={config.mode === "quality"}
          onchange={(e) => toggleMaxWidth((e.target as HTMLInputElement).checked)}
        />
        <span>出力幅を制限する</span>
      </label>
      {#if config.mode === "quality"}
        <p class="hint">
          Quality モードは 4:5 に変換しないため、出力幅の上限は適用されません。
        </p>
      {:else if config.max_width !== null}
        <div class="max-width-row">
          <input
            type="number"
            min={MAX_WIDTH_MIN}
            max={MAX_WIDTH_MAX}
            aria-label="出力幅の上限 (px)"
            value={config.max_width}
            onchange={(e) => commitMaxWidth(e.currentTarget)}
          />
          <span class="derived">→ {maxWidthLabel}</span>
        </div>
      {/if}
    </div>

    <div class="field">
      <span class="label" id="output-folder-label">出力先</span>
      <button
        class="folder-btn"
        aria-labelledby="output-folder-label"
        title={outputFolder || undefined}
        onclick={onPickOutputFolder}
      >
        {outputFolder || "フォルダーを選択..."}
      </button>
    </div>

    <div class="field">
      <label class="checkbox">
        <input type="checkbox" bind:checked={config.delete_originals} />
        <span>元ファイルを削除</span>
      </label>
      {#if config.delete_originals}
        <p class="danger-hint">変換実行時に確認します。削除したファイルは元に戻せません。</p>
      {/if}
    </div>

    {#if config.mode === "pad"}
      <div class="exif-frame-section">
        <label class="checkbox">
          <input
            type="checkbox"
            checked={exifFrameEnabled}
            onchange={(e) => onExifFrameEnabledChange((e.target as HTMLInputElement).checked)}
          />
          <span>Exifフレーム</span>
        </label>

        {#if exifFrameEnabled}
          <div class="exif-frame-controls">
            <select
              aria-label="Exifフレームのプリセット"
              value={selectedPresetName}
              onchange={(e) => onPresetChange((e.target as HTMLSelectElement).value)}
            >
              {#each presets as preset (preset.name)}
                <option value={preset.name}>{preset.name}</option>
              {/each}
              {#if presets.length === 0}
                <option value="default">default</option>
              {/if}
            </select>
            <button
              class="gear-btn"
              onclick={onOpenExifSettings}
              aria-label="Exifフレーム設定を開く"
              title="Exifフレーム設定"
            >⚙</button>
          </div>
        {/if}
      </div>
    {/if}
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

  .danger-hint {
    margin: 4px 0 0;
    font-size: 11px;
    line-height: 1.5;
    color: var(--danger);
  }

  .hint {
    margin: 4px 0 0;
    font-size: 11px;
    line-height: 1.5;
    color: var(--text-secondary);
  }

  .max-width-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .max-width-row input[type="number"] {
    width: 90px;
    background: var(--bg-primary);
    border: 1px solid var(--border-color);
    color: var(--text-primary);
    padding: 4px 8px;
    border-radius: var(--radius-sm);
    font-size: 12px;
  }

  .derived {
    font-size: 11px;
    color: var(--text-secondary);
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

  .exif-frame-section {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .exif-frame-controls {
    display: flex;
    gap: 4px;
    align-items: center;
  }

  .exif-frame-controls select {
    flex: 1;
  }

  .gear-btn {
    padding: 4px 8px;
    background: var(--bg-primary);
    border: 1px solid var(--border-color);
    color: var(--text-secondary);
    border-radius: var(--radius-sm);
    cursor: pointer;
    font-size: 14px;
    line-height: 1;
    flex-shrink: 0;
  }

  .gear-btn:hover {
    border-color: var(--accent);
    color: var(--text-primary);
  }
</style>
