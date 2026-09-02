<script lang="ts">
  import Button from "./ui/Button.svelte";
  import Dialog from "./ui/Dialog.svelte";

  interface Props {
    /** 編集中の写真のファイル名 */
    name: string;
    onCancel: () => void;
    onDiscard: () => void;
    onSave: () => void;
    /** 承認前など、保存できない状態では「保存して移動」を出さない */
    canSave: boolean;
  }

  let { name, onCancel, onDiscard, onSave, canSave }: Props = $props();
</script>

<!-- 破棄が選べるので alertdialog。初期フォーカスは破棄でない側に置く -->
<Dialog title="保存していない変更があります" danger initialFocus="footer button" onClose={onCancel}>
  <p>{name} の変更はまだ書き込まれていません。</p>
  {#snippet actions()}
    <Button variant="text" onclick={onCancel}>キャンセル</Button>
    <Button variant="text" danger onclick={onDiscard}>破棄して移動</Button>
    {#if canSave}
      <Button variant="filled" onclick={onSave}>保存して移動</Button>
    {/if}
  {/snippet}
</Dialog>
