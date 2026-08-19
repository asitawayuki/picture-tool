<script lang="ts">
  import Dialog from "./ui/Dialog.svelte";
  import Button from "./ui/Button.svelte";
  import LinearProgress from "./ui/LinearProgress.svelte";
  import type { ProgressPayload } from "./types";

  interface Props {
    progress: ProgressPayload | null;
    onCancel: () => void;
  }

  let { progress, onCancel }: Props = $props();
</script>

{#if progress}
  <!-- 変換中は Esc や scrim クリックで閉じさせない。閉じても処理は止まらず、
       進捗の見えない状態になるだけなので -->
  <Dialog title="変換中..." dismissible={false} onClose={onCancel}>
    <LinearProgress
      value={progress.total > 0 ? progress.current : null}
      max={progress.total}
      label="変換の進捗"
    />
    <div class="info">
      <span>{progress.current} / {progress.total}</span>
      <span class="file">{progress.file_name}</span>
    </div>
    {#snippet actions()}
      <Button variant="outlined" danger onclick={onCancel}>キャンセル</Button>
    {/snippet}
  </Dialog>
{/if}

<style>
  .info {
    display: flex;
    justify-content: space-between;
    gap: var(--space-3);
    margin-top: var(--space-2);
    font: var(--md-sys-typescale-body-sm);
    color: var(--md-sys-color-on-surface-variant);
  }

  .file {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
