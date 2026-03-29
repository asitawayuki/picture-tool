# Exifフレーム v2 設計仕様書

## 概要

v1（2026-03-25-exif-frame-design.md）からの設計見直し。Exifフレームを**Padモード専用**に限定し、Padが生成する余白をExif情報の表示領域として再利用する。独立したフレーム領域は追加しない。

### v1からの主な変更点

- BottomBar / SideBar / FullBorder レイアウト選択を廃止
- 独立したExifフレーム色・パディング設定を廃止（Pad背景色と同期）
- Crop / Quality モードではExifフレーム無効
- プレースホルダーロゴ全廃止、実ロゴ（Sony SVG、GM PNG）に差替え
- モデル名マッピング（型番→表示名変換）廃止。型番直接表示
- レンズブランドロゴ（GM）をレンズ型番の直前に配置
- 縦構図レイアウトを1行凝縮・90度回転に変更

## コア設計方針

### Padモード統合

ExifフレームはPadモードの**拡張機能**として動作する。処理パイプラインは以下のように変更：

```
v1: 入力 → Pad変換 → Exifフレーム追加 → サイズ制限 → 出力
v2: 入力 → Pad変換（Exif情報込み）→ サイズ制限 → 出力
```

v1ではPad変換後に独立したフレーム領域を追加していたが、v2ではPad変換自体にExif描画を統合する。

### 余白不足時の画像縮小

元画像のアスペクト比が4:5に近い（またはぴったり）場合、Padの余白がExif情報を配置するには不足する。この場合：

1. 必要なExif表示領域の高さ（横構図）または幅（縦構図）を算出
2. 画像を縮小して必要なスペースを確保
3. 4:5アスペクト比は維持
4. 縮小率が20%を超える場合（= 元画像の辺の長さが80%未満になる場合）はExifフレームをスキップ（画像が過度に小さくなるのを防止）

## レイアウト

### 構図判定

画像のアスペクト比に基づいて構図を判定：
- **横構図**: `width / height > 1.0`（4:5 Padで上下に余白）
- **縦構図**: `width / height < 1.0`（4:5 Padで左右に余白）
- **正方形**: `width / height == 1.0`（4:5 Padで上下に余白 → 横構図と同じ扱い）

### 横構図・正方形：下部余白に2行横書き

```
┌─────────────────────────┐
│       (上部余白)         │
│  ┌───────────────────┐  │
│  │                   │  │
│  │      写真         │  │
│  │                   │  │
│  └───────────────────┘  │
│                         │
│ [Sony] | ILCE-7M4 | [GM] FE 24-70mm F2.8 GM II  │
│          35mm  f/2.8  1/250s  ISO 400             │
└─────────────────────────┘
```

- 1行目: メーカーロゴ → `|` → カメラ型番 → `|` → [レンズブランドロゴ] レンズ型番
- 2行目: 焦点距離 / F値 / シャッタースピード / ISO / [date_taken] / [custom_text]
- `date_taken` と `custom_text` は `DisplayItems` で有効な場合のみ2行目の末尾に追加
- セパレーターは半角パイプ `|` で統一
- テキスト色: 背景の輝度（`0.299R + 0.587G + 0.114B`）で判定。閾値128未満（暗い）→ `#ffffff`（1行目）/ `#aaaaaa`（2行目）、128以上（明るい）→ `#333333` / `#888888`。White/Blackの2値だけでなくカスタムRGBにも対応

### 縦構図：右余白に1行凝縮・90度回転

```
┌──────────────────────┐
│              │ S  I  │
│              │ O  L  │
│  ┌────────┐ │ N  C  │
│  │        │ │ Y  E  │
│  │  写真  │ │ |  -  │
│  │        │ │    7  │
│  └────────┘ │ .. M  │
│              │    4  │
│              │ .. .. │
└──────────────────────┘
```

（テキストは時計回り90度回転。画像を横に傾けた時に自然に読める方向）

- 1行に凝縮: [Sonyロゴ] | カメラ型番 | [GMロゴ] レンズ型番 | 焦点距離 F値 SS ISO
- 余白の縦方向全体を使って配置

**テキストオーバーフロー時の対応:**
1. まず自動フォントサイズ縮小（`FontConfig.primary_size` の70%まで）
2. それでも収まらない場合、優先度の低い項目から省略: date_taken → custom_text → shutter_speed → iso → focal_length の順
3. 最低限 カメラ型番 + レンズ型番 は表示する（これすら入らない場合はExifフレームスキップ）

### 配置位置の選択

ユーザー設定で配置位置を選択可能。

```rust
pub enum ExifPosition {
    Auto,    // デフォルト: 横構図→下、縦構図→右
    Bottom,  // 常に下部
    Top,     // 常に上部
    Right,   // 常に右側
    Left,    // 常に左側
}
```

`Auto` がデフォルト。手動指定の場合は構図に関わらず指定位置に配置。横書き/1行回転は配置位置に応じて自動選択（上下→横書き2行、左右→1行回転）。

**手動指定時の余白不足:** 縦構図で `Bottom` を指定した場合など、Padの自然な余白が指定方向にない場合も、余白不足時の画像縮小ロジックが自動適用される（縮小率20%超でスキップ）。つまり手動指定は「この方向に配置してほしい」というリクエストであり、不可能な場合はスキップされる。

## データモデル

### ExifFrameConfig（v2）

```rust
pub struct ExifFrameConfig {
    pub name: String,
    pub position: ExifPosition,
    pub items: DisplayItems,
    pub font: FontConfig,
    pub custom_text: String,
}
```

`enabled` フィールドは持たない。有効/無効は呼び出し側で `Option<&ExifFrameConfig>` として制御する。GUIの状態管理で `enabled: bool` が必要な場合は、`process_image` に渡す前に `enabled` なら `Some`、でなければ `None` にフィルタする。

v1から削除されたフィールド:
- `layout` → 廃止（Padの余白位置で自動決定）
- `color` → 廃止（Padの背景色を使用）
- `aspect_ratio` → 廃止（Padのアスペクト比設定を使用）
- `frame_padding` → 廃止（Padのパディングを使用）
- `enabled` → 廃止（`Option` で制御）

### DisplayItems（v2）

```rust
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
```

v1から削除: `brand_logo`（使われていなかった）

### process_image の統合

```rust
pub fn process_image(
    input: &Path,
    output: &Path,
    config: &ProcessingConfig,
    exif_frame: Option<&ExifFrameConfig>,
    asset_dirs: Option<&AssetDirs>,
) -> Result<ProcessResult>
```

注: `progress` コールバックは `process_batch` のみが持つ。`process_image` 単体には不要（v1と同様）。

内部フロー:
1. `config.mode` が `Pad` かつ `exif_frame.is_some()` → Pad + Exif統合処理
2. `config.mode` が `Crop` or `Quality` → `exif_frame` を無視（従来通り）
3. Pad統合処理: 必要な余白を算出 → 画像縮小（必要時）→ Padキャンバス生成 → 画像配置 → Exif描画

## ロゴ

### バンドルロゴ

プレースホルダーSVG（18ファイル）を全削除。実ロゴのみバンドル：

| ファイル | 形式 | 用途 |
|---------|------|------|
| `sony.svg` | SVG | Sonyメーカーロゴ（暗背景用） |
| `sony_light.svg` | SVG | Sonyメーカーロゴ（明背景用） |
| `gmaster.png` | PNG | GM レンズブランドロゴ（暗背景用） |
| `gmaster_light.png` | PNG | GM レンズブランドロゴ（明背景用） |

注: 現在リポジトリにある `sony_logo.svg` と `sony_gmaster.webp` は上記ファイル名にリネーム・変換する。WebPはPNGに変換して配置（`image` クレートのPNG読み込みを活用、WebP対応は不要）。

### ロゴ配置

- **メーカーロゴ（Sony）**: Exif情報の左端に配置
- **レンズブランドロゴ（GM）**: レンズ型番の直前に配置（1行目のテキスト内にインライン）

### model_map.json の簡素化

```json
{
  "logo_match": {
    "SONY": { "maker": "sony.svg" },
    "Sony": { "maker": "sony.svg" }
  },
  "lens_brand_match": [
    { "pattern": "GM", "match_type": "contains", "logo": "gmaster.png" }
  ]
}
```

`logo_match` のキーは `camera_make` との**完全一致**で判定する。Exifの `Make` フィールドはメーカーにより表記揺れがあるため（例: "SONY" / "Sony" / "Sony Corporation"）、想定されるバリエーションをすべてキーとして登録する。ユーザーは `model_map_custom.json` で追加バリエーションを登録可能。

削除:
- `camera` セクション（型番→表示名マッピング）
- `brand` フィールド（使われていなかった）
- Sony/GM以外のメーカー・レンズブランドのエントリ

### ユーザーカスタマイズ

ユーザーが `~/.config/picture-tool/assets/logos/` にロゴを追加し、`model_map_custom.json` にマッピングルールを追加することで他メーカー対応が可能。この仕組みはv1から変更なし。

## 機材名表示

型番直接表示。Exifから取得した `camera_model`、`lens_model` をそのまま使用。

```
v1: ILCE-7M4 → α7IV（model_map経由）
v2: ILCE-7M4 → ILCE-7M4（直接表示）
```

`model_map.rs` のdisplay_nameマッピング機能は廃止。ロゴマッチング機能のみ残す。

## GUI変更

### SettingsPanel

- Exifフレームトグルは `mode == "pad"` の場合のみ表示
- プリセット選択・設定ボタンもPadモード時のみ

### ExifFrameSettings モーダル

簡素化する設定項目：

| 残す | 削除 |
|------|------|
| プリセット選択 | レイアウト選択（BottomBar/SideBar/FullBorder） |
| 表示項目ON/OFF | フレーム色（Pad色と同期） |
| Exif配置位置（Auto/上/下/左/右） | アスペクト比（Padの設定を使用） |
| フォント選択・サイズ | フレームパディング |
| カスタムテキスト | |

### process_images コマンド

Padモード以外で `exif_frame_config` が渡された場合、バックエンドで無視する（エラーにはしない）。

## CLI変更

`--exif-frame` オプションは `--mode pad` と組み合わせた場合のみ有効。他のモードとの組み合わせは警告を出してスキップ。

## Exifデータ取得

現在の `read_exif_info()` は以下を取得しており、v2のフレーム表示に必要な情報は揃っている：

- `camera_make` → ロゴマッチング用
- `camera_model` → カメラ型番表示
- `lens_model` → レンズ型番表示 + レンズブランドマッチング用
- `focal_length` → 撮影パラメータ表示
- `f_number` → 撮影パラメータ表示
- `shutter_speed` → 撮影パラメータ表示
- `iso` → 撮影パラメータ表示
- `date_taken` → 日時表示（オプション）

フォーマット処理（単位付加、プレフィックス除去）は `read_exif_info()` 内で実施済み。追加の関数・モジュール作成は不要。

## エラーハンドリング

v1と同方針。「スキップして次へ進む」。追加ケース：

| ケース | 対応 |
|--------|------|
| Padモード以外でExifフレーム指定 | 無視（警告） |
| 余白計算で画像縮小が必要だが極端に小さくなる場合 | Exifフレームスキップ（警告） |

## テスト戦略

### ユニットテスト

- layout — 横構図/縦構図/正方形の余白計算、画像縮小量の算出
- logo — Sony/GMロゴマッチング、SVG/PNG読み込み
- text — 2行横書き描画、1行凝縮回転描画

### 統合テスト

- Padモード + Exifフレーム: 各構図で出力画像が4:5であることを検証
- Padモード + Exifフレーム + 余白不足: 画像縮小後も4:5維持を検証
- Cropモード + Exifフレーム: Exifフレームがスキップされることを検証
- Qualityモード + Exifフレーム: 同上
- EXIF全None: フレーム生成されるがテキスト空
