# Picture Tool GUI化 + CLI拡張 設計書

## 概要

Instagram投稿用の画像一括変換CLIツール（picture-tool）を、GUIアプリ化する。
CLIも引き続き維持し、コア画像処理ロジックを共有ライブラリとして抽出する。

## 要件

### 機能要件
1. **GUIアプリ** — エクスプローラーライクなフォルダー参照で写真を選択し、プレビュー→変換を実行
2. **元ファイル削除オプション** — 変換完了後に元ファイルを削除する事前設定オプション（CLI: `--delete-originals`フラグ、GUI: チェックボックス）
3. **Windows対応** — CLI/GUI共にWindows上での動作を確認・保証

### 非機能要件
- モダンで高速で使いやすいUI
- Windows環境がメインターゲット
- 写真の選択・入れ替えが頻繁に発生するため、操作性を重視

## アーキテクチャ

### プロジェクト構成（Cargo Workspace）

```
picture-tool-rust/
├── Cargo.toml              # workspace root
├── core/                   # 画像処理ライブラリ（共有ロジック）
│   ├── Cargo.toml
│   └── src/lib.rs
├── cli/                    # CLIバイナリ
│   ├── Cargo.toml
│   └── src/main.rs
├── gui/                    # Tauri アプリ
│   ├── Cargo.toml
│   ├── src/main.rs
│   ├── tauri.conf.json
│   └── frontend/           # Svelte 5 フロントエンド
│       ├── package.json
│       ├── src/
│       └── ...
```

### 技術スタック
- **バックエンド共通**: Rust（image, rayon, walkdir, anyhow）
- **CLI**: clap (derive)
- **GUI**: Tauri v2 + Svelte 5（runes構文）
- **シリアライゼーション**: serde + serde_json（Tauri境界の型変換用）

### 設計方針
- `core`クレートに画像処理ロジックを集約し、CLI/GUIから参照
- GUIのRust側はTauriコマンドとして`core`の関数をラップするだけ
- フロントエンドはSvelte 5のrunes構文（`$state`, `$derived`, `$effect`）を使用
- Tauri境界を越える型はすべて`serde::Serialize / Deserialize`を導出

## Core ライブラリ API

```rust
// core/src/lib.rs
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingConfig {
    pub mode: ConversionMode,
    pub bg_color: BackgroundColor,
    pub quality: u8,          // 1-100, バリデーションはcore側で実施
    pub max_size_mb: usize,
    pub delete_originals: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessResult {
    pub input_path: String,   // Tauri境界用にStringで保持
    pub output_path: String,
    pub final_size_mb: f64,
    pub final_quality: Option<u8>,
}

/// 進捗コールバック: (current, total) -> bool
/// 戻り値がfalseの場合、処理を中断する（キャンセル）
pub type ProgressCallback = Box<dyn Fn(usize, usize) -> bool + Send + Sync>;

/// 設定のバリデーション
pub fn validate_config(config: &ProcessingConfig) -> Result<()>;

/// フォルダーから画像ファイルを収集
pub fn collect_image_files(dir: &Path) -> Result<Vec<PathBuf>>;

/// 単一画像を処理
/// delete_originalsがtrueかつ処理成功時のみ元ファイルを削除
pub fn process_image(
    input_path: &Path,
    output_folder: &Path,
    config: &ProcessingConfig,
) -> Result<ProcessResult>;

/// 複数画像を並列処理
/// on_progressがfalseを返した場合、未処理の画像はスキップし処理済み結果を返す
pub fn process_batch(
    files: &[PathBuf],
    output_folder: &Path,
    config: &ProcessingConfig,
    on_progress: Option<ProgressCallback>,
) -> Vec<Result<ProcessResult>>;

/// サムネイル生成（base64 JPEG文字列を返す）
pub fn generate_thumbnail_base64(
    path: &Path,
    max_dimension: u32,
) -> Result<String>;
```

### 元ファイル削除の安全性ルール
- 変換が**成功**した画像のみ元ファイルを削除する
- 変換が失敗した画像の元ファイルは削除しない
- 元ファイル削除が失敗した場合は警告を出力し、処理は継続する

## GUI 設計

### レイアウト: 3カラム構成

```
┌──────────────┬───────────────────────────┬──────────────────┐
│  フォルダー    │    サムネイルグリッド        │   選択済み (N)   │
│  ツリー       │                           │                  │
│              │  [img] [img] [img] [img]  │  [thumb] name.jpg│
│  📂 Photos   │  [img] [img] [img] [img]  │  [thumb] name.jpg│
│    📂 Travel  │                           │  [thumb] name.jpg│
│    📁 Food   │                           │                  │
│              │                           │  ─── 設定 ───    │
│              │                           │  モード: Crop    │
│              │                           │  品質: 90%       │
│              │                           │  最大: 8MB       │
│              │                           │  出力先: [選択]   │
│              │                           │  ☐ 元ファイル削除 │
│              │                           │  [変換実行 →]    │
└──────────────┴───────────────────────────┴──────────────────┘
```

- **左パネル**: フォルダーツリー（エクスプローラーライク）
  - Windows: ドライブレター（C:\, D:\ 等）をルートノードとして表示
  - 起動時はユーザーのピクチャフォルダー（`known_folder::Pictures`）を初期表示
- **中央パネル**: 選択中フォルダーの画像サムネイルグリッド。クリックで選択/解除
  - サムネイルは遅延読み込み（表示領域に入ったものだけ生成）
  - 1ページ50枚を上限とし、ページネーションで対応
- **右パネル**: 選択済み写真リスト（×ボタンで除外可）、変換設定、出力先選択、実行ボタン

### 型定義

```rust
// gui/src/types.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub name: String,
    pub path: String,        // 絶対パス
    pub is_dir: bool,
    pub is_image: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageEntry {
    pub name: String,
    pub path: String,        // 絶対パス
    pub width: u32,
    pub height: u32,
    pub size_bytes: u64,
    pub thumbnail_base64: Option<String>,  // 遅延読み込み時はNone
}
```

### Tauri コマンド

```rust
/// フォルダー内容を取得（ツリー用）
#[tauri::command]
fn list_directory(path: String) -> Result<Vec<FileEntry>, String>;

/// システムのドライブ一覧を取得（Windows用）
#[tauri::command]
fn list_drives() -> Result<Vec<String>, String>;

/// 画像一覧を取得（サムネイルなし、メタデータのみ）
#[tauri::command]
async fn list_images(path: String) -> Result<Vec<ImageEntry>, String>;

/// 指定画像のサムネイルをbase64で取得（遅延読み込み用）
#[tauri::command]
async fn get_thumbnail(path: String) -> Result<String, String>;

/// 選択された画像を一括変換
#[tauri::command]
async fn process_images(
    app_handle: tauri::AppHandle,
    files: Vec<String>,
    output_folder: String,
    config: ProcessingConfig,
) -> Result<Vec<ProcessResult>, String>;  // core::ProcessResultをそのまま使用

/// 変換をキャンセル
#[tauri::command]
fn cancel_processing(state: tauri::State<ProcessingState>) -> Result<(), String>;

/// フォルダー選択ダイアログ
#[tauri::command]
async fn pick_folder() -> Result<Option<String>, String>;
```

### キャンセル機構

```rust
// gui/src/state.rs
use std::sync::atomic::{AtomicBool, Ordering};

pub struct ProcessingState {
    pub cancel_flag: Arc<AtomicBool>,
}

// process_imagesコマンド内:
// 1. cancel_flagをfalseにリセット
// 2. ProgressCallbackでcancel_flagをチェック
// 3. cancel_processingコマンドがcancel_flagをtrueにセット
// 4. process_batchがcallbackのfalse戻り値を検出して中断
let callback = {
    let flag = Arc::clone(&cancel_flag);
    move |current, total| -> bool {
        app_handle.emit("processing-progress", ProgressPayload {
            current, total, file_name: "...".into(),
        }).ok();
        !flag.load(Ordering::Relaxed) // falseでキャンセル
    }
};
```

### フロントエンド状態管理（Svelte 5 runes）

```svelte
let currentFolder = $state<string>('');
let images = $state<ImageEntry[]>([]);
let selectedImages = $state<ImageEntry[]>([]);
let outputFolder = $state<string>('');
let config = $state<ProcessingConfig>({
  mode: 'crop',
  bgColor: 'white',
  quality: 90,
  maxSize: 8,
  deleteOriginals: false,
});
let processing = $state(false);
let progress = $state<{current: number, total: number} | null>(null);

let selectedCount = $derived(selectedImages.length);
let canProcess = $derived(selectedCount > 0 && !processing && outputFolder !== '');
```

### 進捗通知

`ProgressCallback`でGUIコマンド側がTauriイベントを発火する（`core`はTauriに依存しない）。

```rust
// gui/src/main.rs — process_imagesコマンド内
let on_progress: ProgressCallback = Box::new(move |current, total| -> bool {
    let _ = app_handle.emit("processing-progress", ProgressPayload {
        current,
        total,
        file_name: files[current.saturating_sub(1)].clone(),
    });
    !cancel_flag.load(Ordering::Relaxed)
});

core::process_batch(&file_paths, &output, &config, Some(on_progress));
```

```svelte
// Svelte側
import { listen } from '@tauri-apps/api/event';
import type { UnlistenFn } from '@tauri-apps/api/event';

let unlisten: UnlistenFn | null = $state(null);

$effect(() => {
  listen<ProgressPayload>('processing-progress', (event) => {
    progress = event.payload;
  }).then((fn) => { unlisten = fn; });

  return () => { unlisten?.(); };
});
```

## CLIの変更

現行CLIからの変更は最小限：

- `--delete-originals` フラグの追加
- 内部的に `core` ライブラリを使用するようリファクタリング
- ユーザーから見た既存の動作は変更なし

## エラーハンドリング

| シナリオ | CLI | GUI |
|---------|-----|-----|
| 画像読み込み失敗 | stderr出力、次へ進む | エラーアイコン表示、他の画像は継続 |
| 出力フォルダー書き込み不可 | 即座にエラー終了 | ダイアログで通知、別フォルダーを促す |
| サムネイル生成失敗 | — | プレースホルダー画像を表示 |
| 変換中のキャンセル | Ctrl+C（OS任せ） | キャンセルボタン → AtomicBoolフラグで中断、処理済みファイルは保持 |
| 元ファイル削除失敗 | 警告出力、処理は継続 | 警告表示、処理は継続 |
| 変換失敗 + 削除オプションON | 警告出力、元ファイルは削除しない | 警告表示、元ファイルは削除しない |
| 品質値が範囲外 | エラー終了 | UI側で1-100にスライダー制限、core側でもバリデーション |
| PNG透過画像の変換 | JPEG出力時に白背景で合成（既存動作） | サムネイルで透過→白背景の変化を事前プレビュー |

## 将来の拡張

第一段階完了後に検討する機能：
- Exifフレーム対応
- その他の画像処理モード

`core`ライブラリへの関数追加 → CLI/GUIからオプション公開のパターンで拡張可能。
