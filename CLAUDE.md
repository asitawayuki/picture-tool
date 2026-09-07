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
| `--max-width` | | (無制限) | 出力4:5キャンバス幅の上限 (px, 4-20000)。**crop / pad 限定** |
| `--delete-originals` | | `false` | 変換完了後に元ファイルを削除 |
| `--exif-frame` | `-e` | `false` | Exifフレームを付加（**padモード限定**） |
| `--preset` | `-p` | `default` | Exifフレームのプリセット名 |
| `--preset-file` | | | プリセットJSONを直接指定（`--preset` より優先） |
| `--custom-text` | | | プリセットのカスタムテキストを上書き |

### 終了コード
- `0` — 全部変換できた（画像が 1 枚も見つからなかった場合も 0）
- `1` — **1 枚でも変換できなかった**、または起動時に失敗した（入力フォルダーが無い等）

`--max-size` に収まらなかった枚数は失敗に数えない（変換自体はできている）。
警告として stderr に出るだけで終了コードは 0 のまま。

### 変換モード
- **crop** - 4:5に中央クロップ
- **pad** - 4:5にパディング（背景色指定可）
- **quality** - アスペクト比変換なし、サイズ制限のみ適用

### 出力幅の上限（`--max-width`）
- **crop と pad 限定**。quality モードでは警告を出して無視する（4:5 に変換しないため
  「幅」で長辺を縛れない）
- 実効値は 4 の倍数へ**切り捨て**（1002 → 1000）。切り上げると指定値を超えてしまう
- **上限であって目標ではない**。元がそれより小さければ拡大しない
- 設計と算術的な導出は [`docs/superpowers/specs/2026-08-12-output-width-limit-design.md`](./docs/superpowers/specs/2026-08-12-output-width-limit-design.md)

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
| base64出力（GUI用） | `generate_thumbnail_base64`, `generate_full_image_base64`, `generate_exif_frame_preview_base64`（→ `ExifFramePreview { base64, warnings }`） |
| Exifフレーム | `exif_frame::render_exif_frame`（→ `ExifFrameOutput { image, warnings }`）, `exif_frame::AssetDirs`, `exif_frame::ExifAssets`, `exif_frame::preset::{list_all_presets, save_preset, delete_preset}` |
| メタデータ編集 | `metadata::read_metadata`（→ `MetadataRead { metadata, warnings }`）, `metadata::write_metadata`, `metadata::EditableMetadata` |
| 定数 | `CANCELLED_ERROR`（着手前にキャンセルされた印）, `THUMBNAIL_MAX_DIMENSION` |

`ExifAssets` は**バッチの前に1回だけ**構築して使い回す（画像ごとに作るとモデルマップを
毎回読み直す）。core は `eprintln!` せず、利用者に伝えるべき事象は `ProcessResult.warnings`
に積んで呼び出し元へ渡す。

`render_exif_frame` は `ExifFrameOutput { image, warnings }` を、
`generate_exif_frame_preview_base64` は `ExifFramePreview { base64, warnings }` を返す。
どちらも `warnings` には「写真が小さすぎて Exif フレームを描けなかった」等が載る。
**GUI のプレビューはこの warnings を利用者に出さない**（プレビューは長辺 400px 固定で、
実出力ではフレームが出る写真でも `skip_exif` に落ちるため）。捨てる判断は core ではなく
境界（`gui/src/commands.rs`）の責務。

## GUI

navigation rail による 3 モード構成（変換 / 情報 / フレーム）。
デザイントークンとプリミティブは `gui-frontend/src/styles/` と
`gui-frontend/src/lib/ui/`。**他のファイルに生の色を書かない**
（余白・角丸・タイポもトークン経由。部品固有の内部寸法だけは例外）。

| モード | 左 | 中央 | 右 |
|---|---|---|---|
| 変換 | フォルダーツリー | 写真グリッド | 変換設定 |
| 情報 | フォルダーツリー | 写真グリッド | メタデータ（タイトル・コメント・★の読み書き。元ファイルへ保存する） |
| フレーム | プリセット一覧 | フレームのプレビュー | フレーム設定 |

カラム幅と右パネルの折りたたみは `localStorage` に永続化する
（`gui-frontend/src/lib/shell/columns.ts`）。

テーマは rail 下端のボタンで **システム追従 / ライト / ダーク** を巡る
（`gui-frontend/src/lib/shell/theme.ts`。設定は `localStorage`）。
`data-theme` は**常に明示的に書く** ── メディアクエリ任せにしないのは、
**Linux では WebKitGTK の `prefers-color-scheme` が OS の設定を映さない**ため。
OS の値は Rust が xdg-desktop-portal から読んで渡す（`gui/src/theme.rs` 冒頭に実測メモ）。

フロントの検証は 2 系統:
- `bun test` — runes を含まない純粋ロジック（幅のクランプ・グリッド寸法・LRU・キュー）
- `bun run e2e` — Playwright。`vite dev` に当てる。見た目の検証は
  `e2e/stub.ts` が `window.__TAURI_INTERNALS__` を差し替える。
  5174 が塞がっている環境では `E2E_PORT=5199 bun run e2e`

### Rust ⇄ TypeScript の境界の形

`gui-frontend/src/lib/types.ts` は Rust の DTO を**手で写したもの**で、片側だけ改名しても
コンパイラは何も言わない（webview は JSON しか見ない）。これを検知するのが
`gui-frontend/src/lib/dto.contract.ts` ── **自動生成物なので手で編集しない**。

- 生成元は `gui/src/types.rs` の `mod contract`。Rust が serde で実際に吐く形を
  TypeScript のリテラルとして書き出し、手書きの `types.ts` に代入している
- Rust 側を変えると `cargo test` が「生成物と食い違う」で落ちる。
  `UPDATE_DTO_CONTRACT=1 cargo test -p picture-tool-gui contract` で生成し直すと、
  今度は `types.ts` を直すまで `bun run typecheck` が落ちる
- `e2e/stub.ts` の応答も**この契約から形を取る**。スタブに形を書き写すと写しが
  3 つ目に増え、スタブだけが古い形のまま緑を出し続ける

DTO の形だけでなく、**画面に出ない綴り**（イベント名 `os-color-scheme-changed`、
その `"light"` / `"dark"` ペイロード）も契約に載せてある。ずれても画面は普通に
描画されるので、他に気づく手立てが無い。

検知できないもの: TS 側の型が Rust より広い場合（安全側）と、`invoke` の引数名
（`maxDimension` → `max_dimension`）。後者は未対応。

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
- メタデータ編集の書き込み先は、**フォルダー単位で 1 度だけ** OS ネイティブの確認を
  経て `WritableRoots` に足す（`grant_metadata_editing`）。承認はアプリ終了で消える

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

メタデータ編集だけは書き込み先がこの配下ではなく**利用者の写真そのもの**
（`grant_metadata_editing` で承認したフォルダー配下の JPEG）である。

## 設計方針

- 変換処理は元の画像ファイルを上書きしない（`--delete-originals` で明示的に削除）。
  メタデータ編集だけは元ファイルの**メタデータセグメントのみを差し替える**
  （画素は再エンコードせずバイト単位で不変。読み戻し検証つきの原子的置換）
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
