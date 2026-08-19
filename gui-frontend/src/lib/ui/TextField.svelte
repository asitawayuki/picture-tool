<!--
  value をジェネリックにしてある。`bind:` は双方向なので、prop の型と
  束縛する式の型が相互に代入可能でないと svelte-check が落ちる。
  `value: string | number | null` と固定すると、`bind:value={config.max_size_mb}`
  （number）も `bind:value={title}`（string）も通らなくなる。
-->
<script lang="ts" generics="T extends string | number | null">
  interface Props {
    value: T;
    /** 可視ラベル。id は $props.id() で自動生成して label と結ぶ */
    label: string;
    type?: "text" | "number";
    multiline?: boolean;
    rows?: number;
    /** 入力欄の右端に出す固定文字（"MB" / "px" など） */
    suffix?: string;
    /** 非 null のとき error ロールで表示し aria-invalid を立てる */
    error?: string | null;
    /** 補足文。error があるときは error が優先される */
    hint?: string | null;
    placeholder?: string;
    disabled?: boolean;
    min?: number;
    max?: number;
    /**
     * type="number" の確定時に値を通す。クランプや「4 の倍数に切り捨て」など。
     * ここに寄せることで、正規化後に値が変わらなかった場合の表示ずれ
     * （1000 のときに 1002 を入力すると state が動かず表示だけ 1002 が残る）を
     * このコンポーネント側で 1 回だけ潰せる。
     */
    normalize?: (value: number) => number;
    /** 確定後に呼ばれる。value は既に更新済み */
    onchange?: () => void;
  }

  let {
    value = $bindable(),
    label,
    type = "text",
    multiline = false,
    rows = 3,
    suffix,
    error = null,
    hint = null,
    placeholder,
    disabled = false,
    min,
    max,
    normalize,
    onchange,
  }: Props = $props();

  const id = $props.id();
  const describedById = `${id}-desc`;

  let description = $derived(error ?? hint);

  // ジェネリックの実体はマークアップ側の分岐（text/multiline は string、
  // number は number|null）で決まる。その対応をここの 2 箇所のキャストに閉じ込める。
  /** 文字入力は逐次反映する */
  function handleInput(event: Event) {
    const el = event.currentTarget as HTMLInputElement | HTMLTextAreaElement;
    value = el.value as T;
  }

  /** 数値は確定時にだけ反映する。空欄は null（＝未指定）とする */
  function handleNumberChange(event: Event) {
    const el = event.currentTarget as HTMLInputElement;
    const raw = el.value.trim();
    const parsed = Number(raw);
    let next: number | null =
      raw === "" || !Number.isFinite(parsed) ? null : parsed;
    if (next !== null && normalize) next = normalize(next);
    value = next as T;
    // 正規化の結果が現在値と同じでも DOM は元の入力のままなので、明示的に戻す
    el.value = next === null ? "" : String(next);
    onchange?.();
  }
</script>

<div class="field" class:has-error={error !== null}>
  <label class="field-label" for={id}>{label}</label>
  <div class="control" class:multiline>
    {#if multiline}
      <textarea
        {id}
        {rows}
        {placeholder}
        {disabled}
        aria-invalid={error !== null}
        aria-describedby={description ? describedById : undefined}
        value={value === null ? "" : String(value)}
        oninput={handleInput}
        onchange={() => onchange?.()}
      ></textarea>
    {:else if type === "number"}
      <input
        {id}
        type="number"
        {placeholder}
        {disabled}
        {min}
        {max}
        aria-invalid={error !== null}
        aria-describedby={description ? describedById : undefined}
        value={value === null ? "" : String(value)}
        onchange={handleNumberChange}
      />
    {:else}
      <input
        {id}
        type="text"
        {placeholder}
        {disabled}
        aria-invalid={error !== null}
        aria-describedby={description ? describedById : undefined}
        value={value === null ? "" : String(value)}
        oninput={handleInput}
        onchange={() => onchange?.()}
      />
    {/if}
    {#if suffix}<span class="suffix" aria-hidden="true">{suffix}</span>{/if}
  </div>
  {#if description}
    <p class="description" id={describedById}>{description}</p>
  {/if}
</div>

<style>
  .field {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .field-label {
    font: var(--md-sys-typescale-body-sm);
    color: var(--md-sys-color-on-surface-variant);
  }

  .control {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: 0 var(--space-3);
    background: var(--md-sys-color-surface-container-highest);
    border: 1px solid var(--md-sys-color-outline);
    border-radius: var(--md-sys-shape-corner-sm);
  }

  .control.multiline {
    align-items: stretch;
    padding: var(--space-2) var(--space-3);
  }

  .has-error .control {
    border-color: var(--md-sys-color-error);
  }

  input,
  textarea {
    flex: 1;
    min-width: 0;
    background: none;
    border: none;
    padding: var(--space-2) 0;
    color: var(--md-sys-color-on-surface);
    font: var(--md-sys-typescale-body-md);
  }

  textarea {
    resize: vertical;
    padding: 0;
  }

  input:focus,
  textarea:focus {
    outline: none;
  }

  /* フォーカスは枠で示す。:focus-visible の既定リングは内側の input に
     付くと枠から浮くため、ここだけ :focus-within で外枠に寄せる */
  .control:focus-within {
    outline: var(--md-sys-state-focus-ring);
    outline-offset: var(--md-sys-state-focus-ring-offset);
    border-color: var(--md-sys-color-primary);
  }

  .suffix {
    flex-shrink: 0;
    font: var(--md-sys-typescale-body-sm);
    color: var(--md-sys-color-on-surface-variant);
  }

  .description {
    margin: 0;
    font: var(--md-sys-typescale-body-sm);
    color: var(--md-sys-color-on-surface-variant);
  }

  .has-error .description {
    color: var(--md-sys-color-error);
  }

  input:disabled,
  textarea:disabled {
    opacity: 0.38;
  }
</style>
