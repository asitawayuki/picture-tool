# Picture Tool GUI化 + CLI拡張 実装計画

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** CLIの画像処理ロジックをcoreライブラリに抽出し、Tauri v2 + Svelte 5でGUIアプリを構築する

**Architecture:** Cargo Workspace（core/cli/gui）構成。coreが画像処理を担い、CLIとGUIがそれぞれ利用する。GUIはTauri v2のRustバックエンド + Svelte 5フロントエンド。

**Tech Stack:** Rust, image, rayon, walkdir, anyhow, serde, clap, Tauri v2, Svelte 5 (runes), Vite

**Spec:** `docs/superpowers/specs/2026-03-22-picture-tool-gui-design.md`

---

## ファイル構成

```
picture-tool-rust/
├── Cargo.toml                    # workspace root（修正）
├── core/                         # 画像処理ライブラリ
│   ├── Cargo.toml
│   └── src/lib.rs                # 公開API + 全ロジック
├── cli/                          # CLIバイナリ
│   ├── Cargo.toml
│   └── src/main.rs               # clap + core呼び出し
├── gui/                          # Tauriアプリ
│   ├── Cargo.toml
│   ├── build.rs
│   ├── tauri.conf.json
│   ├── capabilities/default.json
│   ├── icons/                    # アプリアイコン
│   └── src/
│       ├── main.rs               # Tauriセットアップ
│       ├── commands.rs           # Tauriコマンド
│       ├── state.rs              # ProcessingState
│       └── types.rs              # FileEntry, ImageEntry等
├── gui-frontend/                 # Svelte 5フロントエンド
│   ├── package.json
│   ├── vite.config.ts
│   ├── tsconfig.json
│   ├── index.html
│   └── src/
│       ├── main.ts
│       ├── app.css               # グローバルスタイル
│       ├── App.svelte            # メインレイアウト（3カラム）
│       ├── lib/
│       │   ├── types.ts          # TS型定義
│       │   ├── api.ts            # Tauriコマンド呼び出しラッパー
│       │   ├── FolderTree.svelte
│       │   ├── ThumbnailGrid.svelte
│       │   ├── SelectionList.svelte
│       │   ├── SettingsPanel.svelte
│       │   └── ProgressOverlay.svelte
│       └── assets/
├── src/main.rs                   # 削除（cli/に移動）
└── tests/                        # core統合テスト
    └── integration.rs
```

注: `gui-frontend/`はTauriの`gui/`と分離配置する。`tauri.conf.json`の`frontendDist`で参照する。

---

## Phase 1: Core ライブラリ抽出

### Task 1: Cargo Workspace + core クレート骨格

**Files:**
- Modify: `Cargo.toml` (workspace rootに変換)
- Create: `core/Cargo.toml`
- Create: `core/src/lib.rs`

- [ ] **Step 1: ルートCargo.tomlをworkspaceに変換**

`Cargo.toml`を以下に変更:

```toml
[workspace]
members = ["core", "cli"]
resolver = "2"
```

元の`[package]`と`[dependencies]`セクションは削除する（cliとcoreに分散）。

- [ ] **Step 2: core/Cargo.tomlを作成**

```toml
[package]
name = "picture-tool-core"
version = "0.1.0"
edition = "2021"

[dependencies]
image = "0.24"
rayon = "1.10"
walkdir = "2.5"
anyhow = "1.0"
serde = { version = "1.0", features = ["derive"] }
base64 = "0.22"
```

- [ ] **Step 3: core/src/lib.rsの骨格を作成**

最小限の公開APIスタブを定義する:

```rust
use serde::{Serialize, Deserialize};
use std::path::{Path, PathBuf};
use anyhow::Result;

// --- 型定義 ---

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ConversionMode {
    Crop,
    Pad,
    Quality,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum BackgroundColor {
    White,
    Black,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingConfig {
    pub mode: ConversionMode,
    pub bg_color: BackgroundColor,
    pub quality: u8,
    pub max_size_mb: usize,
    pub delete_originals: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessResult {
    pub input_path: String,
    pub output_path: String,
    pub final_size_mb: f64,
    pub final_quality: Option<u8>,
}

/// 進捗コールバック: (current, total) -> bool（falseでキャンセル）
pub type ProgressCallback = Box<dyn Fn(usize, usize) -> bool + Send + Sync>;

// --- 公開API（スタブ） ---

pub fn validate_config(config: &ProcessingConfig) -> Result<()> {
    todo!()
}

pub fn collect_image_files(dir: &Path) -> Result<Vec<PathBuf>> {
    todo!()
}

pub fn process_image(
    input_path: &Path,
    output_folder: &Path,
    config: &ProcessingConfig,
) -> Result<ProcessResult> {
    todo!()
}

pub fn process_batch(
    files: &[PathBuf],
    output_folder: &Path,
    config: &ProcessingConfig,
    on_progress: Option<ProgressCallback>,
) -> Vec<Result<ProcessResult>> {
    todo!()
}

pub fn generate_thumbnail_base64(
    path: &Path,
    max_dimension: u32,
) -> Result<String> {
    todo!()
}
```

- [ ] **Step 4: ビルド確認**

Run: `cargo build -p picture-tool-core`
Expected: コンパイル成功（todo!()はビルドは通る）

- [ ] **Step 5: コミット**

注: `src/main.rs`はまだ残す。Task 3でcliに移動・削除する。

```bash
git add Cargo.toml core/
git commit -m "feat: Cargo Workspace構成とcoreクレート骨格を追加"
```

---

### Task 2: core に画像処理ロジックを実装

**Files:**
- Modify: `core/src/lib.rs`

現行`src/main.rs`から画像処理関数を移植する。`Args`構造体への依存を`ProcessingConfig`に置き換える。

- [ ] **Step 1: テストを先に書く**

`core/src/lib.rs`の末尾に:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn test_config() -> ProcessingConfig {
        ProcessingConfig {
            mode: ConversionMode::Crop,
            bg_color: BackgroundColor::White,
            quality: 90,
            max_size_mb: 8,
            delete_originals: false,
        }
    }

    #[test]
    fn test_validate_config_valid() {
        let config = test_config();
        assert!(validate_config(&config).is_ok());
    }

    #[test]
    fn test_validate_config_quality_zero() {
        let mut config = test_config();
        config.quality = 0;
        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn test_validate_config_quality_over_100() {
        let mut config = test_config();
        config.quality = 101;
        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn test_is_supported_image() {
        assert!(is_supported_image(Path::new("photo.jpg")));
        assert!(is_supported_image(Path::new("photo.JPEG")));
        assert!(is_supported_image(Path::new("photo.png")));
        assert!(is_supported_image(Path::new("photo.webp")));
        assert!(!is_supported_image(Path::new("doc.pdf")));
        assert!(!is_supported_image(Path::new("noext")));
    }

    #[test]
    fn test_collect_image_files_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let files = collect_image_files(dir.path()).unwrap();
        assert!(files.is_empty());
    }

    #[test]
    fn test_collect_image_files_with_images() {
        let dir = tempfile::tempdir().unwrap();
        // ダミー画像ファイルを作成（拡張子のみでOK、collectはファイル存在と拡張子のみチェック）
        fs::write(dir.path().join("a.jpg"), b"fake").unwrap();
        fs::write(dir.path().join("b.png"), b"fake").unwrap();
        fs::write(dir.path().join("c.txt"), b"fake").unwrap();
        let files = collect_image_files(dir.path()).unwrap();
        assert_eq!(files.len(), 2);
    }
}
```

`core/Cargo.toml`にdev-dependencyを追加:

```toml
[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 2: テスト実行（失敗を確認）**

Run: `cargo test -p picture-tool-core`
Expected: FAIL（todo!()でpanic）

- [ ] **Step 3: validate_config, is_supported_image, collect_image_filesを実装**

`core/src/lib.rs`のスタブを以下で置き換え:

```rust
use anyhow::{Context, Result};
use image::{DynamicImage, GenericImageView, RgbaImage};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use walkdir::WalkDir;

// （型定義はStep 3で作成したものを維持）

pub fn validate_config(config: &ProcessingConfig) -> Result<()> {
    if config.quality == 0 || config.quality > 100 {
        anyhow::bail!("Quality must be between 1 and 100");
    }
    Ok(())
}

pub fn is_supported_image(path: &Path) -> bool {
    if let Some(ext) = path.extension() {
        let ext = ext.to_string_lossy().to_lowercase();
        matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "webp")
    } else {
        false
    }
}

pub fn collect_image_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in WalkDir::new(dir)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.is_file() && is_supported_image(path) {
            files.push(path.to_path_buf());
        }
    }
    Ok(files)
}
```

- [ ] **Step 4: テスト実行（validate/collect/is_supportedが通ることを確認）**

Run: `cargo test -p picture-tool-core -- test_validate test_is_supported test_collect`
Expected: 5テストPASS

- [ ] **Step 5: 画像変換・保存関数を実装**

`core/src/lib.rs`に以下を追加（現行main.rsからの移植）:

```rust
impl BackgroundColor {
    pub fn to_rgba(&self) -> image::Rgba<u8> {
        match self {
            BackgroundColor::White => image::Rgba([255, 255, 255, 255]),
            BackgroundColor::Black => image::Rgba([0, 0, 0, 255]),
        }
    }
}

fn convert_aspect_ratio_crop(img: DynamicImage) -> DynamicImage {
    let (width, height) = img.dimensions();
    let target_ratio = 4.0 / 5.0;
    let current_ratio = width as f64 / height as f64;

    if (current_ratio - target_ratio).abs() < 0.001 {
        return img;
    }

    let (crop_width, crop_height) = if current_ratio > target_ratio {
        let new_width = (height as f64 * target_ratio).round() as u32;
        (new_width, height)
    } else {
        let new_height = (width as f64 / target_ratio).round() as u32;
        (width, new_height)
    };

    let x = (width.saturating_sub(crop_width)) / 2;
    let y = (height.saturating_sub(crop_height)) / 2;

    img.crop_imm(x, y, crop_width, crop_height)
}

fn convert_aspect_ratio_pad(img: DynamicImage, bg_color: BackgroundColor) -> DynamicImage {
    let (width, height) = img.dimensions();
    let target_ratio = 4.0 / 5.0;
    let current_ratio = width as f64 / height as f64;

    if (current_ratio - target_ratio).abs() < 0.001 {
        return img;
    }

    let (new_width, new_height) = if current_ratio > target_ratio {
        let new_height = (width as f64 / target_ratio).round() as u32;
        (width, new_height)
    } else {
        let new_width = (height as f64 * target_ratio).round() as u32;
        (new_width, height)
    };

    let mut canvas = RgbaImage::from_pixel(new_width, new_height, bg_color.to_rgba());
    let x = (new_width.saturating_sub(width)) / 2;
    let y = (new_height.saturating_sub(height)) / 2;
    image::imageops::overlay(&mut canvas, &img.to_rgba8(), x.into(), y.into());

    DynamicImage::ImageRgba8(canvas)
}

fn generate_output_path(input_path: &Path, output_folder: &Path) -> Result<PathBuf> {
    let stem = input_path
        .file_stem()
        .context("Failed to get file stem")?
        .to_string_lossy();

    let output_filename = format!("{}_processed.jpg", stem);
    let mut output_path = output_folder.join(&output_filename);

    let mut counter = 1;
    while output_path.exists() {
        let numbered_filename = format!("{}_processed_{}.jpg", stem, counter);
        output_path = output_folder.join(numbered_filename);
        counter += 1;
    }

    Ok(output_path)
}

fn save_with_size_limit(
    img: &DynamicImage,
    output_path: &Path,
    initial_quality: u8,
    max_size_bytes: usize,
) -> Result<(usize, u8)> {
    const MIN_QUALITY: u8 = 60;
    const QUALITY_STEP: u8 = 5;

    let mut quality = initial_quality;

    loop {
        let temp_path = output_path.with_extension("tmp.jpg");
        save_jpeg(img, &temp_path, quality)?;

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

fn save_jpeg(img: &DynamicImage, path: &Path, quality: u8) -> Result<()> {
    let file =
        File::create(path).with_context(|| format!("Failed to create file: {}", path.display()))?;
    let mut writer = BufWriter::new(file);
    let rgb_img = img.to_rgb8();
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
```

- [ ] **Step 6: process_imageを実装**

```rust
pub fn process_image(
    input_path: &Path,
    output_folder: &Path,
    config: &ProcessingConfig,
) -> Result<ProcessResult> {
    let img = image::open(input_path)
        .with_context(|| format!("Failed to open image: {}", input_path.display()))?;

    let converted = match config.mode {
        ConversionMode::Crop => convert_aspect_ratio_crop(img),
        ConversionMode::Pad => convert_aspect_ratio_pad(img, config.bg_color),
        ConversionMode::Quality => img,
    };

    let output_path = generate_output_path(input_path, output_folder)?;
    let max_size_bytes = config.max_size_mb * 1024 * 1024;

    let (final_size, final_quality) =
        save_with_size_limit(&converted, &output_path, config.quality, max_size_bytes)?;

    // 成功時のみ元ファイルを削除
    if config.delete_originals {
        if let Err(e) = fs::remove_file(input_path) {
            eprintln!(
                "Warning: Failed to delete original file {}: {}",
                input_path.display(),
                e
            );
        }
    }

    let final_size_mb = final_size as f64 / (1024.0 * 1024.0);

    Ok(ProcessResult {
        input_path: input_path.to_string_lossy().to_string(),
        output_path: output_path.to_string_lossy().to_string(),
        final_size_mb,
        final_quality: if final_quality < config.quality {
            Some(final_quality)
        } else {
            None
        },
    })
}
```

- [ ] **Step 7: process_batchを実装**

注: rayonの並列実行のため、キャンセルフラグが反映されるまでに既に実行中のタスクは完了する。
これは許容範囲の動作であり、キャンセル後の処理済みファイルは保持される。

```rust
pub fn process_batch(
    files: &[PathBuf],
    output_folder: &Path,
    config: &ProcessingConfig,
    on_progress: Option<ProgressCallback>,
) -> Vec<Result<ProcessResult>> {
    let total = files.len();
    let cancelled = Arc::new(AtomicBool::new(false));
    let processed_count = AtomicUsize::new(0);

    files
        .par_iter()
        .map(|path| {
            if cancelled.load(Ordering::Relaxed) {
                return Err(anyhow::anyhow!("Processing cancelled"));
            }

            let result = process_image(path, output_folder, config);

            let current = processed_count.fetch_add(1, Ordering::SeqCst) + 1;
            if let Some(ref cb) = on_progress {
                if !cb(current, total) {
                    cancelled.store(true, Ordering::Relaxed);
                }
            }

            result
        })
        .collect()
}
```

- [ ] **Step 8: generate_thumbnail_base64を実装**

```rust
use base64::Engine as _;

pub fn generate_thumbnail_base64(
    path: &Path,
    max_dimension: u32,
) -> Result<String> {
    let img = image::open(path)
        .with_context(|| format!("Failed to open image for thumbnail: {}", path.display()))?;

    let thumbnail = img.thumbnail(max_dimension, max_dimension);
    let rgb = thumbnail.to_rgb8();

    let mut jpeg_bytes = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut jpeg_bytes);
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut cursor, 75);
    encoder.encode(
        rgb.as_raw(),
        rgb.width(),
        rgb.height(),
        image::ColorType::Rgb8,
    )?;

    Ok(base64::engine::general_purpose::STANDARD.encode(&jpeg_bytes))
}
```

- [ ] **Step 9: 全テスト実行**

Run: `cargo test -p picture-tool-core`
Expected: 全テストPASS

- [ ] **Step 10: コミット**

```bash
git add core/
git commit -m "feat: coreライブラリに画像処理ロジックを実装"
```

---

### Task 3: CLI を core 利用にリファクタリング

**Files:**
- Create: `cli/Cargo.toml`
- Create: `cli/src/main.rs`
- Modify: `Cargo.toml` (workspace membersにcli追加)
- Delete: `src/main.rs`

- [ ] **Step 1: ルートCargo.tomlにcliを追加**

`Cargo.toml`:
```toml
[workspace]
members = ["core", "cli"]
resolver = "2"
```

- [ ] **Step 2: cli/Cargo.tomlを作成**

```toml
[package]
name = "picture-tool"
version = "0.1.0"
edition = "2021"

[dependencies]
picture-tool-core = { path = "../core" }
clap = { version = "4.5", features = ["derive"] }
anyhow = "1.0"
```

- [ ] **Step 3: cli/src/main.rsを作成**

```rust
use anyhow::{Context, Result};
use clap::Parser;
use picture_tool_core::{self as core, BackgroundColor, ConversionMode, ProcessingConfig};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

#[derive(Parser, Debug)]
#[command(author, version, about = "画像バッチ処理ツール - 4:5のアスペクト比に変換し、サイズ制限付きで保存")]
struct Args {
    /// 入力フォルダーパス
    #[arg(short, long)]
    input: PathBuf,

    /// 変換モード (crop, pad, quality)
    #[arg(short, long, default_value = "crop")]
    mode: CliConversionMode,

    /// パディング時の背景色 (white, black)
    #[arg(short, long, default_value = "white")]
    bg_color: CliBgColor,

    /// 初期JPEG品質 (1-100)
    #[arg(short, long, default_value = "90")]
    quality: u8,

    /// 最大ファイルサイズ (MB)
    #[arg(long, default_value = "8")]
    max_size: usize,

    /// 出力先フォルダー
    #[arg(short, long, default_value = "./")]
    output: PathBuf,

    /// 変換完了後に元ファイルを削除
    #[arg(long, default_value = "false")]
    delete_originals: bool,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum CliConversionMode { Crop, Pad, Quality }

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum CliBgColor { White, Black }

impl From<CliConversionMode> for ConversionMode {
    fn from(m: CliConversionMode) -> Self {
        match m {
            CliConversionMode::Crop => ConversionMode::Crop,
            CliConversionMode::Pad => ConversionMode::Pad,
            CliConversionMode::Quality => ConversionMode::Quality,
        }
    }
}

impl From<CliBgColor> for BackgroundColor {
    fn from(c: CliBgColor) -> Self {
        match c {
            CliBgColor::White => BackgroundColor::White,
            CliBgColor::Black => BackgroundColor::Black,
        }
    }
}

fn main() -> Result<()> {
    let args = Args::parse();

    let config = ProcessingConfig {
        mode: args.mode.into(),
        bg_color: args.bg_color.into(),
        quality: args.quality,
        max_size_mb: args.max_size,
        delete_originals: args.delete_originals,
    };

    core::validate_config(&config)?;

    // 入力フォルダーの検証
    if !args.input.exists() {
        anyhow::bail!("Input folder does not exist: {}", args.input.display());
    }
    if !args.input.is_dir() {
        anyhow::bail!("Input path is not a directory: {}", args.input.display());
    }

    // 出力フォルダーの検証と作成
    if !args.output.exists() {
        fs::create_dir_all(&args.output)
            .with_context(|| format!("Failed to create output folder: {}", args.output.display()))?;
        println!("Created output folder: {}", args.output.display());
    } else if !args.output.is_dir() {
        anyhow::bail!("Output path is not a directory: {}", args.output.display());
    }

    println!("Processing images in: {}", args.input.display());
    println!("Output folder: {}", args.output.display());

    let image_files = core::collect_image_files(&args.input)?;
    let total_count = image_files.len();

    if total_count == 0 {
        println!("No image files found.");
        return Ok(());
    }

    println!("Found {} images\n", total_count);

    let start = Instant::now();
    let success_count = AtomicUsize::new(0);
    let failed_count = AtomicUsize::new(0);

    let on_progress = |_current: usize, _total: usize| -> bool {
        true // CLI版はキャンセルなし
    };

    let results = core::process_batch(
        &image_files,
        &args.output,
        &config,
        Some(Box::new(on_progress)),
    );

    for (i, result) in results.iter().enumerate() {
        let path = &image_files[i];
        match result {
            Ok(r) => {
                success_count.fetch_add(1, Ordering::SeqCst);
                let quality_info = r.final_quality.map_or(String::new(), |q| format!(", quality: {}%", q));
                println!(
                    "[{}/{}] {} → {} ({:.1} MB{}) ✓",
                    i + 1, total_count,
                    path.file_name().unwrap().to_string_lossy(),
                    PathBuf::from(&r.output_path).file_name().unwrap().to_string_lossy(),
                    r.final_size_mb, quality_info
                );
            }
            Err(e) => {
                failed_count.fetch_add(1, Ordering::SeqCst);
                eprintln!(
                    "[{}/{}] {} ✗ Error: {}",
                    i + 1, total_count,
                    path.file_name().unwrap().to_string_lossy(), e
                );
            }
        }
    }

    let duration = start.elapsed();
    let success = success_count.load(Ordering::SeqCst);
    let failed = failed_count.load(Ordering::SeqCst);

    println!("\nCompleted: {} successful, {} failed", success, failed);
    println!("Total time: {:.1}s", duration.as_secs_f64());

    Ok(())
}
```

- [ ] **Step 4: 旧src/main.rsを削除**

- [ ] **Step 5: ビルドと動作確認**

Run: `cargo build -p picture-tool`
Expected: ビルド成功

テスト画像がある場合の動作確認:
```bash
cargo run -p picture-tool -- --input ./test-images --output ./test-output --mode crop
```

- [ ] **Step 6: 旧src/を削除してコミット**

```bash
git rm -r src/
git add Cargo.toml cli/
git commit -m "refactor: CLIをcoreライブラリ利用に移行し、--delete-originalsフラグを追加"
```

---

## Phase 2: Tauri GUI バックエンド

### Task 4: Tauri v2 プロジェクト初期化

**Files:**
- Modify: `Cargo.toml` (workspace membersにgui追加)
- Create: `gui/Cargo.toml`
- Create: `gui/src/main.rs`
- Create: `gui/src/types.rs`
- Create: `gui/src/state.rs`
- Create: `gui/src/commands.rs`
- Create: `gui/build.rs`
- Create: `gui/tauri.conf.json`
- Create: `gui/capabilities/default.json`
- Create: `gui-frontend/package.json`
- Create: `gui-frontend/vite.config.ts`
- Create: `gui-frontend/tsconfig.json`
- Create: `gui-frontend/index.html`
- Create: `gui-frontend/src/main.ts`
- Create: `gui-frontend/src/App.svelte`

- [ ] **Step 1: workspace membersにguiを追加**

`Cargo.toml`:
```toml
[workspace]
members = ["core", "cli", "gui"]
resolver = "2"
```

- [ ] **Step 2: gui/Cargo.tomlを作成**

```toml
[package]
name = "picture-tool-gui"
version = "0.1.0"
edition = "2021"

[dependencies]
picture-tool-core = { path = "../core" }
tauri = { version = "2", features = [] }
tauri-plugin-dialog = "2"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
anyhow = "1.0"
tokio = { version = "1", features = ["rt"] }

[build-dependencies]
tauri-build = { version = "2", features = [] }
```

- [ ] **Step 3: gui/build.rsを作成**

```rust
fn main() {
    tauri_build::build();
}
```

- [ ] **Step 4: gui/tauri.conf.jsonを作成**

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "Picture Tool",
  "version": "0.1.0",
  "identifier": "com.picture-tool.app",
  "build": {
    "frontendDist": "../gui-frontend/dist",
    "devUrl": "http://localhost:5173",
    "beforeDevCommand": "cd ../gui-frontend && npm run dev",
    "beforeBuildCommand": "cd ../gui-frontend && npm run build"
  },
  "app": {
    "title": "Picture Tool",
    "windows": [
      {
        "title": "Picture Tool",
        "width": 1200,
        "height": 800,
        "minWidth": 900,
        "minHeight": 600
      }
    ]
  },
  "plugins": {
    "dialog": {}
  }
}
```

- [ ] **Step 5: gui/capabilities/default.jsonを作成**

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "identifier": "default",
  "description": "Default capability",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "dialog:default",
    "dialog:allow-open"
  ]
}
```

- [ ] **Step 6: gui/src/types.rsを作成**

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub is_image: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageEntry {
    pub name: String,
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub size_bytes: u64,
    pub thumbnail_base64: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressPayload {
    pub current: usize,
    pub total: usize,
    pub file_name: String,
}
```

- [ ] **Step 7: gui/src/state.rsを作成**

```rust
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

pub struct ProcessingState {
    pub cancel_flag: Arc<AtomicBool>,
}

impl ProcessingState {
    pub fn new() -> Self {
        Self {
            cancel_flag: Arc::new(AtomicBool::new(false)),
        }
    }
}
```

- [ ] **Step 8: gui/src/commands.rsの骨格を作成**

```rust
use crate::state::ProcessingState;
use crate::types::*;
use picture_tool_core as core;

#[tauri::command]
pub fn list_directory(path: String) -> Result<Vec<FileEntry>, String> {
    Ok(vec![]) // Phase 2 Task 5で実装
}

#[tauri::command]
pub fn list_drives() -> Result<Vec<String>, String> {
    Ok(vec![]) // Phase 2 Task 5で実装
}

#[tauri::command]
pub async fn list_images(path: String) -> Result<Vec<ImageEntry>, String> {
    Ok(vec![]) // Phase 2 Task 5で実装
}

#[tauri::command]
pub async fn get_thumbnail(path: String) -> Result<String, String> {
    Err("Not implemented".to_string()) // Phase 2 Task 5で実装
}

#[tauri::command]
pub async fn process_images(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, ProcessingState>,
    files: Vec<String>,
    output_folder: String,
    config: core::ProcessingConfig,
) -> Result<Vec<core::ProcessResult>, String> {
    Err("Not implemented".to_string()) // Phase 2 Task 6で実装
}

#[tauri::command]
pub fn cancel_processing(state: tauri::State<'_, ProcessingState>) -> Result<(), String> {
    Ok(()) // Phase 2 Task 6で実装
}

// 注: pick_folderはTauriコマンドとしては実装しない。
// フロントエンドから@tauri-apps/plugin-dialogのopen()を直接呼び出す。
```

- [ ] **Step 9: gui/src/main.rsを作成**

```rust
mod commands;
mod state;
mod types;

use state::ProcessingState;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(ProcessingState::new())
        .invoke_handler(tauri::generate_handler![
            commands::list_directory,
            commands::list_drives,
            commands::list_images,
            commands::get_thumbnail,
            commands::process_images,
            commands::cancel_processing,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Step 10: Svelteフロントエンドの最小構成を作成**

`gui-frontend/package.json`:
```json
{
  "name": "picture-tool-frontend",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "dev": "vite dev",
    "build": "vite build",
    "preview": "vite preview"
  },
  "dependencies": {
    "@tauri-apps/api": "^2",
    "@tauri-apps/plugin-dialog": "^2"
  },
  "devDependencies": {
    "@sveltejs/vite-plugin-svelte": "^4",
    "svelte": "^5",
    "typescript": "^5.7",
    "vite": "^6"
  }
}
```

`gui-frontend/vite.config.ts`:
```typescript
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
  },
});
```

`gui-frontend/tsconfig.json`:
```json
{
  "compilerOptions": {
    "target": "ES2021",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "strict": true,
    "resolveJsonModule": true,
    "isolatedModules": true,
    "esModuleInterop": true,
    "skipLibCheck": true
  },
  "include": ["src/**/*.ts", "src/**/*.svelte"]
}
```

`gui-frontend/index.html`:
```html
<!doctype html>
<html lang="ja">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Picture Tool</title>
  </head>
  <body>
    <div id="app"></div>
    <script type="module" src="/src/main.ts"></script>
  </body>
</html>
```

`gui-frontend/src/app.css`（空ファイル、Task 7で内容を追加）:
```css
/* Global styles - see Task 7 */
```

`gui-frontend/src/main.ts`:
```typescript
import "./app.css";
import App from "./App.svelte";
import { mount } from "svelte";

const app = mount(App, { target: document.getElementById("app")! });

export default app;
```

`gui-frontend/src/App.svelte`:
```svelte
<main>
  <h1>Picture Tool</h1>
  <p>Loading...</p>
</main>
```

- [ ] **Step 11: npm installとビルド確認**

```bash
cd gui-frontend && npm install && npm run build && cd ..
cargo build -p picture-tool-gui
```

Expected: ビルド成功

- [ ] **Step 12: コミット**

```bash
git add Cargo.toml gui/ gui-frontend/
git commit -m "feat: Tauri v2 + Svelte 5のGUIプロジェクト骨格を追加"
```

---

### Task 5: ファイルシステム系Tauriコマンドを実装

**Files:**
- Modify: `gui/src/commands.rs`

- [ ] **Step 1: list_directory, list_drives, list_images, get_thumbnailを実装**

`gui/src/commands.rs`のスタブを以下で置き換え:

```rust
use crate::state::ProcessingState;
use crate::types::*;
use picture_tool_core as core;
use std::fs;
use std::path::{Path, PathBuf};

#[tauri::command]
pub fn list_directory(path: String) -> Result<Vec<FileEntry>, String> {
    let dir = Path::new(&path);
    if !dir.is_dir() {
        return Err(format!("Not a directory: {}", path));
    }

    let mut entries = Vec::new();
    let read_dir = fs::read_dir(dir).map_err(|e| e.to_string())?;

    for entry in read_dir.flatten() {
        let file_type = entry.file_type().map_err(|e| e.to_string())?;
        let name = entry.file_name().to_string_lossy().to_string();
        let entry_path = entry.path();
        let path_str = entry_path.to_string_lossy().to_string();

        // 隠しファイル/フォルダーをスキップ
        if name.starts_with('.') {
            continue;
        }

        let is_image = if file_type.is_file() {
            core::is_supported_image(&entry_path)
        } else {
            false
        };

        entries.push(FileEntry {
            name,
            path: path_str,
            is_dir: file_type.is_dir(),
            is_image,
        });
    }

    // フォルダーを先に、次にファイルをアルファベット順でソート
    entries.sort_by(|a, b| {
        b.is_dir.cmp(&a.is_dir).then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    Ok(entries)
}

#[tauri::command]
pub fn list_drives() -> Result<Vec<String>, String> {
    #[cfg(target_os = "windows")]
    {
        let mut drives = Vec::new();
        // A-Zのドライブレターをチェック
        for letter in b'A'..=b'Z' {
            let drive = format!("{}:\\", letter as char);
            if Path::new(&drive).exists() {
                drives.push(drive);
            }
        }
        Ok(drives)
    }

    #[cfg(not(target_os = "windows"))]
    {
        Ok(vec!["/".to_string()])
    }
}

#[tauri::command]
pub async fn list_images(path: String) -> Result<Vec<ImageEntry>, String> {
    let dir = Path::new(&path);
    if !dir.is_dir() {
        return Err(format!("Not a directory: {}", path));
    }

    // 直下の画像のみ取得（再帰しない、パフォーマンスのためread_dirを直接使用）
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
            thumbnail_base64: None, // 遅延読み込み
        });
    }

    entries.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    Ok(entries)
}

#[tauri::command]
pub async fn get_thumbnail(path: String) -> Result<String, String> {
    core::generate_thumbnail_base64(Path::new(&path), 200)
        .map_err(|e| e.to_string())
}

// process_images, cancel_processing は次のTaskで実装
```

- [ ] **Step 2: core/src/lib.rsでis_supported_imageをpub化**

`core/src/lib.rs`で`is_supported_image`が`pub`であることを確認。なければ変更:
```rust
pub fn is_supported_image(path: &Path) -> bool {
```

- [ ] **Step 3: ビルド確認**

Run: `cargo build -p picture-tool-gui`
Expected: ビルド成功

- [ ] **Step 4: コミット**

```bash
git add gui/src/commands.rs core/src/lib.rs
git commit -m "feat: ファイルシステム系Tauriコマンドを実装"
```

---

### Task 6: 画像処理Tauriコマンド + キャンセル機構を実装

**Files:**
- Modify: `gui/src/commands.rs`

- [ ] **Step 1: process_imagesとcancel_processingを実装**

`gui/src/commands.rs`に追加（既存のスタブを置き換え）:

```rust
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tauri::Emitter;

#[tauri::command]
pub async fn process_images(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, ProcessingState>,
    files: Vec<String>,
    output_folder: String,
    config: core::ProcessingConfig,
) -> Result<Vec<core::ProcessResult>, String> {
    core::validate_config(&config).map_err(|e| e.to_string())?;

    // 出力フォルダーを作成
    let output = output_folder.clone();
    let output_path = Path::new(&output);
    if !output_path.exists() {
        fs::create_dir_all(output_path).map_err(|e| e.to_string())?;
    }

    // キャンセルフラグをリセット
    state.cancel_flag.store(false, Ordering::Relaxed);

    let file_paths: Vec<PathBuf> = files.iter().map(PathBuf::from).collect();
    let cancel_flag = Arc::clone(&state.cancel_flag);
    let files_clone = files.clone();
    let app_handle_clone = app_handle.clone();

    // process_batchはrayonで並列処理するためブロッキング。
    // Tauriのasync runtimeをブロックしないようspawn_blockingでオフロード。
    let results = tokio::task::spawn_blocking(move || {
        let on_progress: core::ProgressCallback = Box::new(move |current, total| -> bool {
            let file_name = files_clone
                .get(current.saturating_sub(1))
                .cloned()
                .unwrap_or_default();

            let file_name_short = Path::new(&file_name)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            let _ = app_handle_clone.emit(
                "processing-progress",
                ProgressPayload {
                    current,
                    total,
                    file_name: file_name_short,
                },
            );

            !cancel_flag.load(Ordering::Relaxed)
        });

        let output_path = PathBuf::from(&output);
        core::process_batch(&file_paths, &output_path, &config, Some(on_progress))
    })
    .await
    .map_err(|e| e.to_string())?;

    let successes: Vec<core::ProcessResult> = results
        .into_iter()
        .filter_map(|r| r.ok())
        .collect();

    Ok(successes)
}

#[tauri::command]
pub fn cancel_processing(state: tauri::State<'_, ProcessingState>) -> Result<(), String> {
    state.cancel_flag.store(true, Ordering::Relaxed);
    Ok(())
}
```

- [ ] **Step 2: ビルド確認**

Run: `cargo build -p picture-tool-gui`
Expected: ビルド成功

- [ ] **Step 3: コミット**

```bash
git add gui/src/commands.rs
git commit -m "feat: 画像処理Tauriコマンドとキャンセル機構を実装"
```

---

## Phase 3: Svelte 5 フロントエンド

### Task 7: 型定義・APIラッパー・グローバルスタイル

**Files:**
- Create: `gui-frontend/src/lib/types.ts`
- Create: `gui-frontend/src/lib/api.ts`
- Create: `gui-frontend/src/app.css`

- [ ] **Step 1: TypeScript型定義を作成**

`gui-frontend/src/lib/types.ts`:
```typescript
export interface FileEntry {
  name: string;
  path: string;
  is_dir: boolean;
  is_image: boolean;
}

export interface ImageEntry {
  name: string;
  path: string;
  width: number;
  height: number;
  size_bytes: number;
  thumbnail_base64: string | null;
}

export interface ProcessingConfig {
  mode: "crop" | "pad" | "quality";
  bg_color: "white" | "black";
  quality: number;
  max_size_mb: number;
  delete_originals: boolean;
}

export interface ProcessResult {
  input_path: string;
  output_path: string;
  final_size_mb: number;
  final_quality: number | null;
}

export interface ProgressPayload {
  current: number;
  total: number;
  file_name: string;
}
```

- [ ] **Step 2: Tauri APIラッパーを作成**

`gui-frontend/src/lib/api.ts`:
```typescript
import { invoke } from "@tauri-apps/api/core";
import type {
  FileEntry,
  ImageEntry,
  ProcessingConfig,
  ProcessResult,
} from "./types";

export async function listDirectory(path: string): Promise<FileEntry[]> {
  return invoke("list_directory", { path });
}

export async function listDrives(): Promise<string[]> {
  return invoke("list_drives");
}

export async function listImages(path: string): Promise<ImageEntry[]> {
  return invoke("list_images", { path });
}

export async function getThumbnail(path: string): Promise<string> {
  return invoke("get_thumbnail", { path });
}

export async function processImages(
  files: string[],
  outputFolder: string,
  config: ProcessingConfig
): Promise<ProcessResult[]> {
  return invoke("process_images", {
    files,
    outputFolder,
    config,
  });
}

export async function cancelProcessing(): Promise<void> {
  return invoke("cancel_processing");
}
```

- [ ] **Step 3: グローバルスタイルを作成**

`gui-frontend/src/app.css`:
```css
:root {
  --bg-primary: #0f0f1a;
  --bg-secondary: #1a1a2e;
  --bg-hover: #252540;
  --border-color: #333;
  --text-primary: #e0e0e0;
  --text-secondary: #888;
  --text-muted: #666;
  --accent: #818cf8;
  --accent-hover: #6366f1;
  --accent-bg: rgba(99, 102, 241, 0.15);
  --danger: #ef4444;
  --success: #22c55e;
  --warning: #f59e0b;
  --radius: 6px;
  --radius-sm: 4px;
}

* {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
}

body {
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
  background: var(--bg-primary);
  color: var(--text-primary);
  overflow: hidden;
  height: 100vh;
}

#app {
  height: 100vh;
  display: flex;
  flex-direction: column;
}

::-webkit-scrollbar {
  width: 6px;
}

::-webkit-scrollbar-track {
  background: transparent;
}

::-webkit-scrollbar-thumb {
  background: var(--border-color);
  border-radius: 3px;
}

::-webkit-scrollbar-thumb:hover {
  background: var(--text-muted);
}
```

- [ ] **Step 4: main.tsにCSS importを追加**

`gui-frontend/src/main.ts`:
```typescript
import "./app.css";
import App from "./App.svelte";
import { mount } from "svelte";

const app = mount(App, { target: document.getElementById("app")! });

export default app;
```

- [ ] **Step 5: ビルド確認**

```bash
cd gui-frontend && npm run build && cd ..
```
Expected: ビルド成功

- [ ] **Step 6: コミット**

```bash
git add gui-frontend/
git commit -m "feat: フロントエンド型定義・APIラッパー・グローバルスタイルを追加"
```

---

### Task 8: FolderTreeコンポーネント

**Files:**
- Create: `gui-frontend/src/lib/FolderTree.svelte`

- [ ] **Step 1: FolderTree.svelteを作成**

フォルダーツリーコンポーネント。ドライブルート → フォルダー階層をクリックで展開。

```svelte
<script lang="ts">
  import { listDirectory, listDrives } from "./api";
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

  async function loadRoots() {
    const drives = await listDrives();
    roots = drives.map((drive) => ({
      entry: { name: drive, path: drive, is_dir: true, is_image: false },
      children: null,
      expanded: false,
      loading: false,
    }));

    // 最初のドライブを自動展開
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
    if (!node.expanded) {
      toggleNode(node);
    }
  }

  $effect(() => {
    loadRoots();
  });
</script>

<div class="folder-tree">
  <div class="header">フォルダー</div>
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

<style>
  .folder-tree {
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--bg-secondary);
    overflow: hidden;
  }

  .header {
    padding: 12px;
    color: var(--text-secondary);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 1px;
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
</style>
```

- [ ] **Step 2: ビルド確認**

```bash
cd gui-frontend && npm run build && cd ..
```

- [ ] **Step 3: コミット**

```bash
git add gui-frontend/src/lib/FolderTree.svelte
git commit -m "feat: FolderTreeコンポーネントを追加"
```

---

### Task 9: ThumbnailGridコンポーネント

**Files:**
- Create: `gui-frontend/src/lib/ThumbnailGrid.svelte`

- [ ] **Step 1: ThumbnailGrid.svelteを作成**

遅延読み込みサムネイル付き画像グリッド。クリックで選択/解除。ページネーション対応。

```svelte
<script lang="ts">
  import { getThumbnail } from "./api";
  import type { ImageEntry } from "./types";

  interface Props {
    images: ImageEntry[];
    selectedPaths: Set<string>;
    onToggleSelect: (image: ImageEntry) => void;
  }

  let { images, selectedPaths, onToggleSelect }: Props = $props();

  const PAGE_SIZE = 50;
  let currentPage = $state(0);

  let pagedImages = $derived(
    images.slice(currentPage * PAGE_SIZE, (currentPage + 1) * PAGE_SIZE)
  );
  let totalPages = $derived(Math.ceil(images.length / PAGE_SIZE));

  // サムネイル遅延読み込み
  let thumbnailCache = $state<Map<string, string>>(new Map());

  async function loadThumbnail(path: string) {
    if (thumbnailCache.has(path)) return;
    try {
      const base64 = await getThumbnail(path);
      thumbnailCache.set(path, base64);
      thumbnailCache = new Map(thumbnailCache); // reactivity trigger
    } catch {
      // プレースホルダーを維持
    }
  }

  function onVisible(path: string) {
    loadThumbnail(path);
  }

  // ページが変わったらキャッシュ済み以外をロード
  $effect(() => {
    pagedImages.forEach((img) => {
      if (!thumbnailCache.has(img.path)) {
        loadThumbnail(img.path);
      }
    });
  });

  // フォルダー変更時にページリセット
  $effect(() => {
    images; // dependency
    currentPage = 0;
  });
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
      >
        <div class="thumb-wrapper">
          {#if thumbnailCache.has(image.path)}
            <img
              src="data:image/jpeg;base64,{thumbnailCache.get(image.path)}"
              alt={image.name}
              loading="lazy"
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

<style>
  .thumbnail-grid {
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--bg-primary);
    overflow: hidden;
  }

  .grid-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 8px 12px;
    color: var(--text-secondary);
    font-size: 11px;
    border-bottom: 1px solid var(--border-color);
  }

  .pagination {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .pagination button {
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    color: var(--text-primary);
    padding: 2px 8px;
    border-radius: var(--radius-sm);
    cursor: pointer;
  }

  .pagination button:disabled {
    opacity: 0.3;
    cursor: default;
  }

  .grid {
    flex: 1;
    overflow-y: auto;
    padding: 12px;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(120px, 1fr));
    gap: 8px;
    align-content: start;
  }

  .grid-item {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    padding: 4px;
    border: 2px solid transparent;
    border-radius: var(--radius);
    background: none;
    cursor: pointer;
    color: var(--text-primary);
  }

  .grid-item:hover {
    background: var(--bg-hover);
  }

  .grid-item.selected {
    border-color: var(--accent);
  }

  .thumb-wrapper {
    position: relative;
    width: 100%;
    aspect-ratio: 4 / 5;
    border-radius: var(--radius-sm);
    overflow: hidden;
    background: var(--bg-secondary);
  }

  .thumb-wrapper img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .placeholder {
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 24px;
    color: var(--text-muted);
  }

  .check {
    position: absolute;
    top: 4px;
    right: 4px;
    background: var(--accent);
    color: white;
    border-radius: 50%;
    width: 20px;
    height: 20px;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 11px;
    font-weight: bold;
  }

  .filename {
    font-size: 10px;
    color: var(--text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 100%;
  }
</style>
```

- [ ] **Step 2: ビルド確認**

```bash
cd gui-frontend && npm run build && cd ..
```

- [ ] **Step 3: コミット**

```bash
git add gui-frontend/src/lib/ThumbnailGrid.svelte
git commit -m "feat: ThumbnailGridコンポーネントを追加"
```

---

### Task 10: SelectionList + SettingsPanel + ProgressOverlayコンポーネント

**Files:**
- Create: `gui-frontend/src/lib/SelectionList.svelte`
- Create: `gui-frontend/src/lib/SettingsPanel.svelte`
- Create: `gui-frontend/src/lib/ProgressOverlay.svelte`

- [ ] **Step 1: SelectionList.svelteを作成**

選択済み画像の一覧。×ボタンで除外可能。サムネイル付き。

```svelte
<script lang="ts">
  import type { ImageEntry } from "./types";

  interface Props {
    selectedImages: ImageEntry[];
    thumbnailCache: Map<string, string>;
    onRemove: (image: ImageEntry) => void;
  }

  let { selectedImages, thumbnailCache, onRemove }: Props = $props();
</script>

<div class="selection-list">
  <div class="header">選択済み ({selectedImages.length})</div>
  <div class="list">
    {#each selectedImages as image (image.path)}
      <div class="item">
        <div class="thumb">
          {#if thumbnailCache.has(image.path)}
            <img
              src="data:image/jpeg;base64,{thumbnailCache.get(image.path)}"
              alt={image.name}
            />
          {:else}
            <div class="thumb-placeholder">📷</div>
          {/if}
        </div>
        <div class="info">
          <div class="name">{image.name}</div>
          <div class="meta">{image.width}×{image.height}</div>
        </div>
        <button class="remove" onclick={() => onRemove(image)}>×</button>
      </div>
    {/each}
  </div>
</div>

<style>
  .selection-list {
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .header {
    padding: 12px;
    color: var(--text-secondary);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 1px;
    border-bottom: 1px solid var(--border-color);
  }

  .list {
    flex: 1;
    overflow-y: auto;
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px;
    background: var(--accent-bg);
    border-radius: var(--radius);
  }

  .thumb {
    width: 40px;
    height: 50px;
    flex-shrink: 0;
    border-radius: var(--radius-sm);
    overflow: hidden;
    background: var(--bg-primary);
  }

  .thumb img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .thumb-placeholder {
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 16px;
  }

  .info {
    flex: 1;
    min-width: 0;
  }

  .name {
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .meta {
    font-size: 10px;
    color: var(--text-muted);
  }

  .remove {
    background: none;
    border: none;
    color: var(--text-muted);
    font-size: 16px;
    cursor: pointer;
    padding: 4px;
    line-height: 1;
  }

  .remove:hover {
    color: var(--danger);
  }
</style>
```

- [ ] **Step 2: SettingsPanel.svelteを作成**

```svelte
<script lang="ts">
  import type { ProcessingConfig } from "./types";

  interface Props {
    config: ProcessingConfig;
    outputFolder: string;
    canProcess: boolean;
    onPickOutputFolder: () => void;
    onProcess: () => void;
  }

  let { config = $bindable(), outputFolder, canProcess, onPickOutputFolder, onProcess }: Props = $props();
</script>

<div class="settings-panel">
  <div class="header">設定</div>
  <div class="settings">
    <label class="field">
      <span class="label">モード</span>
      <select bind:value={config.mode}>
        <option value="crop">Crop (中央クロップ)</option>
        <option value="pad">Pad (パディング)</option>
        <option value="quality">Quality (サイズのみ)</option>
      </select>
    </label>

    {#if config.mode === "pad"}
      <label class="field">
        <span class="label">背景色</span>
        <select bind:value={config.bg_color}>
          <option value="white">白</option>
          <option value="black">黒</option>
        </select>
      </label>
    {/if}

    <label class="field">
      <span class="label">品質: {config.quality}%</span>
      <input type="range" min="1" max="100" bind:value={config.quality} />
    </label>

    <label class="field">
      <span class="label">最大サイズ: {config.max_size_mb}MB</span>
      <input type="range" min="1" max="50" bind:value={config.max_size_mb} />
    </label>

    <div class="field">
      <span class="label">出力先</span>
      <button class="folder-btn" onclick={onPickOutputFolder}>
        {outputFolder || "フォルダーを選択..."}
      </button>
    </div>

    <label class="checkbox">
      <input type="checkbox" bind:checked={config.delete_originals} />
      <span>元ファイルを削除</span>
    </label>
  </div>

  <div class="action">
    <button class="process-btn" disabled={!canProcess} onclick={onProcess}>
      変換実行 →
    </button>
  </div>
</div>

<style>
  .settings-panel {
    display: flex;
    flex-direction: column;
    border-top: 1px solid var(--border-color);
  }

  .header {
    padding: 12px;
    color: var(--text-secondary);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 1px;
  }

  .settings {
    padding: 0 12px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .label {
    font-size: 12px;
    color: var(--text-secondary);
  }

  select, input[type="range"] {
    width: 100%;
    background: var(--bg-primary);
    border: 1px solid var(--border-color);
    color: var(--text-primary);
    padding: 4px 8px;
    border-radius: var(--radius-sm);
    font-size: 12px;
  }

  .folder-btn {
    width: 100%;
    padding: 6px 8px;
    background: var(--bg-primary);
    border: 1px solid var(--border-color);
    color: var(--text-secondary);
    border-radius: var(--radius-sm);
    cursor: pointer;
    font-size: 11px;
    text-align: left;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .folder-btn:hover {
    border-color: var(--accent);
  }

  .checkbox {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    color: var(--text-secondary);
    cursor: pointer;
  }

  .action {
    padding: 12px;
  }

  .process-btn {
    width: 100%;
    padding: 10px;
    background: var(--accent);
    color: white;
    border: none;
    border-radius: var(--radius);
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
  }

  .process-btn:hover:not(:disabled) {
    background: var(--accent-hover);
  }

  .process-btn:disabled {
    opacity: 0.4;
    cursor: default;
  }
</style>
```

- [ ] **Step 3: ProgressOverlay.svelteを作成**

```svelte
<script lang="ts">
  import type { ProgressPayload } from "./types";

  interface Props {
    progress: ProgressPayload | null;
    onCancel: () => void;
  }

  let { progress, onCancel }: Props = $props();

  let percentage = $derived(
    progress ? Math.round((progress.current / progress.total) * 100) : 0
  );
</script>

{#if progress}
  <div class="overlay">
    <div class="modal">
      <h3>変換中...</h3>
      <div class="progress-bar">
        <div class="progress-fill" style="width: {percentage}%"></div>
      </div>
      <div class="info">
        <span>{progress.current} / {progress.total}</span>
        <span>{progress.file_name}</span>
      </div>
      <button class="cancel-btn" onclick={onCancel}>キャンセル</button>
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.7);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
  }

  .modal {
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: 12px;
    padding: 32px;
    min-width: 400px;
    text-align: center;
  }

  h3 {
    margin-bottom: 20px;
    font-size: 18px;
  }

  .progress-bar {
    height: 8px;
    background: var(--bg-primary);
    border-radius: 4px;
    overflow: hidden;
    margin-bottom: 12px;
  }

  .progress-fill {
    height: 100%;
    background: var(--accent);
    border-radius: 4px;
    transition: width 0.3s ease;
  }

  .info {
    display: flex;
    justify-content: space-between;
    font-size: 12px;
    color: var(--text-secondary);
    margin-bottom: 20px;
  }

  .cancel-btn {
    padding: 8px 24px;
    background: none;
    border: 1px solid var(--border-color);
    color: var(--text-primary);
    border-radius: var(--radius);
    cursor: pointer;
    font-size: 13px;
  }

  .cancel-btn:hover {
    border-color: var(--danger);
    color: var(--danger);
  }
</style>
```

- [ ] **Step 4: ビルド確認**

```bash
cd gui-frontend && npm run build && cd ..
```

- [ ] **Step 5: コミット**

```bash
git add gui-frontend/src/lib/SelectionList.svelte gui-frontend/src/lib/SettingsPanel.svelte gui-frontend/src/lib/ProgressOverlay.svelte
git commit -m "feat: SelectionList, SettingsPanel, ProgressOverlayコンポーネントを追加"
```

---

### Task 11: App.svelte統合 — 3カラムレイアウト

**Files:**
- Modify: `gui-frontend/src/App.svelte`

- [ ] **Step 1: App.svelteを3カラムレイアウトで実装**

全コンポーネントを統合し、状態管理を行うメインコンポーネント:

```svelte
<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { open } from "@tauri-apps/plugin-dialog";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import FolderTree from "./lib/FolderTree.svelte";
  import ThumbnailGrid from "./lib/ThumbnailGrid.svelte";
  import SelectionList from "./lib/SelectionList.svelte";
  import SettingsPanel from "./lib/SettingsPanel.svelte";
  import ProgressOverlay from "./lib/ProgressOverlay.svelte";
  import { listImages, processImages, cancelProcessing } from "./lib/api";
  import type { ImageEntry, ProcessingConfig, ProgressPayload } from "./lib/types";

  // --- 状態 ---
  let images = $state<ImageEntry[]>([]);
  let selectedImages = $state<ImageEntry[]>([]);
  let outputFolder = $state("");
  let config = $state<ProcessingConfig>({
    mode: "crop",
    bg_color: "white",
    quality: 90,
    max_size_mb: 8,
    delete_originals: false,
  });
  let processing = $state(false);
  let progress = $state<ProgressPayload | null>(null);
  let thumbnailCache = $state<Map<string, string>>(new Map());

  // --- 派生状態 ---
  let selectedPaths = $derived(new Set(selectedImages.map((img) => img.path)));
  let canProcess = $derived(
    selectedImages.length > 0 && !processing && outputFolder !== ""
  );

  // --- イベントリスナー ---
  let unlisten: UnlistenFn | null = $state(null);

  $effect(() => {
    listen<ProgressPayload>("processing-progress", (event) => {
      progress = event.payload;
    }).then((fn) => {
      unlisten = fn;
    });

    return () => {
      unlisten?.();
    };
  });

  // --- ハンドラー ---
  async function handleSelectFolder(path: string) {
    try {
      images = await listImages(path);
    } catch (e) {
      console.error("Failed to list images:", e);
      images = [];
    }
  }

  function handleToggleSelect(image: ImageEntry) {
    const idx = selectedImages.findIndex((img) => img.path === image.path);
    if (idx >= 0) {
      selectedImages = selectedImages.filter((_, i) => i !== idx);
    } else {
      selectedImages = [...selectedImages, image];
    }
  }

  function handleRemove(image: ImageEntry) {
    selectedImages = selectedImages.filter((img) => img.path !== image.path);
  }

  async function handlePickOutputFolder() {
    const selected = await open({ directory: true, multiple: false });
    if (selected) {
      outputFolder = selected as string;
    }
  }

  async function handleProcess() {
    if (!canProcess) return;
    processing = true;
    progress = { current: 0, total: selectedImages.length, file_name: "" };

    try {
      const files = selectedImages.map((img) => img.path);
      const results = await processImages(files, outputFolder, config);
      alert(`完了: ${results.length}/${selectedImages.length} 枚を変換しました`);
    } catch (e) {
      alert(`エラー: ${e}`);
    } finally {
      processing = false;
      progress = null;
    }
  }

  async function handleCancel() {
    await cancelProcessing();
  }
</script>

<div class="app">
  <div class="left-panel">
    <FolderTree onSelectFolder={handleSelectFolder} />
  </div>

  <div class="center-panel">
    <ThumbnailGrid
      {images}
      {selectedPaths}
      onToggleSelect={handleToggleSelect}
    />
  </div>

  <div class="right-panel">
    <SelectionList
      {selectedImages}
      {thumbnailCache}
      onRemove={handleRemove}
    />
    <SettingsPanel
      bind:config
      {outputFolder}
      {canProcess}
      onPickOutputFolder={handlePickOutputFolder}
      onProcess={handleProcess}
    />
  </div>
</div>

<ProgressOverlay {progress} onCancel={handleCancel} />

<style>
  .app {
    display: flex;
    height: 100vh;
    overflow: hidden;
  }

  .left-panel {
    width: 220px;
    min-width: 180px;
    border-right: 1px solid var(--border-color);
    overflow: hidden;
  }

  .center-panel {
    flex: 1;
    overflow: hidden;
  }

  .right-panel {
    width: 240px;
    min-width: 200px;
    border-left: 1px solid var(--border-color);
    background: var(--bg-secondary);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
</style>
```

- [ ] **Step 2: ビルド確認**

注: `@tauri-apps/plugin-dialog`はTask 4のpackage.jsonで既にdependenciesに含まれている。

```bash
cd gui-frontend && npm run build && cd ..
cargo build -p picture-tool-gui
```

Expected: フロントエンド・バックエンド共にビルド成功

- [ ] **Step 3: 起動テスト**

```bash
cd gui && cargo tauri dev
```

Expected: ウィンドウが開き、3カラムレイアウトが表示される。フォルダーツリーでナビゲーション可能。

- [ ] **Step 4: コミット**

```bash
git add gui-frontend/
git commit -m "feat: 3カラムレイアウトのApp.svelteを実装し全コンポーネントを統合"
```

---

## Phase 4: 仕上げ

### Task 12: Windows ビルドテスト & 最終調整

**Files:**
- 必要に応じて各ファイルを修正

- [ ] **Step 1: Windowsクロスコンパイルまたはネイティブビルド**

Windows環境で:
```bash
cargo build -p picture-tool --release
cargo build -p picture-tool-gui --release
```

またはLinuxからクロスコンパイル確認:
```bash
cargo check -p picture-tool --target x86_64-pc-windows-msvc
```

- [ ] **Step 2: パス区切り文字の確認**

Windows上で動作させ、以下を確認:
- フォルダーツリーのドライブレター表示（C:\, D:\ 等）
- パス区切り文字（`\`）が正しく処理されること
- 日本語フォルダー名の対応

- [ ] **Step 3: .gitignoreの更新**

```
/target
/gui-frontend/node_modules
/gui-frontend/dist
.superpowers/
```

- [ ] **Step 4: コミット**

```bash
git add .gitignore
git commit -m "chore: Windows対応確認と.gitignore更新"
```

---

## 実装順序まとめ

| Phase | Task | 内容 | 依存 |
|-------|------|------|------|
| 1 | 1 | Workspace + core骨格 | なし |
| 1 | 2 | core画像処理ロジック実装 | Task 1 |
| 1 | 3 | CLI refactor → core利用 | Task 2 |
| 2 | 4 | Tauri v2プロジェクト初期化 | Task 2 |
| 2 | 5 | FS系Tauriコマンド | Task 4 |
| 2 | 6 | 処理系Tauriコマンド + キャンセル | Task 5 |
| 3 | 7 | 型定義・API・スタイル | Task 4 |
| 3 | 8 | FolderTree | Task 7 |
| 3 | 9 | ThumbnailGrid | Task 7 |
| 3 | 10 | SelectionList + Settings + Progress | Task 7 |
| 3 | 11 | App.svelte統合 | Task 8-10 |
| 4 | 12 | Windowsテスト + 最終調整 | Task 11 |

Task 4-6（Tauriバックエンド）とTask 7-10（フロントエンドコンポーネント）は、Task 4完了後に並列実行可能。
