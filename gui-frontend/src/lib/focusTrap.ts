/**
 * モーダル内にキーボードフォーカスを閉じ込める Svelte action。
 *
 * - マウント時に `initialSelector` に一致する要素（無ければ node 自身）へフォーカスする
 * - Tab / Shift+Tab が末端に達したら反対側へ巻き戻す
 * - アンマウント時に元のフォーカス位置へ戻す
 *
 * node 自身にフォーカスできるよう `tabindex="-1"` を付けておくこと。
 */
const FOCUSABLE = [
  "a[href]",
  "button:not([disabled])",
  "input:not([disabled])",
  "select:not([disabled])",
  "textarea:not([disabled])",
  '[tabindex]:not([tabindex="-1"])',
].join(",");

export function focusTrap(node: HTMLElement, initialSelector?: string) {
  const previouslyFocused = document.activeElement as HTMLElement | null;

  function focusable(): HTMLElement[] {
    return Array.from(node.querySelectorAll<HTMLElement>(FOCUSABLE)).filter(
      // display:none の要素を除く（offsetParent は position:fixed で null になるため
      // getClientRects で判定する）
      (el) => el.getClientRects().length > 0
    );
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key !== "Tab") return;

    const items = focusable();
    if (items.length === 0) {
      e.preventDefault();
      node.focus();
      return;
    }

    const first = items[0];
    const last = items[items.length - 1];
    const active = document.activeElement;

    if (e.shiftKey && (active === first || active === node)) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && active === last) {
      e.preventDefault();
      first.focus();
    }
  }

  node.addEventListener("keydown", handleKeydown);

  // 既定は node 自身にフォーカスする。最初のボタンに当てると Space / Enter が
  // ダイアログ側のキーハンドラーとボタン既定動作の両方を発火させてしまう。
  const initial = initialSelector
    ? node.querySelector<HTMLElement>(initialSelector)
    : null;
  (initial ?? node).focus();

  return {
    destroy() {
      node.removeEventListener("keydown", handleKeydown);
      previouslyFocused?.focus?.();
    },
  };
}
