<script lang="ts">
  interface Props {
    /** 0〜5。0 は「未設定」 */
    value: number;
    label?: string;
    readonly?: boolean;
    disabled?: boolean;
  }

  let {
    value = $bindable(),
    label = "レーティング",
    readonly = false,
    disabled = false,
  }: Props = $props();

  const STARS = [1, 2, 3, 4, 5];

  let locked = $derived(readonly || disabled);

  /** 同じ★をもう一度押したら 0 に戻す（spec §2） */
  function pick(star: number) {
    if (locked) return;
    value = value === star ? 0 : star;
  }

  function clamp(next: number) {
    if (locked) return;
    value = Math.min(5, Math.max(0, next));
  }

  function handleKeydown(event: KeyboardEvent) {
    switch (event.key) {
      case "ArrowRight":
      case "ArrowUp":
        event.preventDefault();
        clamp(value + 1);
        break;
      case "ArrowLeft":
      case "ArrowDown":
        event.preventDefault();
        clamp(value - 1);
        break;
      case "Home":
        event.preventDefault();
        clamp(0);
        break;
      case "End":
        event.preventDefault();
        clamp(5);
        break;
    }
  }
</script>

<!-- role="slider" にするのは、0（未設定）を含む 0〜5 の連続量であり、
     radiogroup では「どれも選ばれていない」を表現できないため。 -->
<div
  class="rating"
  class:locked
  role="slider"
  aria-label={label}
  aria-valuemin={0}
  aria-valuemax={5}
  aria-valuenow={value}
  aria-valuetext={value === 0 ? "未設定" : `${value} / 5`}
  aria-readonly={readonly || undefined}
  aria-disabled={disabled || undefined}
  tabindex={locked ? -1 : 0}
  onkeydown={handleKeydown}
>
  {#each STARS as star (star)}
    <!-- aria-hidden の中にフォーカスを入れないため、mousedown の既定動作
         （クリックした要素へのフォーカス）を止める。支援技術に見えているのは
         親の role="slider" だけで、★は装飾として扱う -->
    <button
      class="star"
      class:filled={star <= value}
      type="button"
      tabindex="-1"
      aria-hidden="true"
      disabled={locked}
      onmousedown={(e) => e.preventDefault()}
      onclick={() => pick(star)}
    >★</button>
  {/each}
</div>

<style>
  .rating {
    display: inline-flex;
    gap: var(--space-1);
    border-radius: var(--md-sys-shape-corner-xs);
  }

  .rating.locked {
    opacity: 0.6;
  }

  .star {
    background: none;
    border: none;
    padding: 0;
    font-size: 22px;
    line-height: 1;
    cursor: pointer;
    color: var(--md-sys-color-outline-variant);
    transition: color var(--md-sys-motion-duration-short)
      var(--md-sys-motion-easing-standard);
  }

  .star.filled {
    color: var(--md-sys-color-primary);
  }

  .star:disabled {
    cursor: default;
  }
</style>
