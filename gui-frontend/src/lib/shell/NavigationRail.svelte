<script lang="ts">
  import { MODES, type AppMode } from "./modes";
  import { THEME_ICONS, THEME_LABELS, nextTheme, type ThemePreference } from "./theme";

  interface Props {
    mode: AppMode;
    onModeChange: (mode: AppMode) => void;
    theme: ThemePreference;
    onThemeCycle: () => void;
  }

  let { mode, onModeChange, theme, onThemeCycle }: Props = $props();

  // 押した先が分からないと 1 つのボタンで 3 状態を巡れない。
  // ラベルは現在値、読み上げには次の状態まで入れる
  let themeHint = $derived(
    `テーマ: ${THEME_LABELS[theme]}（クリックで${THEME_LABELS[nextTheme(theme)]}へ）`
  );
</script>

<!-- data-region は Tab 順の検査（spec §7-3）の目印。AppShell の 3 カラムと対になる -->
<nav class="rail" data-region="rail" aria-label="モード">
  {#each MODES as destination (destination.value)}
    {@const selected = mode === destination.value}
    <button
      class="destination"
      class:selected
      type="button"
      aria-current={selected ? "page" : undefined}
      onclick={() => onModeChange(destination.value)}
    >
      <span class="indicator state-layer" aria-hidden="true">{destination.icon}</span>
      <span class="label">{destination.label}</span>
    </button>
  {/each}

  <button
    class="destination theme"
    type="button"
    aria-label={themeHint}
    title={themeHint}
    onclick={onThemeCycle}
  >
    <span class="indicator state-layer" aria-hidden="true">{THEME_ICONS[theme]}</span>
    <span class="label">{THEME_LABELS[theme]}</span>
  </button>
</nav>

<style>
  .rail {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-3);
    /* rail 幅は 80px 固定（spec §3-1）。AppShell の当たり判定と
       Toast の左端が同じ値を見るのでトークンから引く */
    width: var(--rail-width);
    flex-shrink: 0;
    padding: var(--space-3) 0;
    background: var(--md-sys-color-surface);
    border-right: 1px solid var(--md-sys-color-outline-variant);
  }

  .destination {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-1);
    width: 100%;
    padding: 0;
    background: none;
    border: none;
    cursor: pointer;
    color: var(--md-sys-color-on-surface-variant);
  }

  .destination.selected {
    color: var(--md-sys-color-on-surface);
  }

  /* 目的地ではないので rail の下端へ寄せ、区切り線で分ける */
  .destination.theme {
    margin-top: auto;
    padding-top: var(--space-3);
    border-top: 1px solid var(--md-sys-color-outline-variant);
  }

  /* 選択インジケータは pill 形（spec §3-3） */
  .indicator {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 56px;
    height: 32px;
    border-radius: var(--md-sys-shape-corner-full);
    font-size: 18px;
    line-height: 1;
    transition: background var(--md-sys-motion-duration-short)
      var(--md-sys-motion-easing-standard);
  }

  .destination.selected .indicator {
    background: var(--md-sys-color-primary-container);
    color: var(--md-sys-color-on-primary-container);
  }

  .label {
    font: var(--md-sys-typescale-body-sm);
    white-space: nowrap;
  }
</style>
