<script lang="ts">
  import { toasts, dismissToast } from "./toasts.svelte";

  const ICON = { error: "⚠", warning: "⚠", success: "✓" } as const;
</script>

<div class="toast-stack" role="region" aria-label="通知">
  {#each toasts as t (t.id)}
    <div class="toast {t.kind}" role={t.kind === "error" ? "alert" : "status"}>
      <span class="icon" aria-hidden="true">{ICON[t.kind]}</span>
      <span class="message">{t.message}</span>
      <button class="dismiss" aria-label="閉じる" onclick={() => dismissToast(t.id)}>✕</button>
    </div>
  {/each}
</div>

<style>
  .toast-stack {
    position: fixed;
    right: 16px;
    bottom: 16px;
    z-index: 400;
    display: flex;
    flex-direction: column;
    gap: 8px;
    max-width: min(420px, calc(100vw - 32px));
    pointer-events: none;
  }

  .toast {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 10px 12px;
    border-radius: var(--radius);
    border: 1px solid var(--border-color);
    background: var(--bg-secondary);
    color: var(--text-primary);
    font-size: 13px;
    line-height: 1.5;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.4);
    pointer-events: auto;
  }

  .toast.error {
    border-color: var(--danger);
  }

  .toast.warning {
    border-color: var(--warning);
  }

  .toast.success {
    border-color: var(--success);
  }

  .icon {
    flex-shrink: 0;
    line-height: 1.5;
  }

  .toast.error .icon {
    color: var(--danger);
  }

  .toast.warning .icon {
    color: var(--warning);
  }

  .toast.success .icon {
    color: var(--success);
  }

  .message {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .dismiss {
    flex-shrink: 0;
    background: none;
    border: none;
    color: var(--text-secondary);
    cursor: pointer;
    font-size: 12px;
    padding: 0 2px;
    line-height: 1.5;
  }

  .dismiss:hover {
    color: var(--text-primary);
  }
</style>
