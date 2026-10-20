# Picture Tool

Instagram投稿用の画像一括変換ツール。写真を4:5アスペクト比に変換し、ファイルサイズを制限します。CLI/GUIの2つのインターフェースを提供。

## 機能

- **4:5アスペクト比変換** — クロップ / パディング / サイズのみの3モード
- **ファイルサイズ制限** — 品質を自動調整して指定サイズ以下に圧縮
- **Exifフレーム** — 余白にカメラ・レンズ・撮影パラメータとメーカーロゴを描画（**padモード限定**）
- **並列処理** — rayonによる高速バッチ処理
- **GUIアプリ** — フォルダー参照 → 写真選択 → プレビュー → 変換の一連のフロー
- **メタデータ編集** — JPEG のタイトル・コメント・★を GUI から読み書き（元ファイルへ保存）
- **検索・並び替え** — グリッドをファイル名で絞り込み、名前 / 更新日 / 作成日で並べ替え
- **元ファイル削除オプション** — 変換完了後に元ファイルを自動削除可能
- **色とメタデータを保つ書き出し** — 出力は sRGB の ICC プロファイルと Exif（位置情報・シリアル番号・所有者名・作者名は除く）を持つ
- **対応フォーマット** — JPEG, PNG, WebP, JPEG XL, AVIF（出力は常にJPEG。メタデータ編集は JPEG のみ）

## セットアップ

### 前提条件

- [Rust](https://rustup.rs/) 1.90以降（AVIF のコンテナ解析クレート `avif-parse` の要件）
- [libdav1d](https://code.videolan.org/videolan/dav1d) 1.3以降（AVIF の AV1 デコーダ。C 製）。入手方法は OS で異なる
  - Linux: システムの libdav1d の開発パッケージを動的リンクする。Fedora は `sudo dnf install libdav1d-devel`、
    Ubuntu は 24.04 以降で `sudo apt install libdav1d-dev`（Debian 12 の dav1d は 1.0 で要件を満たさない）
  - Windows: dav1d を git clone して meson / ninja で静的ビルドする（下の「Windows での AVIF デコーダのビルド」）
  - macOS: `brew install dav1d`（動的リンク。**未検証**）
- [NASM](https://nasm.us/)（**Windows のみ**。libdav1d を内部ビルドするときにアセンブリのビルドで使う。Linux / macOS はシステムの libdav1d を動的リンクするので不要）
  - Windows: `winget install NASM.NASM`（インストーラは PATH を通さないので、`nasm.exe` のあるフォルダーを PATH に追加する）
- [Bun](https://bun.sh/)（フロントエンドビルド用）
- Tauri v2の[システム依存](https://v2.tauri.app/start/prerequisites/)（GUI使用時）

#### Windows での AVIF デコーダのビルド

> **未検証**: この手順は設計に基づいて書いたもので、Windows の実機ではまだ試していません。
> 通らなかった場合は、下の各項目（特に pkg-config とネットワーク）を疑ってください。

Windows では libdav1d を `dav1d-sys` クレートが内部でビルドします（dav1d を git clone し、meson と ninja で静的ライブラリにする）。
そのため次が要ります。

- meson、ninja、git、NASM（上記）、C コンパイラ（MSVC）
- **pkg-config の実行ファイル**（内部ビルドの最後で pkg-config を呼ぶため、Windows の標準環境では別途導入が要る）
- **ビルド時のネットワーク接続**（`code.videolan.org` から dav1d を取得する）

環境変数 `SYSTEM_DEPS_DAV1D_BUILD_INTERNAL=always` を、**ビルドのたびに**ビルドするシェルで設定します。
PowerShell の例:

```powershell
$env:SYSTEM_DEPS_DAV1D_BUILD_INTERNAL = "always"
cargo build --release
```

`.cargo/config.toml` の `[env]` には置きません。`[env]` は OS で分岐できず、Linux の動的リンクと両立しないためです。

ビルドが「pkg-config が dav1d を見つけない」という趣旨のエラーで止まったときは、その表示を信用せず、
**ビルドログを遡って git や meson の失敗を探してください**。`dav1d-sys` のビルドスクリプトは外部コマンドの
終了コードを見ないため、git clone や meson が失敗しても先へ進み、最後にこの見当違いのエラーで止まります。

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

# 出力の画素数を抑える（4:5 キャンバスの幅を 1080px 以下に。crop / pad のみ）
cargo run -p picture-tool -- -i ./photos -o ./output -m pad --max-width 1080

# Exifフレームを付ける（pad モード限定。他モードでは警告を出して無視される）
cargo run -p picture-tool -- -i ./photos -o ./output -m pad -e

# プリセットを指定して Exifフレーム（`~/.config/picture-tool/presets/` に保存した名前）
cargo run -p picture-tool -- -i ./photos -o ./output -m pad -e -p 散歩用

# 表示項目や配置を細かく指定する（後述）
cargo run -p picture-tool -- -i ./photos -o ./output -m pad -e --preset-file ./my-frame.json
```

### GUI

```bash
make dev
```

デスクトップアプリが起動します。左端の navigation rail で3つのモードを切り替えます：

| モード | 左 | 中央 | 右 |
|---|---|---|---|
| **変換** | フォルダーツリー | 写真グリッド | 変換設定・実行 |
| **情報** | フォルダーツリー | 写真グリッド | メタデータ（タイトル・コメント・★） |
| **フレーム** | プリセット一覧 | フレームのプレビュー | フレーム設定 |

- **テーマ** — rail 下端のボタンで システム追従 / ライト / ダーク を切り替え。
  システム追従は、アプリを起動したまま OS のテーマを変えても追従します
- **検索と並び替え** — グリッド上部の検索欄でファイル名を絞り込み（大文字小文字は区別しません）、
  並び順は 名前 / 更新日 / 作成日 を昇順・降順で選べます。
  **絞り込んでも選択は解除されません** ── 隠れた選択は「N 枚選択中（M 枚は非表示）」として
  枚数が出ます。変換されるのは隠れている分も含めた N 枚です。
  作成日は取得できないファイルシステムがあり、その写真は並び順の末尾に置かれます
- **メタデータ編集** — 情報モードで書き込むと**元の写真ファイルを更新します**。
  フォルダーごとに1度だけ OS のダイアログで許可を求めます（許可はアプリを閉じると消えます）。
  画素は再エンコードされず、メタデータの領域だけが差し替わります

### CLIオプション一覧

| オプション | 短縮 | デフォルト | 説明 |
|-----------|------|-----------|------|
| `--input` | `-i` | (必須) | 入力フォルダーパス |
| `--output` | `-o` | `./` | 出力フォルダー（自動作成） |
| `--mode` | `-m` | `crop` | `crop`, `pad`, `quality` |
| `--bg-color` | `-b` | `white` | `white`, `black`。pad の余白と、透過部分の塗りに使う |
| `--quality` | `-q` | `90` | 初期JPEG品質 (1-100)。目盛りは Lightroom Classic の書き出し品質に近く、品質 20 が LrC の 20% とほぼ同じ量子化テーブルになる。**以前のバージョンで保存した設定値は数値のまま引き継がれ、同じ数値でも画質とサイズが変わる** |
| `--max-size` | | `8` | 最大ファイルサイズ (MB, 1-1024) |
| `--max-width` | | (無制限) | 出力4:5キャンバスの幅の上限 (px, 4-20000)。`crop` / `pad` 限定 |
| `--delete-originals` | | `false` | 変換後に元ファイルを削除 |
| `--exif-frame` | `-e` | `false` | Exifフレームを付加（`--mode pad` 限定） |
| `--preset` | `-p` | `default` | 使用するプリセット名 |
| `--preset-file` | | | プリセットJSONを直接指定（`--preset` より優先） |
| `--custom-text` | | | プリセットのカスタムテキストを上書き |

### Exifフレーム

写真の余白（pad モードで付く帯）に、カメラ・レンズ・撮影パラメータとメーカーロゴを描画します。

- **`--mode pad` でのみ有効**。crop / quality と組み合わせた場合は警告を出して無視します
  （余白が無い＝描画する場所が無いため）
- 帯の位置は写真の向きから自動で決まります（横構図→下、縦構図→右。縦の場合は帯ごと90度回転）
- 表示は2段: 1段目がメーカーロゴ・カメラ型番・レンズ型番、2段目が焦点距離 / F値 / SS / ISO
- **CLI から項目や位置を個別に指定するオプションはありません。** 細かく制御したい場合は
  プリセットJSONを書いて `--preset-file` で渡すか、GUI の Exifフレーム設定で作ったプリセットを
  `--preset` で名前指定してください。プリセットは
  `~/.config/picture-tool/presets/{名前}.json`（OS 別の場所は後述）に保存されます

## 開発コマンド

```bash
make build          # CLI + GUI ビルド
make build-cli      # CLIのみ
make build-gui      # GUIのみ（フロントエンド含む）
make dev            # GUI開発サーバー
make release        # リリースビルド + Tauri のバンドル/インストーラ生成
make clean          # クリーンアップ

make check          # CI と同等の検証を一括実行（下の4つ）
make lint           # cargo fmt --check + clippy -D warnings
make test           # cargo test --workspace
make typecheck      # フロントエンドの型検査（svelte-check）
make test-frontend  # フロントエンドの単体テスト（bun test）
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

透過のある画像は、どのモードでも背景色（`--bg-color`）で塗ってから書き出す。

### サイズ圧縮

1. 初期品質で保存を試行し、`--max-size` に収まればそれを使う
2. 収まらなければ、初期品質と下限（品質 28）の間で、収まる最も高い品質を二分探索で探す
3. 下限（品質 28）でも超過する場合はそのまま保存し、警告を出す

### 出力

- 入力の ICC プロファイルを見て、JPEG / PNG / WebP は sRGB に変換する。出力には sRGB の ICC プロファイルを埋め込む
- ファイル名: `{元のファイル名}_processed.jpg`（重複時は連番追加）
- 元の画像ファイルは上書きしない（`--delete-originals`で明示的に削除）
- 読み込み失敗した画像はスキップして継続
- JPEG XL / AVIF の向きは画像本体の情報（JPEG XL はコードストリーム、AVIF は `irot` / `imir`）に従い、Exif の Orientation は使わない
- 既知の制限: AVIF の切り抜き（`clap`）・グリッド分割された画像・アニメーション（`avis`）には対応しない。HDR・広色域の JPEG XL / AVIF は色管理せず 8bit に落とすため、色がずれることがある
- 既知の制限: ICC プロファイルを持つ JPEG XL / AVIF は sRGB へ変換しない（上の HDR・広色域の項と同じ）。
  原色が sRGB で階調曲線だけがリニアな ICC は、sRGB として扱い変換しない
- 既知の制限: 品質 50 未満の JPEG は 4:2:0 のクロマサブサンプリングで書き出す。
  このツールが使う画像デコーダ（`image` クレートの zune-jpeg 0.5.15）はこの形式を全面緑に読むため、
  品質 50 未満（サイズ制限で品質が下がった出力を含む）の出力を再びこのツールの入力にすると緑に見える。
  他のビューアー・ブラウザー・Instagram は正しく表示する。zune-jpeg 0.5.16 の安定版が出たら更新して直す
- AVIF についての既知の制限（デコーダを libdav1d に替えたことによるもの）
  - 壊れた AVIF を読むと、libdav1d のログや内部エラーのメッセージが stderr に出ることがある（読み込みは 1 枚の失敗として扱われ、処理は続く）
  - ★（評価）の読み書きは JPEG のみ。JPEG XL / AVIF は常に「未評価」になる
  - 主ブランド（`ftyp` の major brand）が `avif` 以外の AVIF は開けない（互換ブランドだけに `avif` があるものを含む）
  - 仕様に反する次のファイルは開けない: 行列係数が予約値 3 の AVIF、1 つの AV1 データにシーケンスヘッダが 2 個以上ある AVIF、
    アルファの寸法が主画像と違う AVIF
  - Linux の配布物（`make release` の deb / rpm / AppImage）は、実行時にシステムの libdav1d（1.3 以上）が要る。
    パッケージの依存としては宣言していないので、入っていない環境では起動時にライブラリが見つからないと言われる

### 終了コード（CLI）

| コード | 条件 |
|---|---|
| `0` | 全部変換できた（画像が1枚も見つからなかった場合も `0`） |
| `1` | **1枚でも変換できなかった**、または起動時に失敗した（入力フォルダーが無い等） |

`--max-size` に収まらなかった枚数は失敗に数えません（変換自体はできています）。
警告として stderr に出るだけで、終了コードは `0` のままです。

## ライセンス

本ソフトウェアは [MIT License](./LICENSE) で提供されます。

### 同梱アセット

| アセット | 権利者 | ライセンス |
|---------|--------|-----------|
| `core/assets/fonts/NotoSansJP-Regular.otf` | Adobe | [SIL Open Font License 1.1](./core/assets/fonts/OFL.txt) |
| `core/assets/logos/*` | 各商標権者 | 下記「商標について」を参照 |

### 使用しているライブラリ

JPEG の書き出しに `jpeg-encoder` を使っています。
This software is based in part on the work of the Independent JPEG Group.

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

同じ階層の `assets/fonts/` に置いた `.ttf` / `.otf` は、Exifフレームのフォントとして
選べるようになります（GUI のフォント選択、またはプリセットJSONの `font.font_path`）。
**GUI が読み込むのはこのディレクトリ配下のフォントだけです。** 任意のパスを指定しても
拒否されます（webview から任意ファイルを読ませないため）。プリセットは1つ上の
`presets/` に保存されます。

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
