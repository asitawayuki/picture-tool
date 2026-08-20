<script lang="ts">
  import Button from "./ui/Button.svelte";
  import Dialog from "./ui/Dialog.svelte";

  interface Props {
    /** 削除される元ファイルの枚数 */
    count: number;
    onCancel: () => void;
    onConfirm: () => void;
  }

  let { count, onCancel, onConfirm }: Props = $props();
</script>

<!-- 破壊的操作なので alertdialog にし、初期フォーカスはキャンセル側に置く -->
<Dialog title="元ファイルを削除します" danger initialFocus="footer button" onClose={onCancel}>
  <p>変換に成功した {count} 枚の元ファイルを削除します。</p>
  <p class="detail">削除したファイルはゴミ箱に入らず、元に戻せません。</p>
  {#snippet actions()}
    <Button variant="text" onclick={onCancel}>キャンセル</Button>
    <Button variant="filled" danger onclick={onConfirm}>削除して変換</Button>
  {/snippet}
</Dialog>

<style>
  .detail {
    color: var(--md-sys-color-on-surface-variant);
    font: var(--md-sys-typescale-body-sm);
  }
</style>
