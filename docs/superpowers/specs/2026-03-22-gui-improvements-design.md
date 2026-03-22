# picture-tool GUI改善 設計書

## 概要

picture-tool GUIの品質改善。バグ修正4件と機能改善4件を2フェーズに分けて実装する。

## Phase 1: バグ修正

### BUG-1: CPU 100% サムネイル問題

**原因:** フォルダー選択時に全画像（50枚/ページ）のサムネイルを一斉リクエスト。各`get_thumbnail`はフルサイズ画像をデコードしてからリサイズするため、20〜30枚を超えるとCPU 100%に張り付く。バックエンド側のキャッシュもない。

**修正方針:**
- **フロントエンド (`ThumbnailGrid.svelte`):** Intersection Observerで可視領域の画像のみロード対象にする
- **フロントエンド (`App.svelte`):** `loadThumbnail`関数内に同時リクエスト数3〜4の並列制限キューを実装。`ThumbnailGrid`と`SelectionList`の両方からのリクエストを一元的にスロットリングする
- **バックエンド (`gui/src/commands.rs`, `gui/src/state.rs`):** `lru`クレートを使用してメモリ内LRUキャッシュを追加。パスをキーにbase64サムネイルを保持し、同一画像の再デコードを回避。**キャッシュ上限: 500エントリ**（200px JPEG base64は概ね5〜20KB/枚、最大約10MB相当）

### BUG-2: フォルダーツリー折りたたみ不具合

**原因:** `FolderTree.svelte`の`selectFolder()`は`!node.expanded`の場合のみ`toggleNode()`を呼ぶ。展開済みフォルダーをクリックしても折りたたまれない。`toggleNode()`自体は折りたたみロジックを持っている。

**修正方針:**
- `selectFolder()`内の`!node.expanded`条件を除去し、常に`toggleNode(node)`を呼び出す
- 既存の`toggleNode()`が展開/折りたたみを正しくトグルするため、新たなロジック追加は不要

### BUG-3: スライダー端が到達しない

**原因:** CSSの`input[type="range"]`スタイリング（カスタムthumbサイズとtrack padding）の干渉が疑われる。

**修正方針:**
- 実コードを確認し、thumb/trackのCSS干渉を特定・修正
- `width`、`padding`、`margin`の調整

### BUG-4: 選択リストのサムネイル非表示

**原因:** `App.svelte`の`thumbnailCache`（常に空のMap）と`ThumbnailGrid.svelte`の独自`thumbnailCache`（実データあり）が分離している。`SelectionList`はApp側の空キャッシュを受け取るため、サムネイルが常にプレースホルダー表示。

**修正方針:**
- サムネイルキャッシュ（`Map<string, string>`）を`App.svelte`に一元化
- `App.svelte`にサムネイルロード関数`loadThumbnail(path: string)`を定義。この関数がキャッシュチェック → `getThumbnail` API呼び出し → キャッシュ更新を行う
- `ThumbnailGrid`はIntersection Observerで可視になった画像のパスに対してpropsで受け取った`onRequestThumbnail(path)`コールバックを呼ぶ。Appがそれを受けて`loadThumbnail`を実行し、キャッシュを更新
- `ThumbnailGrid`と`SelectionList`の両方が同一の`thumbnailCache` propsを参照して表示

## Phase 2: 機能改善

### FEAT-1: 画像プレビュー（ダブルクリック拡大）

**新コンポーネント:** `ImagePreview.svelte` — フルスクリーンモーダル

**UI構成:**
- 背景: 半透明黒オーバーレイ
- 中央: 画面にフィットした高画質画像
- 左上: 選択トグルボタン（未選択: 枠線のみ「○ 選択する」、選択済み: アクセント色「✓ 選択済み」）
- 右上: ✕ 閉じるボタン
- 左右: ← → ナビゲーション矢印
- 下部: 情報バー（ファイル名、解像度、ファイルサイズ、ページ位置）

**操作:**
| 操作 | 動作 |
|------|------|
| ダブルクリック（サムネイル/選択リスト） | モーダルを開く |
| ← / → キー | 前後の画像に移動（ページ境界では自動的に前後ページに遷移） |
| Escape / ✕ / 背景クリック | モーダルを閉じる |
| 左上ボタン / Space キー | 選択トグル |

**ページ境界の動作:** ←/→キーで現在ページの端に達した場合、自動的に前/次ページに遷移する。全画像リストの先頭/末尾では矢印を非表示にする。ページ遷移を実現するため、`currentPage`を`App.svelte`にリフトアップし、`ThumbnailGrid`にpropsとして渡す。`ImagePreview`からのページ変更要求は`onPageChange(page)`コールバックで`App.svelte`に通知する。

**バックエンド:** 新コマンド`get_full_image`を追加。

```
コマンドシグネチャ: get_full_image(path: String, max_width: u32, max_height: u32) -> String
```

- フロントエンドがウィンドウサイズから`max_width`/`max_height`を渡す
- 元画像を指定サイズに収まるようアスペクト比を維持してリサイズ
- JPEG品質はバックエンド側の定数90固定（シグネチャに含めない。プレビュー用途で品質を変える必要がないため）
- base64文字列で返す
- 最大解像度上限: 2560×1600（これを超えるmax_width/max_heightが渡された場合はクランプ）

### FEAT-2: サムネイルサイズ可変

**UI:** `ThumbnailGrid`上部にツールバーを追加。スライダーで列数を制御（2列〜8列）。

**実装:**
- CSSの`grid-template-columns`を`repeat(N, 1fr)`で動的に設定
- サムネイルのリクエストサイズ（200px）は変更不要
- 将来的にキーボードショートカット対応（+/-キー等）

### FEAT-3: フォルダーお気に入り

**UI:**
- フォルダーツリー上部に「⭐ お気に入り」セクションを常時表示
- ドライブ一覧の上に配置し、セパレータで区切る
- お気に入りフォルダーをクリックで直接移動

**操作:**
- 右クリックメニューで「⭐ お気に入りに追加」
- お気に入りセクション内で右クリック「お気に入りから削除」

**永続化:**
- `tauri-plugin-store` v2のフロントエンドJavaScript APIを直接使用（バックエンドコマンドを経由しない。storeプラグインはフロントエンドから直接読み書き可能なため、専用コマンドは不要）
- 保存先: アプリデータディレクトリ内の`favorites.json`
- データ形式: パスの配列 `["C:\\Users\\...\\Photos", ...]`
- 並び順: 追加順（ドラッグ並べ替えは将来対応）
- `FolderTree.svelte`内でstoreの読み書きを直接行う

### FEAT-4: 出力先フォルダー初期位置

**変更箇所:** `SettingsPanel.svelte`のフォルダー選択ダイアログ呼び出し

**修正内容:** `open({ directory: true })` → `open({ directory: true, defaultPath: currentFolder })`

`currentFolder`は`App.svelte`に新設する`$state`変数。`FolderTree`の`onSelectFolder`コールバックで更新し、`SettingsPanel`にpropsとして渡す。

## スコープ外（将来対応）

- システムテーマ対応（ダーク/ライト自動切替）
- EXIFフレーム機能（カメラ情報フレーム付加）

## 変更対象ファイル一覧

### フロントエンド (`gui-frontend/src/`)
| ファイル | 変更内容 |
|---------|---------|
| `App.svelte` | サムネイルキャッシュ一元化、`loadThumbnail`関数（並列制限キュー内蔵）、`currentFolder` state追加、`currentPage` stateリフトアップ、プレビューモーダル状態管理 |
| `lib/FolderTree.svelte` | 折りたたみトグル修正、お気に入りセクション、右クリックメニュー、store直接利用 |
| `lib/ThumbnailGrid.svelte` | Intersection Observer、並列制限、`onRequestThumbnail`コールバック、サイズスライダー、ダブルクリック |
| `lib/SelectionList.svelte` | 共有キャッシュ参照によるサムネイル表示、ダブルクリックプレビュー |
| `lib/SettingsPanel.svelte` | `currentFolder` props追加、出力先defaultPath、スライダーCSS修正 |
| `lib/ImagePreview.svelte` | **新規** — プレビューモーダル（選択トグル付き） |
| `lib/api.ts` | `get_full_image` 追加 |
| `lib/types.ts` | プレビュー関連の型追加 |
| `app.css` | スライダーCSS修正 |

### バックエンド (`gui/src/`)
| ファイル | 変更内容 |
|---------|---------|
| `commands.rs` | `get_full_image` コマンド追加 |
| `state.rs` | サムネイルLRUキャッシュ追加（`lru::LruCache`、上限500エントリ） |
| `main.rs` | `get_full_image`コマンド登録、`tauri-plugin-store`プラグイン登録 |

### 依存関係 (`gui/`)
| ファイル | 変更内容 |
|---------|---------|
| `Cargo.toml` | `lru`クレート、`tauri-plugin-store` v2 依存追加 |
| `tauri.conf.json` | store pluginのcapability追加（`store:allow-set`, `store:allow-get`, `store:allow-save`, `store:allow-load`等） |
| `gui/capabilities/default.json` | store関連permissionを追加（Tauri v2のcapabilityファイルが別ファイルの場合） |
| `gui-frontend/package.json` | `@tauri-apps/plugin-store` 追加 |
