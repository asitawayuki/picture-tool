<script lang="ts">
  import { focusTrap } from "./focusTrap";

  interface Props {
    title: string;
    message: string;
    detail?: string | null;
    confirmLabel: string;
    danger?: boolean;
    onConfirm: () => void;
    onCancel: () => void;
  }

  let {
    title,
    message,
    detail = null,
    confirmLabel,
    danger = false,
    onConfirm,
    onCancel,
  }: Props = $props();

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      onCancel();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="overlay">
  <!-- 破壊的操作なので初期フォーカスはキャンセル側に置く -->
  <div
    class="dialog"
    role="alertdialog"
    aria-modal="true"
    aria-labelledby="confirm-title"
    aria-describedby="confirm-message"
    tabindex="-1"
    use:focusTrap={".btn-cancel"}
  >
    <h2 id="confirm-title">{title}</h2>
    <p id="confirm-message">{message}</p>
    {#if detail}
      <p class="detail">{detail}</p>
    {/if}
    <div class="actions">
      <button class="btn-cancel" onclick={onCancel}>キャンセル</button>
      <button class="btn-confirm" class:danger onclick={onConfirm}>{confirmLabel}</button>
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.6);
    z-index: 500;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .dialog {
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--radius);
    padding: 20px;
    width: 90vw;
    max-width: 420px;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.5);
  }

  h2 {
    margin: 0 0 10px;
    font-size: 15px;
    color: var(--text-primary);
  }

  p {
    margin: 0 0 8px;
    font-size: 13px;
    line-height: 1.6;
    color: var(--text-primary);
  }

  .detail {
    color: var(--text-secondary);
    font-size: 12px;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 18px;
  }

  button {
    padding: 7px 18px;
    border-radius: var(--radius);
    cursor: pointer;
    font-size: 13px;
  }

  .btn-cancel {
    background: var(--bg-hover);
    border: 1px solid var(--border-color);
    color: var(--text-primary);
  }

  .btn-confirm {
    background: var(--accent);
    border: 1px solid var(--accent);
    color: #fff;
  }

  .btn-confirm.danger {
    background: var(--danger);
    border-color: var(--danger);
  }

  .btn-confirm:hover {
    filter: brightness(1.1);
  }
</style>
