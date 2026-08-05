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

## ライセンス

本ソフトウェアは [MIT License](./LICENSE) で提供されます。

### 同梱アセット

| アセット | 権利者 | ライセンス |
|---------|--------|-----------|
| `core/assets/fonts/NotoSansJP-Regular.otf` | Adobe | [SIL Open Font License 1.1](./core/assets/fonts/OFL.txt) |
| `core/assets/logos/*` | 各商標権者 | 下記「商標について」を参照 |

### 商標について

`core/assets/logos/` に同梱しているメーカーロゴおよびレンズブランドロゴ
（SONY、G Master、FUJIFILM）は、それぞれソニーグループ株式会社および
富士フイルムホールディングス株式会社の**登録商標**であり、権利は各社に帰属します。

- 本ツールは各社とは**一切関係がなく**、提携・後援・承認を受けたものではありません
- ロゴは、生成される画像において**撮影機材を識別する目的でのみ**使用しています
- ロゴの著作権・商標権は各社に留保されており、本ソフトウェアの MIT License は
  これらのロゴには適用されません
- 商用利用など、各社のブランドガイドラインに抵触しうる用途では、
  利用者自身の責任で権利者に確認してください

### ロゴを差し替える / 追加する

同梱ロゴを使わず、自分で用意したロゴだけを使うこともできます。
以下のディレクトリに置いたファイルは**同梱ロゴより優先**されます。

| OS | 配置先 |
|----|--------|
| Linux | `~/.config/picture-tool/assets/logos/` |
| macOS | `~/Library/Application Support/picture-tool/assets/logos/` |
| Windows | `%APPDATA%\picture-tool\assets\logos\` |

ファイル名は `{名前}.svg` / `{名前}.png`、暗い背景用のバリアントは
`{名前}_light.svg` / `{名前}_light.png`（SVG 優先）。
どのメーカー名（Exif の Make タグ）でどのファイルを使うかは、
`assets/` の1つ上（例: `~/.config/picture-tool/model_map_custom.json`）に置いた
JSON で上書きできます。`logo_match` と `lens_brand_match` は**両方とも必須**です。

```json
{
  "logo_match": {
    "NIKON CORPORATION": { "maker": "nikon.svg" }
  },
  "lens_brand_match": [
    { "pattern": "NIKKOR Z", "match_type": "contains", "logo": "nikkor_z.svg" }
  ]
}
```
