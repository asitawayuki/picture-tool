# GUI改善 v2 設計

## 概要

5つの改善を実施する: お気に入りツリー展開、スライダー視認性、LrCルーペ方式プレビュー、EXIF情報表示、サムネイル改善。

## 1. お気に入りフォルダーのツリー展開

**変更箇所:** `gui-frontend/src/lib/FolderTree.svelte`

### 現状
お気に入りはフラットな`<button>`リストで表示。サブフォルダーへの展開不可。

### 変更
お気に入りパスを`TreeNode`に変換し、既存の`treeNode`スニペットでレンダリングする。

- `initStore()`でお気に入り読み込み時に、各パスを`TreeNode`として構築
- `toggleNode()`（遅延ロード）をそのまま共有
- フォルダー選択・右クリックメニューも既存ロジックを共有
- お気に入り削除は右クリックメニューから（既存の挙動を維持）

### 表示イメージ
```
⭐ お気に入り
  📁 Photos          ← クリック → 画像一覧表示
    📂 2024           ← 展開可能（通常ツリーと同じ）
      📁 December
    📁 2025
  📁 Travel
    📁 Tokyo
```

## 2. スライダーの視認性修正

**変更箇所:** `gui-frontend/src/lib/SettingsPanel.svelte`, `gui-frontend/src/lib/ThumbnailGrid.svelte`

### 現状
- SettingsPanel: トラック背景が`var(--bg-primary)`(#0f0f1a)で背景と同化
- ThumbnailGrid: トラック背景が`var(--border-color)`(#333)で視認性低い
- 列数スライダーの🖼絵文字が環境依存で文字化け

### 変更
- 両コンポーネントのトラック背景色を`#555`程度に統一
- 🖼絵文字を「列」テキストまたはSVGグリッドアイコンに置換

## 3. プレビューのLrCルーペ方式

**変更箇所:** `gui-frontend/src/lib/ImagePreview.svelte`

### 動作フロー
1. プレビュー表示 → Fit表示（画面にフィット）
2. 画像をクリック → クリック位置を中心に100%表示
3. 100%表示中にマウス移動 → 表示位置がマウスに追従（`transform-origin`更新）
4. 再クリック → Fitに戻る

### 実装方式
- 状態: `zoomed: boolean`
- Fit時: `object-fit: contain`
- 100%時: 画像の元ピクセルサイズ（`image.width`, `image.height`）と表示中の`<img>`要素のレンダリングサイズ（`getBoundingClientRect()`）からスケール倍率`N = naturalWidth / renderedWidth`を算出。`transform: scale(N)`で拡大
- コンテナに`overflow: hidden`を設定し、拡大した画像がモーダル外にはみ出さないようにする
- マウス追従: `onmousemove`で画像要素内の相対座標を算出し、`transform-origin: ${x}% ${y}%`を更新
- カーソル: Fit時は`zoom-in`、100%時は`zoom-out`

### 既存機能との共存
- 左右矢印ボタン・キーボードナビゲーション → 維持
- 画像切り替え時 → `zoomed = false`にリセット
- オーバーレイ（画像外）クリック → モーダルを閉じる
- 画像上のクリック → ズームトグル（イベント伝播を止める）

## 4. EXIF情報表示

**変更箇所:** `core/Cargo.toml`, `core/src/lib.rs`, `gui/src/commands.rs`, `gui/src/types.rs`, `gui-frontend/src/lib/api.ts`, `gui-frontend/src/lib/types.ts`, `gui-frontend/src/lib/ImagePreview.svelte`

### Rust側

**依存追加:** `kamadak-exif`クレートを`core/Cargo.toml`に追加

**新規構造体（core）:**
```rust
#[derive(Debug, Clone, Default, Serialize)]
pub struct ExifInfo {
    pub camera_make: Option<String>,    // "SONY"
    pub camera_model: Option<String>,   // "ILCE-7M4"
    pub lens_model: Option<String>,     // "FE 24-70mm F2.8 GM II"
    pub focal_length: Option<String>,   // "35mm"
    pub f_number: Option<String>,       // "f/2.8"
    pub shutter_speed: Option<String>,  // "1/250s"
    pub iso: Option<u32>,               // 400
    pub date_taken: Option<String>,     // "2024-12-25 14:30"
}
```

**新規関数（core）:** `read_exif_info(path: &Path) -> Result<ExifInfo>`
- EXIF非対応形式（PNG等）やEXIFデータなしの画像 → `ExifInfo::default()`（全フィールドNone）を返す。エラーではない

**新規Tauriコマンド（gui）:** `get_exif_info(path: String) -> ExifInfo`
- gui/src/main.rsの`invoke_handler`にも登録が必要

**TypeScript側の型:**
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

### フロントエンド

**左上オーバーレイ（テキストシャドウのみ、背景なし）:**
```
SONY ILCE-7M4 | FE 24-70mm F2.8 GM II
35mm  f/2.8  1/250s  ISO 400
```

- `text-shadow: 0 1px 3px rgba(0,0,0,0.8)` で読みやすさ確保
- フォント: 12px程度、白文字
- EXIF情報がない画像 → オーバーレイ非表示
- 部分的にデータがない場合 → その項目を省略して詰める

**下部情報バーに撮影日時を追加:**
```
IMG_1234.jpg | 6000×4000 · 12.3MB · 2024-12-25 14:30 | 3 / 25
```

### データフロー
1. プレビュー開く → `getFullImage()` と `getExifInfo()` を並列呼び出し
2. EXIF取得完了 → オーバーレイ表示
3. 画像ナビゲーション → 新しい画像のEXIFを再取得

## 5. サムネイル改善

**変更箇所:** `gui-frontend/src/lib/ThumbnailGrid.svelte`, `gui/src/commands.rs`, `core/src/lib.rs`, `gui-frontend/src/lib/api.ts`

### アスペクト比保持
- `object-fit: cover` → `object-fit: contain` に変更
- 4:5枠内に余白付きで表示（余白はthumb-wrapperの背景色）

### 4K対応の動的解像度
- 現在: `max_dimension: 200` 固定
- 変更: フロントエンドから列数に応じたサムネイルサイズを渡す
- `get_thumbnail(path, max_dimension)` — 引数にサイズを追加
- フロントエンド: グリッドコンテナの`clientWidth`を基準に`Math.ceil(containerWidth / columnCount)`で算出
- Rustキャッシュキー: `"${path}:${max_dimension}"` に変更（同じ画像でもサイズ違いを区別）
- 列数変更時にキャッシュミスしたサムネイルを再取得

## 新規依存

- `kamadak-exif` (core/Cargo.toml)

## 新規ファイル

なし（既存ファイルの修正のみ）

## 変更ファイル一覧

| ファイル | 改善# |
|---------|-------|
| `core/Cargo.toml` | 4 |
| `core/src/lib.rs` | 4, 5 |
| `gui/src/commands.rs` | 4, 5 |
| `gui/src/types.rs` | 4 |
| `gui-frontend/src/lib/api.ts` | 4, 5 |
| `gui-frontend/src/lib/types.ts` | 4 |
| `gui-frontend/src/lib/FolderTree.svelte` | 1 |
| `gui-frontend/src/lib/SettingsPanel.svelte` | 2 |
| `gui-frontend/src/lib/ThumbnailGrid.svelte` | 2, 5 |
| `gui/src/main.rs` | 4 |
| `gui-frontend/src/lib/ImagePreview.svelte` | 3, 4 |
