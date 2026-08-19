<script lang="ts">
  import { onMount } from "svelte";
  import Button from "../ui/Button.svelte";
  import Card from "../ui/Card.svelte";
  import SegmentedButton from "../ui/SegmentedButton.svelte";
  import Select from "../ui/Select.svelte";
  import Slider from "../ui/Slider.svelte";
  import TextField from "../ui/TextField.svelte";
  import { listAvailableFonts } from "../api";
  import { describeError, toast } from "../toasts.svelte";
  import type { DisplayItems, ExifPosition, ExifFrameConfig, FontInfo } from "../types";

  interface Props {
    config: ExifFrameConfig;
    bgColor: "white" | "black";
    isNew: boolean;
    /** 一覧でダブルクリックして名前を変えた状態。保存で旧名が消える */
    isRenamed: boolean;
    /** 既存の別プリセットと名前がぶつかっている */
    nameConflict: boolean;
    canSave: boolean;
    canDelete: boolean;
    /** 見本にしている写真のファイル名。未選択なら null */
    sampleName: string | null;
    onSave: () => void;
    onDelete: () => void;
    /** 見本写真を選び直す（spec §5-3「選び直しはパネル内のボタンから」） */
    onPickSample: () => void;
  }

  let {
    config = $bindable(),
    bgColor = $bindable(),
    isNew,
    isRenamed,
    nameConflict,
    canSave,
    canDelete,
    sampleName,
    onSave,
    onDelete,
    onPickSample,
  }: Props = $props();

  /**
   * 選べるフォント。使うのはこのパネルだけなので App では持たない（spec §3-5）。
   * このパネルはフレームモードへ入るたびに mount されるので、
   * 起動後に `assets/fonts/` へ足したフォントもモードを開き直せば出る。
   */
  let fonts = $state<FontInfo[]>([]);

  onMount(() => {
    listAvailableFonts()
      .then((f) => (fonts = f))
      .catch((e) => toast.error(`フォント一覧の取得に失敗しました: ${describeError(e)}`));
  });

  const POSITIONS: { value: ExifPosition; label: string }[] = [
    { value: "auto", label: "自動" },
    { value: "bottom", label: "下" },
    { value: "top", label: "上" },
    { value: "right", label: "右" },
    { value: "left", label: "左" },
  ];

  const ITEMS: { key: keyof DisplayItems; label: string }[] = [
    { key: "maker_logo", label: "ロゴ" },
    { key: "lens_brand_logo", label: "レンズブランド" },
    { key: "camera_model", label: "カメラ" },
    { key: "lens_model", label: "レンズ" },
    { key: "focal_length", label: "焦点距離" },
    { key: "f_number", label: "F値" },
    { key: "shutter_speed", label: "SS" },
    { key: "iso", label: "ISO" },
    { key: "date_taken", label: "日時" },
    { key: "custom_text", label: "テキスト" },
  ];

  let fontOptions = $derived([
    ...fonts.map((f) => ({ value: f.path ?? "", label: f.display_name })),
    // プリセットが参照するフォントが見つからない場合も選択状態を失わせない
    ...(config.font.font_path && !fonts.some((f) => f.path === config.font.font_path)
      ? [{ value: config.font.font_path, label: `${config.font.font_path}（見つかりません）` }]
      : []),
  ]);
</script>

<div class="panel">
  <div class="scroll">
    <!-- rail の destination として常時見えるようになるため、
         crop / quality しか使わない利用者への注記を出す（spec §5-3） -->
    <p class="note">Exif フレームは pad モードでのみ出力されます。</p>

    <!-- 見本写真の出所は focusedPath（spec §3-2 / §5-3）。フレームモードには
         グリッドが無いので、選び直しの導線をパネル内に置く。押すと変換モードへ移る -->
    <Card level={1} title="見本写真">
      <p class="sample">{sampleName ?? "未選択"}</p>
      <Button variant="outlined" onclick={onPickSample}>
        {sampleName ? "別の写真を選ぶ" : "写真を選ぶ"}
      </Button>
    </Card>

    <Card level={1} title="背景色">
      <!-- 値は変換設定と同じ config.bg_color。置き場所が 2 つあるだけ（spec §5-3） -->
      <SegmentedButton
        bind:value={bgColor}
        label="背景色"
        options={[
          { value: "white", label: "白" },
          { value: "black", label: "黒" },
        ]}
      />
    </Card>

    <Card level={1} title="配置位置">
      <SegmentedButton bind:value={config.position} label="配置位置" options={POSITIONS} />
    </Card>

    <Card level={1} title="表示項目">
      <div class="chips" role="group" aria-label="表示項目">
        {#each ITEMS as item (item.key)}
          <button
            class="chip state-layer"
            class:on={config.items[item.key]}
            type="button"
            aria-pressed={config.items[item.key]}
            onclick={() => (config.items[item.key] = !config.items[item.key])}
          >{item.label}</button>
        {/each}
      </div>
    </Card>

    <Card level={1} title="フォント">
      <!-- 値は config.font.font_path が唯一の持ち主。ローカルの $state に写して
           $effect で書き戻す形にしてはならない ── 初期化は 1 回きりなので、
           プリセットを切り替えても前のフォントを表示し続け、さらに一度触ると
           新しい draft を古い値で上書きする。関数バインディングで直接読み書きし、
           null と "" の変換だけをここで吸収する -->
      <Select
        bind:value={
          () => config.font.font_path ?? "",
          (v) => (config.font.font_path = v === "" ? null : v)
        }
        label="フォント"
        options={fontOptions}
      />
      <div class="sub">
        <Slider
          bind:value={config.font.primary_size}
          label="メイン"
          min={0.015}
          max={0.05}
          step={0.001}
          suffix="%"
          format={(v) => (v * 100).toFixed(1)}
        />
      </div>
      <div class="sub">
        <Slider
          bind:value={config.font.secondary_size}
          label="サブ"
          min={0.01}
          max={0.035}
          step={0.001}
          suffix="%"
          format={(v) => (v * 100).toFixed(1)}
        />
      </div>
    </Card>

    <Card level={1} title="カスタムテキスト">
      <TextField bind:value={config.custom_text} label="テキスト" placeholder="@username" />
    </Card>
  </div>

  <div class="action">
    {#if nameConflict}
      <p class="conflict" role="alert">同じ名前のプリセットが既にあります。</p>
    {/if}
    {#if canDelete}
      <Button variant="text" danger onclick={onDelete}>削除</Button>
    {/if}
    <!-- 押した結果が違うので文言を分ける。「新規保存」は増える、
         「名前を変えて保存」は旧名が消える、「保存」は上書きする -->
    <Button variant="filled" full disabled={!canSave} onclick={onSave}>
      {isNew ? "新規保存" : isRenamed ? "名前を変えて保存" : "保存"}
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

  .note {
    margin: 0;
    padding: var(--space-2) var(--space-3);
    border-radius: var(--md-sys-shape-corner-sm);
    background: var(--md-sys-color-primary-container);
    color: var(--md-sys-color-on-primary-container);
    font: var(--md-sys-typescale-body-sm);
  }

  .sample {
    margin: 0 0 var(--space-2);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font: var(--md-sys-typescale-body-sm);
    color: var(--md-sys-color-on-surface-variant);
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  .chip {
    padding: var(--space-1) var(--space-3);
    border: 1px solid var(--md-sys-color-outline);
    border-radius: var(--md-sys-shape-corner-xs);
    background: none;
    color: var(--md-sys-color-on-surface-variant);
    font: var(--md-sys-typescale-body-sm);
    cursor: pointer;
  }

  .chip.on {
    background: var(--md-sys-color-primary-container);
    border-color: transparent;
    color: var(--md-sys-color-on-primary-container);
  }

  .conflict {
    margin: 0;
    font: var(--md-sys-typescale-body-sm);
    color: var(--md-sys-color-error);
  }

  /* ConvertPanel と違い、ここは警告文・削除・保存の最大 3 つが積まれる。
     Card を積むときと同じで、間隔は子ではなく親が持つ */
  .action {
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-3);
    border-top: 1px solid var(--md-sys-color-outline-variant);
    background: var(--md-sys-color-surface-container);
  }
</style>
