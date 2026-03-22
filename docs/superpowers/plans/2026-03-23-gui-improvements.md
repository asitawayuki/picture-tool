# GUI改善 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** picture-tool GUIのバグ4件修正と機能改善4件を段階的に実装する

**Architecture:** Phase 1でバグ修正（フォルダーツリー、スライダー、サムネイルキャッシュ統合+パフォーマンス）、Phase 2で機能追加（プレビュー、サムネサイズ可変、お気に入り、出力先初期位置）。バックエンド変更 → フロントエンド変更の順で各タスクを進める。

**Tech Stack:** Rust (Tauri v2, lru crate, image crate), Svelte 5 (runes: $state/$derived/$effect/$props/$bindable), TypeScript, tauri-plugin-store v2, bun

**Spec:** `docs/superpowers/specs/2026-03-22-gui-improvements-design.md`

---

## Task 1: BUG-2 — フォルダーツリー折りたたみ修正

**Files:**
- Modify: `gui-frontend/src/lib/FolderTree.svelte:63-67`

- [ ] **Step 1: `selectFolder()`の条件分岐を修正**

現在の `selectFolder()` は `!node.expanded` の場合のみ `toggleNode()` を呼ぶ。条件を除去して常に呼ぶようにする。

```svelte
// gui-frontend/src/lib/FolderTree.svelte
// 変更前:
function selectFolder(node: TreeNode) {
    selectedPath = node.entry.path;
    onSelectFolder(node.entry.path);
    if (!node.expanded) {
      toggleNode(node);
    }
  }

// 変更後:
function selectFolder(node: TreeNode) {
    selectedPath = node.entry.path;
    onSelectFolder(node.entry.path);
    toggleNode(node);
  }
```

- [ ] **Step 2: 動作確認**

Run: `make dev`
確認手順:
1. フォルダーツリーでフォルダーをクリック → 展開される
2. 同じフォルダーを再クリック → 折りたたまれる（📂→📁）
3. 折りたたんでも画像一覧は維持される（selectedPathは変わらない）

- [ ] **Step 3: Commit**

```bash
git add gui-frontend/src/lib/FolderTree.svelte
git commit -m "fix: フォルダーツリーの折りたたみが効かない問題を修正"
```

---

## Task 2: BUG-3 — スライダー端の未到達修正

**Files:**
- Modify: `gui-frontend/src/lib/SettingsPanel.svelte` (CSS)
- Modify: `gui-frontend/src/app.css` (必要に応じて)

- [ ] **Step 1: スライダーCSSの問題を調査・修正**

`SettingsPanel.svelte`の `input[type="range"]` に `padding: 4px 8px` が設定されている。この水平paddingがthumbの可動域を制限している原因。paddingを除去し、range input用の適切なスタイリングに変更する。

```css
/* gui-frontend/src/lib/SettingsPanel.svelte の <style> 内 */

/* 変更前: */
select, input[type="range"] {
    width: 100%;
    background: var(--bg-primary);
    border: 1px solid var(--border-color);
    color: var(--text-primary);
    padding: 4px 8px;
    border-radius: var(--radius-sm);
    font-size: 12px;
  }

/* 変更後: selectとrange inputを分離 */
select {
    width: 100%;
    background: var(--bg-primary);
    border: 1px solid var(--border-color);
    color: var(--text-primary);
    padding: 4px 8px;
    border-radius: var(--radius-sm);
    font-size: 12px;
  }

  input[type="range"] {
    width: 100%;
    height: 20px;
    -webkit-appearance: none;
    appearance: none;
    background: transparent;
    cursor: pointer;
    padding: 0;
    margin: 0;
  }

  input[type="range"]::-webkit-slider-track {
    height: 4px;
    background: var(--bg-primary);
    border-radius: 2px;
    border: 1px solid var(--border-color);
  }

  input[type="range"]::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: var(--accent);
    border: none;
    margin-top: -6px;
    cursor: pointer;
  }

  input[type="range"]::-webkit-slider-thumb:hover {
    background: var(--accent-hover);
  }

  input[type="range"]::-moz-range-track {
    height: 4px;
    background: var(--bg-primary);
    border-radius: 2px;
    border: 1px solid var(--border-color);
  }

  input[type="range"]::-moz-range-thumb {
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: var(--accent);
    border: none;
    cursor: pointer;
  }
```

- [ ] **Step 2: 動作確認**

Run: `make dev`
確認手順:
1. 品質スライダーを左端（1）まで動かせるか
2. 品質スライダーを右端（100）まで動かせるか
3. 最大サイズスライダーを左端（1）・右端（50）まで動かせるか
4. ラベルの数値が正しく更新されるか

- [ ] **Step 3: Commit**

```bash
git add gui-frontend/src/lib/SettingsPanel.svelte
git commit -m "fix: スライダーが端まで到達しない問題を修正"
```

---

## Task 3: BUG-1 バックエンド — サムネイルLRUキャッシュ追加

**Files:**
- Modify: `gui/Cargo.toml`
- Modify: `gui/src/state.rs`
- Modify: `gui/src/commands.rs`

- [ ] **Step 1: `lru`クレートを依存に追加**

```toml
# gui/Cargo.toml の [dependencies] に追加
lru = "0.12"
```

- [ ] **Step 2: `state.rs`にLRUキャッシュを追加**

```rust
// gui/src/state.rs 全体を置換
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use lru::LruCache;
use std::num::NonZeroUsize;

pub struct ProcessingState {
    pub cancel_flag: Arc<AtomicBool>,
    pub thumbnail_cache: Mutex<LruCache<String, String>>,
}

impl ProcessingState {
    pub fn new() -> Self {
        Self {
            cancel_flag: Arc::new(AtomicBool::new(false)),
            thumbnail_cache: Mutex::new(
                LruCache::new(NonZeroUsize::new(500).unwrap())
            ),
        }
    }
}
```

- [ ] **Step 3: `get_thumbnail`コマンドでキャッシュを使用**

```rust
// gui/src/commands.rs の get_thumbnail を修正
#[tauri::command]
pub async fn get_thumbnail(
    state: tauri::State<'_, ProcessingState>,
    path: String,
) -> Result<String, String> {
    // キャッシュ確認
    {
        let mut cache = state.thumbnail_cache.lock().unwrap();
        if let Some(cached) = cache.get(&path) {
            return Ok(cached.clone());
        }
    }

    // キャッシュミス: 生成
    let result = core::generate_thumbnail_base64(Path::new(&path), 200)
        .map_err(|e| e.to_string())?;

    // キャッシュに保存
    {
        let mut cache = state.thumbnail_cache.lock().unwrap();
        cache.put(path, result.clone());
    }

    Ok(result)
}
```

- [ ] **Step 4: ビルド確認**

Run: `cd gui && cargo build 2>&1`
Expected: コンパイル成功

- [ ] **Step 5: Commit**

```bash
git add gui/Cargo.toml gui/src/state.rs gui/src/commands.rs
git commit -m "perf: サムネイルLRUキャッシュをバックエンドに追加（上限500エントリ）"
```

---

## Task 4: BUG-4 + BUG-1 フロントエンド — サムネイルキャッシュ統合とパフォーマンス改善

**Files:**
- Modify: `gui-frontend/src/App.svelte`
- Modify: `gui-frontend/src/lib/ThumbnailGrid.svelte`
- Modify: `gui-frontend/src/lib/SelectionList.svelte`

- [ ] **Step 1: `App.svelte`にキャッシュ一元管理と並列制限ロード関数を追加**

```svelte
<!-- gui-frontend/src/App.svelte の <script> 内 -->
<!-- 既存の import に getThumbnail を追加 -->
import { listImages, processImages, cancelProcessing, getThumbnail } from "./lib/api";

<!-- 既存の thumbnailCache 宣言はそのまま維持（型も同じ） -->
<!-- 以下を新規追加: -->

// --- サムネイルロード（並列制限キュー） ---
let activeRequests = 0;
const MAX_CONCURRENT = 3;
const pendingQueue: string[] = [];

function processQueue() {
    while (activeRequests < MAX_CONCURRENT && pendingQueue.length > 0) {
      const path = pendingQueue.shift()!;
      if (thumbnailCache.has(path)) continue;

      activeRequests++;
      getThumbnail(path)
        .then((base64) => {
          thumbnailCache.set(path, base64);
          thumbnailCache = new Map(thumbnailCache);
        })
        .catch(() => {})
        .finally(() => {
          activeRequests--;
          processQueue();
        });
    }
  }

function handleRequestThumbnail(path: string) {
    if (thumbnailCache.has(path)) return;
    if (!pendingQueue.includes(path)) {
      pendingQueue.push(path);
    }
    processQueue();
  }

// --- currentFolder state（FEAT-4用） ---
let currentFolder = $state("");

// --- handleSelectFolder を修正して currentFolder を更新 ---
// 変更前:
// async function handleSelectFolder(path: string) {
// 変更後:
async function handleSelectFolder(path: string) {
    currentFolder = path;
    try {
      images = await listImages(path);
    } catch (e) {
      console.error("Failed to list images:", e);
      images = [];
    }
  }
```

- [ ] **Step 2: `App.svelte`のテンプレートを修正してコールバックを渡す**

```svelte
<!-- gui-frontend/src/App.svelte テンプレート -->
<div class="center-panel">
    <ThumbnailGrid
      {images}
      {selectedPaths}
      {thumbnailCache}
      onToggleSelect={handleToggleSelect}
      onRequestThumbnail={handleRequestThumbnail}
    />
  </div>

  <div class="right-panel">
    <SelectionList
      {selectedImages}
      {thumbnailCache}
      onRemove={handleRemove}
      onRequestThumbnail={handleRequestThumbnail}
    />
    <SettingsPanel
      bind:config
      {outputFolder}
      {canProcess}
      {currentFolder}
      onPickOutputFolder={handlePickOutputFolder}
      onProcess={handleProcess}
    />
  </div>
```

- [ ] **Step 3: `ThumbnailGrid.svelte`からローカルキャッシュを除去し、Intersection Observerを追加**

```svelte
<!-- gui-frontend/src/lib/ThumbnailGrid.svelte 全体置換 -->
<script lang="ts">
  import type { ImageEntry } from "./types";

  interface Props {
    images: ImageEntry[];
    selectedPaths: Set<string>;
    thumbnailCache: Map<string, string>;
    onToggleSelect: (image: ImageEntry) => void;
    onRequestThumbnail: (path: string) => void;
  }

  let { images, selectedPaths, thumbnailCache, onToggleSelect, onRequestThumbnail }: Props = $props();

  const PAGE_SIZE = 50;
  let currentPage = $state(0);

  let pagedImages = $derived(
    images.slice(currentPage * PAGE_SIZE, (currentPage + 1) * PAGE_SIZE)
  );
  let totalPages = $derived(Math.ceil(images.length / PAGE_SIZE));

  // フォルダー変更時にページリセット
  $effect(() => {
    images;
    currentPage = 0;
  });

  // Intersection Observer でサムネイルの遅延ロード
  function observeThumbnail(node: HTMLElement, path: string) {
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries[0].isIntersecting) {
          onRequestThumbnail(path);
          observer.disconnect();
        }
      },
      { rootMargin: "200px" }
    );
    observer.observe(node);
    return {
      destroy() {
        observer.disconnect();
      },
    };
  }
</script>

<div class="thumbnail-grid">
  <div class="grid-header">
    <span class="count">{images.length} 枚</span>
    {#if totalPages > 1}
      <div class="pagination">
        <button
          onclick={() => (currentPage = Math.max(0, currentPage - 1))}
          disabled={currentPage === 0}>←</button>
        <span>{currentPage + 1} / {totalPages}</span>
        <button
          onclick={() => (currentPage = Math.min(totalPages - 1, currentPage + 1))}
          disabled={currentPage >= totalPages - 1}>→</button>
      </div>
    {/if}
  </div>

  <div class="grid">
    {#each pagedImages as image (image.path)}
      <button
        class="grid-item"
        class:selected={selectedPaths.has(image.path)}
        onclick={() => onToggleSelect(image)}
        use:observeThumbnail={image.path}
      >
        <div class="thumb-wrapper">
          {#if thumbnailCache.has(image.path)}
            <img
              src="data:image/jpeg;base64,{thumbnailCache.get(image.path)}"
              alt={image.name}
            />
          {:else}
            <div class="placeholder">📷</div>
          {/if}
          {#if selectedPaths.has(image.path)}
            <span class="check">✓</span>
          {/if}
        </div>
        <span class="filename">{image.name}</span>
      </button>
    {/each}
  </div>
</div>
<!-- CSSは既存のまま維持（変更なし） -->
```

- [ ] **Step 4: `SelectionList.svelte`に`onRequestThumbnail`を追加**

```svelte
<!-- gui-frontend/src/lib/SelectionList.svelte の <script> 修正 -->
<script lang="ts">
  import type { ImageEntry } from "./types";

  interface Props {
    selectedImages: ImageEntry[];
    thumbnailCache: Map<string, string>;
    onRemove: (image: ImageEntry) => void;
    onRequestThumbnail: (path: string) => void;
  }

  let { selectedImages, thumbnailCache, onRemove, onRequestThumbnail }: Props = $props();

  // 選択リストに追加された画像のサムネイルがキャッシュにない場合、ロード要求
  $effect(() => {
    for (const img of selectedImages) {
      if (!thumbnailCache.has(img.path)) {
        onRequestThumbnail(img.path);
      }
    }
  });
</script>
<!-- テンプレートとCSSは既存のまま -->
```

- [ ] **Step 5: 動作確認**

Run: `make dev`
確認手順:
1. フォルダーを開いて画像のサムネイルが順次表示されるか
2. CPU使用率が100%に張り付かないか（タスクマネージャーで確認）
3. 画像を選択して右パネルにサムネイルが表示されるか
4. 高速にフォルダーを切り替えても問題ないか

- [ ] **Step 6: Commit**

```bash
git add gui-frontend/src/App.svelte gui-frontend/src/lib/ThumbnailGrid.svelte gui-frontend/src/lib/SelectionList.svelte
git commit -m "fix: サムネイルキャッシュ統合とパフォーマンス改善

- App.svelteにキャッシュを一元化し並列制限（同時3リクエスト）
- ThumbnailGridにIntersection Observerで遅延ロード
- SelectionListのサムネイル非表示バグを修正"
```

---

## Task 5: FEAT-4 — 出力先フォルダー初期位置

**Files:**
- Modify: `gui-frontend/src/lib/SettingsPanel.svelte`
- Modify: `gui-frontend/src/App.svelte` (Task 4で`currentFolder`は追加済み)

- [ ] **Step 1: `SettingsPanel.svelte`に`currentFolder` propsを追加しダイアログに渡す**

```svelte
<!-- gui-frontend/src/lib/SettingsPanel.svelte の <script> 修正 -->
<!-- openのimportは不要（ダイアログ呼び出しはApp.svelte側で行う） -->
<script lang="ts">
  import type { ProcessingConfig } from "./types";

  interface Props {
    config: ProcessingConfig;
    outputFolder: string;
    canProcess: boolean;
    currentFolder: string;
    onPickOutputFolder: () => void;
    onProcess: () => void;
  }

  let { config = $bindable(), outputFolder, canProcess, currentFolder, onPickOutputFolder, onProcess }: Props = $props();
</script>
```

- [ ] **Step 2: `App.svelte`の`handlePickOutputFolder`を修正して`currentFolder`を使用**

```typescript
// gui-frontend/src/App.svelte
// 変更前:
async function handlePickOutputFolder() {
    const selected = await open({ directory: true, multiple: false });
    if (selected) {
      outputFolder = selected as string;
    }
  }

// 変更後:
async function handlePickOutputFolder() {
    const selected = await open({
      directory: true,
      multiple: false,
      defaultPath: currentFolder || undefined,
    });
    if (selected) {
      outputFolder = selected as string;
    }
  }
```

- [ ] **Step 3: 動作確認**

Run: `make dev`
確認手順:
1. フォルダーツリーでフォルダーを選択
2. 「フォルダーを選択...」ボタンをクリック
3. ダイアログが選択中フォルダーから開始するか確認

- [ ] **Step 4: Commit**

```bash
git add gui-frontend/src/App.svelte gui-frontend/src/lib/SettingsPanel.svelte
git commit -m "feat: 出力先フォルダー選択を現在のフォルダーから開始"
```

---

## Task 6: FEAT-2 — サムネイルサイズ可変

**Files:**
- Modify: `gui-frontend/src/lib/ThumbnailGrid.svelte`

- [ ] **Step 1: ツールバーにスライダーを追加し、グリッド列数を動的に制御**

```svelte
<!-- gui-frontend/src/lib/ThumbnailGrid.svelte -->
<!-- <script> 内に追加 -->
let columnCount = $state(4);

<!-- grid-header のテンプレートを修正 -->
<div class="grid-header">
    <span class="count">{images.length} 枚</span>
    <div class="toolbar-right">
      <div class="size-control">
        <span class="size-label">🖼</span>
        <input
          type="range"
          min="2"
          max="8"
          bind:value={columnCount}
          class="size-slider"
        />
      </div>
      {#if totalPages > 1}
        <div class="pagination">
          <button
            onclick={() => (currentPage = Math.max(0, currentPage - 1))}
            disabled={currentPage === 0}>←</button>
          <span>{currentPage + 1} / {totalPages}</span>
          <button
            onclick={() => (currentPage = Math.min(totalPages - 1, currentPage + 1))}
            disabled={currentPage >= totalPages - 1}>→</button>
        </div>
      {/if}
    </div>
  </div>

<!-- .grid の style 属性を追加 -->
<div class="grid" style="grid-template-columns: repeat({columnCount}, 1fr);">
```

- [ ] **Step 2: ツールバーのCSSを追加**

```css
/* ThumbnailGrid.svelte の <style> に追加 */
.toolbar-right {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .size-control {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .size-label {
    font-size: 12px;
  }

  .size-slider {
    width: 80px;
    height: 16px;
    -webkit-appearance: none;
    appearance: none;
    background: transparent;
    cursor: pointer;
    padding: 0;
    margin: 0;
  }

  .size-slider::-webkit-slider-track {
    height: 3px;
    background: var(--border-color);
    border-radius: 2px;
  }

  .size-slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--accent);
    border: none;
    margin-top: -5px;
    cursor: pointer;
  }

  .size-slider::-moz-range-track {
    height: 3px;
    background: var(--border-color);
    border-radius: 2px;
  }

  .size-slider::-moz-range-thumb {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--accent);
    border: none;
    cursor: pointer;
  }
```

- [ ] **Step 3: `.grid`の既存CSSから`grid-template-columns`を削除**

```css
/* 変更前: */
.grid {
    ...
    grid-template-columns: repeat(auto-fill, minmax(120px, 1fr));
    ...
  }

/* 変更後: (grid-template-columns をinline styleに移行したため削除) */
.grid {
    flex: 1;
    overflow-y: auto;
    padding: 12px;
    display: grid;
    gap: 8px;
    align-content: start;
  }
```

- [ ] **Step 4: 動作確認**

Run: `make dev`
確認手順:
1. スライダーで列数を2〜8に変更できるか
2. サムネイルがグリッドに正しく配置されるか

- [ ] **Step 5: Commit**

```bash
git add gui-frontend/src/lib/ThumbnailGrid.svelte
git commit -m "feat: サムネイルサイズ可変スライダーを追加"
```

---

## Task 7: FEAT-1 バックエンド — `get_full_image`コマンド追加

**Files:**
- Modify: `core/src/lib.rs`
- Modify: `gui/src/commands.rs`
- Modify: `gui/src/main.rs`

- [ ] **Step 1: `core/src/lib.rs`に`generate_full_image_base64`関数を追加**

`generate_thumbnail_base64`の直後に追加:

```rust
// core/src/lib.rs — generate_thumbnail_base64 の直後に追加
pub fn generate_full_image_base64(
    path: &Path,
    max_width: u32,
    max_height: u32,
) -> Result<String> {
    use base64::Engine as _;

    // 解像度上限クランプ
    let max_width = max_width.min(2560);
    let max_height = max_height.min(1600);

    let img = image::open(path)
        .with_context(|| format!("Failed to open image: {}", path.display()))?;

    let (w, h) = img.dimensions();

    // リサイズが必要か判定
    let resized = if w > max_width || h > max_height {
        img.resize(max_width, max_height, image::imageops::FilterType::Lanczos3)
    } else {
        img
    };

    let rgb = resized.to_rgb8();

    let mut jpeg_bytes = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut jpeg_bytes);
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut cursor, 90);
    encoder.encode(
        rgb.as_raw(),
        rgb.width(),
        rgb.height(),
        image::ColorType::Rgb8,
    )?;

    Ok(base64::engine::general_purpose::STANDARD.encode(&jpeg_bytes))
}
```

- [ ] **Step 2: テストを追加**

```rust
// core/src/lib.rs の #[cfg(test)] mod tests 内に追加
#[test]
fn generate_full_image_returns_valid_base64_jpeg() {
    let dir = tempfile::tempdir().unwrap();
    let img_path = dir.path().join("test.jpg");

    // 100x100 テスト画像作成
    let img = image::RgbImage::from_fn(100, 100, |_, _| image::Rgb([128, 128, 128]));
    img.save(&img_path).unwrap();

    let result = generate_full_image_base64(&img_path, 50, 50).unwrap();
    assert!(!result.is_empty());

    // base64デコードしてJPEGとして有効か確認
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD.decode(&result).unwrap();
    assert!(bytes.len() > 0);
}

#[test]
fn generate_full_image_clamps_resolution_to_max() {
    let dir = tempfile::tempdir().unwrap();
    let img_path = dir.path().join("test.jpg");

    let img = image::RgbImage::from_fn(100, 100, |_, _| image::Rgb([128, 128, 128]));
    img.save(&img_path).unwrap();

    // 上限を超えた値を渡しても動作する
    let result = generate_full_image_base64(&img_path, 10000, 10000).unwrap();
    assert!(!result.is_empty());
}
```

- [ ] **Step 3: テスト実行**

Run: `cd core && cargo test generate_full_image 2>&1`
Expected: 2テスト PASS

- [ ] **Step 4: `gui/src/commands.rs`に`get_full_image`コマンドを追加**

```rust
// gui/src/commands.rs — get_thumbnail の後に追加
#[tauri::command]
pub async fn get_full_image(
    path: String,
    max_width: u32,
    max_height: u32,
) -> Result<String, String> {
    core::generate_full_image_base64(Path::new(&path), max_width, max_height)
        .map_err(|e| e.to_string())
}
```

- [ ] **Step 5: `main.rs`にコマンドを登録**

```rust
// gui/src/main.rs の invoke_handler に追加
.invoke_handler(tauri::generate_handler![
    commands::list_directory,
    commands::list_drives,
    commands::list_images,
    commands::get_thumbnail,
    commands::get_full_image,   // 追加
    commands::process_images,
    commands::cancel_processing,
])
```

- [ ] **Step 6: ビルド確認**

Run: `cd gui && cargo build 2>&1`
Expected: コンパイル成功

- [ ] **Step 7: Commit**

```bash
git add core/src/lib.rs gui/src/commands.rs gui/src/main.rs
git commit -m "feat: get_full_image コマンドを追加（プレビュー用高画質画像取得）"
```

---

## Task 8: FEAT-1 フロントエンド — 画像プレビューモーダル

**Files:**
- Modify: `gui-frontend/src/lib/api.ts`
- Create: `gui-frontend/src/lib/ImagePreview.svelte`
- Modify: `gui-frontend/src/App.svelte`
- Modify: `gui-frontend/src/lib/ThumbnailGrid.svelte`
- Modify: `gui-frontend/src/lib/SelectionList.svelte`

- [ ] **Step 1: `api.ts`に`getFullImage`を追加**

```typescript
// gui-frontend/src/lib/api.ts に追加
export async function getFullImage(
  path: string,
  maxWidth: number,
  maxHeight: number
): Promise<string> {
  return invoke("get_full_image", { path, maxWidth, maxHeight });
}
```

- [ ] **Step 2: `ImagePreview.svelte`を新規作成**

```svelte
<!-- gui-frontend/src/lib/ImagePreview.svelte -->
<script lang="ts">
  import { getFullImage } from "./api";
  import type { ImageEntry } from "./types";

  interface Props {
    image: ImageEntry;
    images: ImageEntry[];
    selectedPaths: Set<string>;
    onToggleSelect: (image: ImageEntry) => void;
    onClose: () => void;
    onNavigate: (image: ImageEntry) => void;
  }

  let { image, images, selectedPaths, onToggleSelect, onClose, onNavigate }: Props = $props();

  let fullImageData = $state<string | null>(null);
  let loading = $state(false);

  let currentIndex = $derived(images.findIndex((img) => img.path === image.path));
  let hasPrev = $derived(currentIndex > 0);
  let hasNext = $derived(currentIndex < images.length - 1);
  let isSelected = $derived(selectedPaths.has(image.path));

  $effect(() => {
    loadFullImage(image.path);
  });

  async function loadFullImage(path: string) {
    loading = true;
    fullImageData = null;
    try {
      const maxW = Math.min(window.innerWidth - 80, 2560);
      const maxH = Math.min(window.innerHeight - 120, 1600);
      fullImageData = await getFullImage(path, maxW, maxH);
    } catch (e) {
      console.error("Failed to load full image:", e);
    } finally {
      loading = false;
    }
  }

  function goPrev() {
    if (hasPrev) onNavigate(images[currentIndex - 1]);
  }

  function goNext() {
    if (hasNext) onNavigate(images[currentIndex + 1]);
  }

  function handleKeydown(e: KeyboardEvent) {
    switch (e.key) {
      case "ArrowLeft":
        e.preventDefault();
        goPrev();
        break;
      case "ArrowRight":
        e.preventDefault();
        goNext();
        break;
      case "Escape":
        e.preventDefault();
        onClose();
        break;
      case " ":
        e.preventDefault();
        onToggleSelect(image);
        break;
    }
  }

  function handleOverlayClick(e: MouseEvent) {
    if ((e.target as HTMLElement).classList.contains("preview-overlay")) {
      onClose();
    }
  }

  function formatSize(bytes: number): string {
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)}KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)}MB`;
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div
  class="preview-overlay"
  role="dialog"
  aria-modal="true"
  onclick={handleOverlayClick}
>
  <!-- 選択ボタン -->
  <button
    class="select-btn"
    class:selected={isSelected}
    onclick={() => onToggleSelect(image)}
  >
    {#if isSelected}
      <span>✓ 選択済み</span>
    {:else}
      <span>○ 選択する</span>
    {/if}
  </button>

  <!-- 閉じるボタン -->
  <button class="close-btn" onclick={onClose}>✕</button>

  <!-- ナビゲーション矢印 -->
  {#if hasPrev}
    <button class="nav-btn nav-prev" onclick={goPrev}>‹</button>
  {/if}
  {#if hasNext}
    <button class="nav-btn nav-next" onclick={goNext}>›</button>
  {/if}

  <!-- 画像 -->
  <div class="image-container">
    {#if loading}
      <div class="loading">読み込み中...</div>
    {:else if fullImageData}
      <img
        src="data:image/jpeg;base64,{fullImageData}"
        alt={image.name}
        class="preview-image"
      />
    {/if}
  </div>

  <!-- 情報バー -->
  <div class="info-bar">
    <span>{image.name}</span>
    <span>{image.width} × {image.height} · {formatSize(image.size_bytes)}</span>
    <span>{currentIndex + 1} / {images.length}</span>
  </div>
</div>

<style>
  .preview-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.92);
    z-index: 200;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .select-btn {
    position: absolute;
    top: 16px;
    left: 16px;
    padding: 6px 14px;
    border-radius: 6px;
    font-size: 13px;
    cursor: pointer;
    z-index: 210;
    display: flex;
    align-items: center;
    gap: 6px;
    background: transparent;
    border: 1px solid rgba(255, 255, 255, 0.3);
    color: rgba(255, 255, 255, 0.7);
  }

  .select-btn.selected {
    background: var(--accent);
    border-color: var(--accent);
    color: white;
  }

  .select-btn:hover {
    background: var(--accent-hover);
    border-color: var(--accent-hover);
    color: white;
  }

  .close-btn {
    position: absolute;
    top: 16px;
    right: 16px;
    background: none;
    border: none;
    color: rgba(255, 255, 255, 0.7);
    font-size: 24px;
    cursor: pointer;
    z-index: 210;
    padding: 4px 8px;
  }

  .close-btn:hover {
    color: white;
  }

  .nav-btn {
    position: absolute;
    top: 50%;
    transform: translateY(-50%);
    background: none;
    border: none;
    color: rgba(255, 255, 255, 0.5);
    font-size: 48px;
    cursor: pointer;
    z-index: 210;
    padding: 16px;
    line-height: 1;
  }

  .nav-btn:hover {
    color: white;
  }

  .nav-prev {
    left: 8px;
  }

  .nav-next {
    right: 8px;
  }

  .image-container {
    max-width: calc(100vw - 120px);
    max-height: calc(100vh - 100px);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .preview-image {
    max-width: 100%;
    max-height: calc(100vh - 100px);
    object-fit: contain;
    border-radius: 4px;
  }

  .loading {
    color: rgba(255, 255, 255, 0.5);
    font-size: 16px;
  }

  .info-bar {
    position: absolute;
    bottom: 0;
    left: 0;
    right: 0;
    padding: 10px 20px;
    background: rgba(0, 0, 0, 0.6);
    display: flex;
    justify-content: space-between;
    color: rgba(255, 255, 255, 0.6);
    font-size: 12px;
    z-index: 210;
  }
</style>
```

- [ ] **Step 3: `App.svelte`にプレビュー状態管理と`currentPage`リフトアップを追加**

```svelte
<!-- gui-frontend/src/App.svelte の <script> に追加 -->

// --- ページ状態（App.svelteにリフトアップ） ---
const PAGE_SIZE = 50;
let currentPage = $state(0);

// --- プレビュー状態 ---
let previewImage = $state<ImageEntry | null>(null);

function handlePreview(image: ImageEntry) {
    previewImage = image;
  }

function handleClosePreview() {
    previewImage = null;
  }

function handleNavigatePreview(image: ImageEntry) {
    // プレビューでナビゲートした画像が現在ページ外なら、ページを更新
    const idx = images.findIndex((img) => img.path === image.path);
    if (idx >= 0) {
      const targetPage = Math.floor(idx / PAGE_SIZE);
      if (targetPage !== currentPage) {
        currentPage = targetPage;
      }
    }
    previewImage = image;
  }
```

```svelte
<!-- App.svelte の import に追加 -->
import ImagePreview from "./lib/ImagePreview.svelte";
```

```svelte
<!-- App.svelte の handleSelectFolder 内に currentPage リセットを追加 -->
// handleSelectFolder の冒頭で:
currentPage = 0;
```

```svelte
<!-- App.svelte のテンプレートに追加（ProgressOverlay の直前） -->
{#if previewImage}
  <ImagePreview
    image={previewImage}
    {images}
    {selectedPaths}
    onToggleSelect={handleToggleSelect}
    onClose={handleClosePreview}
    onNavigate={handleNavigatePreview}
  />
{/if}
```

- [ ] **Step 4: `ThumbnailGrid.svelte`に`currentPage` propsとダブルクリックハンドラーを追加**

`currentPage`をローカル`$state`からpropsに変更し、`onPageChange`コールバックで親に通知:

```svelte
<!-- ThumbnailGrid.svelte の Props interface を変更 -->
interface Props {
    images: ImageEntry[];
    selectedPaths: Set<string>;
    thumbnailCache: Map<string, string>;
    currentPage: number;
    onToggleSelect: (image: ImageEntry) => void;
    onRequestThumbnail: (path: string) => void;
    onPreview: (image: ImageEntry) => void;
    onPageChange: (page: number) => void;
  }

let { images, selectedPaths, thumbnailCache, currentPage, onToggleSelect, onRequestThumbnail, onPreview, onPageChange }: Props = $props();

<!-- ローカルの currentPage $state を削除 -->
<!-- PAGE_SIZE は残す（ローカル定数として） -->
const PAGE_SIZE = 50;

<!-- ページネーションボタンの onclick を onPageChange に変更 -->
<button
  onclick={() => onPageChange(Math.max(0, currentPage - 1))}
  disabled={currentPage === 0}>←</button>
...
<button
  onclick={() => onPageChange(Math.min(totalPages - 1, currentPage + 1))}
  disabled={currentPage >= totalPages - 1}>→</button>

<!-- フォルダー変更時のページリセット $effect を削除（App側で管理） -->

<!-- grid-item にダブルクリックを追加 -->
<button
  class="grid-item"
  class:selected={selectedPaths.has(image.path)}
  onclick={() => onToggleSelect(image)}
  ondblclick={(e) => { e.preventDefault(); onPreview(image); }}
  use:observeThumbnail={image.path}
>
```

```svelte
<!-- App.svelte の ThumbnailGrid に currentPage, onPageChange, onPreview を追加 -->
<ThumbnailGrid
  {images}
  {selectedPaths}
  {thumbnailCache}
  {currentPage}
  onToggleSelect={handleToggleSelect}
  onRequestThumbnail={handleRequestThumbnail}
  onPreview={handlePreview}
  onPageChange={(page) => (currentPage = page)}
/>
```

- [ ] **Step 5: `SelectionList.svelte`にダブルクリックハンドラーを追加**

```svelte
<!-- SelectionList.svelte の Props interface に追加 -->
onPreview: (image: ImageEntry) => void;

<!-- $props() に追加 -->
let { selectedImages, thumbnailCache, onRemove, onRequestThumbnail, onPreview }: Props = $props();

<!-- .item にダブルクリックを追加、.thumb部分にクリック拡大 -->
<div class="item" ondblclick={() => onPreview(image)}>
```

```svelte
<!-- App.svelte の SelectionList に onPreview を追加 -->
<SelectionList
  {selectedImages}
  {thumbnailCache}
  onRemove={handleRemove}
  onRequestThumbnail={handleRequestThumbnail}
  onPreview={handlePreview}
/>
```

- [ ] **Step 6: 動作確認**

Run: `make dev`
確認手順:
1. サムネイルをダブルクリック → モーダルで高画質プレビュー表示
2. ←→キーで前後画像に移動
3. Escapeで閉じる
4. 左上ボタンまたはSpaceで選択トグル
5. 選択リストからもダブルクリックでプレビュー表示
6. 背景クリックで閉じる

- [ ] **Step 7: Commit**

```bash
git add gui-frontend/src/lib/api.ts gui-frontend/src/lib/ImagePreview.svelte gui-frontend/src/App.svelte gui-frontend/src/lib/ThumbnailGrid.svelte gui-frontend/src/lib/SelectionList.svelte
git commit -m "feat: 画像プレビューモーダル追加（ダブルクリック拡大、キーボードナビゲーション、選択トグル）"
```

---

## Task 9: FEAT-3 — フォルダーお気に入り

**Files:**
- Modify: `gui/Cargo.toml`
- Modify: `gui/src/main.rs`
- Modify: `gui/capabilities/default.json`
- Modify: `gui-frontend/package.json`
- Modify: `gui-frontend/src/lib/FolderTree.svelte`

- [ ] **Step 1: バックエンド依存を追加**

```toml
# gui/Cargo.toml の [dependencies] に追加
tauri-plugin-store = "2"
```

```rust
// gui/src/main.rs の tauri::Builder に追加
.plugin(tauri_plugin_store::Builder::default().build())
```

- [ ] **Step 2: capability permissionsを追加**

```json
// gui/capabilities/default.json
{
  "$schema": "https://schema.tauri.app/config/2",
  "identifier": "default",
  "description": "Default capability",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "dialog:default",
    "dialog:allow-open",
    "store:allow-set",
    "store:allow-get",
    "store:allow-save",
    "store:allow-load"
  ]
}
```

- [ ] **Step 3: フロントエンド依存を追加**

Run: `cd gui-frontend && bun add @tauri-apps/plugin-store`

- [ ] **Step 4: ビルド確認**

Run: `cd gui && cargo build 2>&1`
Expected: コンパイル成功

- [ ] **Step 5: `FolderTree.svelte`にお気に入り機能を追加**

```svelte
<!-- gui-frontend/src/lib/FolderTree.svelte 全体置換 -->
<script lang="ts">
  import { listDirectory, listDrives } from "./api";
  import { load } from "@tauri-apps/plugin-store";
  import type { FileEntry } from "./types";

  interface Props {
    onSelectFolder: (path: string) => void;
  }

  let { onSelectFolder }: Props = $props();

  interface TreeNode {
    entry: FileEntry;
    children: TreeNode[] | null;
    expanded: boolean;
    loading: boolean;
  }

  let roots = $state<TreeNode[]>([]);
  let selectedPath = $state("");
  let favorites = $state<string[]>([]);

  // コンテキストメニュー状態
  let contextMenu = $state<{ x: number; y: number; path: string; isFavorite: boolean } | null>(null);

  // --- お気に入り永続化 ---
  let store: Awaited<ReturnType<typeof load>> | null = null;

  async function initStore() {
    store = await load("favorites.json", { autoSave: false });
    const saved = await store.get<string[]>("favorites");
    if (saved) {
      favorites = saved;
    }
  }

  async function saveFavorites() {
    if (store) {
      await store.set("favorites", favorites);
      await store.save();
    }
  }

  async function addFavorite(path: string) {
    if (!favorites.includes(path)) {
      favorites = [...favorites, path];
      await saveFavorites();
    }
  }

  async function removeFavorite(path: string) {
    favorites = favorites.filter((f) => f !== path);
    await saveFavorites();
  }

  // --- コンテキストメニュー ---
  function handleContextMenu(e: MouseEvent, path: string, isFavorite: boolean) {
    e.preventDefault();
    contextMenu = { x: e.clientX, y: e.clientY, path, isFavorite };
  }

  function closeContextMenu() {
    contextMenu = null;
  }

  async function handleContextAction() {
    if (!contextMenu) return;
    if (contextMenu.isFavorite) {
      await removeFavorite(contextMenu.path);
    } else {
      await addFavorite(contextMenu.path);
    }
    closeContextMenu();
  }

  // --- ツリー操作 ---
  async function loadRoots() {
    const drives = await listDrives();
    roots = drives.map((drive) => ({
      entry: { name: drive, path: drive, is_dir: true, is_image: false },
      children: null,
      expanded: false,
      loading: false,
    }));

    if (roots.length > 0) {
      await toggleNode(roots[0]);
    }
  }

  async function toggleNode(node: TreeNode) {
    if (!node.entry.is_dir) return;

    if (node.expanded) {
      node.expanded = false;
      return;
    }

    if (node.children === null) {
      node.loading = true;
      try {
        const entries = await listDirectory(node.entry.path);
        node.children = entries
          .filter((e) => e.is_dir)
          .map((entry) => ({
            entry,
            children: null,
            expanded: false,
            loading: false,
          }));
      } catch (e) {
        node.children = [];
      }
      node.loading = false;
    }

    node.expanded = true;
  }

  function selectFolder(node: TreeNode) {
    selectedPath = node.entry.path;
    onSelectFolder(node.entry.path);
    toggleNode(node);
  }

  function selectFavorite(path: string) {
    selectedPath = path;
    onSelectFolder(path);
  }

  function getFolderName(path: string): string {
    const parts = path.replace(/[/\\]+$/, "").split(/[/\\]/);
    return parts[parts.length - 1] || path;
  }

  $effect(() => {
    loadRoots();
    initStore();
  });
</script>

<svelte:window onclick={closeContextMenu} />

<div class="folder-tree">
  <!-- お気に入りセクション -->
  {#if favorites.length > 0}
    <div class="section-header">⭐ お気に入り</div>
    <div class="favorites">
      {#each favorites as fav}
        <button
          class="tree-item"
          class:selected={selectedPath === fav}
          onclick={() => selectFavorite(fav)}
          oncontextmenu={(e) => handleContextMenu(e, fav, true)}
          title={fav}
        >
          <span class="icon">📁</span>
          <span class="name">{getFolderName(fav)}</span>
        </button>
      {/each}
    </div>
  {/if}

  <!-- ドライブセクション -->
  <div class="section-header">💾 ドライブ</div>
  <div class="tree-content">
    {#each roots as node}
      {@render treeNode(node, 0)}
    {/each}
  </div>
</div>

{#snippet treeNode(node: TreeNode, depth: number)}
  <button
    class="tree-item"
    class:selected={selectedPath === node.entry.path}
    style="padding-left: {12 + depth * 16}px"
    onclick={() => selectFolder(node)}
    oncontextmenu={(e) => handleContextMenu(e, node.entry.path, favorites.includes(node.entry.path))}
  >
    <span class="icon">
      {#if node.loading}
        ⏳
      {:else if node.expanded}
        📂
      {:else}
        📁
      {/if}
    </span>
    <span class="name">{node.entry.name}</span>
  </button>

  {#if node.expanded && node.children}
    {#each node.children as child}
      {@render treeNode(child, depth + 1)}
    {/each}
  {/if}
{/snippet}

<!-- コンテキストメニュー -->
{#if contextMenu}
  <div
    class="context-menu"
    style="left: {contextMenu.x}px; top: {contextMenu.y}px"
  >
    <button class="context-item" onclick={handleContextAction}>
      {#if contextMenu.isFavorite}
        ✕ お気に入りから削除
      {:else}
        ⭐ お気に入りに追加
      {/if}
    </button>
  </div>
{/if}

<style>
  .folder-tree {
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--bg-secondary);
    overflow: hidden;
  }

  .section-header {
    padding: 8px 12px;
    color: var(--text-secondary);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    border-bottom: 1px solid var(--border-color);
  }

  .favorites {
    border-bottom: 1px solid var(--border-color);
  }

  .tree-content {
    flex: 1;
    overflow-y: auto;
    padding: 4px 0;
  }

  .tree-item {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 4px 12px;
    border: none;
    background: none;
    color: var(--text-primary);
    font-size: 13px;
    cursor: pointer;
    text-align: left;
  }

  .tree-item:hover {
    background: var(--bg-hover);
  }

  .tree-item.selected {
    background: var(--accent-bg);
    color: var(--accent);
  }

  .icon {
    flex-shrink: 0;
    font-size: 14px;
  }

  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .context-menu {
    position: fixed;
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--radius);
    padding: 4px 0;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.4);
    z-index: 300;
    min-width: 180px;
  }

  .context-item {
    display: block;
    width: 100%;
    padding: 8px 14px;
    border: none;
    background: none;
    color: var(--text-primary);
    font-size: 13px;
    cursor: pointer;
    text-align: left;
  }

  .context-item:hover {
    background: var(--bg-hover);
  }
</style>
```

- [ ] **Step 6: 動作確認**

Run: `make dev`
確認手順:
1. フォルダーを右クリック →「⭐ お気に入りに追加」が表示される
2. クリックするとツリー上部にお気に入りセクションが表示される
3. お気に入りをクリックでそのフォルダーに移動
4. お気に入りを右クリック →「✕ お気に入りから削除」
5. アプリを再起動してもお気に入りが残っている

- [ ] **Step 7: Commit**

```bash
git add gui/Cargo.toml gui/src/main.rs gui/capabilities/default.json gui-frontend/package.json gui-frontend/bun.lockb gui-frontend/src/lib/FolderTree.svelte
git commit -m "feat: フォルダーお気に入り機能を追加（tauri-plugin-store v2で永続化）"
```

---

## 完了後の最終確認

すべてのタスク完了後に以下を確認:

- [ ] `make build` でビルド成功
- [ ] `make test` でテスト全通過
- [ ] 各機能の手動動作確認
