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
    right: var(--space-4);
    bottom: var(--space-4);
    z-index: 600;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    max-width: min(420px, calc(100vw - var(--space-6) * 2));
    pointer-events: none;
  }

  /* トーストは M3 の snackbar。面の上に置く反転色でコントラストを稼ぐ */
  .toast {
    display: flex;
    align-items: flex-start;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-4);
    border-radius: var(--md-sys-shape-corner-xs);
    background: var(--md-sys-color-inverse-surface);
    color: var(--md-sys-color-inverse-on-surface);
    font: var(--md-sys-typescale-body-md);
    box-shadow: var(--md-sys-elevation-shadow-3);
    pointer-events: auto;
  }

  /* 種別は左端の帯で示す。反転面の上では error / warning の塗りが読めないため。
     warning と success が同じ primary なのは意図した劣化である ──
     spec §1-1 は使うロールを限定していて warning / success を定義しない。
     この 2 つはアイコン（⚠ / ✓）で区別する。退行ではない */
  .toast.error {
    border-left: 4px solid var(--md-sys-color-error);
  }

  .toast.warning {
    border-left: 4px solid var(--md-sys-color-primary);
  }

  .toast.success {
    border-left: 4px solid var(--md-sys-color-primary);
  }

  .icon {
    flex-shrink: 0;
    line-height: 1.5;
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
    color: inherit;
    opacity: 0.7;
    cursor: pointer;
    font: var(--md-sys-typescale-body-sm);
    padding: 0 var(--space-1);
    line-height: 1.5;
  }

  .dismiss:hover {
    opacity: 1;
  }
</style>
