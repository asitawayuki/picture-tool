/**
 * モーダル内にキーボードフォーカスを閉じ込める Svelte action。
 *
 * - マウント時に `initialSelector` に一致する要素（無ければ node 自身）へフォーカスする
 * - Tab / Shift+Tab が末端に達したら反対側へ巻き戻す
 * - アンマウント時に元のフォーカス位置へ戻す
 *
 * node 自身にフォーカスできるよう `tabindex="-1"` を付けておくこと。
 */
/**
 * **`tabindex="-1"` の要素は列挙しない。** Tab 順に入らないのだから
 * 先頭／末尾の判定に混ぜる理由が無く、混ぜると端がずれる
 * （`Rating` の★のように `aria-hidden` かつ `tabindex="-1"` の要素がある）。
 *
 * 実用上はコストの方が大きい。`focusable()` は **Tab のたびに**
 * 全一致要素へ `getClientRects()` を掛けるので、`PhotoViewer` の
 * フィルムストリップ（写真の枚数だけ `button` が並ぶ）が毎回そこに入ると、
 * Tab 1 回ごとに枚数分の強制レイアウトが走る。除くと現在位置の 1 枚だけになる。
 *
 * `node` 自身の `tabindex="-1"` も外れるが、`focusTrap` は `node` を
 * `items` ではなく `active === node` で別に見ているので影響しない。
 */
const FOCUSABLE = ["a[href]", "button", "input", "select", "textarea", "[tabindex]"]
  .map((sel) => `${sel}:not([disabled]):not([tabindex="-1"])`)
  .join(",");

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
