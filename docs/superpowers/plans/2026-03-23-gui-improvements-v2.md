# GUI改善 v2 実装計画

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 5つのGUI改善を実施: お気に入りツリー展開、スライダー視認性、LrCルーペ、EXIF表示、サムネイル改善

**Architecture:** Rustコア(core)にEXIF読み取り機能を追加し、GUIバックエンド(gui)に新Tauriコマンドを追加。フロントエンド(Svelte 5)で既存コンポーネントを拡張。各改善は独立して実装・テスト可能。

**Tech Stack:** Rust (kamadak-exif), Tauri v2, Svelte 5 (runes), TypeScript

**Spec:** `docs/superpowers/specs/2026-03-23-gui-improvements-v2-design.md`

---

## ファイル構造

| ファイル | 変更種別 | 担当タスク |
|---------|---------|-----------|
| `core/Cargo.toml` | Modify | Task 1 |
| `core/src/lib.rs` | Modify | Task 1 |
| `gui/src/commands.rs` | Modify | Task 2, 4 |
| `gui/src/main.rs` | Modify | Task 2 |
| `gui-frontend/src/lib/types.ts` | Modify | Task 2 |
| `gui-frontend/src/lib/api.ts` | Modify | Task 2, 5 |
| `gui-frontend/src/lib/ImagePreview.svelte` | Modify | Task 3, 6 |
| `gui-frontend/src/lib/ThumbnailGrid.svelte` | Modify | Task 5, 8 |
| `gui-frontend/src/lib/SelectionList.svelte` | Modify | Task 5 |
| `gui-frontend/src/App.svelte` | Modify | Task 5 |
| `gui-frontend/src/lib/FolderTree.svelte` | Modify | Task 7 |
| `gui-frontend/src/lib/SettingsPanel.svelte` | Modify | Task 8 |

---

### Task 1: EXIF読み取り機能をcoreに追加

**Files:**
- Modify: `core/Cargo.toml`
- Modify: `core/src/lib.rs`

- [ ] **Step 1: kamadak-exifを依存に追加**

`core/Cargo.toml`の`[dependencies]`に追加:
```toml
kamadak-exif = "0.5"
```

- [ ] **Step 2: ExifInfo構造体とread_exif_info関数のテストを書く**

`core/src/lib.rs`のテストモジュールに追加:
```rust
#[test]
fn read_exif_info_returns_default_for_nonexistent_file() {
    let result = read_exif_info(Path::new("/nonexistent/image.jpg"));
    assert!(result.is_ok());
    let info = result.unwrap();
    assert!(info.camera_make.is_none());
    assert!(info.camera_model.is_none());
    assert!(info.iso.is_none());
}
```

- [ ] **Step 3: テストが失敗することを確認**

Run: `cd /home/biwak/myShrimp/picture-tool-rust && cargo test -p picture-tool-core read_exif`
Expected: コンパイルエラー（read_exif_info未定義）

- [ ] **Step 4: ExifInfo構造体を定義**

`core/src/lib.rs`に構造体を追加。注意: `use serde::{Deserialize, Serialize};`は既存のimportに含まれているので追加不要。
```rust
#[derive(Debug, Clone, Default, Serialize)]
pub struct ExifInfo {
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens_model: Option<String>,
    pub focal_length: Option<String>,
    pub f_number: Option<String>,
    pub shutter_speed: Option<String>,
    pub iso: Option<u32>,
    pub date_taken: Option<String>,
}
```

- [ ] **Step 5: read_exif_info関数を実装**

`core/src/lib.rs`の先頭にuse文を追加:
```rust
use kamadak_exif as exif;
```

関数を追加:
```rust
pub fn read_exif_info(path: &Path) -> Result<ExifInfo> {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return Ok(ExifInfo::default()),
    };
    let mut bufreader = std::io::BufReader::new(file);
    let exif_data = match exif::Reader::new().read_from_container(&mut bufreader) {
        Ok(e) => e,
        Err(_) => return Ok(ExifInfo::default()),
    };

    let get_string = |tag: exif::Tag| -> Option<String> {
        exif_data.get_field(tag, exif::In::PRIMARY)
            .map(|f| f.display_value().with_unit(&exif_data).to_string())
    };

    let iso = exif_data
        .get_field(exif::Tag::PhotographicSensitivity, exif::In::PRIMARY)
        .and_then(|f| match f.value {
            exif::Value::Short(ref v) => v.first().map(|&x| x as u32),
            exif::Value::Long(ref v) => v.first().copied(),
            _ => f.display_value().to_string().parse::<u32>().ok(),
        });

    let shutter_speed = exif_data
        .get_field(exif::Tag::ExposureTime, exif::In::PRIMARY)
        .map(|f| {
            let s = f.display_value().to_string();
            if s.ends_with(" s") {
                s.replace(" s", "s")
            } else {
                format!("{s}s")
            }
        });

    let focal_length = exif_data
        .get_field(exif::Tag::FocalLength, exif::In::PRIMARY)
        .map(|f| {
            let s = f.display_value().to_string();
            if s.ends_with(" mm") {
                s.replace(" mm", "mm")
            } else {
                s
            }
        });

    let f_number = exif_data
        .get_field(exif::Tag::FNumber, exif::In::PRIMARY)
        .map(|f| {
            let s = f.display_value().to_string();
            format!("f/{s}")
        });

    Ok(ExifInfo {
        camera_make: get_string(exif::Tag::Make).map(|s| s.trim().to_string()),
        camera_model: get_string(exif::Tag::Model).map(|s| s.trim().to_string()),
        lens_model: get_string(exif::Tag::LensModel).map(|s| s.trim().to_string()),
        focal_length,
        f_number,
        shutter_speed,
        iso,
        date_taken: get_string(exif::Tag::DateTimeOriginal),
    })
}
```

- [ ] **Step 6: テストが通ることを確認**

Run: `cd /home/biwak/myShrimp/picture-tool-rust && cargo test -p picture-tool-core read_exif`
Expected: PASS

- [ ] **Step 7: コミット**

```bash
git add core/Cargo.toml core/src/lib.rs
git commit -m "feat: ExifInfo構造体とread_exif_info関数を追加（kamadak-exif）"
```

---

### Task 2: EXIF Tauriコマンドとフロントエンド型を追加

**Files:**
- Modify: `gui/src/commands.rs`
- Modify: `gui/src/main.rs`
- Modify: `gui-frontend/src/lib/types.ts`
- Modify: `gui-frontend/src/lib/api.ts`

注意: `gui/src/types.rs`にExifInfoを二重定義しない。`core::ExifInfo`は既に`Serialize`を持つので、Tauriコマンドから直接返せる。

- [ ] **Step 1: gui/src/commands.rsにget_exif_infoコマンドを追加**

`gui/src/commands.rs`に追加:
```rust
#[tauri::command]
pub async fn get_exif_info(path: String) -> Result<core::ExifInfo, String> {
    core::read_exif_info(Path::new(&path)).map_err(|e| e.to_string())
}
```

- [ ] **Step 2: gui/src/main.rsのinvoke_handlerに登録**

`commands::cancel_processing,`の後に追加:
```rust
commands::get_exif_info,
```

- [ ] **Step 3: フロントエンドの型定義を追加**

`gui-frontend/src/lib/types.ts`の末尾に追加:
```typescript
export interface ExifInfo {
  camera_make: string | null;
  camera_model: string | null;
  lens_model: string | null;
  focal_length: string | null;
  f_number: string | null;
  shutter_speed: string | null;
  iso: number | null;
  date_taken: string | null;
}
```

- [ ] **Step 4: フロントエンドのAPI関数を追加**

`gui-frontend/src/lib/api.ts`のimport文に`ExifInfo`を追加:
```typescript
import type {
  FileEntry,
  ImageEntry,
  ProcessingConfig,
  ProcessResult,
  ExifInfo,
} from "./types";
```

ファイル末尾に追加:
```typescript
export async function getExifInfo(path: string): Promise<ExifInfo> {
  return invoke("get_exif_info", { path });
}
```

- [ ] **Step 5: ビルド確認**

Run: `cd /home/biwak/myShrimp/picture-tool-rust && cargo build -p picture-tool-gui`
Expected: SUCCESS

- [ ] **Step 6: コミット**

```bash
git add gui/src/commands.rs gui/src/main.rs gui-frontend/src/lib/types.ts gui-frontend/src/lib/api.ts
git commit -m "feat: get_exif_info Tauriコマンドとフロントエンド型・API追加"
```

---

### Task 3: ImagePreviewにEXIF表示を追加

**Files:**
- Modify: `gui-frontend/src/lib/ImagePreview.svelte`

- [ ] **Step 1: EXIF取得ロジックを追加**

`ImagePreview.svelte`の`<script>`セクションを修正。

importを修正:
```typescript
import { getFullImage, getExifInfo } from "./api";
import type { ImageEntry, ExifInfo } from "./types";
```

状態変数を追加（`let loading`の後に）:
```typescript
let exifInfo = $state<ExifInfo | null>(null);
```

既存の`$effect`（L24-26）を修正してEXIFも並列取得:
```typescript
$effect(() => {
    loadFullImage(image.path);
    loadExifInfo(image.path);
});
```

EXIF取得関数を追加（`loadFullImage`の後に）:
```typescript
async function loadExifInfo(path: string) {
    exifInfo = null;
    try {
        exifInfo = await getExifInfo(path);
    } catch (e) {
        console.error("Failed to load EXIF info:", e);
    }
}
```

EXIF表示用ヘルパー関数を追加:
```typescript
function formatExifLine1(info: ExifInfo): string | null {
    const parts: string[] = [];
    const camera = [info.camera_make, info.camera_model].filter(Boolean).join(" ");
    if (camera) parts.push(camera);
    if (info.lens_model) parts.push(info.lens_model);
    return parts.length > 0 ? parts.join(" | ") : null;
}

function formatExifLine2(info: ExifInfo): string | null {
    const parts: string[] = [];
    if (info.focal_length) parts.push(info.focal_length);
    if (info.f_number) parts.push(info.f_number);
    if (info.shutter_speed) parts.push(info.shutter_speed);
    if (info.iso != null) parts.push(`ISO ${info.iso}`);
    return parts.length > 0 ? parts.join("  ") : null;
}
```

- [ ] **Step 2: EXIF表示テンプレートを追加**

select-btnの直後、nav-btnの前に追加。`top: 56px`はselect-btn（top:16px + 高さ約24px + margin）の下に配置する位置:
```svelte
{#if exifInfo && (formatExifLine1(exifInfo) || formatExifLine2(exifInfo))}
    <div class="exif-overlay">
        {#if formatExifLine1(exifInfo)}
            <div class="exif-line">{formatExifLine1(exifInfo)}</div>
        {/if}
        {#if formatExifLine2(exifInfo)}
            <div class="exif-line">{formatExifLine2(exifInfo)}</div>
        {/if}
    </div>
{/if}
```

- [ ] **Step 3: 下部情報バーに撮影日時を追加**

既存の`info-bar`テンプレート（L124-128）を修正:
```svelte
<div class="info-bar">
    <span>{image.name}</span>
    <span>
        {image.width} × {image.height} · {formatSize(image.size_bytes)}{#if exifInfo?.date_taken} · {exifInfo.date_taken}{/if}
    </span>
    <span>{currentIndex + 1} / {images.length}</span>
</div>
```

- [ ] **Step 4: EXIFオーバーレイのスタイルを追加**

`<style>`セクションに追加:
```css
.exif-overlay {
    position: absolute;
    top: 56px;
    left: 16px;
    z-index: 210;
    pointer-events: none;
}

.exif-line {
    color: rgba(255, 255, 255, 0.85);
    font-size: 12px;
    line-height: 1.5;
    text-shadow: 0 1px 3px rgba(0, 0, 0, 0.8), 0 0 6px rgba(0, 0, 0, 0.5);
}
```

- [ ] **Step 5: ビルド確認**

Run: `cd /home/biwak/myShrimp/picture-tool-rust/gui-frontend && bun run check`
Expected: SUCCESS

- [ ] **Step 6: コミット**

```bash
git add gui-frontend/src/lib/ImagePreview.svelte
git commit -m "feat: プレビューモーダルにEXIF情報オーバーレイ追加"
```

---

### Task 4: サムネイル動的解像度（Rust側）

**Files:**
- Modify: `gui/src/commands.rs`

注意: `core/src/lib.rs`の`generate_thumbnail_base64`は既に`max_dimension: u32`引数を持つ。変更不要。

- [ ] **Step 1: get_thumbnailにmax_dimension引数を追加、キャッシュキーにサイズを含める**

`gui/src/commands.rs`の`get_thumbnail`コマンド全体を置換:
```rust
#[tauri::command]
pub async fn get_thumbnail(
    state: tauri::State<'_, ProcessingState>,
    path: String,
    max_dimension: u32,
) -> Result<String, String> {
    let cache_key = format!("{}:{}", path, max_dimension);

    {
        let mut cache = state.thumbnail_cache.lock().unwrap();
        if let Some(cached) = cache.get(&cache_key) {
            return Ok(cached.clone());
        }
    }

    let result = core::generate_thumbnail_base64(Path::new(&path), max_dimension)
        .map_err(|e| e.to_string())?;

    {
        let mut cache = state.thumbnail_cache.lock().unwrap();
        cache.put(cache_key, result.clone());
    }

    Ok(result)
}
```

- [ ] **Step 2: ビルド確認**

Run: `cd /home/biwak/myShrimp/picture-tool-rust && cargo build -p picture-tool-gui`
Expected: SUCCESS

- [ ] **Step 3: コミット**

```bash
git add gui/src/commands.rs
git commit -m "feat: get_thumbnailにmax_dimension引数追加、キャッシュキーにサイズ含む"
```

---

### Task 5: サムネイル改善（フロントエンド）

**Files:**
- Modify: `gui-frontend/src/lib/api.ts`
- Modify: `gui-frontend/src/lib/ThumbnailGrid.svelte`
- Modify: `gui-frontend/src/lib/SelectionList.svelte`
- Modify: `gui-frontend/src/App.svelte`

- [ ] **Step 1: api.tsのgetThumbnailにmaxDimension引数を追加**

`gui-frontend/src/lib/api.ts`の`getThumbnail`を修正:
```typescript
export async function getThumbnail(path: string, maxDimension: number): Promise<string> {
  return invoke("get_thumbnail", { path, maxDimension });
}
```

- [ ] **Step 2: ThumbnailGrid.svelteのProps型とサムネイルサイズ計算を追加**

`ThumbnailGrid.svelte`のProps内の`onRequestThumbnail`の型を修正:
```typescript
onRequestThumbnail: (path: string, maxDimension: number) => void;
```

`columnCount`の後にサムネイルサイズ計算を追加:
```typescript
let gridElement: HTMLDivElement | undefined = $state();
let thumbSize = $derived.by(() => {
    void columnCount;
    const containerWidth = gridElement?.clientWidth ?? window.innerWidth * 0.5;
    return Math.ceil(containerWidth / columnCount);
});
```

- [ ] **Step 3: ThumbnailGrid.svelteのobserveThumbnailを修正**

既存の`rootMargin: "200px"`を維持しつつ、引数にthumbSizeを渡すよう修正:
```typescript
function observeThumbnail(node: HTMLElement, path: string) {
    const observer = new IntersectionObserver(
        (entries) => {
            if (entries[0].isIntersecting) {
                onRequestThumbnail(path, thumbSize);
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
```

- [ ] **Step 4: ThumbnailGrid.svelteのグリッドdivにbind:thisを追加**

```svelte
<div class="grid" bind:this={gridElement} style="grid-template-columns: repeat({columnCount}, 1fr);">
```

- [ ] **Step 5: CSSのobject-fitをcontainに変更**

`ThumbnailGrid.svelte`のCSS:
```css
.thumb-wrapper img {
    width: 100%;
    height: 100%;
    object-fit: contain;
}
```

- [ ] **Step 6: SelectionList.svelteのonRequestThumbnail型を修正**

`SelectionList.svelte`のProps内:
```typescript
onRequestThumbnail: (path: string, maxDimension: number) => void;
```

SelectionListのサムネイルは小さい固定サイズ（40x50px）なので、`$effect`内の呼び出しに固定値200を渡す:
```typescript
$effect(() => {
    for (const img of selectedImages) {
        if (!thumbnailCache.has(img.path)) {
            onRequestThumbnail(img.path, 200);
        }
    }
});
```

- [ ] **Step 7: App.svelteのhandleRequestThumbnailとキャッシュ処理を修正**

`App.svelte`の`handleRequestThumbnail`のシグネチャとキャッシュキーを修正。フロントエンドキャッシュもpathのみではなく`path:size`キーにして、列数変更時にサイズの異なるサムネイルを正しく再取得できるようにする:

`handleRequestThumbnail`を修正:
```typescript
function handleRequestThumbnail(path: string, maxDimension: number) {
    const cacheKey = `${path}:${maxDimension}`;
    if (thumbnailCache.has(cacheKey)) return;
    if (!pendingQueue.includes(cacheKey)) {
        pendingQueue.push(cacheKey);
    }
    processQueue();
}
```

`processQueue`を修正（キャッシュキーからpathとsizeを分離して呼び出す）:
```typescript
function processQueue() {
    while (activeRequests < MAX_CONCURRENT && pendingQueue.length > 0) {
        const cacheKey = pendingQueue.shift()!;
        if (thumbnailCache.has(cacheKey)) continue;
        const [path, sizeStr] = cacheKey.split(/:([\d]+)$/);
        const maxDimension = parseInt(sizeStr, 10) || 200;
        activeRequests++;
        getThumbnail(path, maxDimension)
            .then((base64) => {
                thumbnailCache.set(cacheKey, base64);
                thumbnailCache = new Map(thumbnailCache);
            })
            .catch(() => {})
            .finally(() => {
                activeRequests--;
                processQueue();
            });
    }
}
```

`ThumbnailGrid`と`SelectionList`の`thumbnailCache.has(image.path)`もキャッシュキー形式に対応する必要がある。ただし、フロントエンドのキャッシュキーをサイズ付きに変えると、テンプレート側の`thumbnailCache.has(image.path)`や`thumbnailCache.get(image.path)`が壊れる。

よりシンプルな代替アプローチ: **フロントエンドキャッシュはpath単位のまま維持し、サムネイルの上書きを許可する。** Rust側のキャッシュがサイズ別で管理するので、フロントエンドはpath単位で最新のサムネイルを保持するだけで良い:

```typescript
function handleRequestThumbnail(path: string, maxDimension: number) {
    if (thumbnailCache.has(path)) return;
    if (!pendingQueue.some(item => item.path === path)) {
        pendingQueue.push({ path, maxDimension });
    }
    processQueue();
}
```

`pendingQueue`の型変更:
```typescript
const pendingQueue: { path: string; maxDimension: number }[] = [];
```

`processQueue`を修正:
```typescript
function processQueue() {
    while (activeRequests < MAX_CONCURRENT && pendingQueue.length > 0) {
        const { path, maxDimension } = pendingQueue.shift()!;
        if (thumbnailCache.has(path)) continue;
        activeRequests++;
        getThumbnail(path, maxDimension)
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
```

列数変更でサムネイルサイズが変わった場合: 既にキャッシュ済みの低解像度サムネイルが表示される。これは許容範囲（フォルダー再選択で再取得される）。

- [ ] **Step 8: ビルド確認**

Run: `cd /home/biwak/myShrimp/picture-tool-rust/gui-frontend && bun run check`
Expected: SUCCESS

- [ ] **Step 9: コミット**

```bash
git add gui-frontend/src/lib/api.ts gui-frontend/src/lib/ThumbnailGrid.svelte gui-frontend/src/lib/SelectionList.svelte gui-frontend/src/App.svelte
git commit -m "feat: サムネイルのアスペクト比保持と4K対応の動的解像度"
```

---

### Task 6: LrCルーペ方式のズーム

**Files:**
- Modify: `gui-frontend/src/lib/ImagePreview.svelte`

- [ ] **Step 1: ズーム状態と計算ロジックを追加**

`ImagePreview.svelte`の`<script>`セクションに状態変数を追加（`let exifInfo`の後に）:
```typescript
let zoomed = $state(false);
let transformOrigin = $state("50% 50%");
let imageElement: HTMLImageElement | undefined = $state();
```

ズーム関連関数を追加:
```typescript
function getZoomScale(): number {
    if (!imageElement || !image) return 1;
    const rendered = imageElement.getBoundingClientRect();
    if (rendered.width === 0) return 1;
    return image.width / rendered.width;
}

function handleImageClick(e: MouseEvent) {
    e.stopPropagation();
    if (zoomed) {
        zoomed = false;
    } else {
        updateTransformOrigin(e);
        zoomed = true;
    }
}

function handleImageMouseMove(e: MouseEvent) {
    if (!zoomed) return;
    updateTransformOrigin(e);
}

function updateTransformOrigin(e: MouseEvent) {
    if (!imageElement) return;
    const rect = imageElement.getBoundingClientRect();
    const x = ((e.clientX - rect.left) / rect.width) * 100;
    const y = ((e.clientY - rect.top) / rect.height) * 100;
    transformOrigin = `${x}% ${y}%`;
}
```

画像切り替え時にズームリセットする`$effect`を追加:
```typescript
$effect(() => {
    void image.path;
    zoomed = false;
});
```

- [ ] **Step 2: テンプレートを修正**

既存の`{:else if fullImageData}`ブロック内（L115-121）の`<img>`要素を修正。`{#if loading}`/`{:else if fullImageData}`の条件構造はそのまま維持:
```svelte
{#if loading}
    <div class="loading">読み込み中...</div>
{:else if fullImageData}
    <img
        bind:this={imageElement}
        src="data:image/jpeg;base64,{fullImageData}"
        alt={image.name}
        class="preview-image"
        class:zoomed
        style="transform-origin: {transformOrigin}; {zoomed ? `transform: scale(${getZoomScale()});` : ''}"
        onclick={handleImageClick}
        onmousemove={handleImageMouseMove}
    />
{/if}
```

`<div class="image-container">`にzoomedクラスを追加:
```svelte
<div class="image-container" class:zoomed>
```

- [ ] **Step 3: ズーム用CSSを追加**

既存の`.image-container`と`.preview-image`を修正し、ズーム用CSSを追加:
```css
.image-container {
    max-width: calc(100vw - 120px);
    max-height: calc(100vh - 100px);
    display: flex;
    align-items: center;
    justify-content: center;
}

.image-container.zoomed {
    overflow: hidden;
}

.preview-image {
    max-width: 100%;
    max-height: calc(100vh - 100px);
    object-fit: contain;
    border-radius: 4px;
    cursor: zoom-in;
    transition: transform 0.15s ease-out;
}

.preview-image.zoomed {
    cursor: zoom-out;
    transition: none;
}
```

- [ ] **Step 4: ビルド確認**

Run: `cd /home/biwak/myShrimp/picture-tool-rust/gui-frontend && bun run check`
Expected: SUCCESS

- [ ] **Step 5: コミット**

```bash
git add gui-frontend/src/lib/ImagePreview.svelte
git commit -m "feat: LrCルーペ方式のズーム（クリックでFit⇔100%、マウス追従）"
```

---

### Task 7: お気に入りフォルダーのツリー展開

**Files:**
- Modify: `gui-frontend/src/lib/FolderTree.svelte`

- [ ] **Step 1: お気に入りをTreeNodeとしてレンダリング**

`FolderTree.svelte`の`<script>`セクションに`favoriteNodes`状態を追加（`let contextMenu`の後に）:
```typescript
let favoriteNodes = $state<TreeNode[]>([]);
```

`initStore()`でお気に入り読み込み時にTreeNode配列を構築するヘルパーを追加:
```typescript
function buildFavoriteNodes(paths: string[]): TreeNode[] {
    return paths.map((path) => ({
        entry: { name: getFolderName(path), path, is_dir: true, is_image: false },
        children: null,
        expanded: false,
        loading: false,
    }));
}
```

`initStore()`を修正:
```typescript
async function initStore() {
    store = await load("favorites.json", { autoSave: false });
    const saved = await store.get<string[]>("favorites");
    if (saved) {
        favorites = saved;
        favoriteNodes = buildFavoriteNodes(saved);
    }
}
```

`addFavorite`と`removeFavorite`もfavoriteNodesを更新:
```typescript
async function addFavorite(path: string) {
    if (!favorites.includes(path)) {
        favorites = [...favorites, path];
        favoriteNodes = buildFavoriteNodes(favorites);
        await saveFavorites();
    }
}

async function removeFavorite(path: string) {
    favorites = favorites.filter((f) => f !== path);
    favoriteNodes = buildFavoriteNodes(favorites);
    await saveFavorites();
}
```

`selectFavorite`関数を削除（`selectFolder`で代替）。

- [ ] **Step 2: お気に入りセクションのテンプレートを修正**

お気に入り表示部分（L141-157）を既存のtreeNodeスニペットで置換。`.favorites`にスクロール制御を追加:
```svelte
{#if favorites.length > 0}
    <div class="section-header">⭐ お気に入り</div>
    <div class="favorites">
        {#each favoriteNodes as node}
            {@render treeNode(node, 0)}
        {/each}
    </div>
{/if}
```

- [ ] **Step 3: お気に入りのスクロール制御CSS追加**

`.favorites`のCSSを修正して、展開されたサブフォルダーが多い場合にスクロール可能にする:
```css
.favorites {
    border-bottom: 1px solid var(--border-color);
    max-height: 40vh;
    overflow-y: auto;
}
```

- [ ] **Step 4: ビルド確認**

Run: `cd /home/biwak/myShrimp/picture-tool-rust/gui-frontend && bun run check`
Expected: SUCCESS

- [ ] **Step 5: コミット**

```bash
git add gui-frontend/src/lib/FolderTree.svelte
git commit -m "feat: お気に入りフォルダーのツリー展開（既存treeNodeスニペット再利用）"
```

---

### Task 8: スライダー視認性修正

**Files:**
- Modify: `gui-frontend/src/lib/SettingsPanel.svelte`
- Modify: `gui-frontend/src/lib/ThumbnailGrid.svelte`

- [ ] **Step 1: SettingsPanelのスライダートラック色を修正**

`SettingsPanel.svelte`のCSSで、`input[type="range"]::-webkit-slider-track`と`::-moz-range-track`の`background`を修正:

変更前: `background: var(--bg-primary);`
変更後: `background: #555;`

両方のベンダープレフィックス版で同様に修正。

- [ ] **Step 2: ThumbnailGridのスライダートラック色を修正**

`ThumbnailGrid.svelte`のCSSで、`.size-slider::-webkit-slider-track`と`::-moz-range-track`の`background`を修正:

変更前: `background: var(--border-color);`
変更後: `background: #555;`

- [ ] **Step 3: 列数スライダーのアイコンを修正**

`ThumbnailGrid.svelte`（L49）の絵文字を文字テキストに置換:

変更前: `<span class="size-label">🖼</span>`
変更後: `<span class="size-label">列</span>`

- [ ] **Step 4: ビルド確認**

Run: `cd /home/biwak/myShrimp/picture-tool-rust/gui-frontend && bun run check`
Expected: SUCCESS

- [ ] **Step 5: コミット**

```bash
git add gui-frontend/src/lib/SettingsPanel.svelte gui-frontend/src/lib/ThumbnailGrid.svelte
git commit -m "fix: スライダーのトラック色を#555に統一、列数アイコンの文字化け修正"
```

---

### Task 9: 統合ビルド確認

- [ ] **Step 1: 全体ビルド**

Run: `cd /home/biwak/myShrimp/picture-tool-rust && make build`
Expected: SUCCESS

- [ ] **Step 2: テスト**

Run: `cd /home/biwak/myShrimp/picture-tool-rust && make test`
Expected: ALL PASS
