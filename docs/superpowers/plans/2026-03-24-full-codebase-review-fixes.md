# Full Codebase Review Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** レビューで検出された全問題（CI失敗、バグ、セキュリティ、フロントエンド、テスト不足）を修正する

**Architecture:** 既存コードベースへの修正のみ。新規ファイルは `gui-frontend/src/vite-env.d.ts` の1つだけ。Core → GUI Backend → Frontend → Tests の順に修正し、各段階でテストを通す

**Tech Stack:** Rust (image 0.24, rayon, anyhow, tauri v2), Svelte 5 (runes), TypeScript, Vite

---

### Task 1: CI修復 — Clippy + rustfmt

**Files:**
- Modify: `core/src/lib.rs:3` (use exif; 削除)
- Modify: 全Rustファイル (cargo fmt)

- [ ] **Step 1: `use exif;` を削除**

`core/src/lib.rs:3` の `use exif;` を削除する。コード内の `exif::Reader`, `exif::Tag` 等は完全修飾パスなので影響なし。

- [ ] **Step 2: cargo fmt を実行**

Run: `cargo fmt`

- [ ] **Step 3: clippy が通ることを確認**

Run: `cargo clippy -- -D warnings`
Expected: warnings/errors なし（core の clippy エラーが消えた後、gui の `new_without_default` 等が出る可能性あり → Task 3 で対応）

- [ ] **Step 4: テストが通ることを確認**

Run: `cargo test`
Expected: 26 tests passed

- [ ] **Step 5: コミット**

```bash
git add core/src/lib.rs cli/src/main.rs gui/src/commands.rs gui/src/state.rs
git commit -m "fix: Clippy single_component_path_imports + rustfmt"
```

---

### Task 2: Core ライブラリバグ修正

**Files:**
- Modify: `core/src/lib.rs`

#### 2a: `validate_config` に `max_size_mb` チェック追加

- [ ] **Step 1: テストを先に書く**

```rust
#[test]
fn validate_config_rejects_zero_max_size() {
    let mut config = test_config();
    config.max_size_mb = 0;
    assert!(validate_config(&config).is_err());
}

#[test]
fn validate_config_accepts_valid_max_size() {
    let mut config = test_config();
    config.max_size_mb = 1;
    assert!(validate_config(&config).is_ok());
    config.max_size_mb = 50;
    assert!(validate_config(&config).is_ok());
}
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `cargo test validate_config_rejects_zero`
Expected: FAIL

- [ ] **Step 3: `validate_config` を修正**

```rust
pub fn validate_config(config: &ProcessingConfig) -> Result<()> {
    if config.quality == 0 || config.quality > 100 {
        anyhow::bail!("Quality must be between 1 and 100");
    }
    if config.max_size_mb == 0 {
        anyhow::bail!("max_size_mb must be at least 1");
    }
    Ok(())
}
```

- [ ] **Step 4: テスト通過を確認**

Run: `cargo test validate_config`
Expected: PASS

#### 2b: `save_with_size_limit` — tmpファイルクリーンアップ + `to_rgb8()` 最適化

- [ ] **Step 5: `save_with_size_limit` を修正**

tmpファイルのクリーンアップを確実にし、`to_rgb8()` をループ外に移動する。

```rust
fn save_with_size_limit(
    img: &DynamicImage,
    output_path: &Path,
    initial_quality: u8,
    max_size_bytes: usize,
) -> Result<(usize, u8)> {
    const MIN_QUALITY: u8 = 60;
    const QUALITY_STEP: u8 = 5;

    let rgb_img = img.to_rgb8();
    let mut quality = initial_quality;

    loop {
        let temp_path = output_path.with_extension("tmp.jpg");

        // save_jpeg_rgb が失敗してもtmpファイルをクリーンアップ
        let save_result = save_jpeg_rgb(&rgb_img, &temp_path, quality);
        if save_result.is_err() {
            let _ = fs::remove_file(&temp_path);
            return Err(save_result.unwrap_err());
        }

        let metadata = fs::metadata(&temp_path)
            .with_context(|| format!("Failed to get metadata: {}", temp_path.display()))?;
        let file_size = metadata.len() as usize;

        if file_size <= max_size_bytes || quality <= MIN_QUALITY {
            fs::rename(&temp_path, output_path)
                .with_context(|| format!("Failed to rename file: {}", output_path.display()))?;
            return Ok((file_size, quality));
        }

        fs::remove_file(&temp_path).ok();
        quality = quality.saturating_sub(QUALITY_STEP).max(MIN_QUALITY);
    }
}
```

- [ ] **Step 6: `save_jpeg` を `save_jpeg_rgb` にリファクタ**

`save_jpeg` を `save_jpeg_rgb` に変更し、`&image::RgbImage` を受け取るようにする。`save_jpeg` は `save_jpeg_rgb` を呼ぶラッパーにする（`generate_thumbnail_base64` 等の既存呼び出し元への影響なし）。

```rust
fn save_jpeg_rgb(rgb_img: &image::RgbImage, path: &Path, quality: u8) -> Result<()> {
    let file = File::create(path)
        .with_context(|| format!("Failed to create file: {}", path.display()))?;
    let mut writer = BufWriter::new(file);
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut writer, quality);
    encoder
        .encode(
            rgb_img.as_raw(),
            rgb_img.width(),
            rgb_img.height(),
            image::ColorType::Rgb8,
        )
        .with_context(|| format!("Failed to encode JPEG: {}", path.display()))?;
    Ok(())
}

fn save_jpeg(img: &DynamicImage, path: &Path, quality: u8) -> Result<()> {
    let rgb_img = img.to_rgb8();
    save_jpeg_rgb(&rgb_img, path, quality)
}
```

注: `save_jpeg` は現在 `save_with_size_limit` からのみ呼ばれるが、将来の互換性のために残す。直接呼び出しは `save_jpeg_rgb` に変更。

#### 2c: `collect_image_files` — `follow_links(false)` に変更

- [ ] **Step 7: `follow_links(true)` を `follow_links(false)` に変更**

```rust
for entry in WalkDir::new(dir)
    .follow_links(false)
    .into_iter()
    .filter_map(|e| e.ok())
```

#### 2d: `read_exif_info` — エラー種別の分岐

- [ ] **Step 8: ファイルオープンエラーを適切に処理**

```rust
pub fn read_exif_info(path: &Path) -> Result<ExifInfo> {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(ExifInfo::default()),
        Err(e) => return Err(e).with_context(|| format!("Failed to open for EXIF: {}", path.display())),
    };
    let mut bufreader = std::io::BufReader::new(file);
    let exif_data = match exif::Reader::new().read_from_container(&mut bufreader) {
        Ok(e) => e,
        Err(_) => return Ok(ExifInfo::default()), // EXIFデータなしは正常
    };
    // ... 以降は同じ
}
```

#### 2e: `generate_thumbnail_base64` — `max_dimension` 上限クランプ

- [ ] **Step 9: `max_dimension` にクランプを追加**

```rust
pub fn generate_thumbnail_base64(path: &Path, max_dimension: u32) -> Result<String> {
    use base64::Engine as _;

    let max_dimension = max_dimension.min(1024);
    // ... 以降は同じ
}
```

- [ ] **Step 10: テスト通過を確認**

Run: `cargo test`
Expected: 全テスト PASS

- [ ] **Step 11: コミット**

```bash
git add core/src/lib.rs
git commit -m "fix: core library bugs (validate max_size, tmp cleanup, follow_links, exif errors, thumbnail clamp)"
```

---

### Task 3: GUI バックエンド修正

**Files:**
- Modify: `gui/src/commands.rs`
- Modify: `gui/src/state.rs`
- Modify: `gui/tauri.conf.json`

#### 3a: `get_thumbnail` と `list_images` を `spawn_blocking` に移行

- [ ] **Step 1: `list_images` を `spawn_blocking` で非同期化**

```rust
#[tauri::command]
pub async fn list_images(path: String) -> Result<Vec<ImageEntry>, String> {
    tokio::task::spawn_blocking(move || {
        let dir = Path::new(&path);
        if !dir.is_dir() {
            return Err(format!("Not a directory: {}", path));
        }

        let read_dir = fs::read_dir(dir).map_err(|e| e.to_string())?;

        let direct_files: Vec<PathBuf> = read_dir
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file() && core::is_supported_image(p))
            .collect();

        let mut entries = Vec::new();
        for file_path in direct_files {
            let name = file_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let path_str = file_path.to_string_lossy().to_string();

            let (width, height) = match image::image_dimensions(&file_path) {
                Ok(dims) => dims,
                Err(_) => (0, 0),
            };

            let size_bytes = fs::metadata(&file_path)
                .map(|m| m.len())
                .unwrap_or(0);

            entries.push(ImageEntry {
                name,
                path: path_str,
                width,
                height,
                size_bytes,
                thumbnail_base64: None,
            });
        }

        entries.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        Ok(entries)
    })
    .await
    .map_err(|e| e.to_string())?
}
```

- [ ] **Step 2: `get_thumbnail` を `spawn_blocking` で非同期化**

```rust
#[tauri::command]
pub async fn get_thumbnail(
    state: tauri::State<'_, ProcessingState>,
    path: String,
    max_dimension: u32,
) -> Result<String, String> {
    let cache_key = format!("{}:{}", path, max_dimension);

    {
        let cache = state.thumbnail_cache.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(cached) = cache.peek(&cache_key) {
            return Ok(cached.clone());
        }
    }

    let path_clone = path.clone();
    let result = tokio::task::spawn_blocking(move || {
        core::generate_thumbnail_base64(Path::new(&path_clone), max_dimension)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    {
        let mut cache = state.thumbnail_cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.put(cache_key, result.clone());
    }

    Ok(result)
}
```

注: `cache.get` → `cache.peek` に変更（読み取り時にMRU更新を避けるため `lock()` を素早く解放）。`unwrap()` → `unwrap_or_else(|e| e.into_inner())` に変更。

- [ ] **Step 3: `get_full_image` と `get_exif_info` も `spawn_blocking` に移行**

```rust
#[tauri::command]
pub async fn get_full_image(
    path: String,
    max_width: u32,
    max_height: u32,
) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        core::generate_full_image_base64(Path::new(&path), max_width, max_height)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn get_exif_info(path: String) -> Result<core::ExifInfo, String> {
    tokio::task::spawn_blocking(move || {
        core::read_exif_info(Path::new(&path)).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
```

#### 3b: `process_images` — 失敗情報をフロントエンドに返す

- [ ] **Step 4: 失敗カウントを返す**

`process_images` の戻り値の後に失敗数を含む情報を返す。既存の `ProcessResult` の Vec を返しつつ、エラーのあったファイル数をログに記録。

```rust
    // 成功・失敗を分離
    let mut successes = Vec::new();
    let mut error_count = 0u32;
    for result in results {
        match result {
            Ok(r) => successes.push(r),
            Err(e) => {
                error_count += 1;
                eprintln!("Processing error: {}", e);
            }
        }
    }

    if error_count > 0 {
        let _ = app_handle.emit("processing-error", format!("{} files failed", error_count));
    }

    Ok(successes)
```

#### 3c: `ProcessingState` に `Default` 実装

- [ ] **Step 5: `state.rs` に `Default` を追加**

```rust
impl Default for ProcessingState {
    fn default() -> Self {
        Self::new()
    }
}
```

#### 3d: `tauri.conf.json` に CSP を追加

- [ ] **Step 6: CSP 設定を追加**

```json
{
  "app": {
    "security": {
      "csp": "default-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'"
    },
    "windows": [...]
  }
}
```

- [ ] **Step 7: ビルド確認**

Run: `cargo check -p picture-tool-gui`
Expected: PASS

- [ ] **Step 8: コミット**

```bash
git add gui/src/commands.rs gui/src/state.rs gui/tauri.conf.json
git commit -m "fix: GUI backend (spawn_blocking, mutex poison recovery, error propagation, CSP)"
```

---

### Task 4: フロントエンド修正

**Files:**
- Modify: `gui-frontend/src/App.svelte`
- Modify: `gui-frontend/src/lib/FolderTree.svelte`
- Modify: `gui-frontend/src/lib/ImagePreview.svelte`
- Modify: `gui-frontend/src/lib/ProgressOverlay.svelte`
- Modify: `gui-frontend/src/lib/ThumbnailGrid.svelte`
- Modify: `gui-frontend/src/lib/SelectionList.svelte`
- Create: `gui-frontend/src/vite-env.d.ts`

#### 4a: CSS副作用インポートの型エラー修正

- [ ] **Step 1: `vite-env.d.ts` を作成**

```typescript
/// <reference types="vite/client" />
```

#### 4b: `App.svelte` — リスナーリーク修正 + `unlisten` の `$state` 除去

- [ ] **Step 2: `$effect` のリスナー管理を修正**

```svelte
// --- イベントリスナー ---
let unlisten: UnlistenFn | null = null;

$effect(() => {
  let cancelled = false;
  listen<ProgressPayload>("processing-progress", (event) => {
    progress = event.payload;
  }).then((fn) => {
    if (cancelled) {
      fn();
    } else {
      unlisten = fn;
    }
  }).catch((e) => {
    console.error("Failed to listen for progress:", e);
  });

  return () => {
    cancelled = true;
    unlisten?.();
    unlisten = null;
  };
});
```

#### 4c: `App.svelte` — `thumbnailCache` のパフォーマンス改善

- [ ] **Step 3: `Map` の再代入を避ける**

`$state` のMapへの `.set()` 後の `new Map()` コピーを、バージョンカウンターによるリアクティビティトリガーに変更。

```svelte
let thumbnailCache = new Map<string, string>();
let cacheVersion = $state(0);
```

`handleRequestThumbnail` 内:
```svelte
  getThumbnail(path, maxDimension)
    .then((base64) => {
      thumbnailCache.set(path, base64);
      cacheVersion++;
    })
```

`ThumbnailGrid` と `SelectionList` に `cacheVersion` を prop として渡し、テンプレート内で `void cacheVersion` を使ってリアクティビティを確保する。

注: この変更は `ThumbnailGrid.svelte`, `SelectionList.svelte`, `App.svelte` の3ファイルにまたがる。Props interface に `cacheVersion: number` を追加し、テンプレート内で `thumbnailCache` を参照する箇所の近くに `{void cacheVersion, ''}` を置くか、`$derived` でキャッシュ参照を包む。

**代替案（よりシンプル）:** `$state.raw` を使い、変更の度に新しいMapを作る現行方式を維持するが、全コピーではなく影響を最小化する。検討の結果、現行の `new Map(thumbnailCache)` 方式はサムネイル数が数百程度なら実用上問題ないため、**この項目は低優先度とし、現行のままにする**。

#### 4d: `FolderTree.svelte` — 非同期エラーハンドリング

- [ ] **Step 4: `$effect` 内の非同期呼び出しにエラーハンドリング追加**

```svelte
$effect(() => {
  loadRoots().catch((e) => console.error("Failed to load roots:", e));
  initStore().catch((e) => console.error("Failed to init store:", e));
});
```

- [ ] **Step 5: `selectFolder` の `toggleNode` 呼び出しにエラーハンドリング追加**

```svelte
function selectFolder(node: TreeNode) {
  selectedPath = node.entry.path;
  onSelectFolder(node.entry.path);
  toggleNode(node).catch(() => {});  // toggleNode内でcatchしているが念のため
}
```

#### 4e: `ImagePreview.svelte` — ズームスケール修正 + アクセシビリティ

- [ ] **Step 6: `getZoomScale` を `naturalWidth` ベースに修正**

```typescript
function getZoomScale(): number {
  if (!imageElement || !image) return 1;
  const rendered = imageElement.getBoundingClientRect();
  if (rendered.width === 0) return 1;
  return imageElement.naturalWidth / rendered.width;
}
```

- [ ] **Step 7: dialog のアクセシビリティ属性を追加**

```svelte
<div
  class="preview-overlay"
  role="dialog"
  aria-modal="true"
  aria-label="画像プレビュー"
  onclick={handleOverlayClick}
>
```

#### 4f: `ProgressOverlay.svelte` — ゼロ除算防止

- [ ] **Step 8: `percentage` 計算を安全にする**

```svelte
let percentage = $derived(
  progress && progress.total > 0
    ? Math.round((progress.current / progress.total) * 100)
    : 0
);
```

#### 4g: `SelectionList.svelte` — アクセシビリティ

- [ ] **Step 9: `ondblclick` の `<div>` に `role` を追加**

```svelte
<div class="item" role="button" tabindex="0" ondblclick={() => onPreview(image)}
  onkeydown={(e) => { if (e.key === 'Enter') onPreview(image); }}>
```

- [ ] **Step 10: フロントエンドビルド確認**

Run: `cd gui-frontend && bun run build`
Expected: ビルド成功

- [ ] **Step 11: コミット**

```bash
git add gui-frontend/
git commit -m "fix: frontend (listener leak, async error handling, zoom calc, a11y, zero-division)"
```

---

### Task 5: テスト追加

**Files:**
- Modify: `core/src/lib.rs` (テストモジュール)

- [ ] **Step 1: サイズ制限テストを追加**

```rust
#[test]
fn save_with_size_limit_actually_reduces_quality() {
    let dir = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    let input = dir.path().join("large.jpg");
    // 大きめの画像を生成
    create_test_image(&input, 4000, 5000);

    let config = ProcessingConfig {
        mode: ConversionMode::Quality,
        max_size_mb: 1,
        quality: 95,
        ..test_config()
    };
    let result = process_image(&input, out.path(), &config).unwrap();

    // 1MB以下または品質がMIN_QUALITYまで下がっていること
    assert!(
        result.final_size_mb <= 1.0 || result.final_quality == Some(60),
        "サイズ制限が機能していない: size={:.2}MB, quality={:?}",
        result.final_size_mb, result.final_quality
    );
}
```

- [ ] **Step 2: 極小画像のテストを追加**

```rust
#[test]
fn crop_mode_handles_tiny_image() {
    let dir = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    let input = dir.path().join("tiny.jpg");
    create_test_image(&input, 2, 3);

    let config = ProcessingConfig {
        mode: ConversionMode::Crop,
        ..test_config()
    };
    // パニックせずに処理完了すること
    let result = process_image(&input, out.path(), &config);
    assert!(result.is_ok());
}

#[test]
fn pad_mode_handles_tiny_image() {
    let dir = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    let input = dir.path().join("tiny.jpg");
    create_test_image(&input, 2, 3);

    let config = ProcessingConfig {
        mode: ConversionMode::Pad,
        ..test_config()
    };
    let result = process_image(&input, out.path(), &config);
    assert!(result.is_ok());
}
```

- [ ] **Step 3: PNG入力のテストを追加**

```rust
#[test]
fn process_image_handles_png_input() {
    let dir = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    let input = dir.path().join("photo.png");
    // PNGとして保存（create_test_imageはImageBuffer::saveで拡張子から形式を判定）
    create_test_image(&input, 800, 1000);

    let result = process_image(&input, out.path(), &test_config()).unwrap();
    assert!(result.output_path.ends_with(".jpg"), "出力はJPEGであるべき");
    let output_img = image::open(&result.output_path);
    assert!(output_img.is_ok());
}
```

- [ ] **Step 4: テスト全通過を確認**

Run: `cargo test`
Expected: 全テスト PASS（元の26 + 新規6 = 32テスト）

- [ ] **Step 5: コミット**

```bash
git add core/src/lib.rs
git commit -m "test: add size limit, tiny image, PNG input, and max_size_mb validation tests"
```

---

### Task 6: 最終検証

- [ ] **Step 1: 全Rustチェック**

Run: `cargo clippy -- -D warnings && cargo fmt --check && cargo test`
Expected: 全 PASS

- [ ] **Step 2: フロントエンドビルド**

Run: `cd gui-frontend && bun run build`
Expected: ビルド成功

- [ ] **Step 3: GUI全体ビルド**

Run: `make build-gui`
Expected: ビルド成功
