# picture-tool GUI改善 設計書

## 概要

picture-tool GUIの品質改善。バグ修正4件と機能改善4件を2フェーズに分けて実装する。

## Phase 1: バグ修正

### BUG-1: CPU 100% サムネイル問題

**原因:** フォルダー選択時に全画像（50枚/ページ）のサムネイルを一斉リクエスト。各`get_thumbnail`はフルサイズ画像をデコードしてからリサイズするため、20〜30枚を超えるとCPU 100%に張り付く。バックエンド側のキャッシュもない。

**修正方針:**
- **フロントエンド (`ThumbnailGrid.svelte`):** Intersection Observerで可視領域の画像のみロード。同時リクエスト数を3〜4に制限する並列制限キューを実装
- **バックエンド (`gui/src/commands.rs`, `gui/src/state.rs`):** メモリ内LRUキャッシュを追加。パスをキーにbase64サムネイルを保持し、同一画像の再デコードを回避

### BUG-2: フォルダーツリー折りたたみ不具合

**原因:** `FolderTree.svelte`の`selectFolder()`はフォルダー選択と未展開時の展開のみを行い、展開済みフォルダーの折りたたみ処理がない。

**修正方針:**
- `selectFolder()`にトグルロジックを追加。展開済みなら折りたたむ
- クリック = 選択 + 展開/折りたたみトグル

### BUG-3: スライダー端が到達しない

**原因:** CSSの`input[type="range"]`スタイリング（カスタムthumbサイズとtrack padding）の干渉が疑われる。

**修正方針:**
- 実コードを確認し、thumb/trackのCSS干渉を特定・修正
- `width`、`padding`、`margin`の調整

### BUG-4: 選択リストのサムネイル非表示

**原因:** `App.svelte`の`thumbnailCache`（常に空のMap）と`ThumbnailGrid.svelte`の独自`thumbnailCache`（実データあり）が分離している。`SelectionList`はApp側の空キャッシュを受け取るため、サムネイルが常にプレースホルダー表示。

**修正方針:**
- サムネイルキャッシュを`App.svelte`に一元化
- `ThumbnailGrid`内のキャッシュ管理ロジックをAppに移動し、ロード関数をコールバックで渡す
- `ThumbnailGrid`と`SelectionList`の両方が同一キャッシュを参照

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
| ← / → キー | 前後の画像に移動 |
| Escape / ✕ / 背景クリック | モーダルを閉じる |
| 左上ボタン / Space キー | 選択トグル |

**バックエンド:** 新コマンド`get_full_image`を追加。画面サイズに合わせたリサイズ版をbase64で返す（元画像そのままだと転送コストが大きいため）。

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
- `tauri-plugin-store`でアプリデータディレクトリ内の`favorites.json`に保存
- データ形式: パスの配列 `["C:\\Users\\...\\Photos", ...]`
- 並び順: 追加順（ドラッグ並べ替えは将来対応）

**バックエンド:** お気に入りの読み書き用Tauriコマンドを追加（`get_favorites`, `add_favorite`, `remove_favorite`）

### FEAT-4: 出力先フォルダー初期位置

**変更箇所:** `SettingsPanel.svelte`のフォルダー選択ダイアログ呼び出し

**修正内容:** `open({ directory: true })` → `open({ directory: true, defaultPath: currentFolder })`

`currentFolder`はフォルダーツリーで現在選択中のパスをpropsで受け取る。

## スコープ外（将来対応）

- システムテーマ対応（ダーク/ライト自動切替）
- EXIFフレーム機能（カメラ情報フレーム付加）

## 変更対象ファイル一覧

### フロントエンド (`gui-frontend/src/`)
| ファイル | 変更内容 |
|---------|---------|
| `App.svelte` | サムネイルキャッシュ一元化、プレビューモーダル状態管理 |
| `lib/FolderTree.svelte` | 折りたたみトグル修正、お気に入りセクション、右クリックメニュー |
| `lib/ThumbnailGrid.svelte` | Intersection Observer、並列制限、サイズスライダー、ダブルクリック |
| `lib/SelectionList.svelte` | 共有キャッシュ利用、ダブルクリックプレビュー |
| `lib/SettingsPanel.svelte` | 出力先defaultPath、スライダーCSS修正 |
| `lib/ImagePreview.svelte` | **新規** — プレビューモーダル |
| `lib/api.ts` | `get_full_image`, `get_favorites`, `add_favorite`, `remove_favorite` 追加 |
| `lib/types.ts` | プレビュー・お気に入り関連の型追加 |
| `app.css` | スライダーCSS修正 |

### バックエンド (`gui/src/`)
| ファイル | 変更内容 |
|---------|---------|
| `commands.rs` | `get_full_image`, `get_favorites`, `add_favorite`, `remove_favorite` コマンド追加 |
| `state.rs` | サムネイルLRUキャッシュ、お気に入り状態追加 |
| `main.rs` | 新コマンド登録、`tauri-plugin-store`プラグイン登録 |

### 設定 (`gui/`)
| ファイル | 変更内容 |
|---------|---------|
| `Cargo.toml` | `tauri-plugin-store` 依存追加 |
| `tauri.conf.json` | store pluginの許可設定（必要に応じて） |
