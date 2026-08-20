<script lang="ts">
  import { renderExifFramePreview } from "../api";
  import { describeError, toast } from "../toasts.svelte";
  import type { ExifFrameConfig } from "../types";

  interface Props {
    config: ExifFrameConfig | null;
    bgColor: "white" | "black";
    imagePath: string | null;
  }

  let { config, bgColor, imagePath }: Props = $props();

  let src = $state("");
  let loading = $state(false);

  let debounceTimer: ReturnType<typeof setTimeout>;
  /** プレビューは設定を触るたびに再生成されるため、同じ警告を毎回出さないよう記録する */
  const reportedWarnings = new Set<string>();

  $effect(() => {
    // 依存は $effect の同期フェーズで読む必要がある。
    // 非同期コールバック内でしか参照しないと依存として追跡されない
    const snapshot = config === null ? null : ($state.snapshot(config) as ExifFrameConfig);
    const bg = bgColor;
    const path = imagePath;
    if (!path || !snapshot) return;

    // **loading は debounce の前に立てる。** 後ろ（setTimeout の中）で立てると、
    // 待っている 300ms のあいだ `!loading && !src` になり、まだ 1 度も要求して
    // いないのに「生成できませんでした」が出る（フレームモードへ入った直後の
    // 300ms がそれに当たる）
    loading = true;
    clearTimeout(debounceTimer);
    debounceTimer = setTimeout(async () => {
      try {
        const preview = await renderExifFramePreview(path, snapshot, bg);
        src = preview.data_url;
        // アセット由来の警告（カスタム model_map の不備など）は返ってくるので
        // 従来どおり toast する。フレーム描画由来の警告は Rust 側で捨てている
        // （プレビューは長辺 400px 固定で偽陽性になるため。spec §5-3）
        for (const warning of preview.warnings) {
          if (reportedWarnings.has(warning)) continue;
          reportedWarnings.add(warning);
          toast.error(warning);
        }
      } catch (e) {
        toast.error(`プレビューの生成に失敗しました: ${describeError(e)}`);
      } finally {
        loading = false;
      }
    }, 300);
    return () => clearTimeout(debounceTimer);
  });
</script>

<div class="preview">
  {#if !imagePath}
    <!-- フレームモードに写真グリッドは出ないので「グリッドで選べ」とは書かない。
         選び直しの導線は右パネルの「写真を選ぶ」 -->
    <p class="status">見本にする写真がありません。右の「見本写真」から選んでください。</p>
  {:else if loading && !src}
    <p class="status">読み込み中...</p>
  {:else if src}
    <img {src} alt="Exif フレームのプレビュー" class:stale={loading} />
  {:else}
    <p class="status">プレビューを生成できませんでした。</p>
  {/if}
</div>

<style>
  .preview {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    padding: var(--space-5);
    background: var(--md-sys-color-surface);
  }

  img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
    border-radius: var(--md-sys-shape-corner-sm);
    box-shadow: var(--md-sys-elevation-shadow-2);
    transition: opacity var(--md-sys-motion-duration-short)
      var(--md-sys-motion-easing-standard);
  }

  /* 再生成中も直前の絵を出したままにする。消すとちらつく */
  img.stale {
    opacity: 0.6;
  }

  .status {
    margin: 0;
    color: var(--md-sys-color-on-surface-variant);
  }
</style>
