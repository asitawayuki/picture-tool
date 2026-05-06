# Exifフレーム v2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** ExifフレームをPadモード専用に限定し、Padの余白にExif情報を統合描画する。プレースホルダーロゴを実ロゴに置換し、型番直接表示に変更する。

**Architecture:** `core/src/exif_frame/` を全面書き直し。v1の3レイアウト（BottomBar/SideBar/FullBorder）を廃止し、Padの余白位置に応じて自動的にレイアウトを決定する。`process_image` のパイプラインでPad変換とExif描画を統合する。フロントエンドはPadモード時のみExifフレーム設定を表示するよう簡素化。

**Tech Stack:** Rust (image 0.24, ab_glyph 0.2, resvg 0.37, rust-embed 8), Svelte 5 (runes), Tauri v2

**Spec:** `docs/superpowers/specs/2026-03-29-exif-frame-v2-design.md`

---

## File Structure

### 変更するファイル

| ファイル | 責務 | 変更内容 |
|---------|------|---------|
| `core/src/exif_frame/mod.rs` | Config型 + render関数 | ExifFrameConfig v2化、render_exif_frame書き直し |
| `core/src/exif_frame/layout.rs` | レイアウト計算 | Pad余白統合レイアウト、画像縮小計算 |
| `core/src/exif_frame/text.rs` | テキスト描画 | 90度回転描画追加、オーバーフロー処理 |
| `core/src/exif_frame/logo.rs` | ロゴ読み込み | レンズブランドロゴ対応、インライン配置 |
| `core/src/exif_frame/preset.rs` | プリセット管理 | v2フォーマット対応 |
| `core/src/lib.rs` | パイプライン | Pad+Exif統合処理 |
| `core/assets/model_map.json` | ロゴマッチング | camera削除、簡素化 |
| `core/assets/logos/` | ロゴアセット | プレースホルダー削除、実ロゴ配置 |
| `core/assets/presets/` | デフォルトプリセット | v2形式に更新 |
| `gui/src/commands.rs` | Tauriコマンド | preview関数にbg_color追加 |
| `gui-frontend/src/lib/types.ts` | TS型定義 | v2 ExifFrameConfig |
| `gui-frontend/src/lib/api.ts` | API呼び出し | bg_colorパラメータ追加 |
| `gui-frontend/src/lib/ExifFrameSettings.svelte` | 設定モーダル | 簡素化 |
| `gui-frontend/src/lib/SettingsPanel.svelte` | サイドパネル | Padモード条件表示 |

### テストファイル

| ファイル | テスト対象 |
|---------|-----------|
| `core/src/exif_frame/layout.rs` 内 `#[cfg(test)]` | レイアウト計算 |
| `core/src/exif_frame/text.rs` 内 `#[cfg(test)]` | テキスト描画・回転 |
| `core/tests/exif_frame_integration.rs` | Pad+Exif統合 |

---

## Task 1: データモデル変更（ExifFrameConfig v2）

**Files:**
- Modify: `core/src/exif_frame/mod.rs:12-135`

このタスクではConfig型を v2 仕様に変更する。コンパイルを通すために依存するファイルも最小限修正する。

- [ ] **Step 1: ExifPosition enum を追加し、FrameLayout / FrameColor / OutputAspectRatio を削除**

`core/src/exif_frame/mod.rs` の先頭の型定義を以下に置き換え:

```rust
use serde::{Deserialize, Serialize};

// --- v1 の FrameLayout, FrameColor, OutputAspectRatio を削除 ---

/// Exif情報の配置位置
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ExifPosition {
    /// デフォルト: 横構図→下、縦構図→右
    Auto,
    Bottom,
    Top,
    Right,
    Left,
}

impl Default for ExifPosition {
    fn default() -> Self {
        Self::Auto
    }
}
```

- [ ] **Step 2: DisplayItems から brand_logo を削除**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayItems {
    pub maker_logo: bool,
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

impl Default for DisplayItems {
    fn default() -> Self {
        Self {
            maker_logo: true,
            lens_brand_logo: true,
            camera_model: true,
            lens_model: true,
            focal_length: true,
            f_number: true,
            shutter_speed: true,
            iso: true,
            date_taken: false,
            custom_text: false,
        }
    }
}
```

- [ ] **Step 3: ExifFrameConfig v2 に変更**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExifFrameConfig {
    pub name: String,
    pub position: ExifPosition,
    pub items: DisplayItems,
    pub font: FontConfig,
    pub custom_text: String,
}

impl Default for ExifFrameConfig {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            position: ExifPosition::Auto,
            items: DisplayItems::default(),
            font: FontConfig::default(),
            custom_text: String::new(),
        }
    }
}
```

FontConfig は既存のまま（font_path, primary_size, secondary_size）。

- [ ] **Step 4: render_exif_frame を一時的にスタブ化してコンパイルを通す**

`render_exif_frame` の本体をプレースホルダーにする（Task 6 で本実装）:

```rust
pub fn render_exif_frame(
    image: &DynamicImage,
    exif: &crate::ExifInfo,
    config: &ExifFrameConfig,
    bg_color: &crate::BackgroundColor,
    asset_dirs: &AssetDirs,
) -> Result<DynamicImage> {
    // TODO: Task 6 で実装。現時点では画像をそのまま返す
    Ok(image.clone())
}
```

注: シグネチャに `bg_color: &BackgroundColor` を追加（Padの背景色をフレーム色として使用するため）。

- [ ] **Step 5: build_primary_text と build_secondary_text を更新**

`build_primary_text` から display_name マッピングを削除し、型番直接表示に変更:

```rust
fn build_primary_text(exif: &crate::ExifInfo, items: &DisplayItems) -> String {
    let mut parts = Vec::new();
    if items.camera_model {
        if let Some(ref model) = exif.camera_model {
            parts.push(model.clone());
        }
    }
    if items.lens_model {
        if let Some(ref lens) = exif.lens_model {
            parts.push(lens.clone());
        }
    }
    parts.join(" | ")
}
```

`build_secondary_text` は既存ロジックをそのまま維持（focal_length, f_number, shutter_speed, iso, date_taken, custom_text を "  " で結合）。

- [ ] **Step 6: lib.rs の process_image 呼び出しを更新**

`core/src/lib.rs` の `process_image` 内で `render_exif_frame` の呼び出しに `&config.bg_color` を追加:

```rust
// lines 217-228 付近
if let (Some(ef_config), Some(dirs)) = (exif_frame_config, asset_dirs) {
    let exif = read_exif_info(input_path).unwrap_or_default();
    match exif_frame::render_exif_frame(&converted, &exif, ef_config, &config.bg_color, dirs) {
        Ok(framed) => converted = framed,
        Err(e) => {
            eprintln!("Warning: Exif frame failed: {}", e);
        }
    }
}
```

- [ ] **Step 7: ビルド確認**

Run: `cd /home/biwak/myShrimp/picture-tool-rust && cargo build 2>&1 | head -30`
Expected: コンパイル成功（warningは可）

- [ ] **Step 8: コミット**

```bash
git add core/src/exif_frame/mod.rs core/src/lib.rs
git commit -m "refactor: ExifFrameConfig v2 データモデル変更

- FrameLayout/FrameColor/OutputAspectRatio/frame_padding 削除
- ExifPosition enum 追加（Auto/Top/Bottom/Left/Right）
- DisplayItems から brand_logo 削除
- render_exif_frame に bg_color パラメータ追加
- build_primary_text から display_name マッピング削除（型番直接表示）"
```

---

## Task 2: ロゴアセット整理

**Files:**
- Delete: `core/assets/logos/*.svg` (18個のプレースホルダー)
- Create: `core/assets/logos/sony.svg`, `core/assets/logos/sony_light.svg`
- Create: `core/assets/logos/gmaster.png`, `core/assets/logos/gmaster_light.png`
- Modify: `core/assets/model_map.json`

- [ ] **Step 1: プレースホルダーSVG 18個を全削除**

```bash
rm core/assets/logos/sony.svg core/assets/logos/sony_light.svg
rm core/assets/logos/alpha.svg core/assets/logos/alpha_light.svg
rm core/assets/logos/canon.svg core/assets/logos/canon_light.svg
rm core/assets/logos/nikon.svg core/assets/logos/nikon_light.svg
rm core/assets/logos/fujifilm.svg core/assets/logos/fujifilm_light.svg
rm core/assets/logos/sigma.svg core/assets/logos/sigma_light.svg
rm core/assets/logos/gmaster.svg core/assets/logos/gmaster_light.svg
rm core/assets/logos/sony_g.svg core/assets/logos/sony_g_light.svg
rm core/assets/logos/sigma_art.svg core/assets/logos/sigma_art_light.svg
```

- [ ] **Step 2: 実ロゴを配置**

ユーザーが用意した実ロゴファイルを `core/assets/logos/` に配置:
- `sony.svg` — Sony メーカーロゴ（暗背景用）
- `sony_light.svg` — Sony メーカーロゴ（明背景用）
- `gmaster.png` — GM レンズブランドロゴ（暗背景用）
- `gmaster_light.png` — GM レンズブランドロゴ（明背景用）

注: `sony_logo.svg` が既にルートの `core/assets/` にある場合は `core/assets/logos/sony.svg` にコピー。`sony_gmaster.webp` は PNG に変換して `core/assets/logos/gmaster.png` に配置。light バリアントがない場合は同じファイルをコピーして `_light` 付きにする（暫定対応）。

```bash
# 既存ファイルの確認
ls core/assets/sony_logo.svg core/assets/sony_gmaster.webp 2>/dev/null

# Sony SVG をコピー
cp core/assets/sony_logo.svg core/assets/logos/sony.svg
cp core/assets/sony_logo.svg core/assets/logos/sony_light.svg

# GM WebP → PNG 変換（Pythonで変換、またはImageMagickがあれば convert コマンド）
# convert core/assets/sony_gmaster.webp core/assets/logos/gmaster.png
# cp core/assets/logos/gmaster.png core/assets/logos/gmaster_light.png
```

WebP→PNG 変換が必要。`convert`（ImageMagick）または Python PIL が使える環境で実行する。

- [ ] **Step 3: model_map.json を v2 用に簡素化**

`core/assets/model_map.json` を以下に置き換え:

```json
{
  "logo_match": {
    "SONY": { "maker": "sony.svg" },
    "Sony": { "maker": "sony.svg" },
    "Sony Corporation": { "maker": "sony.svg" }
  },
  "lens_brand_match": [
    { "pattern": "GM", "match_type": "contains", "logo": "gmaster.png" }
  ]
}
```

`camera` セクション（型番→表示名マッピング）と `brand` フィールドを削除。

- [ ] **Step 4: 古いルートのロゴファイルを削除**

```bash
rm -f core/assets/sony_logo.svg core/assets/sony_gmaster.webp
```

- [ ] **Step 5: ビルド確認**

Run: `cargo build 2>&1 | head -20`
Expected: コンパイル成功

- [ ] **Step 6: コミット**

```bash
git add -A core/assets/logos/ core/assets/model_map.json
git add core/assets/sony_logo.svg core/assets/sony_gmaster.webp  # 削除のステージ
git commit -m "refactor: プレースホルダーロゴを実ロゴに置換

- 18個のプレースホルダーSVGを全削除
- 実Sony SVGロゴとGM PNGロゴを配置
- model_map.json を簡素化（camera セクション削除、Sony/GMのみ）"
```

---

## Task 3: layout.rs 書き直し（Pad余白統合レイアウト）

**Files:**
- Modify: `core/src/exif_frame/layout.rs`

- [ ] **Step 1: テスト先行 — 横構図のレイアウト計算**

`core/src/exif_frame/layout.rs` の末尾に `#[cfg(test)]` モジュールを追加:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::BackgroundColor;

    fn default_config() -> (crate::exif_frame::ExifFrameConfig, BackgroundColor) {
        (crate::exif_frame::ExifFrameConfig::default(), BackgroundColor::Black)
    }

    #[test]
    fn landscape_photo_exif_at_bottom() {
        // 3:2 横構図 (1200x800) → 4:5 Pad → 下部にExif
        let (config, bg) = default_config();
        let result = calculate_pad_exif_layout(1200, 800, &config, &bg);

        // 4:5 出力
        assert_eq!(result.canvas_width * 5, result.canvas_height * 4,
            "Output must be 4:5 aspect ratio");
        // 写真は元サイズ以下
        assert!(result.photo_width <= 1200);
        assert!(result.photo_height <= 800);
        // Exifバーが下部にある
        assert!(result.exif_area_y > result.photo_y + result.photo_height - 1,
            "Exif area must be below the photo");
        assert!(!result.skip_exif);
    }

    #[test]
    fn portrait_photo_exif_at_right() {
        // 2:3 縦構図 (800x1200) → 4:5 Pad → 右側にExif
        let (config, bg) = default_config();
        let result = calculate_pad_exif_layout(800, 1200, &config, &bg);

        assert_eq!(result.canvas_width * 5, result.canvas_height * 4);
        assert!(result.exif_area_x > result.photo_x + result.photo_width - 1,
            "Exif area must be to the right of the photo");
        assert!(!result.skip_exif);
    }

    #[test]
    fn already_4_5_shrinks_photo() {
        // 既に4:5 (800x1000) → 余白なし → 画像縮小が発生
        let (config, bg) = default_config();
        let result = calculate_pad_exif_layout(800, 1000, &config, &bg);

        assert_eq!(result.canvas_width * 5, result.canvas_height * 4);
        // 画像が縮小されている
        assert!(result.photo_width < 800 || result.photo_height < 1000,
            "Photo must be shrunk to make room for exif");
        assert!(!result.skip_exif);
    }

    #[test]
    fn square_photo_exif_at_bottom() {
        // 1:1 正方形 (1000x1000) → 4:5 → 上下余白 → 下部にExif
        let (config, bg) = default_config();
        let result = calculate_pad_exif_layout(1000, 1000, &config, &bg);

        assert_eq!(result.canvas_width * 5, result.canvas_height * 4);
        assert!(result.exif_area_y > result.photo_y + result.photo_height - 1);
        assert!(!result.skip_exif);
    }

    #[test]
    fn tiny_image_skips_exif() {
        // 短辺 < 200px → Exif スキップ
        let (config, bg) = default_config();
        let result = calculate_pad_exif_layout(150, 100, &config, &bg);
        assert!(result.skip_exif);
    }

    #[test]
    fn manual_position_bottom_on_portrait() {
        // 縦構図だが Bottom 指定 → 画像縮小して下部に配置
        let mut config = crate::exif_frame::ExifFrameConfig::default();
        config.position = crate::exif_frame::ExifPosition::Bottom;
        let bg = BackgroundColor::Black;
        let result = calculate_pad_exif_layout(800, 1200, &config, &bg);

        assert_eq!(result.canvas_width * 5, result.canvas_height * 4);
        // 下部配置
        assert!(result.exif_area_y > result.photo_y + result.photo_height - 1);
    }
}
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `cargo test --lib -p picture-tool-core exif_frame::layout::tests 2>&1 | tail -20`
Expected: FAIL（`calculate_pad_exif_layout` が存在しない）

- [ ] **Step 3: PadExifLayout 構造体と calculate_pad_exif_layout を実装**

`core/src/exif_frame/layout.rs` を全面書き直し:

```rust
use crate::BackgroundColor;
use crate::exif_frame::{ExifFrameConfig, ExifPosition};

const MIN_SHORT_SIDE: u32 = 200;
/// 縮小率がこの値を超えたらExifスキップ（= 元の80%未満になったら）
const MAX_SHRINK_RATIO: f32 = 0.20;
/// Exifバーの高さ（または幅）を短辺の何%にするか
const EXIF_BAR_RATIO: f32 = 0.06;

/// Pad+Exif統合レイアウトの計算結果
#[derive(Debug, Clone)]
pub struct PadExifLayout {
    /// 最終キャンバスサイズ（4:5）
    pub canvas_width: u32,
    pub canvas_height: u32,
    /// 写真の配置座標とサイズ（縮小後）
    pub photo_x: u32,
    pub photo_y: u32,
    pub photo_width: u32,
    pub photo_height: u32,
    /// Exif情報表示エリア
    pub exif_area_x: u32,
    pub exif_area_y: u32,
    pub exif_area_width: u32,
    pub exif_area_height: u32,
    /// Exif表示が横書き(false)か回転(true)か
    pub is_rotated: bool,
    /// Exifをスキップするか
    pub skip_exif: bool,
}

/// 配置方向を判定
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExifPlacement {
    Bottom,
    Top,
    Right,
    Left,
}

/// 構図とExifPositionから配置方向を決定
fn resolve_placement(
    photo_width: u32,
    photo_height: u32,
    position: ExifPosition,
) -> ExifPlacement {
    match position {
        ExifPosition::Auto => {
            if photo_width as f32 / photo_height as f32 >= 1.0 {
                ExifPlacement::Bottom // 横構図・正方形
            } else {
                ExifPlacement::Right // 縦構図
            }
        }
        ExifPosition::Bottom => ExifPlacement::Bottom,
        ExifPosition::Top => ExifPlacement::Top,
        ExifPosition::Right => ExifPlacement::Right,
        ExifPosition::Left => ExifPlacement::Left,
    }
}

pub fn calculate_pad_exif_layout(
    photo_width: u32,
    photo_height: u32,
    config: &ExifFrameConfig,
    _bg_color: &BackgroundColor,
) -> PadExifLayout {
    let short_side = photo_width.min(photo_height);

    // 極小画像はスキップ
    if short_side < MIN_SHORT_SIDE {
        return skip_layout(photo_width, photo_height);
    }

    let placement = resolve_placement(photo_width, photo_height, config.position);
    let is_rotated = matches!(placement, ExifPlacement::Right | ExifPlacement::Left);

    // 4:5 キャンバスサイズを算出（写真がちょうど収まるサイズ）
    // 4:5 = width:height なので height = width * 5 / 4
    let (canvas_w, canvas_h) = fit_to_4_5(photo_width, photo_height);

    // Exifバーに必要なサイズ
    let exif_bar_size = (short_side as f32 * EXIF_BAR_RATIO).max(30.0) as u32;

    // 配置方向に応じた余白を計算
    let available_space = match placement {
        ExifPlacement::Bottom | ExifPlacement::Top => {
            // 横構図: 上下の余白
            canvas_h.saturating_sub(photo_height)
        }
        ExifPlacement::Right | ExifPlacement::Left => {
            // 縦構図: 左右の余白
            canvas_w.saturating_sub(photo_width)
        }
    };

    // 余白がExifバーに足りない場合は画像を縮小
    let (final_photo_w, final_photo_h, final_canvas_w, final_canvas_h) =
        if available_space < exif_bar_size {
            let deficit = exif_bar_size - available_space;
            let (shrunk_w, shrunk_h) = shrink_photo_for_exif(
                photo_width, photo_height, deficit, placement,
            );
            // 縮小率チェック
            let shrink_ratio_w = 1.0 - (shrunk_w as f32 / photo_width as f32);
            let shrink_ratio_h = 1.0 - (shrunk_h as f32 / photo_height as f32);
            if shrink_ratio_w > MAX_SHRINK_RATIO || shrink_ratio_h > MAX_SHRINK_RATIO {
                return skip_layout(photo_width, photo_height);
            }
            let (cw, ch) = fit_to_4_5(shrunk_w, shrunk_h);
            // キャンバスは元の写真ベースの4:5を維持
            (shrunk_w, shrunk_h, canvas_w.max(cw), canvas_h.max(ch))
        } else {
            (photo_width, photo_height, canvas_w, canvas_h)
        };

    // 写真とExif領域の座標を計算
    let (photo_x, photo_y, exif_x, exif_y, exif_w, exif_h) = match placement {
        ExifPlacement::Bottom => {
            let total_content_h = final_photo_h + exif_bar_size;
            let top_margin = (final_canvas_h - total_content_h) / 2;
            let px = (final_canvas_w - final_photo_w) / 2;
            let py = top_margin;
            let ex = px;
            let ey = py + final_photo_h;
            (px, py, ex, ey, final_photo_w, exif_bar_size)
        }
        ExifPlacement::Top => {
            let total_content_h = final_photo_h + exif_bar_size;
            let top_margin = (final_canvas_h - total_content_h) / 2;
            let px = (final_canvas_w - final_photo_w) / 2;
            let py = top_margin + exif_bar_size;
            let ex = px;
            let ey = top_margin;
            (px, py, ex, ey, final_photo_w, exif_bar_size)
        }
        ExifPlacement::Right => {
            let total_content_w = final_photo_w + exif_bar_size;
            let left_margin = (final_canvas_w - total_content_w) / 2;
            let px = left_margin;
            let py = (final_canvas_h - final_photo_h) / 2;
            let ex = px + final_photo_w;
            let ey = py;
            (px, py, ex, ey, exif_bar_size, final_photo_h)
        }
        ExifPlacement::Left => {
            let total_content_w = final_photo_w + exif_bar_size;
            let left_margin = (final_canvas_w - total_content_w) / 2;
            let px = left_margin + exif_bar_size;
            let py = (final_canvas_h - final_photo_h) / 2;
            let ex = left_margin;
            let ey = py;
            (px, py, ex, ey, exif_bar_size, final_photo_h)
        }
    };

    PadExifLayout {
        canvas_width: final_canvas_w,
        canvas_height: final_canvas_h,
        photo_x,
        photo_y,
        photo_width: final_photo_w,
        photo_height: final_photo_h,
        exif_area_x: exif_x,
        exif_area_y: exif_y,
        exif_area_width: exif_w,
        exif_area_height: exif_h,
        is_rotated,
        skip_exif: false,
    }
}

/// 写真を4:5キャンバスに収めるサイズを算出
fn fit_to_4_5(photo_w: u32, photo_h: u32) -> (u32, u32) {
    let target_ratio = 4.0 / 5.0; // width / height
    let photo_ratio = photo_w as f32 / photo_h as f32;

    if photo_ratio > target_ratio {
        // 横長: 幅基準でキャンバス決定
        let canvas_w = photo_w;
        let canvas_h = (photo_w as f32 / target_ratio).ceil() as u32;
        (canvas_w, canvas_h)
    } else {
        // 縦長または正方形: 高さ基準でキャンバス決定
        let canvas_h = photo_h;
        let canvas_w = (photo_h as f32 * target_ratio).ceil() as u32;
        (canvas_w, canvas_h)
    }
}

/// Exif領域確保のために写真を縮小する量を計算
fn shrink_photo_for_exif(
    photo_w: u32,
    photo_h: u32,
    deficit: u32,
    placement: ExifPlacement,
) -> (u32, u32) {
    // deficit分だけ写真を小さくし、4:5に再フィットさせた時に余白が生まれるようにする
    // アスペクト比を保ったまま縮小
    let scale = match placement {
        ExifPlacement::Bottom | ExifPlacement::Top => {
            // 高さ方向に余白が必要 → 写真の高さを縮小
            let needed_h = photo_h.saturating_sub(deficit);
            needed_h as f32 / photo_h as f32
        }
        ExifPlacement::Right | ExifPlacement::Left => {
            let needed_w = photo_w.saturating_sub(deficit);
            needed_w as f32 / photo_w as f32
        }
    };
    let new_w = (photo_w as f32 * scale).floor() as u32;
    let new_h = (photo_h as f32 * scale).floor() as u32;
    (new_w.max(1), new_h.max(1))
}

/// Exifスキップ時のレイアウト（通常のPadのみ）
fn skip_layout(photo_w: u32, photo_h: u32) -> PadExifLayout {
    let (cw, ch) = fit_to_4_5(photo_w, photo_h);
    PadExifLayout {
        canvas_width: cw,
        canvas_height: ch,
        photo_x: (cw - photo_w) / 2,
        photo_y: (ch - photo_h) / 2,
        photo_width: photo_w,
        photo_height: photo_h,
        exif_area_x: 0,
        exif_area_y: 0,
        exif_area_width: 0,
        exif_area_height: 0,
        is_rotated: false,
        skip_exif: true,
    }
}
```

- [ ] **Step 4: テスト実行**

Run: `cargo test --lib -p picture-tool-core exif_frame::layout::tests 2>&1 | tail -30`
Expected: 全テスト PASS

テスト失敗時はレイアウト計算ロジックを修正。特に `fit_to_4_5` の端数処理と `shrink_photo_for_exif` の縮小量に注意。

- [ ] **Step 5: コミット**

```bash
git add core/src/exif_frame/layout.rs
git commit -m "feat: Pad余白統合レイアウト計算を実装

- calculate_pad_exif_layout: 構図に応じた自動レイアウト
- 横構図→下部、縦構図→右側にExif配置
- 余白不足時の画像縮小（20%超でスキップ）
- ExifPosition による手動配置指定対応"
```

---

## Task 4: text.rs に回転テキスト描画を追加

**Files:**
- Modify: `core/src/exif_frame/text.rs`

- [ ] **Step 1: テスト先行 — 回転テキスト描画**

`core/src/exif_frame/text.rs` 末尾にテストを追加:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use image::RgbaImage;

    fn test_font() -> FontArc {
        load_font(None).expect("bundled font should load")
    }

    #[test]
    fn draw_rotated_text_does_not_panic() {
        let font = test_font();
        let mut img = RgbaImage::new(200, 800);
        // 90度回転描画がパニックしないことを確認
        draw_text_rotated_90(
            &mut img, &font, 16.0,
            "ILCE-7M4 | FE 24-70mm F2.8 GM II | 35mm f/2.8",
            100, 400, // center x, y
            Rgba([255, 255, 255, 255]),
        );
    }

    #[test]
    fn measure_text_width_positive() {
        let font = test_font();
        let width = measure_text_width(&font, 20.0, "Hello");
        assert!(width > 0.0);
    }

    #[test]
    fn truncate_text_with_ellipsis() {
        let font = test_font();
        let full_width = measure_text_width(&font, 20.0, "Very long text that should be truncated");
        let truncated = truncate_text(&font, 20.0, "Very long text that should be truncated", full_width * 0.5);
        assert!(truncated.ends_with("..."));
        assert!(truncated.len() < "Very long text that should be truncated".len());
    }

    #[test]
    fn auto_fit_text_shrinks_font() {
        let font = test_font();
        let text = "ILCE-7M4 | FE 24-70mm F2.8 GM II | 35mm f/2.8 1/250s ISO400";
        let narrow_width = 200.0;
        let (fitted, final_size) = auto_fit_text(&font, 20.0, text, narrow_width, 0.7);
        assert!(final_size < 20.0, "Font size should be reduced");
        let fitted_width = measure_text_width(&font, final_size, &fitted);
        assert!(fitted_width <= narrow_width + 1.0);
    }
}
```

- [ ] **Step 2: テスト失敗確認**

Run: `cargo test --lib -p picture-tool-core exif_frame::text::tests 2>&1 | tail -20`
Expected: FAIL（`draw_text_rotated_90` と `auto_fit_text` が未定義）

- [ ] **Step 3: draw_text_rotated_90 を実装**

`core/src/exif_frame/text.rs` に追加:

```rust
/// テキストを90度時計回りに回転して描画
/// (center_x, center_y) を中心に、テキストが右方向を向くよう回転
pub fn draw_text_rotated_90(
    img: &mut RgbaImage,
    font: &FontArc,
    size: f32,
    text: &str,
    center_x: i32,
    center_y: i32,
    color: Rgba<u8>,
) {
    // テキストを一時的な横書きバッファに描画
    let text_width = measure_text_width(font, size, text).ceil() as u32;
    let text_height = (size * 1.4).ceil() as u32;

    if text_width == 0 || text_height == 0 {
        return;
    }

    let mut temp = RgbaImage::new(text_width + 4, text_height + 4);
    draw_text_on_image(&mut temp, font, size, text, 2, 2, color);

    // 90度時計回りに回転: (x, y) → (height - 1 - y, x)
    let rot_w = temp.height();
    let rot_h = temp.width();
    let mut rotated = RgbaImage::new(rot_w, rot_h);
    for y in 0..temp.height() {
        for x in 0..temp.width() {
            let pixel = temp.get_pixel(x, y);
            if pixel[3] > 0 {
                let new_x = temp.height() - 1 - y;
                let new_y = x;
                if new_x < rot_w && new_y < rot_h {
                    rotated.put_pixel(new_x, new_y, *pixel);
                }
            }
        }
    }

    // 回転済み画像を中心座標に配置
    let paste_x = center_x - (rot_w as i32 / 2);
    let paste_y = center_y - (rot_h as i32 / 2);

    for y in 0..rot_h {
        for x in 0..rot_w {
            let px = paste_x + x as i32;
            let py = paste_y + y as i32;
            if px >= 0 && py >= 0 && (px as u32) < img.width() && (py as u32) < img.height() {
                let src = rotated.get_pixel(x, y);
                if src[3] > 0 {
                    let dst = img.get_pixel_mut(px as u32, py as u32);
                    // アルファブレンディング
                    let alpha = src[3] as f32 / 255.0;
                    for i in 0..3 {
                        dst[i] = (src[i] as f32 * alpha + dst[i] as f32 * (1.0 - alpha)) as u8;
                    }
                    dst[3] = (dst[3] as f32 + alpha * (255.0 - dst[3] as f32)) as u8;
                }
            }
        }
    }
}
```

- [ ] **Step 4: auto_fit_text を実装**

テキストが指定幅に収まるようフォントサイズを縮小し、それでも収まらなければ省略:

```rust
/// テキストを指定幅に収める（フォント縮小 → 省略）
/// min_size_ratio: primary_size に対する最小比率（例: 0.7 = 70%まで縮小）
/// 戻り値: (収まったテキスト, 最終フォントサイズ)
pub fn auto_fit_text(
    font: &FontArc,
    size: f32,
    text: &str,
    max_width: f32,
    min_size_ratio: f32,
) -> (String, f32) {
    let min_size = size * min_size_ratio;
    let mut current_size = size;

    // まずフォントサイズを縮小して試す
    while current_size >= min_size {
        let width = measure_text_width(font, current_size, text);
        if width <= max_width {
            return (text.to_string(), current_size);
        }
        current_size -= 0.5;
    }
    current_size = min_size;

    // それでも収まらなければテキストを省略
    let truncated = truncate_text(font, current_size, text, max_width);
    (truncated, current_size)
}
```

- [ ] **Step 5: テスト実行**

Run: `cargo test --lib -p picture-tool-core exif_frame::text::tests 2>&1 | tail -20`
Expected: 全テスト PASS

- [ ] **Step 6: コミット**

```bash
git add core/src/exif_frame/text.rs
git commit -m "feat: 回転テキスト描画と自動フィット機能を追加

- draw_text_rotated_90: 90度時計回り回転テキスト描画
- auto_fit_text: フォント縮小→省略の段階的フィット"
```

---

## Task 5: logo.rs にレンズブランドロゴ対応を追加

**Files:**
- Modify: `core/src/exif_frame/logo.rs`
- Modify: `core/src/exif_frame/mod.rs`（ModelMap関連）

- [ ] **Step 1: ModelMap の簡素化**

`core/src/exif_frame/mod.rs` 内の `ModelMap` 関連コードから `camera`（display_nameマッピング）セクションの処理を削除。`logo_match` と `lens_brand_match` のみ残す。

`ModelMap` 構造体:
```rust
#[derive(Debug, Deserialize)]
pub struct ModelMap {
    pub logo_match: HashMap<String, LogoMatchEntry>,
    pub lens_brand_match: Vec<LensBrandMatchEntry>,
}

#[derive(Debug, Deserialize)]
pub struct LogoMatchEntry {
    pub maker: String,
}

#[derive(Debug, Deserialize)]
pub struct LensBrandMatchEntry {
    pub pattern: String,
    pub match_type: String,
    pub logo: String,
}
```

既存の `camera` フィールド、`brand` フィールド、`display_name()` メソッドを削除。

- [ ] **Step 2: resolve_lens_brand_logo 関数を追加**

`core/src/exif_frame/logo.rs` に追加:

```rust
/// レンズモデル名からレンズブランドロゴを解決して読み込む
pub fn resolve_lens_brand_logo(
    user_dir: Option<&Path>,
    lens_model: &str,
    model_map: &crate::exif_frame::ModelMap,
    use_light: bool,
    target_size: u32,
) -> Option<DynamicImage> {
    for rule in &model_map.lens_brand_match {
        let matched = match rule.match_type.as_str() {
            "contains" => lens_model.contains(&rule.pattern),
            _ => false,
        };
        if matched {
            return resolve_and_load_logo(user_dir, &rule.logo, use_light, target_size);
        }
    }
    None
}
```

- [ ] **Step 3: ビルド確認**

Run: `cargo build -p picture-tool-core 2>&1 | tail -20`
Expected: コンパイル成功

- [ ] **Step 4: コミット**

```bash
git add core/src/exif_frame/logo.rs core/src/exif_frame/mod.rs
git commit -m "feat: レンズブランドロゴ解決とModelMap簡素化

- ModelMap から camera セクションと brand フィールドを削除
- resolve_lens_brand_logo: lens_brand_match ルールでロゴ解決"
```

---

## Task 6: render_exif_frame 本実装

**Files:**
- Modify: `core/src/exif_frame/mod.rs`

- [ ] **Step 1: render_exif_frame のスタブを本実装に置き換え**

```rust
pub fn render_exif_frame(
    image: &DynamicImage,
    exif: &crate::ExifInfo,
    config: &ExifFrameConfig,
    bg_color: &crate::BackgroundColor,
    asset_dirs: &AssetDirs,
) -> Result<DynamicImage> {
    let photo_w = image.width();
    let photo_h = image.height();

    // 1. レイアウト計算
    let layout = layout::calculate_pad_exif_layout(photo_w, photo_h, config, bg_color);

    if layout.skip_exif {
        // Exifスキップ → 通常のPadのみ（背景色キャンバスに写真配置）
        let mut canvas = RgbaImage::from_pixel(
            layout.canvas_width, layout.canvas_height, bg_color.to_rgba(),
        );
        image::imageops::overlay(
            &mut canvas, &image.to_rgba8(),
            layout.photo_x as i64, layout.photo_y as i64,
        );
        return Ok(DynamicImage::ImageRgba8(canvas));
    }

    // 2. 写真をリサイズ（縮小が必要な場合）
    let photo = if layout.photo_width != photo_w || layout.photo_height != photo_h {
        image.resize_exact(
            layout.photo_width, layout.photo_height,
            image::imageops::FilterType::Lanczos3,
        )
    } else {
        image.clone()
    };

    // 3. キャンバス生成（背景色）
    let mut canvas = RgbaImage::from_pixel(
        layout.canvas_width, layout.canvas_height, bg_color.to_rgba(),
    );

    // 4. 写真配置
    image::imageops::overlay(
        &mut canvas, &photo.to_rgba8(),
        layout.photo_x as i64, layout.photo_y as i64,
    );

    // 5. ModelMap読み込み
    let model_map = load_model_map(asset_dirs);

    // 6. テキスト色判定（背景輝度ベース）
    let bg_rgba = bg_color.to_rgba();
    let luminance = 0.299 * bg_rgba[0] as f32
        + 0.587 * bg_rgba[1] as f32
        + 0.114 * bg_rgba[2] as f32;
    let is_dark = luminance < 128.0;
    let primary_color = if is_dark {
        Rgba([255, 255, 255, 255])
    } else {
        Rgba([0x33, 0x33, 0x33, 255])
    };
    let secondary_color = if is_dark {
        Rgba([0xaa, 0xaa, 0xaa, 255])
    } else {
        Rgba([0x88, 0x88, 0x88, 255])
    };
    let use_light = is_dark;

    // 7. フォント読み込み
    let font = text::load_font(config.font.font_path.as_deref())
        .unwrap_or_else(|_| text::load_font(None).expect("bundled font must exist"));

    // 8. ロゴ読み込み
    let short_side = layout.photo_width.min(layout.photo_height);
    let logo_size = if layout.is_rotated {
        (layout.exif_area_width as f32 * 0.6) as u32
    } else {
        (layout.exif_area_height as f32 * 0.6) as u32
    };

    let maker_logo = if config.items.maker_logo {
        exif.camera_make.as_ref().and_then(|make| {
            model_map.logo_match.get(make.as_str()).and_then(|entry| {
                logo::resolve_and_load_logo(
                    asset_dirs.user_logos_dir.as_deref(),
                    &entry.maker, use_light, logo_size,
                )
            })
        })
    } else {
        None
    };

    let lens_brand_logo = if config.items.lens_brand_logo {
        exif.lens_model.as_ref().and_then(|lens| {
            logo::resolve_lens_brand_logo(
                asset_dirs.user_logos_dir.as_deref(),
                lens, &model_map, use_light,
                if layout.is_rotated { logo_size / 2 } else { logo_size / 2 },
            )
        })
    } else {
        None
    };

    // 9. テキスト構築
    let primary_text = build_primary_text(exif, &config.items);
    let secondary_text = build_secondary_text(exif, &config.items, &config.custom_text);

    // 10. 描画（横書き / 回転）
    if layout.is_rotated {
        draw_exif_rotated(
            &mut canvas, &layout, &font, &config.font,
            &primary_text, &secondary_text,
            primary_color, secondary_color,
            maker_logo.as_ref(), lens_brand_logo.as_ref(),
        );
    } else {
        draw_exif_horizontal(
            &mut canvas, &layout, &font, &config.font,
            &primary_text, &secondary_text,
            primary_color, secondary_color,
            maker_logo.as_ref(), lens_brand_logo.as_ref(),
        );
    }

    Ok(DynamicImage::ImageRgba8(canvas))
}
```

- [ ] **Step 2: draw_exif_horizontal を実装**

```rust
/// 横書き2行レイアウトでExif情報を描画
fn draw_exif_horizontal(
    canvas: &mut RgbaImage,
    layout: &layout::PadExifLayout,
    font: &FontArc,
    font_config: &FontConfig,
    primary_text: &str,
    secondary_text: &str,
    primary_color: Rgba<u8>,
    secondary_color: Rgba<u8>,
    maker_logo: Option<&DynamicImage>,
    lens_brand_logo: Option<&DynamicImage>,
) {
    let area_x = layout.exif_area_x;
    let area_y = layout.exif_area_y;
    let area_w = layout.exif_area_width;
    let area_h = layout.exif_area_height;
    let short_side = layout.photo_width.min(layout.photo_height);

    let primary_size = short_side as f32 * font_config.primary_size;
    let secondary_size = short_side as f32 * font_config.secondary_size;
    let padding = (area_h as f32 * 0.15) as u32;

    let mut text_x = area_x + padding;

    // メーカーロゴ描画
    if let Some(logo) = maker_logo {
        let logo_y = area_y + (area_h - logo.height()) / 2;
        image::imageops::overlay(canvas, &logo.to_rgba8(), text_x as i64, logo_y as i64);
        text_x += logo.width() + padding;

        // セパレーター線
        let sep_top = area_y + padding;
        let sep_bottom = area_y + area_h - padding;
        for y in sep_top..sep_bottom {
            if text_x < canvas.width() && y < canvas.height() {
                canvas.put_pixel(text_x, y, secondary_color);
            }
        }
        text_x += padding;
    }

    // 1行目テキスト（プライマリ）
    let max_text_w = (area_x + area_w).saturating_sub(text_x + padding) as f32;
    let (fitted_primary, final_primary_size) =
        text::auto_fit_text(font, primary_size, primary_text, max_text_w, 0.7);
    let primary_y = area_y + (area_h / 2) - (final_primary_size * 1.1) as u32;
    text::draw_text_on_image(
        canvas, font, final_primary_size, &fitted_primary,
        text_x as i32, primary_y as i32, primary_color,
    );

    // 2行目テキスト（セカンダリ）
    let secondary_y = primary_y + (final_primary_size * 1.3) as u32;
    let (fitted_secondary, final_secondary_size) =
        text::auto_fit_text(font, secondary_size, secondary_text, max_text_w, 0.7);
    text::draw_text_on_image(
        canvas, font, final_secondary_size, &fitted_secondary,
        text_x as i32, secondary_y as i32, secondary_color,
    );
}
```

- [ ] **Step 3: draw_exif_rotated を実装**

```rust
/// 1行凝縮・90度回転でExif情報を描画
fn draw_exif_rotated(
    canvas: &mut RgbaImage,
    layout: &layout::PadExifLayout,
    font: &FontArc,
    font_config: &FontConfig,
    primary_text: &str,
    secondary_text: &str,
    primary_color: Rgba<u8>,
    secondary_color: Rgba<u8>,
    maker_logo: Option<&DynamicImage>,
    _lens_brand_logo: Option<&DynamicImage>,
) {
    let area_x = layout.exif_area_x;
    let area_y = layout.exif_area_y;
    let area_w = layout.exif_area_width;
    let area_h = layout.exif_area_height;
    let short_side = layout.photo_width.min(layout.photo_height);

    let font_size = short_side as f32 * font_config.primary_size;
    let center_x = (area_x + area_w / 2) as i32;
    let center_y = (area_y + area_h / 2) as i32;

    // 1行に凝縮
    let combined = if secondary_text.is_empty() {
        primary_text.to_string()
    } else {
        format!("{} | {}", primary_text, secondary_text)
    };

    // テキストの最大幅 = Exif領域の高さ（回転するため）
    let max_width = area_h as f32 * 0.9;
    let (fitted, final_size) = text::auto_fit_text(font, font_size, &combined, max_width, 0.7);

    // 回転テキスト描画
    text::draw_text_rotated_90(
        canvas, font, final_size, &fitted,
        center_x, center_y, primary_color,
    );

    // メーカーロゴ（回転エリアの端に配置）
    if let Some(logo) = maker_logo {
        let logo_x = center_x - (logo.width() as i32 / 2);
        let logo_y = area_y as i32 + (area_h as f32 * 0.05) as i32;
        if logo_x >= 0 && logo_y >= 0 {
            image::imageops::overlay(
                canvas, &logo.to_rgba8(),
                logo_x as i64, logo_y as i64,
            );
        }
    }
}
```

- [ ] **Step 4: load_model_map ヘルパーを追加**

```rust
fn load_model_map(asset_dirs: &AssetDirs) -> ModelMap {
    // バンドルJSON読み込み
    let bundled = ModelMapAssets::get("model_map.json")
        .and_then(|f| serde_json::from_slice::<ModelMap>(&f.data).ok())
        .unwrap_or_else(|| ModelMap {
            logo_match: HashMap::new(),
            lens_brand_match: Vec::new(),
        });

    // ユーザーカスタムマッピングをマージ
    if let Some(ref path) = asset_dirs.user_model_map {
        if let Ok(data) = std::fs::read_to_string(path) {
            if let Ok(custom) = serde_json::from_str::<ModelMap>(&data) {
                let mut merged = bundled;
                merged.logo_match.extend(custom.logo_match);
                // lens_brand_match はユーザーのルールを先頭に追加（優先）
                let mut new_rules = custom.lens_brand_match;
                new_rules.extend(merged.lens_brand_match);
                merged.lens_brand_match = new_rules;
                return merged;
            }
        }
    }
    bundled
}
```

- [ ] **Step 5: ビルド確認**

Run: `cargo build -p picture-tool-core 2>&1 | tail -20`
Expected: コンパイル成功

- [ ] **Step 6: コミット**

```bash
git add core/src/exif_frame/mod.rs
git commit -m "feat: render_exif_frame v2 実装

- Pad余白統合描画（横書き2行 / 1行回転）
- 背景輝度によるテキスト色自動判定
- メーカー・レンズブランドロゴ描画
- auto_fit によるオーバーフロー処理"
```

---

## Task 7: process_image パイプライン統合

**Files:**
- Modify: `core/src/lib.rs:199-259`

- [ ] **Step 1: process_image の Pad+Exif 統合**

`core/src/lib.rs` の `process_image` 内のパイプラインを変更。Padモード時にExifフレームを統合:

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

    let converted = match config.mode {
        ConversionMode::Crop => crop_to_4_5(&img),
        ConversionMode::Pad => {
            // Pad + Exif統合: Exifが有効ならrender_exif_frameがPad処理も行う
            if let (Some(ef_config), Some(dirs)) = (exif_frame_config, asset_dirs) {
                let exif = read_exif_info(input_path).unwrap_or_default();
                match exif_frame::render_exif_frame(&img, &exif, ef_config, &config.bg_color, dirs) {
                    Ok(framed) => framed,
                    Err(e) => {
                        eprintln!("Warning: Exif frame failed, falling back to pad only: {}", e);
                        pad_to_4_5(&img, config.bg_color)
                    }
                }
            } else {
                pad_to_4_5(&img, config.bg_color)
            }
        }
        ConversionMode::Quality => img.clone(),
    };

    // Crop/Qualityモードでは exif_frame_config を無視（上記のmatchで処理済み）

    let output_path = generate_output_path(input_path, output_folder);
    let file_size = save_with_size_limit(&converted, &output_path, config.quality, config.max_size_mb)?;

    if config.delete_originals {
        std::fs::remove_file(input_path)?;
    }

    Ok(ProcessResult {
        input: input_path.to_path_buf(),
        output: output_path,
        file_size,
    })
}
```

核心: Padモード時に `render_exif_frame` が Pad+Exif 両方を処理する。v1 のように Pad → Exif の2段階ではなく、1ステップで統合。

- [ ] **Step 2: ビルド確認**

Run: `cargo build 2>&1 | tail -20`
Expected: コンパイル成功

- [ ] **Step 3: コミット**

```bash
git add core/src/lib.rs
git commit -m "refactor: process_image パイプラインをPad+Exif統合に変更

- PadモードでExif有効時: render_exif_frame がPad+Exif統合処理
- Crop/QualityモードではExif設定を無視
- フレーム失敗時はPadのみにフォールバック"
```

---

## Task 8: GUI コマンドとプリセット更新

**Files:**
- Modify: `gui/src/commands.rs`
- Modify: `core/src/exif_frame/preset.rs`
- Modify: `core/assets/presets/` 内のデフォルトプリセットJSON

- [ ] **Step 1: render_exif_frame_preview に bg_color パラメータを追加**

`gui/src/commands.rs` の `render_exif_frame_preview` を修正:

```rust
#[tauri::command]
pub async fn render_exif_frame_preview(
    path: String,
    config: core::exif_frame::ExifFrameConfig,
    bg_color: core::BackgroundColor,
) -> Result<String, String> {
    let path = PathBuf::from(&path);
    tokio::task::spawn_blocking(move || {
        let img = image::open(&path).map_err(|e| e.to_string())?;
        let small = img.resize(400, 400, image::imageops::FilterType::Triangle);
        let exif = core::read_exif_info(&path).unwrap_or_default();
        let asset_dirs = get_asset_dirs();
        let result = core::exif_frame::render_exif_frame(&small, &exif, &config, &bg_color, &asset_dirs)
            .map_err(|e| e.to_string())?;
        // JPEG base64 エンコード
        let mut buf = Vec::new();
        result.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Jpeg)
            .map_err(|e| e.to_string())?;
        Ok(format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(&buf)))
    }).await.map_err(|e| e.to_string())?
}
```

- [ ] **Step 2: デフォルトプリセットを v2 形式に更新**

`core/assets/presets/` 内の既存プリセットJSONを v2 形式に更新。例: `default.json`:

```json
{
  "name": "default",
  "position": "auto",
  "items": {
    "maker_logo": true,
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
  "custom_text": ""
}
```

- [ ] **Step 3: ビルド確認**

Run: `cargo build 2>&1 | tail -20`
Expected: コンパイル成功

- [ ] **Step 4: コミット**

```bash
git add gui/src/commands.rs core/assets/presets/
git commit -m "feat: GUIコマンドにbg_color追加、プリセットv2化

- render_exif_frame_preview に bg_color パラメータ追加
- デフォルトプリセットを v2 形式に更新"
```

---

## Task 9: フロントエンド型定義とAPI更新

**Files:**
- Modify: `gui-frontend/src/lib/types.ts`
- Modify: `gui-frontend/src/lib/api.ts`

- [ ] **Step 1: types.ts を v2 に更新**

```typescript
// --- v1 の FrameLayout, FrameColor, OutputAspectRatio を削除 ---

export type ExifPosition = "auto" | "bottom" | "top" | "right" | "left";

export interface DisplayItems {
  maker_logo: boolean;
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
  position: ExifPosition;
  items: DisplayItems;
  font: FontConfig;
  custom_text: string;
}
```

`brand_logo`, `layout`, `color`, `aspect_ratio`, `frame_padding` を削除。

- [ ] **Step 2: api.ts の renderExifFramePreview に bg_color を追加**

```typescript
export async function renderExifFramePreview(
  path: string,
  config: ExifFrameConfig,
  bgColor: "white" | "black",
): Promise<string> {
  return invoke<string>("render_exif_frame_preview", {
    path,
    config,
    bgColor,
  });
}
```

- [ ] **Step 3: コミット**

```bash
git add gui-frontend/src/lib/types.ts gui-frontend/src/lib/api.ts
git commit -m "refactor: フロントエンド型定義をExifFrame v2に更新

- ExifPosition 型追加
- FrameLayout/FrameColor/OutputAspectRatio 型削除
- renderExifFramePreview に bgColor パラメータ追加"
```

---

## Task 10: ExifFrameSettings.svelte 簡素化

**Files:**
- Modify: `gui-frontend/src/lib/ExifFrameSettings.svelte`

- [ ] **Step 1: v1 の設定項目を削除し v2 に置き換え**

削除する UI:
- レイアウト選択（BottomBar / SideBar / FullBorder の3ボタン）
- フレーム色選択（白/黒の丸ボタン）
- アスペクト比選択（4:5 / 1:1 / 16:9 / 自由 の4ボタン）
- フレームパディングスライダー

追加する UI:
- **Exif配置位置**（Auto / 上 / 下 / 左 / 右 の5ボタン）

残す UI:
- プリセット選択
- 表示項目ON/OFFトグル（`brand_logo` を削除）
- フォント選択・サイズスライダー
- カスタムテキスト入力
- ライブプレビュー

`defaultConfig()` を更新:

```typescript
function defaultConfig(): ExifFrameConfig {
  return {
    name: "default",
    position: "auto",
    items: {
      maker_logo: true,
      lens_brand_logo: true,
      camera_model: true,
      lens_model: true,
      focal_length: true,
      f_number: true,
      shutter_speed: true,
      iso: true,
      date_taken: false,
      custom_text: false,
    },
    font: {
      font_path: null,
      primary_size: 0.025,
      secondary_size: 0.018,
    },
    custom_text: "",
  };
}
```

- [ ] **Step 2: Props に bgColor を追加**

```typescript
interface Props {
  visible: boolean;
  previewImagePath: string | null;
  bgColor: "white" | "black";
  onClose: () => void;
  onSave: (config: ExifFrameConfig) => void;
}
```

ライブプレビューの呼び出しを更新:
```typescript
const preview = await renderExifFramePreview(previewImagePath, config, bgColor);
```

- [ ] **Step 3: 配置位置セレクター UI を実装**

```svelte
<div class="setting-group">
  <label>配置位置</label>
  <div class="position-selector">
    {#each [
      { value: "auto", label: "自動" },
      { value: "bottom", label: "下" },
      { value: "top", label: "上" },
      { value: "right", label: "右" },
      { value: "left", label: "左" },
    ] as opt}
      <button
        class="position-btn"
        class:active={config.position === opt.value}
        onclick={() => config.position = opt.value}
      >
        {opt.label}
      </button>
    {/each}
  </div>
</div>
```

- [ ] **Step 4: フロントエンドビルド確認**

Run: `cd gui-frontend && bun run check 2>&1 | tail -20`
Expected: エラーなし

- [ ] **Step 5: コミット**

```bash
git add gui-frontend/src/lib/ExifFrameSettings.svelte
git commit -m "refactor: ExifFrameSettings を v2 に簡素化

- レイアウト/色/アスペクト比/パディング設定を削除
- ExifPosition セレクター追加（Auto/上/下/左/右）
- bgColor を Props から受け取りプレビューに反映"
```

---

## Task 11: SettingsPanel Padモード条件表示

**Files:**
- Modify: `gui-frontend/src/lib/SettingsPanel.svelte`
- Modify: `gui-frontend/src/App.svelte`（bgColor の受け渡し）

- [ ] **Step 1: SettingsPanel で Exifフレーム項目を Padモード時のみ表示**

`gui-frontend/src/lib/SettingsPanel.svelte` の Exifフレームセクション（lines 79-105付近）を条件付きに:

```svelte
{#if mode === "pad"}
  <div class="exif-frame-section">
    <label class="checkbox">
      <input
        type="checkbox"
        checked={exifFrameEnabled}
        onchange={(e) => onExifFrameEnabledChange((e.target as HTMLInputElement).checked)}
      />
      <span>Exifフレーム</span>
    </label>

    {#if exifFrameEnabled}
      <div class="exif-frame-controls">
        <select
          value={selectedPresetName}
          onchange={(e) => onPresetChange((e.target as HTMLSelectElement).value)}
        >
          {#each presets as preset}
            <option value={preset.name}>{preset.name}</option>
          {/each}
          {#if presets.length === 0}
            <option value="default">default</option>
          {/if}
        </select>
        <button class="gear-btn" onclick={onOpenExifSettings} title="Exifフレーム設定">⚙</button>
      </div>
    {/if}
  </div>
{/if}
```

Props に `mode: string` が必要。既に渡されているか確認し、なければ追加。

- [ ] **Step 2: App.svelte で ExifFrameSettings に bgColor を渡す**

`App.svelte` の `ExifFrameSettings` コンポーネント呼び出しに `bgColor` を追加:

```svelte
<ExifFrameSettings
  visible={showExifFrameSettings}
  previewImagePath={selectedImages[0]?.path ?? null}
  bgColor={bgColor}
  onClose={() => showExifFrameSettings = false}
  onSave={handleExifFrameSave}
/>
```

また、`handleProcess` で Padモード以外のとき `exifFrameConfig` を `null` にする:

```typescript
const efConfig = (mode === "pad" && exifFrameEnabled) ? activeExifFrameConfig : null;
await processImages(files, outputFolder, processingConfig, efConfig);
```

- [ ] **Step 3: フロントエンドビルド確認**

Run: `cd gui-frontend && bun run check 2>&1 | tail -20`
Expected: エラーなし

- [ ] **Step 4: コミット**

```bash
git add gui-frontend/src/lib/SettingsPanel.svelte gui-frontend/src/App.svelte
git commit -m "feat: ExifフレームUIをPadモード時のみ表示

- SettingsPanel: mode === 'pad' の条件で表示制御
- App.svelte: bgColor を ExifFrameSettings に渡す
- Pad以外のモードでは exifFrameConfig を null に"
```

---

## Task 12: 統合テスト

**Files:**
- Create: `core/tests/exif_frame_v2_integration.rs`

- [ ] **Step 1: 統合テストファイルを作成**

```rust
//! Exifフレーム v2 統合テスト
use picture_tool_core::*;
use picture_tool_core::exif_frame::*;
use std::path::PathBuf;
use tempfile::TempDir;

fn create_test_image(width: u32, height: u32) -> image::DynamicImage {
    image::DynamicImage::ImageRgb8(image::RgbImage::new(width, height))
}

fn default_exif() -> ExifInfo {
    ExifInfo {
        camera_make: Some("SONY".to_string()),
        camera_model: Some("ILCE-7M4".to_string()),
        lens_model: Some("FE 24-70mm F2.8 GM II".to_string()),
        focal_length: Some("35mm".to_string()),
        f_number: Some("f/2.8".to_string()),
        shutter_speed: Some("1/250s".to_string()),
        iso: Some(400),
        date_taken: None,
    }
}

fn default_asset_dirs() -> AssetDirs {
    AssetDirs {
        user_logos_dir: None,
        user_fonts_dir: None,
        user_model_map: None,
    }
}

#[test]
fn pad_exif_landscape_produces_4_5() {
    // 3:2 横構図
    let img = create_test_image(1200, 800);
    let exif = default_exif();
    let config = ExifFrameConfig::default();
    let bg = BackgroundColor::Black;
    let dirs = default_asset_dirs();

    let result = render_exif_frame(&img, &exif, &config, &bg, &dirs).unwrap();
    let ratio = result.width() as f32 / result.height() as f32;
    let expected = 4.0 / 5.0;
    assert!((ratio - expected).abs() < 0.02,
        "Expected 4:5 ratio, got {:.3} ({}x{})", ratio, result.width(), result.height());
}

#[test]
fn pad_exif_portrait_produces_4_5() {
    // 2:3 縦構図
    let img = create_test_image(800, 1200);
    let exif = default_exif();
    let config = ExifFrameConfig::default();
    let bg = BackgroundColor::White;
    let dirs = default_asset_dirs();

    let result = render_exif_frame(&img, &exif, &config, &bg, &dirs).unwrap();
    let ratio = result.width() as f32 / result.height() as f32;
    let expected = 4.0 / 5.0;
    assert!((ratio - expected).abs() < 0.02,
        "Expected 4:5 ratio, got {:.3}", ratio);
}

#[test]
fn pad_exif_already_4_5_still_works() {
    // 既に4:5 → 画像縮小してExifスペース確保
    let img = create_test_image(800, 1000);
    let exif = default_exif();
    let config = ExifFrameConfig::default();
    let bg = BackgroundColor::Black;
    let dirs = default_asset_dirs();

    let result = render_exif_frame(&img, &exif, &config, &bg, &dirs).unwrap();
    let ratio = result.width() as f32 / result.height() as f32;
    let expected = 4.0 / 5.0;
    assert!((ratio - expected).abs() < 0.02);
}

#[test]
fn pad_exif_no_exif_data_still_produces_image() {
    let img = create_test_image(1200, 800);
    let exif = ExifInfo::default(); // 全フィールド None
    let config = ExifFrameConfig::default();
    let bg = BackgroundColor::White;
    let dirs = default_asset_dirs();

    let result = render_exif_frame(&img, &exif, &config, &bg, &dirs).unwrap();
    assert!(result.width() > 0 && result.height() > 0);
}

#[test]
fn process_image_crop_mode_ignores_exif() {
    let tmp = TempDir::new().unwrap();
    let input_path = tmp.path().join("test.jpg");
    let output_path = tmp.path().join("output");
    std::fs::create_dir_all(&output_path).unwrap();

    // テスト用JPEGを作成
    let img = create_test_image(1200, 800);
    img.save(&input_path).unwrap();

    let config = ProcessingConfig {
        mode: ConversionMode::Crop,
        bg_color: BackgroundColor::White,
        quality: 90,
        max_size_mb: 8,
        delete_originals: false,
    };
    let ef_config = ExifFrameConfig::default();
    let dirs = default_asset_dirs();

    // CropモードでExifConfig渡してもエラーにならない
    let result = process_image(&input_path, &output_path, &config, Some(&ef_config), Some(&dirs));
    assert!(result.is_ok());
}

#[test]
fn process_image_quality_mode_ignores_exif() {
    let tmp = TempDir::new().unwrap();
    let input_path = tmp.path().join("test.jpg");
    let output_path = tmp.path().join("output");
    std::fs::create_dir_all(&output_path).unwrap();

    let img = create_test_image(1200, 800);
    img.save(&input_path).unwrap();

    let config = ProcessingConfig {
        mode: ConversionMode::Quality,
        bg_color: BackgroundColor::White,
        quality: 90,
        max_size_mb: 8,
        delete_originals: false,
    };
    let ef_config = ExifFrameConfig::default();
    let dirs = default_asset_dirs();

    let result = process_image(&input_path, &output_path, &config, Some(&ef_config), Some(&dirs));
    assert!(result.is_ok());
}
```

- [ ] **Step 2: テスト実行**

Run: `cargo test --test exif_frame_v2_integration 2>&1 | tail -30`
Expected: 全テスト PASS

失敗時はレイアウト計算やrender処理を修正。

- [ ] **Step 3: コミット**

```bash
git add core/tests/exif_frame_v2_integration.rs
git commit -m "test: Exifフレーム v2 統合テスト追加

- Pad+Exif 横構図/縦構図/正方形で4:5出力検証
- Exifデータなしでもクラッシュしないことを検証
- Crop/QualityモードでExif設定が無視されることを検証"
```

---

## Task 13: クリーンアップ

**Files:**
- Modify: `core/src/exif_frame/mod.rs` (旧コード削除)
- Delete: 不要になったファイル

- [ ] **Step 0: CLI に Padモードチェックを追加**

`cli/src/main.rs` で `--exif-frame` が `--mode pad` 以外と組み合わされた場合に警告を出す:

```rust
if args.exif_frame && args.mode != ConversionMode::Pad {
    eprintln!("Warning: --exif-frame is only supported with --mode pad. Ignoring.");
    // exif_frame_config を None にする
}
```

- [ ] **Step 1: v1 の旧型定義・旧コードを完全削除**

`core/src/exif_frame/mod.rs` から残っている v1 コードを確認・削除:
- `FrameLayout` enum（Step 1で削除済みのはず）
- `FrameColor` enum とその impl
- `OutputAspectRatio` enum
- 旧 `FrameDimensions` 構造体（layout.rs 側）
- 旧 `calculate_frame_dimensions` 関数（layout.rs 側）
- `model_map.rs`（ルートにある場合。ModelMapが mod.rs 内に移動済みか確認）

- [ ] **Step 2: 旧プレースホルダーロゴが残っていないか確認**

```bash
ls core/assets/logos/
# sony.svg, sony_light.svg, gmaster.png, gmaster_light.png のみであること
```

- [ ] **Step 3: 全テスト実行**

Run: `cargo test 2>&1 | tail -30`
Expected: 全テスト PASS

- [ ] **Step 4: フロントエンドビルド確認**

Run: `cd gui-frontend && bun run check && bun run build 2>&1 | tail -20`
Expected: エラーなし

- [ ] **Step 5: GUI全体ビルド確認**

Run: `make build-gui 2>&1 | tail -20`
Expected: ビルド成功

- [ ] **Step 6: コミット**

```bash
git add -A
git commit -m "chore: Exifフレーム v1 旧コードのクリーンアップ

- 旧型定義(FrameLayout/FrameColor/OutputAspectRatio)完全削除
- 旧レイアウト計算コード削除
- 不要なロゴアセット確認"
```

---

## 実行順序と依存関係

```
Task 1 (データモデル) ──┐
Task 2 (ロゴアセット) ──┼──→ Task 5 (ロゴ対応) ──→ Task 6 (render) ──→ Task 7 (pipeline) ──→ Task 12 (統合テスト) ──→ Task 13 (cleanup)
Task 3 (レイアウト) ────┘                              ↑
Task 4 (テキスト) ─────────────────────────────────────┘

Task 9 (型定義/API) ──→ Task 10 (ExifSettings) ──→ Task 11 (SettingsPanel)
```

Task 1-4 は並列実行可能。Task 9-11 は Task 8 完了後に並列実行可能。
