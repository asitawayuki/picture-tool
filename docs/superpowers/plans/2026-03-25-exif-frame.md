# Exifフレーム機能 実装計画

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 写真にEXIF情報とメーカー/ブランドロゴを含むフレームを付加する機能を、core/CLI/GUIに実装する

**Architecture:** coreライブラリに `exif_frame` モジュールを追加し、`image` + `imageproc` + `ab_glyph` でフレーム描画、`resvg` でSVGロゴのラスタライズを行う。プリセットはJSON設定で管理し、GUI/CLI共通で利用可能。GUIにはモーダル設定画面（ライブプレビュー付き）を追加する。

**Tech Stack:** Rust (`image 0.24`, `imageproc`, `ab_glyph`, `resvg`, `usvg`, `rust-embed`), Svelte 5 (runes), Tauri v2

**Spec:** `docs/superpowers/specs/2026-03-25-exif-frame-design.md`

---

## ファイル構成

### 新規作成ファイル

| ファイル | 責務 |
|---------|------|
| `core/src/exif_frame/mod.rs` | 公開型定義 (`ExifFrameConfig`, `AssetDirs` 等) + `render_exif_frame()` エントリポイント |
| `core/src/exif_frame/layout.rs` | 3レイアウト（BottomBar, SideBar, FullBorder）の座標計算とキャンバス合成 |
| `core/src/exif_frame/logo.rs` | ロゴファイル検索・読み込み（PNG/SVG）・マッチングロジック |
| `core/src/exif_frame/text.rs` | `ab_glyph` によるテキスト描画 |
| `core/src/exif_frame/preset.rs` | プリセットJSON読み書き・CRUD |
| `core/src/model_map.rs` | カメラ型番→表示名マッピング、ロゴマッチング |
| `core/assets/fonts/` | バンドルフォント (NotoSansJP-Regular.ttf) |
| `core/assets/logos/` | バンドルロゴ (SVG/PNG) |
| `core/assets/model_map.json` | デフォルトモデルマッピング |
| `core/assets/presets/default.json` | デフォルトプリセット |
| `gui-frontend/src/lib/ExifFrameSettings.svelte` | モーダル設定画面コンポーネント |

### 変更ファイル

| ファイル | 変更内容 |
|---------|---------|
| `core/Cargo.toml` | `imageproc`, `ab_glyph`, `resvg`, `usvg`, `rust-embed` 依存追加 |
| `core/src/lib.rs` | `mod exif_frame; mod model_map;` 追加、`process_image()` / `process_batch()` シグネチャ変更 |
| `cli/Cargo.toml` | `serde_json` 依存追加（プリセット読み込み用） |
| `cli/src/main.rs` | `--exif-frame`, `--preset`, `--preset-file`, `--custom-text` オプション追加 |
| `gui/Cargo.toml` | `serde_json` は既存 |
| `gui/src/commands.rs` | `process_images` にExifフレーム設定追加、新Tauriコマンド追加 |
| `gui-frontend/src/lib/types.ts` | `ExifFrameConfig`, `FontInfo`, `LogoInfo` 等の型追加 |
| `gui-frontend/src/lib/api.ts` | Exifフレーム関連のTauri invoke関数追加 |
| `gui-frontend/src/lib/SettingsPanel.svelte` | Exifフレーム ON/OFF トグル + プリセット選択追加 |
| `gui-frontend/src/App.svelte` | Exifフレーム設定状態管理、モーダル表示制御追加 |

---

## Task 1: core依存追加とモジュール骨格

**Files:**
- Modify: `core/Cargo.toml`
- Create: `core/src/exif_frame/mod.rs`
- Create: `core/src/exif_frame/layout.rs`
- Create: `core/src/exif_frame/logo.rs`
- Create: `core/src/exif_frame/text.rs`
- Create: `core/src/exif_frame/preset.rs`
- Create: `core/src/model_map.rs`
- Modify: `core/src/lib.rs`

- [ ] **Step 1: core/Cargo.toml に依存を追加**

```toml
[dependencies]
image = "0.24"
imageproc = "0.23"
ab_glyph = "0.2"
resvg = "0.37"
usvg = "0.37"
rust-embed = "8"
dirs = "5"
rayon = "1.10"
walkdir = "2.5"
anyhow = "1.0"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
base64 = "0.22"
kamadak-exif = "0.5"

[dev-dependencies]
tempfile = "3"
```

注: `serde_json` を `[dependencies]` に移動（現在は `[dev-dependencies]` のみ）。プリセットJSON解析に必要。`dirs` はユーザー設定ディレクトリ取得用。`[dev-dependencies]` の `tempfile` を明示的に維持。

- [ ] **Step 2: アセットディレクトリを作成（rust-embedのビルドに必要）**

`rust-embed` はコンパイル時にフォルダーの存在を確認するため、空でも先にディレクトリを作成:
```bash
mkdir -p core/assets/fonts core/assets/logos core/assets/presets
touch core/assets/fonts/.gitkeep core/assets/logos/.gitkeep core/assets/presets/.gitkeep
```

- [ ] **Step 3: モジュールファイルを空の骨格で作成**

`core/src/exif_frame/mod.rs`:
```rust
pub mod layout;
pub mod logo;
pub mod text;
pub mod preset;
```

`core/src/exif_frame/layout.rs`:
```rust
// 3レイアウトの座標計算と描画
```

`core/src/exif_frame/logo.rs`:
```rust
// ロゴファイル検索・読み込み・マッチング
```

`core/src/exif_frame/text.rs`:
```rust
// ab_glyph テキスト描画
```

`core/src/exif_frame/preset.rs`:
```rust
// プリセットJSON読み書き
```

`core/src/model_map.rs`:
```rust
// カメラ型番→表示名マッピング
```

- [ ] **Step 4: core/src/lib.rs にモジュール宣言を追加**

`core/src/lib.rs` 冒頭（既存の `use` 文の前）に追加:
```rust
pub mod exif_frame;
pub mod model_map;
```

- [ ] **Step 5: ビルド確認**

Run: `cd /home/biwak/myShrimp/picture-tool-rust && cargo build -p picture-tool-core`
Expected: コンパイル成功（警告は許容）

- [ ] **Step 6: コミット**

```bash
git add core/Cargo.toml core/src/exif_frame/ core/src/model_map.rs core/src/lib.rs core/assets/
git commit -m "feat: exif_frame モジュール骨格と依存追加"
```

---

## Task 2: データモデル定義

**Files:**
- Modify: `core/src/exif_frame/mod.rs`
- Test: `core/src/exif_frame/mod.rs` (インラインテスト)

- [ ] **Step 1: テストを書く — ExifFrameConfig のJSON往復**

`core/src/exif_frame/mod.rs` にテスト追加:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exif_frame_config_json_roundtrip() {
        let config = ExifFrameConfig {
            name: "test".to_string(),
            layout: FrameLayout::BottomBar,
            color: FrameColor::White,
            aspect_ratio: OutputAspectRatio::Fixed(4, 5),
            items: DisplayItems::all_enabled(),
            font: FontConfig::default(),
            custom_text: "@user".to_string(),
            frame_padding: 0.05,
        };
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: ExifFrameConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "test");
        assert_eq!(deserialized.custom_text, "@user");
    }

    #[test]
    fn frame_color_custom_json() {
        let color = FrameColor::Custom(255, 128, 0);
        let json = serde_json::to_string(&color).unwrap();
        let deserialized: FrameColor = serde_json::from_str(&json).unwrap();
        match deserialized {
            FrameColor::Custom(r, g, b) => {
                assert_eq!((r, g, b), (255, 128, 0));
            }
            _ => panic!("Expected Custom variant"),
        }
    }

    #[test]
    fn output_aspect_ratio_json() {
        let fixed = OutputAspectRatio::Fixed(4, 5);
        let json = serde_json::to_string(&fixed).unwrap();
        assert!(json.contains("fixed"));

        let free = OutputAspectRatio::Free;
        let json = serde_json::to_string(&free).unwrap();
        let deserialized: OutputAspectRatio = serde_json::from_str(&json).unwrap();
        matches!(deserialized, OutputAspectRatio::Free);
    }
}
```

- [ ] **Step 2: テストを実行して失敗を確認**

Run: `cargo test -p picture-tool-core exif_frame::tests -- --nocapture`
Expected: FAIL（型が未定義）

- [ ] **Step 3: データモデルを実装**

`core/src/exif_frame/mod.rs`:
```rust
pub mod layout;
pub mod logo;
pub mod preset;
pub mod text;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// フレームレイアウトの種類（3パターン）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameLayout {
    BottomBar,
    SideBar,
    FullBorder,
}

/// フレーム背景色
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameColor {
    White,
    Black,
    Custom(u8, u8, u8),
}

impl FrameColor {
    pub fn to_rgba(&self) -> image::Rgba<u8> {
        match self {
            FrameColor::White => image::Rgba([255, 255, 255, 255]),
            FrameColor::Black => image::Rgba([0, 0, 0, 255]),
            FrameColor::Custom(r, g, b) => image::Rgba([*r, *g, *b, 255]),
        }
    }

    pub fn is_dark(&self) -> bool {
        match self {
            FrameColor::Black => true,
            FrameColor::White => false,
            FrameColor::Custom(r, g, b) => {
                (*r as f32 * 0.299 + *g as f32 * 0.587 + *b as f32 * 0.114) < 128.0
            }
        }
    }
}

/// 出力アスペクト比
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputAspectRatio {
    Fixed(u32, u32),
    Free,
}

/// 表示項目のON/OFF
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DisplayItems {
    pub maker_logo: bool,
    pub brand_logo: bool,
    pub lens_brand_logo: bool,
    pub camera_model: bool,
    pub lens_model: bool,
    pub focal_length: bool,
    pub f_number: bool,
    pub shutter_speed: bool,
    pub iso: bool,
    pub date_taken: bool,
    pub custom_text: bool,
}

impl DisplayItems {
    pub fn all_enabled() -> Self {
        Self {
            maker_logo: true,
            brand_logo: true,
            lens_brand_logo: true,
            camera_model: true,
            lens_model: true,
            focal_length: true,
            f_number: true,
            shutter_speed: true,
            iso: true,
            date_taken: true,
            custom_text: true,
        }
    }
}

/// フォント設定
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FontConfig {
    /// カスタムフォントのパス（UTF-8）。Noneでバンドルフォント使用
    pub font_path: Option<String>,
    /// カメラ/レンズ名のサイズ（画像短辺比率。例: 0.025）
    pub primary_size: f32,
    /// 撮影パラメータのサイズ（画像短辺比率。例: 0.018）
    pub secondary_size: f32,
}

impl Default for FontConfig {
    fn default() -> Self {
        Self {
            font_path: None,
            primary_size: 0.025,
            secondary_size: 0.018,
        }
    }
}

/// Exifフレーム設定（1プリセット = この構造体1つ）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExifFrameConfig {
    pub name: String,
    pub layout: FrameLayout,
    pub color: FrameColor,
    pub aspect_ratio: OutputAspectRatio,
    pub items: DisplayItems,
    pub font: FontConfig,
    pub custom_text: String,
    /// フレーム幅の割合（画像短辺比率。例: 0.05 = 5%）
    pub frame_padding: f32,
}

impl Default for ExifFrameConfig {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            layout: FrameLayout::BottomBar,
            color: FrameColor::White,
            aspect_ratio: OutputAspectRatio::Fixed(4, 5),
            items: DisplayItems::all_enabled(),
            font: FontConfig::default(),
            custom_text: String::new(),
            frame_padding: 0.05,
        }
    }
}

/// アセットディレクトリの検索パス
#[derive(Debug, Clone)]
pub struct AssetDirs {
    pub user_logos_dir: Option<PathBuf>,
    pub user_fonts_dir: Option<PathBuf>,
    pub user_model_map: Option<PathBuf>,
}

impl Default for AssetDirs {
    fn default() -> Self {
        let config_dir = dirs_config_dir();
        Self {
            user_logos_dir: config_dir.as_ref().map(|d| d.join("assets/logos")),
            user_fonts_dir: config_dir.as_ref().map(|d| d.join("assets/fonts")),
            user_model_map: config_dir.as_ref().map(|d| d.join("model_map_custom.json")),
        }
    }
}

fn dirs_config_dir() -> Option<PathBuf> {
    // ~/.config/picture-tool/ (Linux/Mac) or %APPDATA%\picture-tool (Windows)
    dirs::config_dir().map(|d| d.join("picture-tool"))
}

/// フォント情報（GUI一覧表示用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FontInfo {
    pub display_name: String,
    pub path: Option<String>,
    pub is_bundled: bool,
}

/// ロゴ情報（GUI一覧表示用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogoInfo {
    pub filename: String,
    pub matched_to: Option<String>,
    pub is_bundled: bool,
}
```

注: `dirs` クレートが必要。`core/Cargo.toml` に `dirs = "5"` を追加。

- [ ] **Step 4: テストを実行して成功を確認**

Run: `cargo test -p picture-tool-core exif_frame::tests -- --nocapture`
Expected: 3テストすべてPASS

- [ ] **Step 5: コミット**

```bash
git add core/src/exif_frame/mod.rs core/Cargo.toml
git commit -m "feat: Exifフレームのデータモデル定義（JSON往復テスト付き）"
```

---

## Task 3: モデルマッピング

**Files:**
- Modify: `core/src/model_map.rs`
- Create: `core/assets/model_map.json`
- Test: `core/src/model_map.rs` (インラインテスト)

- [ ] **Step 1: テストを書く**

`core/src/model_map.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_model_lookup() {
        let map = ModelMap::load_bundled();
        assert_eq!(map.camera_display_name("ILCE-7M4"), "α7IV");
        assert_eq!(map.camera_display_name("UNKNOWN-123"), "UNKNOWN-123"); // 未登録→生値
    }

    #[test]
    fn maker_logo_lookup() {
        let map = ModelMap::load_bundled();
        let logo = map.maker_logo("SONY");
        assert!(logo.is_some());
        assert_eq!(logo.unwrap().maker, "sony.svg");
        assert_eq!(logo.unwrap().brand.as_deref(), Some("alpha.svg"));
    }

    #[test]
    fn maker_logo_unknown() {
        let map = ModelMap::load_bundled();
        assert!(map.maker_logo("UNKNOWN_MAKER").is_none());
    }

    #[test]
    fn lens_brand_match_priority() {
        let map = ModelMap::load_bundled();
        // "FE 24-70mm f/2.8 GM II" → "GM" が "G " より先にマッチ
        let logo = map.lens_brand_logo("FE 24-70mm f/2.8 GM II");
        assert_eq!(logo.as_deref(), Some("gmaster.svg"));
    }

    #[test]
    fn lens_brand_match_g_lens() {
        let map = ModelMap::load_bundled();
        // "FE 70-200mm f/4 G OSS II" → "G " にマッチ（" G " 部分一致）
        let logo = map.lens_brand_logo("FE 70-200mm f/4 G OSS II");
        assert_eq!(logo.as_deref(), Some("sony_g.svg"));
    }

    #[test]
    fn custom_map_merge() {
        let mut map = ModelMap::load_bundled();
        let custom_json = r#"{
            "camera": { "CUSTOM-1": "Custom Camera" },
            "logo_match": {},
            "lens_brand_match": []
        }"#;
        map.merge_custom(custom_json).unwrap();
        assert_eq!(map.camera_display_name("CUSTOM-1"), "Custom Camera");
        // 既存マッピングも維持
        assert_eq!(map.camera_display_name("ILCE-7M4"), "α7IV");
    }
}
```

- [ ] **Step 2: テスト実行で失敗を確認**

Run: `cargo test -p picture-tool-core model_map::tests -- --nocapture`
Expected: FAIL

- [ ] **Step 3: model_map.json を作成**

`core/assets/model_map.json`:
```json
{
  "camera": {
    "ILCE-7M4": "α7IV",
    "ILCE-7M3": "α7III",
    "ILCE-7RM5": "α7RV",
    "ILCE-7RM4": "α7RIV",
    "ILCE-7RM3": "α7RIII",
    "ILCE-7SM3": "α7SIII",
    "ILCE-7CR": "α7CR",
    "ILCE-7CL": "α7CII",
    "ILCE-7C": "α7C",
    "ILCE-9M3": "α9III",
    "ILCE-9M2": "α9II",
    "ILCE-1": "α1",
    "ILCE-6700": "α6700",
    "ILCE-6600": "α6600",
    "ILCE-6400": "α6400"
  },
  "logo_match": {
    "SONY": { "maker": "sony.svg", "brand": "alpha.svg" },
    "Canon": { "maker": "canon.svg", "brand": null },
    "NIKON CORPORATION": { "maker": "nikon.svg", "brand": null },
    "FUJIFILM": { "maker": "fujifilm.svg", "brand": null },
    "SIGMA": { "maker": "sigma.svg", "brand": null }
  },
  "lens_brand_match": [
    { "pattern": "GM", "match_type": "contains", "logo": "gmaster.svg" },
    { "pattern": " G ", "match_type": "contains", "logo": "sony_g.svg" },
    { "pattern": "Art", "match_type": "contains", "logo": "sigma_art.svg" }
  ]
}
```

- [ ] **Step 4: ModelMap を実装**

`core/src/model_map.rs`:
```rust
use anyhow::Result;
use rust_embed::Embed;
use serde::Deserialize;
use std::collections::HashMap;

/// model_map.json のみを埋め込む（assets/ 全体を埋め込むとフォント等と重複するため）
#[derive(Embed)]
#[folder = "assets/"]
#[include = "model_map.json"]
struct ModelMapAssets;

#[derive(Debug, Deserialize)]
struct ModelMapJson {
    camera: HashMap<String, String>,
    logo_match: HashMap<String, LogoMatchEntry>,
    lens_brand_match: Vec<LensBrandRule>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LogoMatchEntry {
    pub maker: String,
    pub brand: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LensBrandRule {
    pattern: String,
    match_type: String, // "contains" のみ初期サポート
    logo: String,
}

pub struct ModelMap {
    camera: HashMap<String, String>,
    logo_match: HashMap<String, LogoMatchEntry>,
    lens_brand_match: Vec<LensBrandRule>,
}

impl ModelMap {
    pub fn load_bundled() -> Self {
        let data = ModelMapAssets::get("model_map.json")
            .expect("bundled model_map.json not found");
        let json: ModelMapJson = serde_json::from_slice(&data.data)
            .expect("invalid bundled model_map.json");
        Self {
            camera: json.camera,
            logo_match: json.logo_match,
            lens_brand_match: json.lens_brand_match,
        }
    }

    pub fn merge_custom(&mut self, json_str: &str) -> Result<()> {
        let custom: ModelMapJson = serde_json::from_str(json_str)?;
        for (k, v) in custom.camera {
            self.camera.insert(k, v);
        }
        for (k, v) in custom.logo_match {
            self.logo_match.insert(k, v);
        }
        // カスタムのlens_brand_matchは先頭に挿入（優先度高）
        let mut merged = custom.lens_brand_match;
        merged.extend(self.lens_brand_match.drain(..));
        self.lens_brand_match = merged;
        Ok(())
    }

    pub fn camera_display_name(&self, model: &str) -> &str {
        self.camera.get(model).map(|s| s.as_str()).unwrap_or(model)
    }

    pub fn maker_logo(&self, make: &str) -> Option<&LogoMatchEntry> {
        self.logo_match.get(make)
    }

    pub fn lens_brand_logo(&self, lens_model: &str) -> Option<&str> {
        for rule in &self.lens_brand_match {
            match rule.match_type.as_str() {
                "contains" => {
                    if lens_model.contains(&rule.pattern) {
                        return Some(&rule.logo);
                    }
                }
                _ => {} // 将来の拡張用
            }
        }
        None
    }
}
```

- [ ] **Step 5: テスト実行で成功を確認**

Run: `cargo test -p picture-tool-core model_map::tests -- --nocapture`
Expected: 5テストすべてPASS

- [ ] **Step 6: コミット**

```bash
git add core/src/model_map.rs core/assets/model_map.json
git commit -m "feat: カメラモデルマッピングとロゴマッチングロジック"
```

---

## Task 4: プリセット管理

**Files:**
- Modify: `core/src/exif_frame/preset.rs`
- Create: `core/assets/presets/default.json`
- Test: `core/src/exif_frame/preset.rs` (インラインテスト)

- [ ] **Step 1: テストを書く**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn load_bundled_presets() {
        let presets = load_bundled_presets();
        assert!(!presets.is_empty());
        assert!(presets.iter().any(|p| p.name == "default"));
    }

    #[test]
    fn save_and_load_user_preset() {
        let dir = TempDir::new().unwrap();
        let config = ExifFrameConfig::default();

        save_preset(&dir.path(), &config).unwrap();

        let loaded = load_user_presets(&dir.path());
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "default");
    }

    #[test]
    fn save_preset_overwrites_existing() {
        let dir = TempDir::new().unwrap();
        let mut config = ExifFrameConfig::default();
        save_preset(&dir.path(), &config).unwrap();

        config.custom_text = "updated".to_string();
        save_preset(&dir.path(), &config).unwrap();

        let loaded = load_user_presets(&dir.path());
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].custom_text, "updated");
    }

    #[test]
    fn delete_user_preset() {
        let dir = TempDir::new().unwrap();
        let config = ExifFrameConfig::default();
        save_preset(&dir.path(), &config).unwrap();

        delete_preset(&dir.path(), "default").unwrap();

        let loaded = load_user_presets(&dir.path());
        assert!(loaded.is_empty());
    }

    #[test]
    fn list_all_presets_merges_bundled_and_user() {
        let dir = TempDir::new().unwrap();
        let mut user_preset = ExifFrameConfig::default();
        user_preset.name = "my_custom".to_string();
        save_preset(&dir.path(), &user_preset).unwrap();

        let all = list_all_presets(Some(&dir.path()));
        assert!(all.iter().any(|p| p.name == "default")); // バンドル
        assert!(all.iter().any(|p| p.name == "my_custom")); // ユーザー
    }
}
```

- [ ] **Step 2: テスト実行で失敗を確認**

Run: `cargo test -p picture-tool-core exif_frame::preset::tests -- --nocapture`
Expected: FAIL

- [ ] **Step 3: デフォルトプリセットJSON作成**

`core/assets/presets/default.json`:
```json
{
  "name": "default",
  "layout": "bottom_bar",
  "color": "white",
  "aspect_ratio": { "fixed": [4, 5] },
  "items": {
    "maker_logo": true,
    "brand_logo": true,
    "lens_brand_logo": true,
    "camera_model": true,
    "lens_model": true,
    "focal_length": true,
    "f_number": true,
    "shutter_speed": true,
    "iso": true,
    "date_taken": false,
    "custom_text": false
  },
  "font": {
    "font_path": null,
    "primary_size": 0.025,
    "secondary_size": 0.018
  },
  "custom_text": "",
  "frame_padding": 0.05
}
```

- [ ] **Step 4: preset.rs を実装**

`core/src/exif_frame/preset.rs`:
```rust
use super::ExifFrameConfig;
use anyhow::Result;
use rust_embed::Embed;
use std::fs;
use std::path::Path;

#[derive(Embed)]
#[folder = "assets/presets/"]
struct PresetAssets;

/// バンドルプリセットを読み込み
pub fn load_bundled_presets() -> Vec<ExifFrameConfig> {
    let mut presets = Vec::new();
    for file in PresetAssets::iter() {
        if file.ends_with(".json") {
            if let Some(data) = PresetAssets::get(&file) {
                if let Ok(config) = serde_json::from_slice::<ExifFrameConfig>(&data.data) {
                    presets.push(config);
                }
            }
        }
    }
    presets
}

/// ユーザープリセットディレクトリから読み込み
pub fn load_user_presets(presets_dir: &Path) -> Vec<ExifFrameConfig> {
    let mut presets = Vec::new();
    if let Ok(entries) = fs::read_dir(presets_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "json") {
                if let Ok(data) = fs::read_to_string(&path) {
                    if let Ok(config) = serde_json::from_str::<ExifFrameConfig>(&data) {
                        presets.push(config);
                    }
                }
            }
        }
    }
    presets
}

/// 全プリセット一覧（バンドル + ユーザー、ユーザー側が優先）
pub fn list_all_presets(user_presets_dir: Option<&Path>) -> Vec<ExifFrameConfig> {
    let bundled = load_bundled_presets();
    let user = user_presets_dir
        .map(|d| load_user_presets(d))
        .unwrap_or_default();

    let mut result = bundled;
    for u in user {
        if let Some(pos) = result.iter().position(|b| b.name == u.name) {
            result[pos] = u; // ユーザーが同名バンドルを上書き
        } else {
            result.push(u);
        }
    }
    result
}

/// プリセットを保存（同名は上書き）
pub fn save_preset(presets_dir: &Path, config: &ExifFrameConfig) -> Result<()> {
    fs::create_dir_all(presets_dir)?;
    let filename = sanitize_filename(&config.name);
    let path = presets_dir.join(format!("{}.json", filename));
    let json = serde_json::to_string_pretty(config)?;
    fs::write(path, json)?;
    Ok(())
}

/// プリセットを削除
pub fn delete_preset(presets_dir: &Path, name: &str) -> Result<()> {
    let filename = sanitize_filename(name);
    let path = presets_dir.join(format!("{}.json", filename));
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}
```

- [ ] **Step 5: テスト実行で成功を確認**

Run: `cargo test -p picture-tool-core exif_frame::preset::tests -- --nocapture`
Expected: 5テストすべてPASS

- [ ] **Step 6: コミット**

```bash
git add core/src/exif_frame/preset.rs core/assets/presets/
git commit -m "feat: プリセット管理（CRUD + バンドル/ユーザーマージ）"
```

---

## Task 5: ロゴ読み込み（PNG/SVG）

**Files:**
- Modify: `core/src/exif_frame/logo.rs`
- Create: `core/assets/logos/` (テスト用プレースホルダー)
- Create: `core/tests/fixtures/test_logo.png`
- Create: `core/tests/fixtures/test_logo.svg`
- Test: `core/src/exif_frame/logo.rs` (インラインテスト)

- [ ] **Step 1: テスト用アセット作成**

テスト用の小さなPNGロゴ（10x10px 赤い正方形）とSVGロゴを作成。

`core/tests/fixtures/test_logo.svg`:
```svg
<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
  <circle cx="50" cy="50" r="40" fill="red"/>
</svg>
```

テスト用PNG はコードで生成する（外部ファイル不要）。

- [ ] **Step 2: テストを書く**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn load_svg_logo() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/test_logo.svg");
        let img = load_logo_file(&path, 64).unwrap();
        assert_eq!(img.width(), 64);
        assert_eq!(img.height(), 64);
    }

    #[test]
    fn load_png_logo() {
        // テスト用PNGを生成
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.png");
        let img = image::RgbaImage::from_pixel(100, 100, image::Rgba([255, 0, 0, 255]));
        img.save(&path).unwrap();

        let loaded = load_logo_file(&path, 32).unwrap();
        assert_eq!(loaded.width(), 32);
        assert_eq!(loaded.height(), 32);
    }

    #[test]
    fn resolve_logo_prefers_svg() {
        let dir = tempfile::TempDir::new().unwrap();
        // 同名のSVGとPNGを配置
        std::fs::write(dir.path().join("test.svg"), r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10" fill="blue"/></svg>"#).unwrap();
        let png = image::RgbaImage::from_pixel(10, 10, image::Rgba([255, 0, 0, 255]));
        png.save(dir.path().join("test.png")).unwrap();

        let resolved = resolve_logo_file(Some(dir.path()), "test", false);
        assert!(resolved.is_some());
        assert!(resolved.unwrap().to_str().unwrap().ends_with(".svg"));
    }

    #[test]
    fn resolve_logo_light_variant() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("test_light.svg"), r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10" fill="white"/></svg>"#).unwrap();
        std::fs::write(dir.path().join("test.svg"), r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10" fill="black"/></svg>"#).unwrap();

        let resolved = resolve_logo_file(Some(dir.path()), "test", true); // dark background → light logo
        assert!(resolved.unwrap().to_str().unwrap().contains("_light"));
    }
}
```

- [ ] **Step 3: テスト実行で失敗を確認**

Run: `cargo test -p picture-tool-core exif_frame::logo::tests -- --nocapture`
Expected: FAIL

- [ ] **Step 4: logo.rs を実装**

`core/src/exif_frame/logo.rs`:
```rust
use anyhow::{Context, Result};
use image::DynamicImage;
use rust_embed::Embed;
use std::path::{Path, PathBuf};

/// バンドルロゴのみを埋め込む
#[derive(Embed)]
#[folder = "assets/logos/"]
struct LogoAssets;

/// バンドルロゴからDynamicImageを読み込み
pub fn load_bundled_logo(filename: &str, target_size: u32) -> Result<DynamicImage> {
    let data = LogoAssets::get(filename)
        .with_context(|| format!("bundled logo not found: {}", filename))?;
    let ext = filename.rsplit('.').next().unwrap_or("");
    match ext {
        "svg" => load_svg_from_bytes(&data.data, target_size),
        _ => {
            let img = image::load_from_memory(&data.data)
                .context("failed to decode bundled logo")?;
            Ok(img.resize(target_size, target_size, image::imageops::FilterType::Lanczos3))
        }
    }
}

/// ロゴファイルを読み込み、指定サイズにリサイズ（アスペクト比維持）
pub fn load_logo_file(path: &Path, target_size: u32) -> Result<DynamicImage> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let img = match ext.to_lowercase().as_str() {
        "svg" => load_svg(path, target_size)?,
        _ => {
            let img = image::open(path).context("failed to open logo file")?;
            img.resize(target_size, target_size, image::imageops::FilterType::Lanczos3)
        }
    };
    Ok(img)
}

fn load_svg(path: &Path, target_size: u32) -> Result<DynamicImage> {
    let svg_data = std::fs::read(path)?;
    load_svg_from_bytes(&svg_data, target_size)
}

fn load_svg_from_bytes(svg_data: &[u8], target_size: u32) -> Result<DynamicImage> {
    let options = usvg::Options::default();
    let tree = usvg::Tree::from_data(&svg_data, &options)?;
    let size = tree.size();
    let scale = target_size as f32 / size.width().max(size.height());
    let width = (size.width() * scale) as u32;
    let height = (size.height() * scale) as u32;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)
        .context("failed to create pixmap")?;
    let transform = resvg::tiny_skia::Transform::from_scale(scale, scale);
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    let rgba = image::RgbaImage::from_raw(width, height, pixmap.data().to_vec())
        .context("failed to create image from pixmap")?;
    Ok(DynamicImage::ImageRgba8(rgba))
}

/// ロゴファイルのパスを解決（SVG優先、lightバリアント対応）
/// base_name: 拡張子なしのファイル名（例: "sony"）
/// use_light: trueなら "{base_name}_light" バリアントを優先
pub fn resolve_logo_file(
    dir: Option<&Path>,
    base_name: &str,
    use_light: bool,
) -> Option<PathBuf> {
    let dir = dir?;
    if !dir.exists() {
        return None;
    }

    let candidates = if use_light {
        vec![
            format!("{}_light.svg", base_name),
            format!("{}_light.png", base_name),
            format!("{}.svg", base_name),
            format!("{}.png", base_name),
        ]
    } else {
        vec![
            format!("{}.svg", base_name),
            format!("{}.png", base_name),
        ]
    };

    for candidate in candidates {
        let path = dir.join(&candidate);
        if path.exists() {
            return Some(path);
        }
    }
    None
}

/// ロゴを解決（ユーザーディレクトリ優先 → バンドルフォールバック）
/// DynamicImage を直接返す
pub fn resolve_and_load_logo(
    user_dir: Option<&Path>,
    filename: &str,
    use_light: bool,
    target_size: u32,
) -> Option<DynamicImage> {
    let base_name = filename.trim_end_matches(".svg").trim_end_matches(".png");

    // 1. ユーザーディレクトリから検索
    if let Some(path) = resolve_logo_file(user_dir, base_name, use_light) {
        if let Ok(img) = load_logo_file(&path, target_size) {
            return Some(img);
        }
    }

    // 2. バンドルロゴからフォールバック
    let candidates = if use_light {
        vec![
            format!("{}_light.svg", base_name),
            format!("{}_light.png", base_name),
            format!("{}.svg", base_name),
            format!("{}.png", base_name),
        ]
    } else {
        vec![
            format!("{}.svg", base_name),
            format!("{}.png", base_name),
        ]
    };
    for candidate in candidates {
        if let Ok(img) = load_bundled_logo(&candidate, target_size) {
            return Some(img);
        }
    }
    None
}
```

- [ ] **Step 5: テスト実行で成功を確認**

Run: `cargo test -p picture-tool-core exif_frame::logo::tests -- --nocapture`
Expected: 4テストすべてPASS

- [ ] **Step 6: コミット**

```bash
git add core/src/exif_frame/logo.rs core/tests/fixtures/
git commit -m "feat: ロゴ読み込み（PNG/SVG対応、lightバリアント自動選択）"
```

---

## Task 6: テキスト描画

**Files:**
- Modify: `core/src/exif_frame/text.rs`
- Create: `core/assets/fonts/` (NotoSansJP-Regular.ttf は手動ダウンロード)
- Test: `core/src/exif_frame/text.rs` (インラインテスト)

- [ ] **Step 1: フォントファイルの準備**

Google Fonts から NotoSansJP-Regular.ttf をダウンロードして `core/assets/fonts/` に配置。

注: このステップは手動。`rust-embed` がビルド時にこのファイルを埋め込む。
ファイルサイズが大きい場合（数MB）、サブセット化を検討。

- [ ] **Step 2: テストを書く**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_bundled_font() {
        let font = load_font(None).unwrap();
        // フォントが読み込めることを確認（FontArc は Font トレイトを実装）
        let scaled = font.as_scaled(ab_glyph::PxScale::from(24.0));
        assert!(scaled.ascent() > 0.0);
    }

    #[test]
    fn measure_text_non_zero() {
        let font = load_font(None).unwrap();
        let width = measure_text_width(&font, 24.0, "Hello World");
        assert!(width > 0.0);
    }

    #[test]
    fn measure_empty_text() {
        let font = load_font(None).unwrap();
        let width = measure_text_width(&font, 24.0, "");
        assert_eq!(width, 0.0);
    }

    #[test]
    fn truncate_long_text() {
        let font = load_font(None).unwrap();
        let result = truncate_text(&font, 24.0, "This is a very long text that should be truncated", 100.0);
        assert!(result.ends_with("..."));
        let width = measure_text_width(&font, 24.0, &result);
        assert!(width <= 100.0 + 1.0); // 1px tolerance
    }

    #[test]
    fn draw_text_no_panic() {
        let mut img = image::RgbaImage::new(200, 50);
        let font = load_font(None).unwrap();
        draw_text_on_image(&mut img, &font, 16.0, "Test Text", 10, 10, image::Rgba([0, 0, 0, 255]));
        // パニックしないことを確認
    }
}
```

- [ ] **Step 3: テスト実行で失敗を確認**

Run: `cargo test -p picture-tool-core exif_frame::text::tests -- --nocapture`
Expected: FAIL

- [ ] **Step 4: text.rs を実装**

`core/src/exif_frame/text.rs`:
```rust
use ab_glyph::{Font, FontArc, PxScale, ScaleFont};
use anyhow::{Context, Result};
use image::{Rgba, RgbaImage};
use rust_embed::Embed;
use std::sync::OnceLock;

#[derive(Embed)]
#[folder = "assets/fonts/"]
struct FontAssets;

/// バンドルフォントのグローバルキャッシュ（一度だけ読み込み）
static BUNDLED_FONT: OnceLock<FontArc> = OnceLock::new();

/// フォントを読み込み。pathがNoneならバンドルフォント使用
/// FontArcは内部でArc<Vec<u8>>を保持するためメモリリークしない
pub fn load_font(path: Option<&str>) -> Result<FontArc> {
    match path {
        Some(p) => {
            let data = std::fs::read(p).context("failed to read font file")?;
            FontArc::try_from_vec(data).map_err(|_| anyhow::anyhow!("invalid font file"))
        }
        None => {
            let font = BUNDLED_FONT.get_or_init(|| {
                let font_file = FontAssets::iter()
                    .find(|f| f.ends_with(".ttf") || f.ends_with(".otf"))
                    .expect("no bundled font found");
                let data = FontAssets::get(&font_file)
                    .expect("failed to load bundled font");
                FontArc::try_from_vec(data.data.to_vec())
                    .expect("invalid bundled font")
            });
            Ok(font.clone())
        }
    }
}

/// テキスト幅を計測
pub fn measure_text_width(font: &FontArc, size: f32, text: &str) -> f32 {
    if text.is_empty() {
        return 0.0;
    }
    let scaled = font.as_scaled(PxScale::from(size));
    let mut width = 0.0;
    let mut prev = None;
    for ch in text.chars() {
        let glyph_id = scaled.glyph_id(ch);
        if let Some(prev_id) = prev {
            width += scaled.kern(prev_id, glyph_id);
        }
        width += scaled.h_advance(glyph_id);
        prev = Some(glyph_id);
    }
    width
}

/// テキストを指定幅に収まるよう切り詰め（"..."付加）
pub fn truncate_text(font: &FontArc, size: f32, text: &str, max_width: f32) -> String {
    let full_width = measure_text_width(font, size, text);
    if full_width <= max_width {
        return text.to_string();
    }
    let ellipsis = "...";
    let ellipsis_width = measure_text_width(font, size, ellipsis);
    let target_width = max_width - ellipsis_width;
    if target_width <= 0.0 {
        return ellipsis.to_string();
    }

    let mut result = String::new();
    let scaled = font.as_scaled(PxScale::from(size));
    let mut width = 0.0;
    let mut prev = None;
    for ch in text.chars() {
        let glyph_id = scaled.glyph_id(ch);
        if let Some(prev_id) = prev {
            width += scaled.kern(prev_id, glyph_id);
        }
        width += scaled.h_advance(glyph_id);
        if width > target_width {
            break;
        }
        result.push(ch);
        prev = Some(glyph_id);
    }
    result.push_str(ellipsis);
    result
}

/// 画像上にテキストを描画
pub fn draw_text_on_image(
    img: &mut RgbaImage,
    font: &FontArc,
    size: f32,
    text: &str,
    x: i32,
    y: i32,
    color: Rgba<u8>,
) {
    imageproc::drawing::draw_text_mut(
        img,
        color,
        x,
        y,
        PxScale::from(size),
        font,
        text,
    );
}
```

- [ ] **Step 5: テスト実行で成功を確認**

Run: `cargo test -p picture-tool-core exif_frame::text::tests -- --nocapture`
Expected: 5テストすべてPASS

- [ ] **Step 6: コミット**

```bash
git add core/src/exif_frame/text.rs core/assets/fonts/
git commit -m "feat: ab_glyph テキスト描画（計測・切り詰め・描画）"
```

---

## Task 7: レイアウトエンジン

**Files:**
- Modify: `core/src/exif_frame/layout.rs`
- Test: `core/src/exif_frame/layout.rs` (インラインテスト)

- [ ] **Step 1: テストを書く**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::exif_frame::*;

    fn test_config(layout: FrameLayout) -> ExifFrameConfig {
        ExifFrameConfig {
            layout,
            frame_padding: 0.05,
            ..ExifFrameConfig::default()
        }
    }

    #[test]
    fn bottom_bar_dimensions() {
        let config = test_config(FrameLayout::BottomBar);
        let dims = calculate_frame_dimensions(1000, 1250, &config);
        // 短辺1000 × padding 0.05 = 50px のフレーム高さ
        assert_eq!(dims.frame_height, 50);
        assert_eq!(dims.total_width, 1000);
        assert_eq!(dims.total_height, 1250 + 50);
        assert_eq!(dims.photo_x, 0);
        assert_eq!(dims.photo_y, 0);
    }

    #[test]
    fn sidebar_dimensions() {
        let config = test_config(FrameLayout::SideBar);
        let dims = calculate_frame_dimensions(1000, 1250, &config);
        let sidebar_width = (1000.0 * 0.05 * 2.0) as u32; // サイドバーは幅方向
        assert_eq!(dims.total_width, 1000 + sidebar_width);
        assert_eq!(dims.total_height, 1250);
    }

    #[test]
    fn full_border_dimensions() {
        let config = test_config(FrameLayout::FullBorder);
        let dims = calculate_frame_dimensions(1000, 1250, &config);
        let padding = 50; // 短辺1000 × 0.05
        // 上下左右にパディング + 下部にEXIF情報エリア
        assert_eq!(dims.total_width, 1000 + padding * 2);
        assert!(dims.total_height > 1250 + padding * 2); // EXIF分が追加
    }

    #[test]
    fn fixed_aspect_ratio_adjustment() {
        let config = ExifFrameConfig {
            aspect_ratio: OutputAspectRatio::Fixed(4, 5),
            ..test_config(FrameLayout::BottomBar)
        };
        let dims = calculate_frame_dimensions(1000, 1250, &config);
        let ratio = dims.total_width as f64 / dims.total_height as f64;
        let target = 4.0 / 5.0;
        assert!((ratio - target).abs() < 0.01,
            "Expected ratio ~{}, got {}", target, ratio);
    }

    #[test]
    fn free_aspect_ratio_no_adjustment() {
        let config = ExifFrameConfig {
            aspect_ratio: OutputAspectRatio::Free,
            ..test_config(FrameLayout::BottomBar)
        };
        let dims = calculate_frame_dimensions(1000, 1250, &config);
        // Freeなら写真幅と同じ
        assert_eq!(dims.total_width, 1000);
    }

    #[test]
    fn minimum_image_skips_frame() {
        let config = test_config(FrameLayout::BottomBar);
        // 短辺200px未満はフレームスキップ
        let dims = calculate_frame_dimensions(150, 200, &config);
        assert!(dims.skip_frame);
    }
}
```

- [ ] **Step 2: テスト実行で失敗を確認**

Run: `cargo test -p picture-tool-core exif_frame::layout::tests -- --nocapture`
Expected: FAIL

- [ ] **Step 3: layout.rs を実装**

`core/src/exif_frame/layout.rs`:
```rust
use crate::exif_frame::{ExifFrameConfig, FrameLayout, OutputAspectRatio};

/// レイアウト計算結果
#[derive(Debug)]
pub struct FrameDimensions {
    /// 最終出力幅
    pub total_width: u32,
    /// 最終出力高さ
    pub total_height: u32,
    /// 写真の配置X座標
    pub photo_x: u32,
    /// 写真の配置Y座標
    pub photo_y: u32,
    /// フレーム領域の高さ（BottomBar, FullBorder用）
    pub frame_height: u32,
    /// フレーム領域の幅（SideBar用）
    pub frame_width: u32,
    /// ロゴの配置座標とサイズ
    pub logo_x: u32,
    pub logo_y: u32,
    pub logo_size: u32,
    /// テキストの配置座標
    pub primary_text_x: u32,
    pub primary_text_y: u32,
    pub secondary_text_x: u32,
    pub secondary_text_y: u32,
    /// フレーム付加をスキップすべきか
    pub skip_frame: bool,
}

const MIN_SHORT_SIDE: u32 = 200;

/// レイアウトに応じたフレーム寸法を計算
pub fn calculate_frame_dimensions(
    photo_width: u32,
    photo_height: u32,
    config: &ExifFrameConfig,
) -> FrameDimensions {
    let short_side = photo_width.min(photo_height);

    // 極小画像チェック
    if short_side < MIN_SHORT_SIDE {
        return FrameDimensions {
            total_width: photo_width,
            total_height: photo_height,
            photo_x: 0,
            photo_y: 0,
            frame_height: 0,
            frame_width: 0,
            logo_x: 0,
            logo_y: 0,
            logo_size: 0,
            primary_text_x: 0,
            primary_text_y: 0,
            secondary_text_x: 0,
            secondary_text_y: 0,
            skip_frame: true,
        };
    }

    let padding = (short_side as f32 * config.frame_padding) as u32;

    let (mut total_w, mut total_h, photo_x, photo_y, frame_h, frame_w) = match config.layout {
        FrameLayout::BottomBar => {
            let frame_h = padding;
            (photo_width, photo_height + frame_h, 0, 0, frame_h, 0)
        }
        FrameLayout::SideBar => {
            let frame_w = padding * 2; // サイドバーは幅方向に2倍
            (photo_width + frame_w, photo_height, 0, 0, 0, frame_w)
        }
        FrameLayout::FullBorder => {
            let frame_h = padding; // 下部EXIF領域
            (
                photo_width + padding * 2,
                photo_height + padding * 2 + frame_h,
                padding,
                padding,
                frame_h,
                0,
            )
        }
    };

    // アスペクト比調整
    if let OutputAspectRatio::Fixed(target_w, target_h) = config.aspect_ratio {
        let target_ratio = target_w as f64 / target_h as f64;
        let current_ratio = total_w as f64 / total_h as f64;
        if current_ratio > target_ratio {
            // 幅が広すぎる → 高さを増やす
            total_h = (total_w as f64 / target_ratio) as u32;
        } else {
            // 高さが高すぎる → 幅を増やす
            total_w = (total_h as f64 * target_ratio) as u32;
        }
    }

    // ロゴ・テキスト配置（レイアウト依存）
    let logo_size = (frame_h.max(frame_w) as f32 * 0.6) as u32;
    let (logo_x, logo_y) = match config.layout {
        FrameLayout::BottomBar => {
            (padding / 2, photo_height + (frame_h.saturating_sub(logo_size)) / 2)
        }
        FrameLayout::SideBar => {
            let center_x = photo_width + (frame_w.saturating_sub(logo_size)) / 2;
            (center_x, padding)
        }
        FrameLayout::FullBorder => {
            (padding + padding / 2, photo_height + padding * 2 + (frame_h.saturating_sub(logo_size)) / 2)
        }
    };

    let text_offset_x = logo_x + logo_size + padding / 2;
    let (primary_text_x, primary_text_y, secondary_text_x, secondary_text_y) = match config.layout {
        FrameLayout::BottomBar | FrameLayout::FullBorder => {
            (text_offset_x, logo_y, text_offset_x, logo_y + logo_size / 2)
        }
        FrameLayout::SideBar => {
            let x = photo_width + frame_w / 4;
            (x, logo_y + logo_size + padding / 2, x, logo_y + logo_size + padding)
        }
    };

    FrameDimensions {
        total_width: total_w,
        total_height: total_h,
        photo_x,
        photo_y,
        frame_height: frame_h,
        frame_width: frame_w,
        logo_x,
        logo_y,
        logo_size,
        primary_text_x,
        primary_text_y,
        secondary_text_x,
        secondary_text_y,
        skip_frame: false,
    }
}
```

注: テキスト・ロゴの正確な配置は後のタスクで微調整する。ここでは基本的な座標計算のフレームワークを構築する。

- [ ] **Step 4: テスト実行で成功を確認**

Run: `cargo test -p picture-tool-core exif_frame::layout::tests -- --nocapture`
Expected: 6テストすべてPASS

- [ ] **Step 5: コミット**

```bash
git add core/src/exif_frame/layout.rs
git commit -m "feat: 3レイアウトの座標計算エンジン"
```

---

## Task 8: render_exif_frame() メイン関数

**Files:**
- Modify: `core/src/exif_frame/mod.rs`
- Test: 統合テスト

- [ ] **Step 1: 統合テストを書く**

`core/src/exif_frame/mod.rs` のテストモジュールに追加:
```rust
#[test]
fn render_bottom_bar_white() {
    let img = DynamicImage::ImageRgba8(RgbaImage::from_pixel(800, 1000, Rgba([128, 128, 128, 255])));
    let exif = ExifInfo {
        camera_make: Some("SONY".to_string()),
        camera_model: Some("ILCE-7M4".to_string()),
        lens_model: Some("FE 24-70mm f/2.8 GM II".to_string()),
        focal_length: Some("35mm".to_string()),
        f_number: Some("f/2.8".to_string()),
        shutter_speed: Some("1/250s".to_string()),
        iso: Some(400),
        date_taken: None,
    };
    let config = ExifFrameConfig::default(); // BottomBar, White, 4:5
    let asset_dirs = AssetDirs::default();

    let result = render_exif_frame(&img, &exif, &config, &asset_dirs);
    assert!(result.is_ok());
    let output = result.unwrap();
    // 出力画像が元画像より大きい
    assert!(output.height() > img.height());
    // 4:5比率
    let ratio = output.width() as f64 / output.height() as f64;
    assert!((ratio - 0.8).abs() < 0.02);
}

#[test]
fn render_with_all_none_exif() {
    let img = DynamicImage::ImageRgba8(RgbaImage::from_pixel(800, 1000, Rgba([128, 128, 128, 255])));
    let exif = ExifInfo::default(); // 全フィールドNone
    let config = ExifFrameConfig::default();
    let asset_dirs = AssetDirs::default();

    let result = render_exif_frame(&img, &exif, &config, &asset_dirs);
    assert!(result.is_ok()); // クラッシュしない
}

#[test]
fn render_skips_for_tiny_image() {
    let img = DynamicImage::ImageRgba8(RgbaImage::from_pixel(100, 150, Rgba([128, 128, 128, 255])));
    let exif = ExifInfo::default();
    let config = ExifFrameConfig::default();
    let asset_dirs = AssetDirs::default();

    let result = render_exif_frame(&img, &exif, &config, &asset_dirs);
    assert!(result.is_ok());
    let output = result.unwrap();
    // 極小画像はフレームなし → サイズ変わらず
    assert_eq!(output.width(), 100);
    assert_eq!(output.height(), 150);
}
```

- [ ] **Step 2: テスト実行で失敗を確認**

Run: `cargo test -p picture-tool-core render_bottom_bar -- --nocapture`
Expected: FAIL（`render_exif_frame` 未定義）

- [ ] **Step 3: render_exif_frame() を実装**

`core/src/exif_frame/mod.rs` に追加:
```rust
use anyhow::Result;
use image::{DynamicImage, RgbaImage, Rgba};
use crate::ExifInfo;
use crate::model_map::ModelMap;

/// Exifフレーム付き画像を生成
pub fn render_exif_frame(
    image: &DynamicImage,
    exif: &ExifInfo,
    config: &ExifFrameConfig,
    asset_dirs: &AssetDirs,
) -> Result<DynamicImage> {
    let (photo_w, photo_h) = (image.width(), image.height());

    // レイアウト計算
    let dims = layout::calculate_frame_dimensions(photo_w, photo_h, config);
    if dims.skip_frame {
        return Ok(image.clone());
    }

    // モデルマッピング読み込み
    let mut model_map = ModelMap::load_bundled();
    if let Some(ref map_path) = asset_dirs.user_model_map {
        if map_path.exists() {
            if let Ok(custom_json) = std::fs::read_to_string(map_path) {
                let _ = model_map.merge_custom(&custom_json);
            }
        }
    }

    // キャンバス生成
    let bg_color = config.color.to_rgba();
    let mut canvas = RgbaImage::from_pixel(dims.total_width, dims.total_height, bg_color);

    // 写真配置
    image::imageops::overlay(
        &mut canvas,
        &image.to_rgba8(),
        dims.photo_x as i64,
        dims.photo_y as i64,
    );

    // ロゴ描画（ユーザーディレクトリ → バンドルフォールバック）
    if let Some(ref make) = exif.camera_make {
        let use_light = config.color.is_dark();
        if let Some(logo_entry) = model_map.maker_logo(make) {
            if config.items.maker_logo {
                if let Some(logo_img) = logo::resolve_and_load_logo(
                    asset_dirs.user_logos_dir.as_deref(),
                    &logo_entry.maker,
                    use_light,
                    dims.logo_size,
                ) {
                    image::imageops::overlay(&mut canvas, &logo_img.to_rgba8(), dims.logo_x as i64, dims.logo_y as i64);
                }
            }
        }
    }

    // テキスト描画
    // FontArcは内部でArcを持つためcloneは軽量。バンドルフォントはOnceLockキャッシュ済み
    let font = text::load_font(config.font.font_path.as_deref())
        .unwrap_or_else(|_| text::load_font(None).expect("bundled font must exist"));

    let short_side = photo_w.min(photo_h);
    let primary_size = short_side as f32 * config.font.primary_size;
    let secondary_size = short_side as f32 * config.font.secondary_size;
    let text_color = if config.color.is_dark() {
        Rgba([255, 255, 255, 255])
    } else {
        Rgba([51, 51, 51, 255])
    };
    let secondary_text_color = if config.color.is_dark() {
        Rgba([170, 170, 170, 255])
    } else {
        Rgba([136, 136, 136, 255])
    };

    // プライマリテキスト（カメラ + レンズ）
    let primary_text = build_primary_text(exif, &model_map, &config.items);
    if !primary_text.is_empty() {
        let max_width = (dims.total_width - dims.primary_text_x - 10) as f32;
        let truncated = text::truncate_text(&font, primary_size, &primary_text, max_width);
        text::draw_text_on_image(&mut canvas, &font, primary_size, &truncated, dims.primary_text_x as i32, dims.primary_text_y as i32, text_color);
    }

    // セカンダリテキスト（撮影パラメータ）
    let secondary_text = build_secondary_text(exif, &config.items, &config.custom_text);
    if !secondary_text.is_empty() {
        let max_width = (dims.total_width - dims.secondary_text_x - 10) as f32;
        let truncated = text::truncate_text(&font, secondary_size, &secondary_text, max_width);
        text::draw_text_on_image(&mut canvas, &font, secondary_size, &truncated, dims.secondary_text_x as i32, dims.secondary_text_y as i32, secondary_text_color);
    }

    Ok(DynamicImage::ImageRgba8(canvas))
}

fn build_primary_text(exif: &ExifInfo, model_map: &ModelMap, items: &DisplayItems) -> String {
    let mut parts = Vec::new();
    if items.camera_model {
        if let Some(ref model) = exif.camera_model {
            parts.push(model_map.camera_display_name(model).to_string());
        }
    }
    if items.lens_model {
        if let Some(ref lens) = exif.lens_model {
            parts.push(lens.clone());
        }
    }
    parts.join(" | ")
}

fn build_secondary_text(exif: &ExifInfo, items: &DisplayItems, custom_text: &str) -> String {
    let mut parts = Vec::new();
    if items.focal_length {
        if let Some(ref v) = exif.focal_length { parts.push(v.clone()); }
    }
    if items.f_number {
        if let Some(ref v) = exif.f_number { parts.push(v.clone()); }
    }
    if items.shutter_speed {
        if let Some(ref v) = exif.shutter_speed { parts.push(v.clone()); }
    }
    if items.iso {
        if let Some(v) = exif.iso { parts.push(format!("ISO {}", v)); }
    }
    if items.date_taken {
        if let Some(ref v) = exif.date_taken { parts.push(v.clone()); }
    }
    if items.custom_text && !custom_text.is_empty() {
        parts.push(custom_text.to_string());
    }
    parts.join("  ")
}
```

- [ ] **Step 4: テスト実行で成功を確認**

Run: `cargo test -p picture-tool-core exif_frame::tests -- --nocapture`
Expected: 全テストPASS（Task 2のテスト + 新しい3テスト）

- [ ] **Step 5: コミット**

```bash
git add core/src/exif_frame/mod.rs
git commit -m "feat: render_exif_frame() メイン関数（ロゴ・テキスト合成）"
```

---

## Task 9: 既存パイプラインへの統合

**Files:**
- Modify: `core/src/lib.rs` — `process_image()`, `process_batch()` シグネチャ変更
- Modify: `gui/src/commands.rs` — `process_images` コマンド更新
- Modify: `cli/src/main.rs` — 呼び出し更新

- [ ] **Step 1: テストを書く — パイプライン統合**

`core/src/lib.rs` テストセクションに追加:
```rust
#[test]
fn process_image_with_exif_frame() {
    let dir = tempfile::TempDir::new().unwrap();
    let input = create_test_image(dir.path(), "test.jpg", 800, 1000);
    let config = ProcessingConfig {
        mode: ConversionMode::Quality,
        bg_color: BackgroundColor::White,
        quality: 90,
        max_size_mb: 8,
        delete_originals: false,
    };
    let frame_config = exif_frame::ExifFrameConfig::default();
    let asset_dirs = exif_frame::AssetDirs::default();

    let result = process_image(&input, dir.path(), &config, Some(&frame_config), Some(&asset_dirs));
    assert!(result.is_ok());
}

#[test]
fn process_image_without_exif_frame_unchanged() {
    // exif_frame=Noneの場合、既存動作と完全同一
    let dir = tempfile::TempDir::new().unwrap();
    let input = create_test_image(dir.path(), "test.jpg", 800, 1000);
    let config = ProcessingConfig {
        mode: ConversionMode::Quality,
        bg_color: BackgroundColor::White,
        quality: 90,
        max_size_mb: 8,
        delete_originals: false,
    };

    let result = process_image(&input, dir.path(), &config, None, None);
    assert!(result.is_ok());
}
```

注: `create_test_image` ヘルパーが既存テストにある場合はそれを使用。なければテスト用ヘルパーを作成。

- [ ] **Step 2: テスト実行で失敗を確認**

Run: `cargo test -p picture-tool-core process_image_with_exif -- --nocapture`
Expected: FAIL（シグネチャ不一致）

- [ ] **Step 3: process_image() のシグネチャを変更**

`core/src/lib.rs` L196 付近を修正:

```rust
pub fn process_image(
    input_path: &Path,
    output_folder: &Path,
    config: &ProcessingConfig,
    exif_frame_config: Option<&exif_frame::ExifFrameConfig>,
    asset_dirs: Option<&exif_frame::AssetDirs>,
) -> Result<ProcessResult> {
    let img = image::open(input_path)
        .with_context(|| format!("Failed to open image: {}", input_path.display()))?;

    // 既存変換（既存コードの所有権パターンに合わせる）
    let converted = match config.mode {
        ConversionMode::Crop => convert_aspect_ratio_crop(img),
        ConversionMode::Pad => convert_aspect_ratio_pad(img, config.bg_color),
        ConversionMode::Quality => img,
    };

    // Exifフレーム付加（オプション）
    // EXIF読み取り失敗でもフレーム生成は続行（空テキストで描画）
    let framed = if let (Some(fc), Some(ad)) = (exif_frame_config, asset_dirs) {
        let exif = read_exif_info(input_path).unwrap_or_default();
        match exif_frame::render_exif_frame(&converted, &exif, fc, ad) {
            Ok(result) => result,
            Err(e) => {
                eprintln!("Warning: Exif frame rendering failed for {}: {}", input_path.display(), e);
                converted // フレームなしで続行
            }
        }
    } else {
        converted
    };

    // 出力パス生成 & サイズ制限保存
    let output_path = generate_output_path(output_folder, input_path)?;
    let max_size_bytes = config.max_size_mb * 1024 * 1024;
    let (file_size, final_quality) = save_with_size_limit(&framed, &output_path, config.quality, max_size_bytes)?;

    // 元ファイル削除
    if config.delete_originals {
        std::fs::remove_file(input_path)?;
    }

    Ok(ProcessResult {
        input_path: input_path.to_path_buf(),
        output_path,
        file_size,
        quality: final_quality,
    })
}
```

- [ ] **Step 4: process_batch() のシグネチャを変更**

同様に `process_batch()` を修正。`ExifFrameConfig` と `AssetDirs` は `Sync` を実装するため（全フィールドが `Sync`）、`&T` 参照を `rayon::par_iter()` のクロージャ内で安全にキャプチャできる:

```rust
pub fn process_batch(
    files: &[PathBuf],
    output_folder: &Path,
    config: &ProcessingConfig,
    exif_frame_config: Option<&exif_frame::ExifFrameConfig>,
    asset_dirs: Option<&exif_frame::AssetDirs>,
    on_progress: Option<ProgressCallback>,
) -> Vec<Result<ProcessResult>> {
    // exif_frame_config と asset_dirs は &T で Sync なので par_iter クロージャで共有可能
    let results: Vec<_> = files
        .par_iter()
        .enumerate()
        .map(|(i, file)| {
            // 既存のキャンセルチェック・進捗コールバック...

            // process_image に追加引数を渡す
            let result = process_image(
                file,
                output_folder,
                config,
                exif_frame_config, // &ExifFrameConfig は Sync
                asset_dirs,        // &AssetDirs は Sync
            );

            // 進捗通知...
            result
        })
        .collect();
    results
}
```

- [ ] **Step 5: 既存の呼び出し元を更新**

**gui/src/commands.rs** — `process_images` コマンド:
```rust
#[tauri::command]
pub async fn process_images(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    files: Vec<String>,
    output_folder: String,
    config: core::ProcessingConfig,
    exif_frame_config: Option<core::exif_frame::ExifFrameConfig>,
) -> Result<Vec<core::ProcessResult>, String> {
    let asset_dirs = core::exif_frame::AssetDirs::default();
    // ... spawn_blocking内で
    // core::process_batch(&paths, &output, &config, exif_frame_config.as_ref(), Some(&asset_dirs), ...)
}
```

**cli/src/main.rs** — main():
```rust
let results = core::process_batch(
    &files, &args.output, &config,
    None, // TODO: Task 10でCLI対応
    None,
    Some(progress_callback),
);
```

- [ ] **Step 6: 全テスト実行**

Run: `cargo test --workspace`
Expected: 全テストPASS（既存テストも含め壊れない）

- [ ] **Step 7: コミット**

```bash
git add core/src/lib.rs gui/src/commands.rs cli/src/main.rs
git commit -m "feat: process_image/process_batch にExifフレーム統合"
```

---

## Task 10: CLIオプション追加

**Files:**
- Modify: `cli/Cargo.toml`
- Modify: `cli/src/main.rs`

- [ ] **Step 1: cli/Cargo.toml に依存追加**

```toml
[dependencies]
picture-tool-core = { path = "../core" }
clap = { version = "4.5", features = ["derive"] }
anyhow = "1.0"
serde_json = "1.0"
dirs = "5"
```

- [ ] **Step 2: Args構造体にExifフレームオプション追加**

`cli/src/main.rs` の `Args` 構造体に追加:
```rust
/// Exifフレームを付加
#[arg(short = 'e', long, default_value = "false")]
exif_frame: bool,

/// プリセット名
#[arg(short, long, default_value = "default")]
preset: String,

/// プリセットJSONファイル直接指定
#[arg(long)]
preset_file: Option<PathBuf>,

/// カスタムテキスト（プリセットの値を上書き）
#[arg(long, default_value = "")]
custom_text: String,
```

- [ ] **Step 3: main() でExifフレーム設定を構築**

```rust
let (exif_frame_config, asset_dirs) = if args.exif_frame {
    let config = if let Some(ref path) = args.preset_file {
        let json = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read preset file: {}", path.display()))?;
        serde_json::from_str::<core::exif_frame::ExifFrameConfig>(&json)?
    } else {
        // プリセット名で検索
        let presets_dir = dirs::config_dir()
            .map(|d| d.join("picture-tool/presets"));
        let all = core::exif_frame::preset::list_all_presets(presets_dir.as_deref());
        all.into_iter()
            .find(|p| p.name == args.preset)
            .unwrap_or_else(|| {
                eprintln!("Preset '{}' not found, using default", args.preset);
                core::exif_frame::ExifFrameConfig::default()
            })
    };

    let mut config = config;
    if !args.custom_text.is_empty() {
        config.custom_text = args.custom_text.clone();
        config.items.custom_text = true;
    }

    (Some(config), Some(core::exif_frame::AssetDirs::default()))
} else {
    (None, None)
};

let results = core::process_batch(
    &files, &args.output, &config,
    exif_frame_config.as_ref(),
    asset_dirs.as_ref(),
    Some(progress_callback),
);
```

- [ ] **Step 4: ビルドとヘルプ確認**

Run: `cargo build -p picture-tool-cli && ./target/debug/picture-tool --help`
Expected: `--exif-frame`, `--preset`, `--preset-file`, `--custom-text` が表示される

- [ ] **Step 5: コミット**

```bash
git add cli/Cargo.toml cli/src/main.rs
git commit -m "feat: CLI に --exif-frame, --preset, --custom-text オプション追加"
```

---

## Task 11: GUI — TypeScript型定義とAPI関数

**Files:**
- Modify: `gui-frontend/src/lib/types.ts`
- Modify: `gui-frontend/src/lib/api.ts`

- [ ] **Step 1: types.ts にExifフレーム型を追加**

```typescript
// Exif Frame types
export type FrameLayout = "bottom_bar" | "side_bar" | "full_border";

export type FrameColor =
  | "white"
  | "black"
  | { custom: [number, number, number] };

export type OutputAspectRatio = { fixed: [number, number] } | "free";

export interface DisplayItems {
  maker_logo: boolean;
  brand_logo: boolean;
  lens_brand_logo: boolean;
  camera_model: boolean;
  lens_model: boolean;
  focal_length: boolean;
  f_number: boolean;
  shutter_speed: boolean;
  iso: boolean;
  date_taken: boolean;
  custom_text: boolean;
}

export interface FontConfig {
  font_path: string | null;
  primary_size: number;
  secondary_size: number;
}

export interface ExifFrameConfig {
  name: string;
  layout: FrameLayout;
  color: FrameColor;
  aspect_ratio: OutputAspectRatio;
  items: DisplayItems;
  font: FontConfig;
  custom_text: string;
  frame_padding: number;
}

export interface FontInfo {
  display_name: string;
  path: string | null;
  is_bundled: boolean;
}

export interface LogoInfo {
  filename: string;
  matched_to: string | null;
  is_bundled: boolean;
}
```

- [ ] **Step 2: api.ts にExifフレーム関連関数を追加**

```typescript
import type { ExifFrameConfig, FontInfo, LogoInfo } from "./types";

export async function renderExifFramePreview(
  path: string,
  config: ExifFrameConfig
): Promise<string> {
  return invoke("render_exif_frame_preview", { path, config });
}

export async function listPresets(): Promise<ExifFrameConfig[]> {
  return invoke("list_presets");
}

export async function savePreset(config: ExifFrameConfig): Promise<void> {
  return invoke("save_preset", { config });
}

export async function deletePreset(name: string): Promise<void> {
  return invoke("delete_preset", { name });
}

export async function listAvailableFonts(): Promise<FontInfo[]> {
  return invoke("list_available_fonts");
}

export async function listAvailableLogos(): Promise<LogoInfo[]> {
  return invoke("list_available_logos");
}
```

- [ ] **Step 3: processImages の型を更新**

既存の `processImages` 関数にExifフレーム設定を追加:
```typescript
export async function processImages(
  files: string[],
  outputFolder: string,
  config: ProcessingConfig,
  exifFrameConfig?: ExifFrameConfig | null
): Promise<ProcessResult[]> {
  return invoke("process_images", {
    files,
    outputFolder,
    config,
    exifFrameConfig: exifFrameConfig ?? null,
  });
}
```

- [ ] **Step 4: コミット**

```bash
git add gui-frontend/src/lib/types.ts gui-frontend/src/lib/api.ts
git commit -m "feat: Exifフレーム TypeScript型定義とAPI関数"
```

---

## Task 12: GUI — Tauriコマンド追加

**Files:**
- Modify: `gui/src/commands.rs`
- Modify: `gui/src/main.rs` (コマンドハンドラー登録)

- [ ] **Step 1: gui/Cargo.toml に依存追加**

```toml
# 既存の依存に追加
dirs = "5"
base64 = "0.22"
```

- [ ] **Step 2: 新しいTauriコマンドを実装**

`gui/src/commands.rs` に `use` 文を追加:
```rust
use picture_tool_core::exif_frame::{self, ExifFrameConfig, FontInfo, LogoInfo};
```

`gui/src/commands.rs` に追加:
```rust
#[tauri::command]
pub async fn render_exif_frame_preview(
    path: String,
    config: core::exif_frame::ExifFrameConfig,
) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        let path = std::path::Path::new(&path);
        let img = image::open(path).map_err(|e| e.to_string())?;

        // 低解像度にリサイズ（プレビュー用）
        let max_dim = 400u32;
        let thumbnail = img.resize(max_dim, max_dim, image::imageops::FilterType::Triangle);

        let exif = core::read_exif_info(path).unwrap_or_default();
        let asset_dirs = core::exif_frame::AssetDirs::default();

        let result = core::exif_frame::render_exif_frame(&thumbnail, &exif, &config, &asset_dirs)
            .map_err(|e| e.to_string())?;

        // base64エンコード
        let mut buf = Vec::new();
        let mut cursor = std::io::Cursor::new(&mut buf);
        result.write_to(&mut cursor, image::ImageOutputFormat::Jpeg(85))
            .map_err(|e| e.to_string())?;
        Ok(format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(&buf)))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn list_presets() -> Result<Vec<core::exif_frame::ExifFrameConfig>, String> {
    let presets_dir = dirs::config_dir()
        .map(|d| d.join("picture-tool/presets"));
    Ok(core::exif_frame::preset::list_all_presets(presets_dir.as_deref()))
}

#[tauri::command]
pub async fn save_preset(
    config: core::exif_frame::ExifFrameConfig,
) -> Result<(), String> {
    let presets_dir = dirs::config_dir()
        .ok_or("config dir not found")?
        .join("picture-tool/presets");
    core::exif_frame::preset::save_preset(&presets_dir, &config)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_preset(name: String) -> Result<(), String> {
    let presets_dir = dirs::config_dir()
        .ok_or("config dir not found")?
        .join("picture-tool/presets");
    core::exif_frame::preset::delete_preset(&presets_dir, &name)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_available_fonts() -> Result<Vec<core::exif_frame::FontInfo>, String> {
    // バンドルフォント + ユーザーフォントを一覧
    let mut fonts = vec![FontInfo {
        display_name: "Noto Sans JP (bundled)".to_string(),
        path: None,
        is_bundled: true,
    }];
    if let Some(user_dir) = dirs::config_dir().map(|d| d.join("picture-tool/assets/fonts")) {
        if user_dir.exists() {
            for entry in std::fs::read_dir(&user_dir).into_iter().flatten().flatten() {
                let path = entry.path();
                if path.extension().map_or(false, |e| e == "ttf" || e == "otf") {
                    fonts.push(core::exif_frame::FontInfo {
                        display_name: format!("User: {}", path.file_stem().unwrap_or_default().to_string_lossy()),
                        path: Some(path.to_string_lossy().to_string()),
                        is_bundled: false,
                    });
                }
            }
        }
    }
    Ok(fonts)
}

#[tauri::command]
pub async fn list_available_logos() -> Result<Vec<core::exif_frame::LogoInfo>, String> {
    // TODO: バンドルロゴ + ユーザーロゴを一覧
    Ok(vec![])
}
```

- [ ] **Step 3: gui/src/main.rs にコマンド登録**

`invoke_handler` のクロージャに新コマンドを追加:
```rust
.invoke_handler(tauri::generate_handler![
    // 既存コマンド...
    commands::render_exif_frame_preview,
    commands::list_presets,
    commands::save_preset,
    commands::delete_preset,
    commands::list_available_fonts,
    commands::list_available_logos,
])
```

- [ ] **Step 4: ビルド確認**

Run: `cd /home/biwak/myShrimp/picture-tool-rust && cargo build -p picture-tool-gui`
Expected: コンパイル成功

- [ ] **Step 5: コミット**

```bash
git add gui/Cargo.toml gui/src/commands.rs gui/src/main.rs
git commit -m "feat: Exifフレーム用 Tauri コマンド追加"
```

---

## Task 13: GUI — ExifFrameSettings.svelte モーダル

**Files:**
- Create: `gui-frontend/src/lib/ExifFrameSettings.svelte`

- [ ] **Step 1: モーダルコンポーネントを作成**

`gui-frontend/src/lib/ExifFrameSettings.svelte`:

Svelte 5 runes構文で実装。主要構造:
- 左側: スクロール設定エリア（プリセット選択、レイアウト、表示項目、アスペクト比、色、フォント、カスタムテキスト）
- 右側: ライブプレビュー（`renderExifFramePreview` をデバウンス呼び出し）
- 下部: キャンセル / 保存ボタン

Props:
```typescript
interface Props {
  visible: boolean;
  previewImagePath: string | null;
  onClose: () => void;
  onSave: (config: ExifFrameConfig) => void;
}
```

内部状態:
```typescript
let config = $state<ExifFrameConfig>(defaultConfig());
let presets = $state<ExifFrameConfig[]>([]);
let previewSrc = $state<string>("");
let loading = $state(false);
```

ライブプレビュー:
```typescript
let debounceTimer: number;
$effect(() => {
  // configの変更を監視
  const _ = JSON.stringify(config);
  clearTimeout(debounceTimer);
  debounceTimer = setTimeout(async () => {
    if (previewImagePath) {
      loading = true;
      try {
        previewSrc = await renderExifFramePreview(previewImagePath, config);
      } finally {
        loading = false;
      }
    }
  }, 300);
});
```

スタイル: 既存のCSS変数（`var(--bg-secondary)`, `var(--accent)` 等）を使用。
モーダルオーバーレイは `position: fixed; z-index: 1000;`。

このコンポーネントは最も大きく（200-400行程度）、UIの詳細はイテレーションで調整する。初期実装では基本構造と動作を優先する。

- [ ] **Step 2: ビルド確認**

Run: `cd /home/biwak/myShrimp/picture-tool-rust/gui-frontend && bun run check`
Expected: 型エラーなし

- [ ] **Step 3: コミット**

```bash
git add gui-frontend/src/lib/ExifFrameSettings.svelte
git commit -m "feat: ExifFrameSettings モーダルコンポーネント"
```

---

## Task 14: GUI — メイン画面統合

**Files:**
- Modify: `gui-frontend/src/lib/SettingsPanel.svelte`
- Modify: `gui-frontend/src/App.svelte`

- [ ] **Step 1: SettingsPanel にExifフレームトグルを追加**

`SettingsPanel.svelte` の設定セクション末尾（変換実行ボタンの前）に追加:

```svelte
<!-- Exif Frame toggle -->
<div class="setting-group">
  <label class="setting-label">
    <input type="checkbox" bind:checked={exifFrameEnabled} />
    Exifフレーム
  </label>
  {#if exifFrameEnabled}
    <div class="exif-frame-row">
      <select bind:value={selectedPresetName}>
        {#each presets as preset}
          <option value={preset.name}>{preset.name}</option>
        {/each}
      </select>
      <button class="icon-btn" onclick={() => showExifSettings = true} title="Exifフレーム設定">
        ⚙
      </button>
    </div>
  {/if}
</div>
```

Props に追加:
```typescript
exifFrameEnabled: boolean;
selectedPresetName: string;
presets: ExifFrameConfig[];
onOpenExifSettings: () => void;
```

- [ ] **Step 2: App.svelte にExifフレーム状態管理を追加**

```typescript
let exifFrameEnabled = $state(false);
let selectedPresetName = $state("default");
let exifFramePresets = $state<ExifFrameConfig[]>([]);
let showExifFrameSettings = $state(false);

// プリセット読み込み
$effect(() => {
  listPresets().then(presets => {
    exifFramePresets = presets;
  });
});

// 選択中のプリセット
let activeExifFrameConfig = $derived(
  exifFramePresets.find(p => p.name === selectedPresetName) ?? null
);
```

`processImages` 呼び出しを更新:
```typescript
const results = await processImages(
  files,
  outputFolder,
  config,
  exifFrameEnabled ? activeExifFrameConfig : null
);
```

モーダル表示:
```svelte
{#if showExifFrameSettings}
  <ExifFrameSettings
    visible={showExifFrameSettings}
    previewImagePath={selectedImages[0]?.path ?? null}
    onClose={() => showExifFrameSettings = false}
    onSave={async (config) => {
      await savePreset(config);
      exifFramePresets = await listPresets();
      showExifFrameSettings = false;
    }}
  />
{/if}
```

- [ ] **Step 3: ビルド確認**

Run: `cd /home/biwak/myShrimp/picture-tool-rust/gui-frontend && bun run check`
Expected: 型エラーなし

- [ ] **Step 4: コミット**

```bash
git add gui-frontend/src/lib/SettingsPanel.svelte gui-frontend/src/App.svelte
git commit -m "feat: メイン画面にExifフレーム ON/OFF + プリセット選択を統合"
```

---

## Task 15: ロゴアセットの準備

**Files:**
- Create: `core/assets/logos/` (各種SVG/PNGロゴ)

- [ ] **Step 1: ロゴファイルを収集・配置**

以下のロゴを `core/assets/logos/` に配置:
- `sony.svg` + `sony_light.svg` — Sony メーカーロゴ
- `alpha.svg` + `alpha_light.svg` — α ブランドロゴ
- `canon.svg` + `canon_light.svg` — Canon
- `nikon.svg` + `nikon_light.svg` — Nikon
- `fujifilm.svg` + `fujifilm_light.svg` — Fujifilm
- `sigma.svg` + `sigma_light.svg` — Sigma

注: ロゴは公式アセットからのトレースまたは簡易SVGとして作成。個人利用のためライセンスは自己責任。実際のロゴ取得は手動作業。プレースホルダーとして名前テキストの簡易SVGを最初に配置し、実ロゴは後から差し替える。

- [ ] **Step 2: プレースホルダーSVGを生成**

各メーカー用にシンプルなテキストSVGを作成（例）:
```svg
<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200" viewBox="0 0 200 200">
  <circle cx="100" cy="100" r="90" fill="#333" stroke="none"/>
  <text x="100" y="110" text-anchor="middle" fill="#fff" font-family="sans-serif" font-size="36" font-weight="bold">SONY</text>
</svg>
```

- [ ] **Step 3: ビルド確認（rust-embed がアセットを埋め込めるか）**

Run: `cargo build -p picture-tool-core`
Expected: 成功

- [ ] **Step 4: コミット**

```bash
git add core/assets/logos/
git commit -m "feat: メーカーロゴプレースホルダー（SVG）を追加"
```

---

## Task 16: E2Eテストと最終調整

**Files:**
- 全体テスト

- [ ] **Step 1: 全ワークスペーステスト実行**

Run: `make test`
Expected: 全テストPASS

- [ ] **Step 2: CLI E2Eテスト**

テスト用画像で実際にExifフレーム付き画像を生成:
```bash
cargo run -p picture-tool-cli -- \
  --input ./test_images \
  --output ./test_output \
  --mode crop \
  --exif-frame \
  --preset default
```
Expected: `test_output/` にフレーム付き画像が生成される

- [ ] **Step 3: GUI ビルドテスト**

Run: `make build-gui`
Expected: GUI含めたフルビルド成功

- [ ] **Step 4: GUIの動作確認**

Run: `make dev`
手動確認:
1. 設定パネルに「Exifフレーム」トグルが表示される
2. ONにするとプリセット選択と歯車アイコンが表示される
3. 歯車アイコンでモーダルが開く
4. モーダル内でプリセット設定変更→ライブプレビュー更新
5. 画像を選択して変換→Exifフレーム付き画像が出力される

- [ ] **Step 5: 最終コミット**

```bash
git add -A
git commit -m "feat: Exifフレーム機能の完成（E2E確認済み）"
```
