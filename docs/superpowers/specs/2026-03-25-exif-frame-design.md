# Exifフレーム機能 設計仕様書

## 概要

Instagram投稿用画像にEXIF情報（カメラ・レンズ・撮影パラメータ）とメーカー/ブランドロゴを含むフレームを付加する機能。既存の変換モード（crop/pad/quality）の後段オプションとして動作し、GUI/CLI両方から利用可能。

## アプローチ

ハイブリッド方式を採用。レイアウト計算とフレーム描画は `image` + `imageproc` + `ab_glyph` でRust実装。ロゴはPNG/SVGアセットとして読み込み合成（SVGは `resvg` でラスタライズ）。プリセットはJSON設定で管理。

将来、レイアウトの自由度が必要になった時点でフルSVGテンプレートエンジンに進化させる。

## アーキテクチャ

### ファイル構成

```
picture-tool-rust/
├── core/
│   ├── src/
│   │   ├── lib.rs              # 既存（ExifInfo, process_image等）
│   │   ├── exif_frame/
│   │   │   ├── mod.rs          # ExifFrameConfig, render_exif_frame()
│   │   │   ├── layout.rs       # 4レイアウトの描画ロジック
│   │   │   ├── logo.rs         # ロゴ読み込み・マッチング（PNG/SVG）
│   │   │   ├── text.rs         # テキスト描画（ab_glyph）
│   │   │   └── preset.rs       # プリセット管理（JSON読み書き）
│   │   └── model_map.rs        # カメラ型番→表示名マッピング
│   └── assets/
│       ├── fonts/
│       │   └── NotoSansJP-*.ttf
│       ├── logos/
│       │   ├── sony.svg, sony_light.svg
│       │   ├── alpha.svg, alpha_light.svg
│       │   └── ...
│       └── model_map.json
├── cli/                         # --exif-frame, --preset オプション追加
├── gui/src/commands.rs          # render_exif_frame_preview等追加
└── gui-frontend/
    └── src/lib/components/
        └── ExifFrameSettings.svelte
```

### 処理フロー

```
入力画像 → 既存変換(crop/pad/quality) → Exifフレーム付加 → サイズ制限 → 出力JPEG
                                              ↑
                                    ExifFrameConfig(プリセットから)
                                    + ExifInfo(EXIF読み取り)
                                    + ロゴアセット
```

### 追加依存（core）

- `imageproc` — 画像描画プリミティブ
- `ab_glyph` — フォントレンダリング
- `resvg` + `usvg` — SVGロゴのラスタライズ

## データモデル

### Rust構造体

```rust
pub enum FrameLayout {
    BottomBar,      // 下部バー
    SideBar,        // サイドバー
    FullBorder,     // フルボーダー + 下部情報
}

pub enum FrameColor {
    White,
    Black,
    Custom(u8, u8, u8),
}

pub enum OutputAspectRatio {
    Fixed(u32, u32),
    Free,
}

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

pub struct FontConfig {
    pub font_path: Option<PathBuf>,
    pub primary_size: f32,
    pub secondary_size: f32,
}

pub struct ExifFrameConfig {
    pub name: String,
    pub layout: FrameLayout,
    pub color: FrameColor,
    pub aspect_ratio: OutputAspectRatio,
    pub items: DisplayItems,
    pub font: FontConfig,
    pub custom_text: String,
    pub frame_padding: f32,
}
```

### プリセットJSON

```json
{
  "name": "Instagram 4:5 白",
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
    "primary_size": 16.0,
    "secondary_size": 12.0
  },
  "custom_text": "",
  "frame_padding": 0.05
}
```

### モデルマッピング

```json
{
  "camera": {
    "ILCE-7M4": "α7IV",
    "ILCE-7RM5": "α7RV",
    "ILCE-9M3": "α9III"
  },
  "logo_match": {
    "SONY": { "maker": "sony.svg", "brand": "alpha.svg" },
    "Canon": { "maker": "canon.svg", "brand": null },
    "NIKON": { "maker": "nikon.svg", "brand": null }
  },
  "lens_brand_match": {
    "GM": "gmaster.svg",
    "G ": "sony_g.svg",
    "Art": "sigma_art.svg"
  }
}
```

## レイアウト

4つのレイアウトパターンをサポート。フレーム背景色は白/黒/カスタムRGBから選択。

1. **BottomBar（白/黒）** — 写真の下にEXIF情報バー。ロゴ左、テキスト右
2. **SideBar** — 写真の右側にEXIF情報を縦配置
3. **FullBorder** — 写真全体を枠で囲み、下部にEXIF情報

## コア処理ロジック

### メイン関数

```rust
pub fn render_exif_frame(
    image: &DynamicImage,
    exif: &ExifInfo,
    config: &ExifFrameConfig,
    asset_dirs: &AssetDirs,
) -> Result<DynamicImage>
```

### 処理ステップ

1. ロゴ解決 — `exif.camera_make` + `model_map.json` → ロゴ読み込み（SVG→resvgでラスタライズ / PNG→そのまま）
2. テキスト準備 — `exif` + `model_map` → 表示テキスト組み立て
3. レイアウト計算 — `config.layout` に応じてフレーム領域サイズ・各要素の配置座標を算出
4. アスペクト比調整 — `Fixed` なら写真+フレーム全体が目標比率に収まるよう計算
5. キャンバス生成 — 最終出力サイズを `config.color` で塗りつぶし
6. 写真配置 → ロゴ描画 → テキスト描画

### 既存パイプラインへの統合

`process_image()` に `exif_frame: Option<&ExifFrameConfig>` と `asset_dirs: Option<&AssetDirs>` を追加。既存変換後、サイズ制限前にフレーム付加を挿入。

### ロゴ読み込み

```
1. 検索パスからロゴファイル探索（SVG優先 → PNG）
2. SVG → resvgで目標サイズにラスタライズ → DynamicImage
3. PNG → imageで読み込み → リサイズ → DynamicImage
```

フレーム色に応じたバリアント自動選択: 白フレーム → `logo.svg`（暗）、黒フレーム → `logo_light.svg`（明）

### アセット検索優先順位

```
1. ユーザーカスタムディレクトリ (~/.config/picture-tool/assets/)
2. バンドルアセット (core/assets/ → バイナリに埋め込み)
```

## GUI

### メイン画面の変更

- 画面右下に設定アイコンボタン（歯車）を配置
- 既存 `SettingsPanel` に「Exifフレーム: ON/OFF」トグルと「プリセット選択」ドロップダウンを追加

### ExifFrameSettings.svelte（モーダル）

スクロール型設定 + ライブプレビューのモーダルオーバーレイ。

設定項目:
- プリセット選択・新規作成・複製・削除
- レイアウト選択（4パターンのビジュアルセレクター）
- 表示項目ON/OFFトグル（タグUI）
- アスペクト比テンプレート（4:5, 1:1, 16:9, 自由）
- フレーム色（白/黒/カスタム）
- フォント選択・サイズスライダー
- カスタムテキスト入力

ライブプレビュー:
- 右側に常時表示。選択中の画像に現在の設定を適用した結果をリアルタイム表示
- デバウンス300msで過剰呼び出し防止
- 低解像度（max 400px）で高速生成

### Tauriコマンド追加

```rust
render_exif_frame_preview(path: String, config: ExifFrameConfig) -> String // base64
list_presets() -> Vec<ExifFrameConfig>
save_preset(config: ExifFrameConfig) -> ()
delete_preset(name: String) -> ()
list_available_fonts() -> Vec<FontInfo>
list_available_logos() -> Vec<LogoInfo>
```

## CLI

### 追加オプション

| オプション | 短縮 | デフォルト | 説明 |
|-----------|------|-----------|------|
| `--exif-frame` | `-e` | `false` | Exifフレームを付加 |
| `--preset` | `-p` | `"default"` | プリセット名 |
| `--preset-file` | | | プリセットJSONファイル直接指定 |
| `--custom-text` | | `""` | カスタムテキスト（プリセットの値を上書き） |

### プリセット検索パス

```
1. --preset-file <path>（直接指定）
2. ~/.config/picture-tool/presets/<name>.json
3. バンドルプリセット
```

## ロゴ・アセット管理

### バンドルロゴ（初期）

Sony, α, G Master, Sony G, Canon, Nikon, Fujifilm, Sigma

### ロゴ仕様

- 形式: SVG（推奨）またはPNG（透過背景）
- PNG推奨サイズ: 256x256px
- フレーム色に応じた2バリアント: `logo.svg`（暗）+ `logo_light.svg`（明）

### ユーザーカスタマイズ

```
~/.config/picture-tool/
├── assets/
│   ├── logos/
│   └── fonts/
├── presets/
└── model_map_custom.json
```

`model_map_custom.json` はバンドルの `model_map.json` にマージ。同一キーはユーザー側が優先。

### バイナリ埋め込み

`rust-embed` または `include_bytes!` でバンドルアセットをバイナリに埋め込み。

## エラーハンドリング

| ケース | 対応 |
|--------|------|
| EXIF情報なし | フレーム生成するが空テキスト。最小レイアウトにフォールバック |
| ロゴ未発見 | ロゴスキップ、テキスト左寄せ |
| フォント読み込み失敗 | バンドルフォントにフォールバック。それも失敗ならフレームなしで出力（警告） |
| カスタムマッピングJSON不正 | バンドルマッピングのみ使用（警告） |
| プリセットJSON不正 | そのプリセット読み込みスキップ（エラー表示） |
| 極小画像 | 最小閾値未満はフレーム付加スキップ |
| テキスト幅超過 | 省略（`...`）で切り詰め |

方針: 「スキップして次へ進む」。1枚の失敗がバッチ全体を止めない。フレーム生成で予期せぬエラーの場合、フレームなしの画像を出力（変換成功扱い、警告付き）。

## テスト戦略

### ユニットテスト

- layout — 各レイアウトの座標計算
- logo — ロゴマッチング、SVG/PNG読み込み
- text — テキスト描画（空文字列、長文対応）
- preset — JSON読み書きラウンドトリップ
- model_map — 型番変換、カスタムマッピングマージ

### 統合テスト

- 各レイアウト × 各色の組み合わせで画像生成→出力サイズ検証
- Fixed(4,5)指定時のアスペクト比検証
- EXIF全Noneでもクラッシュしない
- ロゴなし（未知メーカー）でもテキストのみで生成
- 既存パイプライン統合（crop→exif_frame→save_with_size_limit）の通し確認

### テスト用アセット

`core/tests/fixtures/` にテスト用PNG/SVGロゴとフォントを配置。
