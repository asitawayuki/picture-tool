<script lang="ts">
  import Button from "../ui/Button.svelte";
  import Card from "../ui/Card.svelte";
  import Select from "../ui/Select.svelte";
  import SegmentedButton from "../ui/SegmentedButton.svelte";
  import Slider from "../ui/Slider.svelte";
  import Switch from "../ui/Switch.svelte";
  import TextField from "../ui/TextField.svelte";
  import type { ProcessingConfig } from "../types";

  interface Props {
    config: ProcessingConfig;
    outputFolder: string;
    selectedCount: number;
    canProcess: boolean;
    exifFrameEnabled: boolean;
    presetNames: string[];
    selectedPresetName: string;
    onPickOutputFolder: () => void;
    onProcess: () => void;
    /** フレームモードへ切り替える。プリセットの編集はそちらで行う（spec §5-3） */
    onEditFrame: () => void;
  }

  let {
    config = $bindable(),
    outputFolder,
    selectedCount,
    canProcess,
    exifFrameEnabled = $bindable(),
    presetNames,
    selectedPresetName = $bindable(),
    onPickOutputFolder,
    onProcess,
    onEditFrame,
  }: Props = $props();

  const MAX_WIDTH_MIN = 4;
  const MAX_WIDTH_MAX = 20000;

  /** トグルを on にしたときに入れる値。off にしても直前の値を覚えておく */
  let lastMaxWidth = $state(1080);

  let maxWidthEnabled = $derived(config.max_width !== null);

  let maxWidthLabel = $derived(
    config.max_width === null ? "" : `${config.max_width}×${(config.max_width * 5) / 4}`
  );

  /**
   * 4 の倍数へ切り捨てる（Rust 側 `target_canvas` と同じ丸め方向）。
   * 切り上げると指定値を超えてしまい、「上限」という機能の目的を果たさない。
   * TextField の normalize に渡すので、DOM への書き戻しは TextField が行う。
   *
   * 空欄は「無制限」ではない ── それを表すのは上のトグルなので、直前の値へ戻す。
   */
  function normalizeMaxWidth(value: number | null): number {
    if (value === null) return lastMaxWidth;
    const clamped = Math.min(Math.max(value, MAX_WIDTH_MIN), MAX_WIDTH_MAX);
    return Math.floor(clamped / 4) * 4;
  }

  /**
   * 最大サイズに「無指定」は無い（Rust 側は必須の数値）。空欄は直前の値へ戻す。
   * normalize 実行時点では config はまだ更新されていないので、
   * `config.max_size_mb` が直前の確定値になる。
   */
  function normalizeMaxSize(value: number | null): number {
    if (value === null) return config.max_size_mb;
    return Math.min(1024, Math.max(1, Math.round(value)));
  }

  function toggleMaxWidth(enabled: boolean) {
    config.max_width = enabled ? lastMaxWidth : null;
  }

  function commitMaxWidth() {
    if (config.max_width !== null) lastMaxWidth = config.max_width;
  }
</script>

<div class="panel">
  <div class="scroll">
    <Card level={1} title="変換モード">
      <SegmentedButton
        bind:value={config.mode}
        label="変換モード"
        options={[
          { value: "crop", label: "Crop" },
          { value: "pad", label: "Pad" },
          { value: "quality", label: "Quality" },
        ]}
      />
      {#if config.mode === "pad"}
        <div class="sub">
          <SegmentedButton
            bind:value={config.bg_color}
            label="背景色"
            options={[
              { value: "white", label: "白" },
              { value: "black", label: "黒" },
            ]}
          />
        </div>
      {/if}
    </Card>

    <Card level={1} title="出力">
      <Slider bind:value={config.quality} label="品質" min={1} max={100} suffix="%" />
      <div class="sub">
        <TextField
          bind:value={config.max_size_mb}
          label="最大サイズ"
          type="number"
          suffix="MB"
          min={1}
          max={1024}
          normalize={normalizeMaxSize}
        />
      </div>
      <div class="sub">
        <Switch
          checked={maxWidthEnabled}
          label="出力幅を制限する"
          disabled={config.mode === "quality"}
          onchange={() => toggleMaxWidth(!maxWidthEnabled)}
        />
        {#if config.mode === "quality"}
          <p class="hint">
            Quality モードは 4:5 に変換しないため、出力幅の上限は適用されません。
          </p>
        {:else if config.max_width !== null}
          <div class="sub">
            <TextField
              bind:value={config.max_width}
              label="出力幅の上限"
              type="number"
              suffix="px"
              min={MAX_WIDTH_MIN}
              max={MAX_WIDTH_MAX}
              normalize={normalizeMaxWidth}
              onchange={commitMaxWidth}
              hint={maxWidthLabel ? `→ ${maxWidthLabel}` : null}
            />
          </div>
        {/if}
      </div>
    </Card>

    {#if config.mode === "pad"}
      <Card level={1} title="Exif フレーム">
        <Switch bind:checked={exifFrameEnabled} label="Exif フレームを付ける" />
        {#if exifFrameEnabled}
          <div class="sub">
            <Select
              bind:value={selectedPresetName}
              label="プリセット"
              options={presetNames.map((name) => ({ value: name, label: name }))}
            />
          </div>
          <div class="sub">
            <Button variant="text" onclick={onEditFrame}>プリセットを編集...</Button>
          </div>
        {/if}
      </Card>
    {/if}

    <Card level={1} title="出力先">
      <p class="path" title={outputFolder || undefined}>
        {outputFolder || "未選択"}
      </p>
      <Button variant="outlined" onclick={onPickOutputFolder}>フォルダーを選択...</Button>
    </Card>

    <Card level={1} title="元ファイル">
      <Switch bind:checked={config.delete_originals} label="元ファイルを削除" danger />
      {#if config.delete_originals}
        <p class="hint danger">
          変換実行時に確認します。削除したファイルは元に戻せません。
        </p>
      {/if}
    </Card>
  </div>

  <!-- 主ボタンはパネル最下部に固定（spec §5-1） -->
  <div class="action">
    <Button variant="filled" full disabled={!canProcess} onclick={onProcess}>
      {selectedCount} 枚を変換
    </Button>
  </div>
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    /* rail の切替は 150ms のフェードのみ（spec §3-3） */
    animation: fade-in var(--md-sys-motion-duration-short)
      var(--md-sys-motion-easing-standard);
  }

  @keyframes fade-in {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }

  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-3);
  }

  .sub {
    margin-top: var(--space-3);
  }

  .hint {
    margin: var(--space-2) 0 0;
    font: var(--md-sys-typescale-body-sm);
    color: var(--md-sys-color-on-surface-variant);
  }

  .hint.danger {
    color: var(--md-sys-color-error);
  }

  .path {
    margin: 0 0 var(--space-3);
    font: var(--md-sys-typescale-body-sm);
    color: var(--md-sys-color-on-surface-variant);
    overflow-wrap: anywhere;
  }

  .action {
    flex-shrink: 0;
    padding: var(--space-3);
    border-top: 1px solid var(--md-sys-color-outline-variant);
    background: var(--md-sys-color-surface-container);
  }
</style>
