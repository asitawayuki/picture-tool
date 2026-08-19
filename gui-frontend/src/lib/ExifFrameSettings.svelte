<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import type { ExifFrameConfig, ExifPosition, DisplayItems, FontInfo } from './types';
  import { renderExifFramePreview, listAvailableFonts } from './api';
  import { focusTrap } from './focusTrap';
  import { toast, describeError } from './toasts.svelte';

  interface Props {
    /** プリセット一覧は App が唯一の保持者。ここでは読むだけ。 */
    presets: ExifFrameConfig[];
    /** ConvertPanel で選択中のプリセット。これを初期値にしないと別プリセットを上書きしてしまう。 */
    selectedPresetName: string;
    previewImagePath: string | null;
    bgColor: "white" | "black";
    onClose: () => void;
    onSave: (config: ExifFrameConfig) => void;
    onDelete: (name: string) => void;
  }

  let { presets, selectedPresetName, previewImagePath, bgColor, onClose, onSave, onDelete }: Props = $props();

  /** バンドルプリセット名。ユーザーファイルが無くても常に存在するため削除させない。 */
  const BUNDLED_PRESET_NAME = 'default';

  // Default config factory
  function defaultConfig(): ExifFrameConfig {
    return {
      name: BUNDLED_PRESET_NAME,
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

  /**
   * プリセットは必ず深いコピーを取ってから編集する。
   * シャローコピーだと `items` / `font` が `presets` 内のオブジェクトと同一参照になり、
   * 編集がそのまま一覧側を書き換えてしまう。
   */
  function cloneConfig(preset: ExifFrameConfig): ExifFrameConfig {
    return structuredClone($state.snapshot(preset)) as ExifFrameConfig;
  }

  function configFor(name: string): ExifFrameConfig {
    const preset = presets.find((p) => p.name === name);
    return preset ? cloneConfig(preset) : defaultConfig();
  }

  // 開いた時点で選択されているプリセットから始める。
  // このダイアログは開くたびに再マウントされるので初期値を1回捕まえれば十分であり、
  // 以降は利用者がここで選び直した内容を正とする（untrack で意図を明示する）。
  let config = $state<ExifFrameConfig>(untrack(() => configFor(selectedPresetName)));
  let editingPresetName = $state(untrack(() => selectedPresetName));
  let previewSrc = $state('');
  let previewLoading = $state(false);
  let fonts = $state<FontInfo[]>([]);

  let canDelete = $derived(
    editingPresetName !== BUNDLED_PRESET_NAME &&
      presets.some((p) => p.name === editingPresetName)
  );
  /** 名前を変えて保存すれば新規プリセットになる */
  let isNewPreset = $derived(!presets.some((p) => p.name === config.name.trim()));
  let canSave = $derived(config.name.trim().length > 0);

  onMount(() => {
    listAvailableFonts()
      .then((f) => {
        fonts = f;
      })
      .catch((e) => {
        toast.error(`フォント一覧の取得に失敗しました: ${describeError(e)}`);
      });
  });

  // Live preview with debounce
  let debounceTimer: ReturnType<typeof setTimeout>;
  // プレビューは設定を触るたびに再生成されるため、同じ警告を毎回出さないよう記録する
  const reportedWarnings = new Set<string>();
  $effect(() => {
    // 依存は $effect の同期フェーズで読む必要がある。
    // 非同期コールバック内でしか参照しないと依存として追跡されない。
    const snapshot = $state.snapshot(config) as ExifFrameConfig;
    const bg = bgColor;
    const path = previewImagePath;
    if (!path) return;

    clearTimeout(debounceTimer);
    debounceTimer = setTimeout(async () => {
      previewLoading = true;
      try {
        const preview = await renderExifFramePreview(path, snapshot, bg);
        previewSrc = preview.data_url;
        // カスタム model_map の不備など。以前はバックエンドで eprintln! するだけで
        // GUI からは見えなかった（S6-M15）。
        for (const warning of preview.warnings) {
          if (reportedWarnings.has(warning)) continue;
          reportedWarnings.add(warning);
          toast.error(warning);
        }
      } catch (e) {
        toast.error(`プレビューの生成に失敗しました: ${describeError(e)}`);
      } finally {
        previewLoading = false;
      }
    }, 300);
    return () => clearTimeout(debounceTimer);
  });

  function selectPreset(name: string) {
    editingPresetName = name;
    config = configFor(name);
  }

  function selectFont(value: string) {
    config.font.font_path = value === '' ? null : value;
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
    if (!canSave) return;
    onSave({ ...$state.snapshot(config), name: config.name.trim() } as ExifFrameConfig);
  }

  function handleDelete() {
    if (!canDelete) return;
    onDelete(editingPresetName);
    onClose();
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      onClose();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="overlay">
  <div
    class="modal"
    role="dialog"
    aria-modal="true"
    aria-labelledby="exif-frame-title"
    tabindex="-1"
    use:focusTrap
  >
    <header>
      <h2 id="exif-frame-title">Exifフレーム設定</h2>
      <button class="close-btn" aria-label="閉じる" onclick={onClose}>✕</button>
    </header>

    <div class="body">
      <!-- Settings -->
      <div class="settings">
        <!-- Preset -->
        <section>
          <label class="label" for="ef-preset">プリセット</label>
          <div class="preset-row">
            <select id="ef-preset" value={editingPresetName} onchange={(e) => selectPreset(e.currentTarget.value)}>
              {#each presets as preset (preset.name)}
                <option value={preset.name}>{preset.name}</option>
              {/each}
            </select>
            <button
              class="delete-btn"
              disabled={!canDelete}
              title={canDelete ? 'このプリセットを削除' : '組み込みプリセットは削除できません'}
              onclick={handleDelete}
            >
              削除
            </button>
          </div>
        </section>

        <!-- Preset name -->
        <section>
          <label class="label" for="ef-name">プリセット名</label>
          <input id="ef-name" type="text" bind:value={config.name} placeholder="プリセット名" />
          <p class="hint">
            {#if isNewPreset}
              この名前で新しいプリセットとして保存されます。
            {:else}
              保存すると「{config.name.trim()}」を上書きします。
            {/if}
          </p>
        </section>

        <!-- Position -->
        <section>
          <span class="label" id="ef-position-label">配置位置</span>
          <div class="position-selector" role="group" aria-labelledby="ef-position-label">
            {#each positionOptions as opt (opt.value)}
              <button
                class="position-btn"
                class:active={config.position === opt.value}
                aria-pressed={config.position === opt.value}
                onclick={() => (config.position = opt.value)}
              >
                {opt.label}
              </button>
            {/each}
          </div>
        </section>

        <!-- Display Items -->
        <section>
          <span class="label" id="ef-items-label">表示項目</span>
          <div class="tags" role="group" aria-labelledby="ef-items-label">
            {#each displayItemKeys as item (item.key)}
              <button
                class="tag"
                class:active={config.items[item.key]}
                aria-pressed={config.items[item.key]}
                onclick={() => (config.items[item.key] = !config.items[item.key])}
              >
                {item.label}
              </button>
            {/each}
          </div>
        </section>

        <!-- Font -->
        <section>
          <label class="label" for="ef-font">フォント</label>
          <select id="ef-font" value={config.font.font_path ?? ''} onchange={(e) => selectFont(e.currentTarget.value)}>
            {#each fonts as font (font.path ?? '')}
              <option value={font.path ?? ''}>{font.display_name}</option>
            {/each}
            {#if config.font.font_path && !fonts.some((f) => f.path === config.font.font_path)}
              <!-- プリセットが参照するフォントが見つからない場合も選択状態を失わせない -->
              <option value={config.font.font_path}>{config.font.font_path}（見つかりません）</option>
            {/if}
          </select>
        </section>

        <!-- Font Size -->
        <section>
          <span class="label">フォントサイズ</span>
          <div class="slider-row">
            <label class="slider-label" for="ef-font-primary">メイン</label>
            <input id="ef-font-primary" type="range" min="0.015" max="0.05" step="0.001" bind:value={config.font.primary_size} />
            <span class="slider-value">{(config.font.primary_size * 100).toFixed(1)}%</span>
          </div>
          <div class="slider-row">
            <label class="slider-label" for="ef-font-secondary">サブ</label>
            <input id="ef-font-secondary" type="range" min="0.01" max="0.035" step="0.001" bind:value={config.font.secondary_size} />
            <span class="slider-value">{(config.font.secondary_size * 100).toFixed(1)}%</span>
          </div>
        </section>

        <!-- Custom Text -->
        <section>
          <label class="label" for="ef-custom-text">カスタムテキスト</label>
          <input id="ef-custom-text" type="text" bind:value={config.custom_text} placeholder="@username" />
        </section>
      </div>

      <!-- Preview -->
      <div class="preview">
        <div class="preview-label">ライブプレビュー</div>
        {#if previewLoading}
          <div class="preview-status">読み込み中...</div>
        {:else if previewSrc}
          <img src={previewSrc} alt="Exifフレームのプレビュー" class="preview-img" />
        {:else}
          <div class="preview-status">画像を選択してください</div>
        {/if}
      </div>
    </div>

    <footer>
      <button class="btn-cancel" onclick={onClose}>キャンセル</button>
      <button class="btn-save" disabled={!canSave} onclick={handleSave}>
        {isNewPreset ? '新規保存' : '保存'}
      </button>
    </footer>
  </div>
</div>

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
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
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
    border-bottom: 1px solid var(--border-color);
  }

  header h2 {
    margin: 0;
    font-size: 16px;
    color: var(--text-primary);
  }

  .close-btn {
    background: var(--bg-hover);
    border: none;
    color: var(--text-secondary);
    width: 28px;
    height: 28px;
    border-radius: var(--radius-sm);
    cursor: pointer;
    font-size: 14px;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .close-btn:hover {
    color: var(--text-primary);
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
    color: var(--text-secondary);
    margin-bottom: 6px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .hint {
    margin: 4px 0 0;
    font-size: 11px;
    color: var(--text-secondary);
  }

  select, input[type="text"] {
    width: 100%;
    background: var(--bg-primary);
    border: 1px solid var(--border-color);
    color: var(--text-primary);
    padding: 6px 10px;
    border-radius: var(--radius-sm);
    font-size: 13px;
  }

  .preset-row {
    display: flex;
    gap: 6px;
    align-items: center;
  }

  .preset-row select {
    flex: 1;
  }

  .delete-btn {
    flex-shrink: 0;
    background: var(--bg-primary);
    border: 1px solid var(--border-color);
    color: var(--text-secondary);
    padding: 6px 12px;
    border-radius: var(--radius-sm);
    cursor: pointer;
    font-size: 12px;
  }

  .delete-btn:hover:not(:disabled) {
    border-color: var(--danger);
    color: var(--danger);
  }

  .delete-btn:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .position-selector {
    display: flex;
    gap: 6px;
  }

  .position-btn {
    flex: 1;
    background: var(--bg-primary);
    border: 1px solid var(--border-color);
    color: var(--text-secondary);
    padding: 6px 4px;
    border-radius: var(--radius-sm);
    cursor: pointer;
    font-size: 12px;
    transition: all 0.15s;
  }

  .position-btn.active {
    border-color: var(--accent);
    color: var(--accent);
    background: var(--accent-bg);
  }

  .tags {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .tag {
    background: var(--bg-primary);
    border: 1px solid var(--border-color);
    color: var(--text-secondary);
    padding: 3px 10px;
    border-radius: 12px;
    cursor: pointer;
    font-size: 11px;
    transition: all 0.15s;
  }

  .tag.active {
    background: var(--accent-bg);
    border-color: var(--accent);
    color: var(--accent);
  }

  .slider-row {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 4px;
  }

  .slider-label {
    font-size: 11px;
    color: var(--text-secondary);
    min-width: 36px;
  }

  .slider-value {
    font-size: 11px;
    color: var(--text-secondary);
    min-width: 40px;
    text-align: right;
  }

  input[type="range"] {
    flex: 1;
    accent-color: var(--accent);
  }

  .preview {
    width: 220px;
    background: var(--bg-primary);
    border-left: 1px solid var(--border-color);
    padding: 16px;
    display: flex;
    flex-direction: column;
    align-items: center;
  }

  .preview-label {
    font-size: 11px;
    color: var(--text-secondary);
    margin-bottom: 12px;
  }

  .preview-img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
    border-radius: var(--radius-sm);
  }

  .preview-status {
    color: var(--text-secondary);
    font-size: 12px;
    text-align: center;
    padding: 40px 0;
  }

  footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 12px 20px;
    border-top: 1px solid var(--border-color);
  }

  .btn-cancel {
    background: var(--bg-hover);
    border: 1px solid var(--border-color);
    color: var(--text-primary);
    padding: 6px 20px;
    border-radius: var(--radius);
    cursor: pointer;
    font-size: 13px;
  }

  .btn-save {
    background: var(--accent);
    border: none;
    color: #fff;
    padding: 6px 20px;
    border-radius: var(--radius);
    cursor: pointer;
    font-size: 13px;
  }

  .btn-save:hover:not(:disabled) {
    background: var(--accent-hover);
  }

  .btn-save:disabled {
    opacity: 0.4;
    cursor: default;
  }
</style>
