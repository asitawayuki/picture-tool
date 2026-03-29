<script lang="ts">
  import type { ExifFrameConfig, ExifPosition, DisplayItems } from './types';
  import { renderExifFramePreview, listPresets } from './api';

  interface Props {
    visible: boolean;
    previewImagePath: string | null;
    bgColor: "white" | "black";
    onClose: () => void;
    onSave: (config: ExifFrameConfig) => void;
  }

  let { visible, previewImagePath, bgColor, onClose, onSave }: Props = $props();

  // Default config factory
  function defaultConfig(): ExifFrameConfig {
    return {
      name: 'default',
      position: 'auto',
      items: {
        maker_logo: true,
        lens_brand_logo: true,
        camera_model: true,
        lens_model: true,
        focal_length: true,
        f_number: true,
        shutter_speed: true,
        iso: true,
        date_taken: false,
        custom_text: false,
      },
      font: { font_path: null, primary_size: 0.025, secondary_size: 0.018 },
      custom_text: '',
    };
  }

  let config = $state<ExifFrameConfig>(defaultConfig());
  let presets = $state<ExifFrameConfig[]>([]);
  let selectedPresetName = $state('default');
  let previewSrc = $state('');
  let previewLoading = $state(false);

  // Load presets on mount
  $effect(() => {
    if (visible) {
      listPresets().then(p => { presets = p; });
    }
  });

  // Live preview with debounce
  let debounceTimer: ReturnType<typeof setTimeout>;
  $effect(() => {
    const _ = JSON.stringify(config);
    if (!visible || !previewImagePath) return;
    clearTimeout(debounceTimer);
    debounceTimer = setTimeout(async () => {
      if (!previewImagePath) return;
      previewLoading = true;
      try {
        previewSrc = await renderExifFramePreview(previewImagePath, config, bgColor);
      } catch (e) {
        console.error('Preview failed:', e);
      } finally {
        previewLoading = false;
      }
    }, 300);
    return () => clearTimeout(debounceTimer);
  });

  // Preset selection handler
  function selectPreset(name: string) {
    selectedPresetName = name;
    const preset = presets.find(p => p.name === name);
    if (preset) {
      config = { ...preset };
    }
  }

  // Position options
  const positionOptions: { value: ExifPosition; label: string }[] = [
    { value: 'auto', label: '自動' },
    { value: 'bottom', label: '下' },
    { value: 'top', label: '上' },
    { value: 'right', label: '右' },
    { value: 'left', label: '左' },
  ];

  // Display item labels (brand_logo removed)
  const displayItemKeys: { key: keyof DisplayItems; label: string }[] = [
    { key: 'maker_logo', label: 'ロゴ' },
    { key: 'lens_brand_logo', label: 'レンズブランド' },
    { key: 'camera_model', label: 'カメラ' },
    { key: 'lens_model', label: 'レンズ' },
    { key: 'focal_length', label: '焦点距離' },
    { key: 'f_number', label: 'F値' },
    { key: 'shutter_speed', label: 'SS' },
    { key: 'iso', label: 'ISO' },
    { key: 'date_taken', label: '日時' },
    { key: 'custom_text', label: 'テキスト' },
  ];

  function handleSave() {
    onSave(config);
  }
</script>

{#if visible}
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
  <div class="overlay" role="presentation" onclick={onClose}>
    <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
    <div class="modal" role="dialog" tabindex="-1" onclick={(e) => e.stopPropagation()}>
      <header>
        <h2>Exifフレーム設定</h2>
        <button class="close-btn" onclick={onClose}>✕</button>
      </header>

      <div class="body">
        <!-- Settings -->
        <div class="settings">
          <!-- Preset -->
          <section>
            <span class="label">プリセット</span>
            <select value={selectedPresetName} onchange={(e) => selectPreset(e.currentTarget.value)}>
              {#each presets as preset}
                <option value={preset.name}>{preset.name}</option>
              {/each}
            </select>
          </section>

          <!-- Position -->
          <div class="setting-group">
            <label>配置位置</label>
            <div class="position-selector">
              {#each positionOptions as opt}
                <button
                  class="position-btn"
                  class:active={config.position === opt.value}
                  onclick={() => config.position = opt.value}
                >
                  {opt.label}
                </button>
              {/each}
            </div>
          </div>

          <!-- Display Items -->
          <section>
            <span class="label">表示項目</span>
            <div class="tags">
              {#each displayItemKeys as item}
                <button
                  class="tag"
                  class:active={config.items[item.key]}
                  onclick={() => config.items[item.key] = !config.items[item.key]}
                >
                  {item.label}
                </button>
              {/each}
            </div>
          </section>

          <!-- Font Size -->
          <section>
            <span class="label">フォントサイズ</span>
            <div class="slider-row">
              <span class="slider-label">メイン</span>
              <input type="range" min="0.015" max="0.05" step="0.001" bind:value={config.font.primary_size} />
              <span class="slider-value">{(config.font.primary_size * 100).toFixed(1)}%</span>
            </div>
            <div class="slider-row">
              <span class="slider-label">サブ</span>
              <input type="range" min="0.01" max="0.035" step="0.001" bind:value={config.font.secondary_size} />
              <span class="slider-value">{(config.font.secondary_size * 100).toFixed(1)}%</span>
            </div>
          </section>

          <!-- Custom Text -->
          <section>
            <span class="label">カスタムテキスト</span>
            <input type="text" bind:value={config.custom_text} placeholder="@username" />
          </section>
        </div>

        <!-- Preview -->
        <div class="preview">
          <div class="preview-label">ライブプレビュー</div>
          {#if previewLoading}
            <div class="preview-loading">読み込み中...</div>
          {:else if previewSrc}
            <img src={previewSrc} alt="Preview" class="preview-img" />
          {:else}
            <div class="preview-empty">画像を選択してください</div>
          {/if}
        </div>
      </div>

      <footer>
        <button class="btn-cancel" onclick={onClose}>キャンセル</button>
        <button class="btn-save" onclick={handleSave}>保存</button>
      </footer>
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background: rgba(0, 0, 0, 0.6);
    z-index: 1000;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .modal {
    background: var(--bg-secondary, #1a1a2e);
    border: 1px solid var(--border-color, #333);
    border-radius: 12px;
    width: 90vw;
    max-width: 800px;
    max-height: 85vh;
    display: flex;
    flex-direction: column;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.5);
  }

  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 16px 20px;
    border-bottom: 1px solid var(--border-color, #333);
  }

  header h2 {
    margin: 0;
    font-size: 16px;
    color: var(--text-primary, #e0e0e0);
  }

  .close-btn {
    background: var(--bg-hover, #252540);
    border: none;
    color: var(--text-secondary, #888);
    width: 28px;
    height: 28px;
    border-radius: var(--radius-sm, 4px);
    cursor: pointer;
    font-size: 14px;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .close-btn:hover {
    color: var(--text-primary, #e0e0e0);
  }

  .body {
    display: flex;
    flex: 1;
    overflow: hidden;
    min-height: 0;
  }

  .settings {
    flex: 1;
    padding: 16px 20px;
    overflow-y: auto;
  }

  section {
    margin-bottom: 16px;
  }

  .label {
    display: block;
    font-size: 11px;
    color: var(--text-secondary, #888);
    margin-bottom: 6px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  select, input[type="text"] {
    width: 100%;
    background: var(--bg-primary, #0f0f1a);
    border: 1px solid var(--border-color, #333);
    color: var(--text-primary, #e0e0e0);
    padding: 6px 10px;
    border-radius: var(--radius-sm, 4px);
    font-size: 13px;
  }

  .setting-group {
    margin-bottom: 16px;
  }

  .setting-group label {
    display: block;
    font-size: 11px;
    color: var(--text-secondary, #888);
    margin-bottom: 6px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .position-selector {
    display: flex;
    gap: 6px;
  }

  .position-btn {
    flex: 1;
    background: var(--bg-primary, #0f0f1a);
    border: 1px solid var(--border-color, #333);
    color: var(--text-secondary, #888);
    padding: 6px 4px;
    border-radius: var(--radius-sm, 4px);
    cursor: pointer;
    font-size: 12px;
    transition: all 0.15s;
  }

  .position-btn.active {
    border-color: var(--accent, #818cf8);
    color: var(--accent, #818cf8);
    background: var(--accent-bg, rgba(99, 102, 241, 0.15));
  }

  .tags {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .tag {
    background: var(--bg-primary, #0f0f1a);
    border: 1px solid var(--border-color, #333);
    color: var(--text-secondary, #888);
    padding: 3px 10px;
    border-radius: 12px;
    cursor: pointer;
    font-size: 11px;
    transition: all 0.15s;
  }

  .tag.active {
    background: var(--accent-bg, rgba(99, 102, 241, 0.15));
    border-color: var(--accent, #818cf8);
    color: var(--accent, #818cf8);
  }

  .slider-row {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 4px;
  }

  .slider-label {
    font-size: 11px;
    color: var(--text-secondary, #888);
    min-width: 36px;
  }

  .slider-value {
    font-size: 11px;
    color: var(--text-secondary, #888);
    min-width: 40px;
    text-align: right;
  }

  input[type="range"] {
    flex: 1;
    accent-color: var(--accent, #818cf8);
  }

  .preview {
    width: 220px;
    background: var(--bg-primary, #0f0f1a);
    border-left: 1px solid var(--border-color, #333);
    padding: 16px;
    display: flex;
    flex-direction: column;
    align-items: center;
  }

  .preview-label {
    font-size: 11px;
    color: var(--text-secondary, #888);
    margin-bottom: 12px;
  }

  .preview-img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
    border-radius: var(--radius-sm, 4px);
  }

  .preview-loading, .preview-empty {
    color: var(--text-secondary, #888);
    font-size: 12px;
    text-align: center;
    padding: 40px 0;
  }

  footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 12px 20px;
    border-top: 1px solid var(--border-color, #333);
  }

  .btn-cancel {
    background: var(--bg-hover, #252540);
    border: 1px solid var(--border-color, #333);
    color: var(--text-primary, #e0e0e0);
    padding: 6px 20px;
    border-radius: var(--radius, 6px);
    cursor: pointer;
    font-size: 13px;
  }

  .btn-save {
    background: var(--accent, #818cf8);
    border: none;
    color: #fff;
    padding: 6px 20px;
    border-radius: var(--radius, 6px);
    cursor: pointer;
    font-size: 13px;
  }

  .btn-save:hover {
    background: var(--accent-hover, #6366f1);
  }
</style>
