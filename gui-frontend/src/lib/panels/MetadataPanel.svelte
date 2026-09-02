<script lang="ts">
  import Button from "../ui/Button.svelte";
  import Card from "../ui/Card.svelte";
  import Rating from "../ui/Rating.svelte";
  import TextField from "../ui/TextField.svelte";
  import { getExifInfo } from "../api";
  import type { RequestKind } from "../browser/requestQueue";
  import type { ExifInfo, ImageEntry } from "../types";
  import type { createMetadataDraft } from "./metadataDraft.svelte";

  interface Props {
    image: ImageEntry | null;
    draft: ReturnType<typeof createMetadataDraft>;
    /** このフォルダーへの書き込みが承認済みか */
    canWrite: boolean;
    /** JPEG 以外は編集できない（Rust 側が弾く） */
    editable: boolean;
    /** 「保存して次の写真へ」で送る先があるか */
    hasNext: boolean;
    onGrant: () => void;
    onSave: () => void;
    onSaveAndNext: () => void;
    thumbnailFor: (path: string, size: number) => string | undefined;
    onRequestThumbnail: (
      path: string,
      size: number,
      kind: RequestKind,
      index: number
    ) => void;
  }

  let {
    image,
    draft,
    canWrite,
    editable,
    hasNext,
    onGrant,
    onSave,
    onSaveAndNext,
    thumbnailFor,
    onRequestThumbnail,
  }: Props = $props();

  // 保存できるのは「JPEG で、承認済みで、変更があって、保存中でない」とき
  let canSave = $derived(editable && canWrite && draft.isDirty && !draft.saving);

  const THUMB = 160;

  let exif = $state<ExifInfo | null>(null);
  let exifToken = 0;

  $effect(() => {
    const path = image?.path ?? null;
    if (!path) {
      exif = null;
      return;
    }
    // パネル先頭のサムネイルはグリッドの index 範囲に入らないので pinned
    onRequestThumbnail(path, THUMB, "pinned", -1);

    const token = ++exifToken;
    getExifInfo(path)
      .then((info) => {
        if (token === exifToken) exif = info;
      })
      .catch(() => {
        // EXIF は無くても表示は成立するので通知しない
        if (token === exifToken) exif = null;
      });
  });

  let exifRows = $derived(
    exif === null
      ? []
      : [
          ["カメラ", [exif.camera_make, exif.camera_model].filter(Boolean).join(" ")],
          ["レンズ", exif.lens_model ?? ""],
          ["焦点距離", exif.focal_length ?? ""],
          ["F値", exif.f_number ?? ""],
          ["SS", exif.shutter_speed ?? ""],
          ["ISO", exif.iso === null ? "" : String(exif.iso)],
          ["撮影日時", exif.date_taken ?? ""],
        ].filter(([, value]) => value !== "")
  );
</script>

<div class="panel">
  <div class="scroll">
    {#if !image}
      <p class="empty">グリッドで写真を 1 枚選んでください。</p>
    {:else}
      {@const thumb = thumbnailFor(image.path, THUMB)}
      <!-- 1. サムネイル + ファイル名 + 未保存表示 -->
      <Card level={1} padding="var(--space-3)">
        <div class="head">
          {#if thumb}
            <img class="thumb" src="data:image/jpeg;base64,{thumb}" alt="" />
          {:else}
            <div class="thumb placeholder" aria-hidden="true">📷</div>
          {/if}
          <div class="head-text">
            <p class="name">{image.name}</p>
            <p class="dims">{image.width}×{image.height}</p>
            {#if draft.isDirty}
              <p class="unsaved">未保存の変更があります</p>
            {/if}
          </div>
        </div>
      </Card>

      <!-- 2. タイトル / 3. コメント -->
      <Card level={1} title="タイトルとコメント">
        <TextField
          bind:value={draft.values.title}
          label="タイトル"
          placeholder="未設定"
          disabled={!editable || draft.loading}
        />
        <div class="sub">
          <TextField
            bind:value={draft.values.comment}
            label="コメント"
            multiline
            rows={4}
            placeholder="未設定"
            disabled={!editable || draft.loading}
          />
        </div>
        <div class="mismatch" aria-live="polite">
          {#each draft.warnings as warning (warning)}
            <p class="warning">{warning}</p>
          {/each}
        </div>
      </Card>

      <!-- 4. レーティング -->
      <Card level={1} title="レーティング">
        <Rating bind:value={draft.values.rating} disabled={!editable || draft.loading} />
      </Card>

      <!-- 5. 撮影情報（読み取り専用） -->
      <Card level={1} title="撮影情報">
        {#if exifRows.length === 0}
          <p class="empty">Exif がありません。</p>
        {:else}
          <dl>
            {#each exifRows as [key, value] (key)}
              <dt>{key}</dt>
              <dd>{value}</dd>
            {/each}
          </dl>
        {/if}
      </Card>

      {#if !editable}
        <Card level={1} title="編集できない形式">
          <p class="empty">タイトル・コメント・評価を書き込めるのは JPEG だけです。</p>
        </Card>
      {/if}

      <!-- 6. 書き込み承認の状態表示 -->
      <Card level={1} title="書き込みの許可">
        {#if canWrite}
          <p class="empty">このフォルダーへ書き込めます（アプリを終了すると解除されます）。</p>
        {:else}
          <p class="empty">このフォルダーへの書き込みはまだ許可されていません。</p>
          <div class="sub">
            <Button variant="outlined" onclick={onGrant}>このフォルダーへの書き込みを許可...</Button>
          </div>
        {/if}
      </Card>
    {/if}
  </div>

  <!-- 7. 連続して付けていく作業が主なので、次へ送りを主ボタンに置く（spec §5-2） -->
  <div class="action">
    <Button
      variant="filled"
      full
      disabled={!canSave || !hasNext}
      onclick={onSaveAndNext}>保存して次の写真へ</Button>
    <Button variant="outlined" full disabled={!canSave} onclick={onSave}>保存</Button>
  </div>
</div>

<style>
  /* .panel / .scroll / .sub / .action は ConvertPanel と同じ骨格（spec §5-1）。
     .action だけはボタンを縦に 2 つ積むので下で上書きする */
  .panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    /* rail の切替は 150ms のフェードのみ（spec §3-3） */
    animation: fade-in var(--md-sys-motion-duration-short)
      var(--md-sys-motion-easing-standard);
  }

  @keyframes fade-in {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }

  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-3);
  }

  .sub {
    margin-top: var(--space-3);
  }

  .head {
    display: flex;
    gap: var(--space-3);
  }

  .thumb {
    width: 64px;
    height: 80px;
    flex-shrink: 0;
    object-fit: cover;
    border-radius: var(--md-sys-shape-corner-sm);
    background: var(--md-sys-color-surface-container-high);
  }

  .placeholder {
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .head-text {
    min-width: 0;
  }

  .name {
    margin: 0;
    font: var(--md-sys-typescale-title-sm);
    overflow-wrap: anywhere;
  }

  .dims,
  .empty {
    margin: 0;
    font: var(--md-sys-typescale-body-sm);
    color: var(--md-sys-color-on-surface-variant);
  }

  .unsaved {
    margin: var(--space-1) 0 0;
    font: var(--md-sys-typescale-body-sm);
    color: var(--md-sys-color-error);
  }

  .mismatch:empty {
    display: none;
  }

  .warning {
    margin: var(--space-2) 0 0;
    font: var(--md-sys-typescale-body-sm);
    color: var(--md-sys-color-on-surface-variant);
  }

  dl {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: var(--space-1) var(--space-3);
    margin: 0;
    font: var(--md-sys-typescale-body-sm);
  }

  dt {
    color: var(--md-sys-color-on-surface-variant);
  }

  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }

  .action {
    flex-shrink: 0;
    padding: var(--space-3);
    border-top: 1px solid var(--md-sys-color-outline-variant);
    background: var(--md-sys-color-surface-container);
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
</style>
