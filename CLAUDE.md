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
| `--max-size` | | `8` | 最大ファイルサイズ (MB) |
| `--delete-originals` | | `false` | 変換完了後に元ファイルを削除 |

### 変換モード
- **crop** - 4:5に中央クロップ
- **pad** - 4:5にパディング（背景色指定可）
- **quality** - アスペクト比変換なし、サイズ制限のみ適用

## Core ライブラリ API

`picture-tool-core` クレートが画像処理ロジックを提供。CLI/GUIが共有利用。

主要関数: `validate_config`, `collect_image_files`, `process_image`, `process_batch`, `generate_thumbnail_base64`

## GUI

3カラムレイアウト: フォルダーツリー | サムネイルグリッド | 選択リスト+設定

## 設計方針

- 元の画像ファイルは上書きしない（`--delete-originals`で明示的に削除）
- 画像読み込み失敗時はスキップして次へ進む
- coreライブラリはTauri非依存（ProgressCallbackで疎結合）
- Svelte 5のrunes構文（`$state`, `$derived`, `$effect`）を使用。旧構文は使わない
