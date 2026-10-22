<!--
  グリッドの絞り込みと並び順の操作。

  状態そのものは `App.svelte` が持つ。ここに持たせられない ── 一覧の順序は
  情報モードの「次の写真」・サムネイル要求・変換対象の 3 箇所が同じ配列を
  index で参照する前提なので、決めるのは App の `$derived` 1 箇所だけである
  （`gridFilter.ts` のモジュールコメント）。
-->
<script lang="ts">
  import TextField from "../ui/TextField.svelte";
  import Select from "../ui/Select.svelte";
  import IconButton from "../ui/IconButton.svelte";
  import {
    RATING_PRESETS,
    SORT_LABELS,
    type GridSort,
    type RatingPresetId,
    type SortKey,
  } from "./gridFilter";

  interface Props {
    query: string;
    /**
     * 並び順。**`bind:` ではなくプロパティを直接書き換える**
     * （`$state` のプロキシなので親まで届く）。ConvertPanel の config と同じ扱い
     */
    sort: GridSort;
    /** ★の絞り込み。7 択の id。対応表は `gridFilter.ts` が持つ */
    ratingPreset: RatingPresetId;
  }

  let { query = $bindable(), sort = $bindable(), ratingPreset = $bindable() }: Props = $props();

  const SORT_OPTIONS = (Object.keys(SORT_LABELS) as SortKey[]).map((value) => ({
    value,
    label: SORT_LABELS[value],
  }));

  // **対応表を写さない。** ラベルも値も `gridFilter.ts` の 1 箇所から取る
  const RATING_OPTIONS = RATING_PRESETS.map((preset) => ({
    value: preset.id,
    label: preset.label,
  }));

  // 日時は「新しい順」から見たいが名前は A→Z から見たい。キーを変えたときの
  // 向きの既定をここで揃える（利用者が明示的に反転させた向きは、そのキーに
  // 留まっている限り保つ）
  function handleKeyChange() {
    sort.desc = sort.key !== "name";
  }
</script>

<div class="filters">
  <div class="search">
    <TextField label="検索" bind:value={query} placeholder="ファイル名" />
  </div>
  <div class="sort">
    <Select
      label="並び順"
      bind:value={sort.key}
      options={SORT_OPTIONS}
      onchange={handleKeyChange}
    />
  </div>
  <IconButton
    label={sort.desc ? "降順（クリックで昇順）" : "昇順（クリックで降順）"}
    icon={sort.desc ? "↓" : "↑"}
    toggle
    pressed={sort.desc}
    onclick={() => (sort.desc = !sort.desc)}
  />
  <div class="rating">
    <Select label="評価" bind:value={ratingPreset} options={RATING_OPTIONS} />
  </div>
</div>

<style>
  .filters {
    display: flex;
    /* ラベルが上に載る部品と、載らない IconButton の下端を揃える */
    align-items: flex-end;
    gap: var(--space-2);
    /* ヘッダーが狭いときに検索欄を縮ませる。min-width を 0 にしないと
       flex アイテムは内容幅より縮まず、ヘッダーごと溢れる */
    min-width: 0;
  }

  .search {
    flex: 1 1 auto;
    min-width: 72px;
    max-width: 200px;
  }

  .sort {
    flex: 0 0 auto;
    width: 104px;
  }

  .rating {
    flex: 0 0 auto;
    width: 166px;
  }
</style>
