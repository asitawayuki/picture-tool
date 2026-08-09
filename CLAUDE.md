# picture-tool

Instagram投稿用の画像一括変換ツール。4:5アスペクト比への変換とファイルサイズ制限をCLI/GUIで提供。

## プロジェクト構成

Cargo Workspace（core/cli/gui）+ Svelte 5フロントエンド

```
picture-tool-rust/
├── core/           # 画像処理ライブラリ（共有ロジック）
├── cli/            # CLIバイナリ
├── gui/            # Tauri v2 バックエンド
├── gui-frontend/   # Svelte 5 フロントエンド
└── Makefile        # ビルド/テスト/開発コマンド
```

## 技術スタック

### バックエンド（Rust）
- **image** - 画像の読み込み・操作・JPEGエンコード
- **rayon** - 並列処理
- **walkdir** - ディレクトリ走査
- **clap** (derive) - コマンドライン引数パース
- **anyhow** - エラーハンドリング
- **serde** - JSON直列化（Tauri境界用）
- **Tauri v2** - GUIフレームワーク

### フロントエンド
- **Svelte 5** (runes構文) - UIフレームワーク
- **Vite** - ビルドツール
- **bun** - パッケージマネージャー

## ビルド・開発コマンド

```bash
make build          # CLI + GUI ビルド
make build-cli      # CLIのみ
make build-gui      # GUIのみ（フロントエンド含む）
make test           # 全テスト実行
make dev            # GUI開発サーバー
make release        # リリースビルド
make install        # フロントエンド依存インストール
make clean          # クリーンアップ
```

## CLI仕様

```bash
picture-tool --input ./photos --output ./out --mode crop --quality 90 --max-size 8
```

### オプション
| オプション | 短縮 | デフォルト | 説明 |
|-----------|------|-----------|------|
| `--input` | `-i` | (必須) | 入力フォルダーパス |
| `--output` | `-o` | `./` | 出力フォルダー（存在しない場合は自動作成） |
| `--mode` | `-m` | `crop` | 変換モード: `crop`, `pad`, `quality` |
| `--bg-color` | `-b` | `white` | パディング時の背景色: `white`, `black` |
| `--quality` | `-q` | `90` | 初期JPEG品質 (1-100) |
| `--max-size` | | `8` | 最大ファイルサイズ (MB, 1-1024) |
| `--delete-originals` | | `false` | 変換完了後に元ファイルを削除 |
| `--exif-frame` | `-e` | `false` | Exifフレームを付加（**padモード限定**） |
| `--preset` | `-p` | `default` | Exifフレームのプリセット名 |
| `--preset-file` | | | プリセットJSONを直接指定（`--preset` より優先） |
| `--custom-text` | | | プリセットのカスタムテキストを上書き |

### 変換モード
- **crop** - 4:5に中央クロップ
- **pad** - 4:5にパディング（背景色指定可）
- **quality** - アスペクト比変換なし、サイズ制限のみ適用

### Exifフレーム（v2）
- **padモード限定**。他モードでは警告を出して無視する（余白が無いため描画できない）
- 帯の位置は写真の向きから自動決定（横→下、縦→右。縦は帯ごと90度回転）
- 表示は常に2段（1段目: ロゴ・カメラ・レンズ / 2段目: 焦点距離・F値・SS・ISO）
- CLIから項目や位置を個別指定するオプションは無い。細かい制御はプリセットJSON
  （`--preset-file`、または GUI で作って `--preset` で名前指定）で行う

## Core ライブラリ API

`picture-tool-core` クレートが画像処理ロジックを提供。CLI/GUIが共有利用。

| 分類 | API |
|------|-----|
| 検証・収集 | `validate_config`, `is_supported_image`, `collect_image_files` |
| 変換 | `process_image`, `process_batch`（`ProgressCallback` で進捗とキャンセル） |
| EXIF | `read_exif_info`, `apply_orientation`, `oriented_dimensions`, `open_image_oriented`, `image_dimensions_oriented` |
| base64出力（GUI用） | `generate_thumbnail_base64`, `generate_full_image_base64`, `generate_exif_frame_preview_base64` |
| Exifフレーム | `exif_frame::render_exif_frame`, `exif_frame::AssetDirs`, `exif_frame::ExifAssets`, `exif_frame::preset::{list_all_presets, save_preset, delete_preset}` |
| 定数 | `CANCELLED_ERROR`（着手前にキャンセルされた印）, `THUMBNAIL_MAX_DIMENSION` |

`ExifAssets` は**バッチの前に1回だけ**構築して使い回す（画像ごとに作るとモデルマップを
毎回読み直す）。core は `eprintln!` せず、利用者に伝えるべき事象は `ProcessResult.warnings`
に積んで呼び出し元へ渡す。

## GUI

3カラムレイアウト: フォルダーツリー | サムネイルグリッド | 選択リスト+設定

### バックエンド（`gui/src/`）の前提

**Tauri コマンドがこのアプリの唯一の信頼境界**。Tauri v2 の capabilities/ACL は
プラグインコマンドにしか効かず、`generate_handler!` で登録した自作コマンドには
適用されない。したがって:

- webview から来たパス・フォント・プリセット名は、必ず `gui/src/security.rs` の
  検証関数を通してから使う。**コマンドを追加するときは必ずここを経由させること**
- webview にプラグイン権限を与えない（`gui/capabilities/default.json` は `core:default` のみ）。
  フォルダー選択もお気に入りの保存も、パスを Rust 側に固定したコマンドで行う
- 不可逆な操作（元ファイル削除）は OS ネイティブのダイアログで確認する。
  webview 内の確認ダイアログは乗っ取られた状態では素通りする

詳細と脅威モデルは `gui/src/security.rs` のモジュールコメント、
判断の経緯は `docs/superpowers/plans/2026-08-04-full-codebase-review-fixes.md` の S6 節。

### ユーザーデータの置き場所

すべて `<OSの設定ディレクトリ>/picture-tool/` 配下（Linux: `~/.config/picture-tool/`）。
パスを組み立てるのは `core::exif_frame::AssetDirs` の1箇所だけ。直書きしないこと。

| パス | 内容 |
|---|---|
| `assets/logos/` | ユーザー追加のロゴ（同梱ロゴより優先） |
| `assets/fonts/` | ユーザー追加のフォント。**GUIが読めるフォントはここだけ** |
| `presets/` | Exifフレームのプリセット（GUI/CLI 共用） |
| `model_map_custom.json` | メーカー名→ロゴのマッピング上書き |

お気に入りフォルダーは Tauri のアプリデータ配下 `favorites.json`（`store` プラグイン、
ただしアクセスは Rust 側の `load_favorites` / `save_favorites` コマンド経由に限定）。

## 設計方針

- 元の画像ファイルは上書きしない（`--delete-originals`で明示的に削除）
- 画像読み込み失敗時はスキップして次へ進む
- coreライブラリはTauri非依存（ProgressCallbackで疎結合）
- core は `eprintln!` しない。利用者に伝える事象は `ProcessResult.warnings` に積み、
  CLI は stderr、GUI は結果ダイアログに出す
- Svelte 5のrunes構文（`$state`, `$derived`, `$effect`）を使用。旧構文は使わない

## リポジトリに含めているもの

- `.claude/agents/`, `.claude/skills/` — **意図的にコミットしている**。このリポジトリで
  作業するときの共通のエージェント定義／スキルであり、チームと将来の自分が同じ手順で
  作業できるようにするため。内容を更新したらコミットすること
  （個人設定は `.claude/settings.local.json` 側に置く）
- `docs/` — 設計仕様と実装計画。入口は [`docs/README.md`](./docs/README.md)。
  **「なぜその形なのか」は plans の実施メモにある**
