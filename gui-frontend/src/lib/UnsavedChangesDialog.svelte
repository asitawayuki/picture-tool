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
    /**
     * 保存が進行中（`metadata.saving`）。3 ボタンとも押せなくし、
     * scrim クリックと Esc も塞ぐ（Task 15 裁定 T3）。
     *
     * ここに抑止が無いと、保存の往復中に「キャンセル」→ 別の写真をクリックで
     * 新しい pendingLeave が積み直され、後から届いた保存成功がユーザーの
     * 答えていない新しいダイアログを勝手に「保存して移動」で解決してしまう。
     * 「保存して移動」の連打も同様に、2 回目の早期 return が
     * pendingLeave を先に消してしまいうる。
     */
    saving: boolean;
  }

  let { name, onCancel, onDiscard, onSave, canSave, saving }: Props = $props();
</script>

<!-- 破棄が選べるので alertdialog。初期フォーカスは破棄でない側に置く -->
<Dialog
  title="保存していない変更があります"
  danger
  initialFocus="footer button"
  dismissible={!saving}
  onClose={onCancel}
>
  <p>{name} の変更はまだ書き込まれていません。</p>
  {#snippet actions()}
    <Button variant="text" disabled={saving} onclick={onCancel}>キャンセル</Button>
    <Button variant="text" danger disabled={saving} onclick={onDiscard}>破棄して移動</Button>
    {#if canSave}
      <Button variant="filled" disabled={saving} onclick={onSave}>保存して移動</Button>
    {/if}
  {/snippet}
</Dialog>
