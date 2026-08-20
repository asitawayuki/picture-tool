<script lang="ts">
  import type { Snippet } from "svelte";
  import { focusTrap } from "../focusTrap";

  interface Props {
    title: string;
    /** 破壊的操作の確認。alertdialog にし、初期フォーカスをキャンセル側へ置く */
    danger?: boolean;
    /** false なら scrim クリックと Esc で閉じない（進行中の処理など） */
    dismissible?: boolean;
    /**
     * 初期フォーカスを当てる要素の CSS セレクタ（ダイアログ内を querySelector する）。
     * 既定はダイアログ自身。最初のボタンに当てると Space / Enter が
     * キーハンドラーとボタン既定動作の両方を発火させてしまう。
     *
     * 破壊的操作では `"footer button"` を渡す。actions snippet の最初のボタンが
     * キャンセル、という並び順を守れば、これでフォーカスが安全側に落ちる。
     * **フォーカス可能な要素そのものを指すセレクタにすること**（ラッパーの
     * span などを指すと focusTrap の focus() が効かない）。
     */
    initialFocus?: string;
    onClose: () => void;
    children: Snippet;
    actions?: Snippet;
  }

  let {
    title,
    danger = false,
    dismissible = true,
    initialFocus,
    onClose,
    children,
    actions,
  }: Props = $props();

  const titleId = $props.id();

  function handleKeydown(event: KeyboardEvent) {
    if (event.key !== "Escape" || !dismissible) return;
    event.preventDefault();
    onClose();
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="scrim">
  <!-- 背景。ダイアログ本体にクリックハンドラーを付けるとキーボード操作を
       持たない対話要素になるため分離する。
       **塗りは dismissible に関わらず常に出す。** 下地を暗くするのは
       「今はこのダイアログしか操作できない」ことを示す表示であって、
       閉じられるかどうかとは別の話。ここを {#if} で消すと、いちばん
       下地を隠したい進捗ダイアログだけが素通しになる。 -->
  <div
    class="backdrop"
    role="presentation"
    onclick={dismissible ? onClose : undefined}
  ></div>

  <div
    class="dialog"
    role={danger ? "alertdialog" : "dialog"}
    aria-modal="true"
    aria-labelledby={titleId}
    tabindex="-1"
    use:focusTrap={initialFocus}
  >
    <header>
      <h2 id={titleId}>{title}</h2>
      {#if dismissible}
        <button class="close state-layer" type="button" aria-label="閉じる" onclick={onClose}>✕</button>
      {/if}
    </header>

    <div class="body">
      {@render children()}
    </div>

    {#if actions}
      <footer>
        {@render actions()}
      </footer>
    {/if}
  </div>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 500;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .backdrop {
    position: absolute;
    inset: 0;
    background: var(--md-sys-color-scrim);
    opacity: 0.5;
  }

  .dialog {
    position: relative;
    display: flex;
    flex-direction: column;
    width: 90vw;
    max-width: 560px;
    max-height: 85vh;
    background: var(--md-sys-elevation-surface-3);
    color: var(--md-sys-color-on-surface);
    border-radius: var(--md-sys-shape-corner-lg);
    box-shadow: var(--md-sys-elevation-shadow-3);
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    padding: var(--space-5) var(--space-5) var(--space-3);
  }

  h2 {
    margin: 0;
    font: var(--md-sys-typescale-title-md);
  }

  .close {
    width: 32px;
    height: 32px;
    flex-shrink: 0;
    border: none;
    border-radius: var(--md-sys-shape-corner-full);
    background: none;
    color: var(--md-sys-color-on-surface-variant);
    cursor: pointer;
    font-size: 14px;
  }

  .body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 0 var(--space-5);
    font: var(--md-sys-typescale-body-md);
  }

  footer {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
    padding: var(--space-4) var(--space-5) var(--space-5);
  }
</style>
