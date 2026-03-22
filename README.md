# Picture Tool

Instagram投稿用の画像一括変換ツール。写真を4:5アスペクト比に変換し、ファイルサイズを制限します。CLI/GUIの2つのインターフェースを提供。

## 機能

- **4:5アスペクト比変換** — クロップ / パディング / サイズのみの3モード
- **ファイルサイズ制限** — 品質を自動調整して指定サイズ以下に圧縮
- **並列処理** — rayonによる高速バッチ処理
- **GUIアプリ** — フォルダー参照 → 写真選択 → プレビュー → 変換の一連のフロー
- **元ファイル削除オプション** — 変換完了後に元ファイルを自動削除可能
- **対応フォーマット** — JPEG, PNG, WebP（出力は常にJPEG）

## セットアップ

### 前提条件

- [Rust](https://rustup.rs/) 1.70以降
- [Bun](https://bun.sh/)（フロントエンドビルド用）
- Tauri v2の[システム依存](https://v2.tauri.app/start/prerequisites/)（GUI使用時）

### インストール

```bash
make install   # フロントエンド依存のインストール
make build     # CLI + GUI ビルド
```

## 使い方

### CLI

```bash
# 基本（4:5にクロップ）
cargo run -p picture-tool -- -i ./photos -o ./output

# パディングモード（黒背景）
cargo run -p picture-tool -- -i ./photos -o ./output -m pad -b black

# サイズ制限のみ（アスペクト比変更なし）
cargo run -p picture-tool -- -i ./photos -o ./output -m quality

# 変換後に元ファイルを削除
cargo run -p picture-tool -- -i ./photos -o ./output --delete-originals

# 品質とサイズ上限を指定
cargo run -p picture-tool -- -i ./photos -o ./output -q 95 --max-size 10
```

### GUI

```bash
make dev
```

3カラムのデスクトップアプリが起動します：

- **左パネル** — フォルダーツリーで写真を探す
- **中央パネル** — サムネイルグリッドで写真をクリック選択
- **右パネル** — 選択した写真の確認、変換設定、実行

### CLIオプション一覧

| オプション | 短縮 | デフォルト | 説明 |
|-----------|------|-----------|------|
| `--input` | `-i` | (必須) | 入力フォルダーパス |
| `--output` | `-o` | `./` | 出力フォルダー（自動作成） |
| `--mode` | `-m` | `crop` | `crop`, `pad`, `quality` |
| `--bg-color` | `-b` | `white` | `white`, `black` |
| `--quality` | `-q` | `90` | 初期JPEG品質 (1-100) |
| `--max-size` | | `8` | 最大ファイルサイズ (MB) |
| `--delete-originals` | | `false` | 変換後に元ファイルを削除 |

## 開発コマンド

```bash
make build          # CLI + GUI ビルド
make build-cli      # CLIのみ
make build-gui      # GUIのみ（フロントエンド含む）
make test           # テスト実行（23件）
make dev            # GUI開発サーバー
make release        # リリースビルド
make clean          # クリーンアップ
```

## プロジェクト構成

```
picture-tool-rust/
├── core/           # 画像処理ライブラリ（CLI/GUI共有）
├── cli/            # CLIバイナリ
├── gui/            # Tauri v2 バックエンド
├── gui-frontend/   # Svelte 5 フロントエンド
└── Makefile
```

## 技術スタック

- **Rust** — image, rayon, clap, anyhow, serde
- **Tauri v2** — デスクトップGUIフレームワーク
- **Svelte 5** — フロントエンドUI（runes構文）
- **Bun + Vite** — フロントエンドビルド

## 動作仕様

### 変換モード

| モード | 動作 |
|--------|------|
| **crop** | 中央を基準に4:5にクロップ |
| **pad** | 余白を追加して4:5に（背景色指定可） |
| **quality** | アスペクト比変更なし、サイズ制限のみ |

### サイズ圧縮

1. 初期品質で保存を試行
2. サイズ超過の場合、品質を5%ずつ下げて再試行
3. 最低品質60%まで下げても超過の場合はそのまま保存

### 出力

- ファイル名: `{元のファイル名}_processed.jpg`（重複時は連番追加）
- 元の画像ファイルは上書きしない（`--delete-originals`で明示的に削除）
- 読み込み失敗した画像はスキップして継続
