# 書き出し品質の改善 実装計画

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** LrC の書き出しに近い画質とファイルサイズにする（低品質でも色が破綻しない・同じ画質なら小さい・広色域の色が正しい・Exif フレームのロゴと文字がきれい）。

**Architecture:** JPEG の書き出しを新モジュール `core/src/encode/`（`jpeg-encoder` =0.7.1、LrC 20% を基準形にした量子化テーブル、ハフマン最適化、品質で切り替えるサンプリング、ICC と Exif の埋め込み、サイズ制限の二分探索）に集める。入力 ICC は `decode` の中で `core/src/color.rs`（moxcms）が sRGB へ変換し、出力 Exif は `core/src/output_exif.rs` が元の Exif から組み立て直す。`process_image` にはアルファを背景色へ合成する 1 ステップを足す。Exif フレームはロゴを表示する寸法で直接読み込み、文字の合成式を直す。

**Tech Stack:** Rust（`picture-tool-core`）、jpeg-encoder =0.7.1、moxcms 0.8.1（`image` 0.25.10 経由で既にビルドされている版）、kamadak-exif 0.6.1（`experimental::Writer`）、resvg/tiny-skia 0.48/0.12、Svelte 5 + Playwright、Python（Pillow 12.3）＋ ImageMagick 7.1（フィクスチャ生成と比較画像）

**Spec:** [`docs/superpowers/specs/2026-10-20-output-quality-design.md`](../specs/2026-10-20-output-quality-design.md)
（spec の「決定事項と、捨てた案」と「やらないこと」は再議論しない。行番号の参照は spec 執筆時点のもので、下の「ファイル構成」が現行の位置）

## Global Constraints

- **検証は `make check`。** 各タスクの終わりで exit 0 にする（`cargo fmt --check`・`cargo clippy --workspace --all-targets -- -D warnings`・`cargo test --workspace`・typecheck・`bun test`）。
  clippy は**未使用コードも失敗にする**ので、置き換えと削除は同じタスクで行う
- **`core` は `eprintln!` しない。** ICC・Exif の失敗は `warnings` に積み、その情報なしで書く。例外は**メモリ予算の `reserve` だけ**で、超えたら今と同じく 1 枚の `Err`（spec「付随処理の失敗」）
- **`jpeg-encoder = "=0.7.1"`。** `Encoder::new` の既定（品質 90 未満で 4:2:0・`Nearest`・Annex K テーブル）に頼らない。
  `set_quantization_tables(Custom, Custom)`・`set_sampling_factor`・`set_chroma_subsampling_method(Average)`・`set_optimized_huffman_tables(true)` を**毎回明示する**。プログレッシブにしない
- **`QuantizationTableType::Custom` は自然順（行優先）で渡す。** LrC のテーブル（spec に zigzag 順で記載）は並べ替えてから渡す。
  **全要素 1〜255**（256 以上は DQT の下位 8bit だけが書かれ、壊れた JPEG がエラーなしで出る）
- **品質の目盛りの絶対条件**（spec 性質 1・2・3・3a）: 品質 20 = LrC 20% と完全一致、品質 100 = 全要素 1、品質を下げるとどの要素も増えるか変わらない、全要素 1〜255。性質 4・5 はこれを崩して合わせない
- **サムネイル・プレビュー**（品質 75 / 85 / 90）も新しいエンコーダと目盛りを通す。ICC と Exif は埋め込まない（spec「サムネイルとプレビュー」）
- **PSNR の定義**（本計画の判断。spec は定義していない）: 8bit RGB の 3 チャンネルすべての画素で MSE を取り、`10·log10(255²/MSE)`。
  比較の基準は「入力を `open_image_oriented` でデコードした RGB8」。spec の実測値（42.8dB 等）はこの定義と同じ方法（Pillow で計測）で、計画時の試作でも再現した（実施メモ「計画時の試作」）
- **「現行」の数値は Task 1 で、差し替え前のコードで測ったものを正とする。** 差し替えた後は作り直せない。計画時の試作（Pillow）の数値は参考
- **実写真をリポジトリに入れない。** 実写真での計測は `#[ignore]` の spike テスト（Task 1）で行い、パスは環境変数で渡す。実施メモにも写真のパスは書かず、ファイル名だけを書く
- **sRGB の ICC**: moxcms の `ColorProfile::new_srgb().encode()` を `OnceLock` で 1 度だけ作る（同梱ファイルを増やさない。本計画の判断）
- **テストを書く・直す前に `test-integrity` スキルを起動する**（グローバル指示）。否定形の検査（「出ない」「消える」）には前提条件の assert を添える
  （memory: negative-assertion-tests-need-a-precondition）。spec が「直す前のコードに戻すと落ちる」ことを求めている検査は、実際に戻して落ちることを確かめ、実施メモに残す
- **この環境の `cargo test` は出力が要約される。** spike の結果はファイルへ書き出す（memory: cargo-test-output-is-summarized）。長いコマンドはフォアグラウンドで走らせる
- **e2e のロール参照は変換パネルの容器へスコープする**（memory: scope-role-queries-in-e2e）。e2e は `cd gui-frontend && bun run e2e`（ポートが塞がっていれば `E2E_PORT=5199`）。dev サーバーが起動しないときは inotify を疑う
- **生の色・寸法を Svelte に直書きしない**（`gui-frontend/src/styles/` の 2 ファイルだけ）。runes 構文を使う
- **Task 3 が決着するまで PR を作らず、マージもしない。** Task 2 のコミットから Task 3 の決着までは、既定値 90 の出力が現行より大きいか PSNR が低い状態になる
- **タスクの順序**: Task 1 → 2 → 3 → 4 は直列。Task 5〜9 は Task 3・4 の結果（目盛りの定数・`MIN_QUALITY`）を使わないので、
  Task 3 で利用者の判断待ちになったら Task 5〜9 を先に進めてよい。Task 4 は Task 3 の決着を待つ。Task 10 は全タスクの後
- **コミットは `commit-message` スキルに従う。** `<対応の概要>` は spec のコミット（`d85b68c`）から逐語継承して **`書き出し品質の改善`**。
  footer は `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`

### 警告の文言（`warnings` に積むもの。新規）

| 事象 | 文言 |
|---|---|
| ICC が読めない（デコーダの `icc_profile()` が `Err`、moxcms が解析できない） | `Ignoring unreadable ICC profile in {path}: {err}` |
| ICC は読めたが sRGB へ変換できない（色空間が画素形式と合わない・変換を作れない） | `Ignoring ICC profile that cannot be converted to sRGB in {path}: {err}` |
| 出力用の Exif が読めない（「Exif が無い」は除く） | `Writing without EXIF: failed to read EXIF from {path}: {err}` |
| Exif を組み立て直せない | `Writing without EXIF: failed to rebuild EXIF for {path}: {err}` |
| 組み立てた Exif が APP1 に入らない（TIFF 本体 65,527 バイト超） | `Writing without EXIF: EXIF of {n} bytes does not fit in a JPEG segment` |

65,527 は `jpeg-encoder` 0.7.1 の `add_app_segment` の判定（`"Exif\0\0"` 6 バイトを足して 65,533 以下）から。doc の 65,528 とは 1 ずれている。
`add_exif_metadata` を呼ぶ前にこの長さで落とすので、エンコーダの `Err` には頼らない。

## Review Focus

spec が含意するが、各タスクの本筋のテストが通らない入力のうち、利用者に効きそうな順。各行のテストは担当タスクに入れてある。

1. **組み立て直した Exif が大きすぎる入力**（長い `UserComment` 等）→ 変換は成功し、Exif なしで書かれ、警告が 1 件出る（Task 7 `oversized_exif_is_dropped_with_a_warning`）
2. **画素形式と合わない ICC**（RGB 画像に Gray の ICC。CMYK の JPEG に CMYK の ICC と同じ経路）→ 画素はそのまま、警告 1 件、変換は成功（Task 6 `icc_that_does_not_match_the_pixels_is_ignored_with_a_warning`）
3. **16bit・アルファ付きの広色域 PNG** → 16bit のまま変換してから背景色へ合成され、色が正しい（Task 6 `sixteen_bit_png_is_converted_before_flattening`）
4. **極端に横長な利用者の SVG ロゴ**（1000:1 など）→ 表示する高さで読むようにしても、幅はバーの長さで打ち切られ、巨大な pixmap を確保しない（Task 8 `extremely_wide_svg_is_bounded_by_the_bar_length`）
5. **初期品質が下限より低い／上限が極端に小さい** → 前者は初期品質 1 回だけ、後者は下限の結果を書いて「収まらなかった」警告（Task 4 の探索テスト）

---

## spec を実装者の目で読んで決めたこと

| 論点 | 決定 | 理由 |
|---|---|---|
| 出力形式 | JPEG だけ（`{stem}_processed.jpg`）。spec の「JPEG / PNG / WebP」は**入力**の話 | 現行コードに出力の分岐は無い |
| エンコーダの関数 | `encode::encode_jpeg(rgb, quality, &Embed) -> Result<Vec<u8>>` の 1 つ。`Embed::NONE`（サムネイル・プレビュー）と `Embed { icc, exif }`（本番）で切り替える | spec は 2 関数か引数かを実装者に任せた。引数の方が呼び出し元の差が 1 か所で見える |
| 目盛りの関数の形 | 下の「目盛りの初期形」。定数 5 つを Task 3 で調整する | 計画時の試作で、単純な倍率では性質 5 から大きく外れることを確かめた |
| sRGB の判定 | RGB のマトリクス型で、`red/green/blue_colorant` と `white_point` が `new_srgb()` と各成分 0.002 以内なら sRGB とみなす。TRC は比べない | moxcms の `ColorProfile` は `PartialEq` を持たず、説明文字列は言語別に揺れる。原色が sRGB で TRC だけ違うもの（Rec.709 の ICC など）は変換しない既知の制限として、コードのコメントに書く |
| ICC 変換の置き場所 | `decode::open_with_warnings` の中（JPEG / PNG / WebP の経路だけ） | spec「5」。JPEG XL / AVIF は通さない |
| 出力 Exif の読み取り | `read_exif_info` は変えない。`output_exif::load` が別に読む | `read_exif_info` は壊れた Exif を黙って default にする公開 API で、そこを変えると呼び出し元が増える。Exif の読み取りはヘッダだけなので 2 回読んでも安い |
| 透過の合成 | `process_image` で向きの適用の直後。アルファを持つ画像だけ `RgbImage` に変える | spec「6」 |
| サイズ制限の探索 | `encode` モジュールへ移し、探索の部分をエンコードから切り離した関数にしてテストする | 二分探索の正しさ（最大の品質・回数・下限）を実エンコードなしで固定できる |
| ロゴの読み込み寸法 | 「長辺」ではなく**箱（最大幅 × 最大高さ）に収める**に変える。メーカーロゴは（バーの長さ, `0.45 × バーの太さ`）、レンズロゴは（バーの長さ, `secondary_size_base × 1.2`） | 高さだけで読むと、極端に横長の利用者ロゴで巨大な pixmap を確保しうる（Review Focus 4） |
| GUI の背景色 | ラベルは「背景色」のまま。pad 以外では下に「透過部分をこの色で塗ります」と添える | spec は文言を実装者に任せた |

### 目盛りの初期形（Task 2 で実装し、Task 3 で定数を調整する）

`B` は LrC 20% のテーブル（輝度・色差それぞれ、自然順）。要素 `(行 y, 列 x)` の周波数を `r = (x + y) / 14`（0〜1）とする。

- **q ≥ 20**: `u = ((q − 20) / 80)^G`、`a = A_LO·(1 − r) + A_HI·r`、`v = B^((1 − u)^a)`（q = 100 では 1）
- **q < 20**: `v = min(B·(20 / q)^BETA, max(B, CAP))`
- どちらも `round` して `1..=255` に収める

定数の初期値は **`G = 2.3`・`A_LO = 0.6`・`A_HI = 0.05`・`BETA = 1.0`・`CAP = 40`**（計画時の試作で品質 90 が最も性質 5 に近かったもの）。
この形は q = 20 で `B`、q = 100 で 1、品質について単調（`B ≥ 1` かつ指数が q について単調減少）なので、性質 1〜3 は定数によらず成り立つ。
**ただし `A_HI` が小さいと、高周波が品質 99 の 8 から 100 の 1 へ跳ぶ**（spec が避けるよう求める形）。Task 3 で `A_HI` を上げるとファイルが大きくなる方向に働く。この綱引きの決め方は Task 3 に書いた。
**Task 3 で式を変えた**（定数だけでは性質 5 を満たせなかった）。今の式は実施メモ「Task 3」と `core/src/encode/quant.rs` の冒頭にある。

## ファイル構成

### 新規

| ファイル | 責務 |
|---|---|
| `core/src/encode/mod.rs` | `Embed`、`encode_jpeg`、`EncodedJpeg`、`encode_within_size_limit`、探索関数 `search_quality`、`MIN_QUALITY` |
| `core/src/encode/quant.rs` | LrC 20% の定数（zigzag 順で spec から逐語）、`ZIGZAG`、`quant_tables(quality) -> QuantTables`、`sampling(quality)`、目盛りの定数 |
| `core/src/color.rs` | `srgb_icc() -> &'static [u8]`、`to_srgb(img, icc, path, &mut warnings) -> DynamicImage`、`is_srgb` |
| `core/src/output_exif.rs` | `load(path, format) -> Result<Option<exif::Exif>>`、`build(&exif::Exif, width, height) -> Result<Option<Vec<u8>>>`、`MAX_EXIF_TIFF_LEN` |
| `core/tests/output_quality_spike.rs` | 実写真でのサイズ・PSNR・テーブルの計測（`#[ignore]`） |
| `core/tests/output_quality.rs` | 出力 JPEG の契約（DQT・サンプリング・ICC・Exif・透過）を `process_image` 越しに固定する統合テスト |
| `core/tests/fixtures/make_output_quality.py` | ICC フィクスチャ・巨大寸法 PNG を作るスクリプト |
| `core/tests/fixtures/icc/` | `adobe.png`・`adobe16.png`（16bit + アルファ）・`adobe.jpg`・`adobe.webp`・`prophoto.png`・`srgb_lrc.jpg`（パッチに LrC の sRGB の ICC を付けたもの）・`broken_icc.png` |
| `core/tests/fixtures/over_limit.png` | 寸法だけ巨大（12000×12000 RGBA）で中身は正当な PNG |

### 変更

| ファイル | 変更 |
|---|---|
| `Cargo.toml`（workspace）/ `core/Cargo.toml` | core に `jpeg-encoder = "=0.7.1"`・`moxcms = "0.8.1"` |
| `core/src/lib.rs` | `encode_jpeg_rgb`（797 行）と `encode_within_size_limit`（750-794 行）を削除し `encode` へ。呼び出し元 539 / 585 / 626 行。`process_image`（349-480 行）に透過の合成・ICC 警告・Exif・`Embed` |
| `core/src/decode/mod.rs` | `open`（90-124 行）の `Format::Other` 経路を `open_with_warnings` へ。`open` は警告を捨てる包み |
| `core/src/exif_frame/mod.rs` | ロゴ読み込み（284-312 行）、`render_exif_bar`（393 行〜、398-409 行と 430-448 行）、フォント寸法の計算の切り出し |
| `core/src/exif_frame/logo.rs` | `load_svg_from_bytes`（52-80 行）の demultiply と箱への収め方、公開関数（12・31・113・126 行）の寸法引数 |
| `core/src/exif_frame/text.rs` | `draw_text_on_image`（106-160 行）の合成式、`draw_text_rotated_90`（164 行）とそのテスト（430・492 行付近）の削除 |
| `cli/src/main.rs` | `--bg-color` のヘルプ（28 行） |
| `gui-frontend/src/lib/panels/ConvertPanel.svelte` | 背景色の選択（94-105 行）を全モードへ |
| `gui-frontend/e2e/convert.spec.ts` | crop / quality でも背景色が出ること |
| `README.md` | 機能（5 行〜）・CLI オプション（129-132 行）・変換モード（190-196 行）・サイズ圧縮（198-202 行）・出力（204 行〜）・ライセンス（230 行〜） |
| `docs/superpowers/specs/2026-08-12-output-width-limit-design.md` | 184-186 行の誤記に訂正の注記 |
| `docs/README.md` | 計画の索引行（計画時に追加済み。Task 10 で更新） |

---

## Task 1: 計測の道具を作り、差し替え前の数値を残す

**Files:**
- Create: `core/tests/output_quality_spike.rs`

**Interfaces:**
- Produces: spike テスト `measure`。環境変数 `PICTURE_TOOL_SPIKE_IMAGES`（`:` 区切りのパス）・`PICTURE_TOOL_SPIKE_QUALITIES`（`,` 区切り）・`PICTURE_TOOL_SPIKE_OUT`（結果を書く TSV のパス）。
  1 行 = `file\tquality\tbytes\tpsnr_db\tluma_max\tchroma_max\tsampling`。`*_max` と `sampling` は出力 JPEG の DQT と SOF を読んで書く
- Produces: 同じファイルの `fn dqt(jpeg: &[u8]) -> Vec<(u8, [u16; 64])>`（テーブル番号と **ファイル内の順＝zigzag 順**の値）と `fn sof_sampling(jpeg: &[u8]) -> Vec<(u8, u8)>`。Task 2 で `core/tests/output_quality.rs` が同じものを使うので、`core/tests/common/jpeg.rs` に置いて `mod common;` で共有する

- [ ] **Step 1: spike を書く**

  `process_image` を `ConversionMode::Quality`・`max_size_mb = 1024`・`delete_originals = false` で呼び、出力を `tempfile` に書く。
  PSNR は `open_image_oriented(入力).to_rgb8()` と `image::open(出力).to_rgb8()` で Global Constraints の定義どおりに計算する。
  冒頭のモジュールコメントは `core/tests/rating_scan_spike.rs` に倣い、目的・「通常のテスト実行では走らない」・実行コマンドを書く。
  実行は `cargo test -p picture-tool-core --release --test output_quality_spike -- --ignored measure`

- [ ] **Step 2: 計測に使う写真を利用者と決める**

  spec は「サンプル写真」に加えて「高感度のノイズが多い写真」と「細部の多い風景」を 1 枚以上ずつ求め、写真は利用者に提供を頼むとしている。
  候補は利用者の手元の LrC 書き出しの JPEG XL（`261010` フォルダー。いずれも品質 92 の sRGB）で、計画時に見た限りでは次のとおり。
  - `20261010-DSC05479.jxl`: 杉林と社殿。細部が多い
  - `20261010-result2.jxl`: 星景。ノイズが多い見込み（ISO は未確認）
  - `20261010-DSC05529.jxl`: 夕景のグラデーション（参考。滑らかな写真の 2 枚目）

  ノイズの多さ（ISO）と書き出し時の圧縮の強さ（品質 92 で足りるか）を利用者に確かめ、写真の組を確定する。
  あわせて、後のタスクで要る素材もここでまとめて頼む（利用者の応答待ちを途中と末尾に散らさないため）。
  - Task 7: カメラが作った撮って出しの JPEG 1 枚（MakerNote・IFD1 付き。Orientation 6 の縦位置だとなおよい）
  - Task 10: LrC 20% の書き出し（`261011` フォルダー）のうち 1 枚の、元に近い高品質の書き出し（JPEG / JPEG XL）

  JPEG XL の 66MP が予算（512MB）で開けないときは、`magick x.jxl -depth 8 x.png` で作った PNG を使い、実施メモに書く。
  サンプル写真は `docs/tmp/20260328-DSC05024.jpg`

- [ ] **Step 3: 差し替え前の数値を取る**

  確定した写真すべてで、品質 `1,5,10,20,40,60,90,100` を測る。結果の TSV は**全行をそのまま**実施メモ「Task 1」に貼る（memory: copy-measured-values-verbatim）。
  サンプル写真の現行の品質 20 / 60 が spec の表（975KB 38.3dB / 1,233KB 42.8dB）とおおむね一致することを確かめる（一致しなければ止まって原因を調べる）

- [ ] **Step 4: 差し替え前の比較画像を作る**

  サンプル写真を現行の品質 `1,5,10,20,40,60,90,100` で書き出し、切り抜き `400x300+500+2800` と、その彩度強調（`magick ... -modulate 100,300`）を `docs/tmp/output-quality/before/` に置く（`docs/tmp/` はコミットしない作業場所）

- [ ] **Step 5: `make check` を通してコミット**

  ```bash
  git add core/tests/output_quality_spike.rs core/tests/common/ docs/superpowers/plans/2026-10-20-output-quality.md
  git commit  # test(core): 書き出し品質の改善: 実写真のサイズと PSNR を測る spike を足し、差し替え前の数値を残す
  ```

## Task 2: エンコーダを `jpeg-encoder` に替え、目盛りを作り直す

**Files:**
- Create: `core/src/encode/mod.rs`, `core/src/encode/quant.rs`, `core/tests/output_quality.rs`
- Modify: `Cargo.toml`, `core/Cargo.toml`, `core/src/lib.rs`

**Interfaces:**
- Produces（`encode/quant.rs`、`pub(crate)`）:
  - `pub(crate) struct QuantTables { pub luma: [u16; 64], pub chroma: [u16; 64] }`（**自然順**）
  - `pub(crate) fn quant_tables(quality: u8) -> QuantTables`（1..=100。範囲外は呼び出し側が弾く。`process_image` は既に 1..=100 を検査している）
  - `pub(crate) fn sampling(quality: u8) -> jpeg_encoder::SamplingFactor`（50 未満 `F_2_2`、50 以上 `F_1_1`）
  - `pub(crate) const LRC_20_LUMA_ZIGZAG: [u16; 64]`・`LRC_20_CHROMA_ZIGZAG`（spec の値を逐語で）、`pub(crate) const ZIGZAG: [usize; 64]`（zigzag の k 番目が自然順のどこか）
- Produces（`encode/mod.rs`）:
  - `pub(crate) struct Embed<'a> { pub icc: Option<&'a [u8]>, pub exif: Option<&'a [u8]> }` と `pub(crate) const NONE: Embed<'static>`（`Embed::NONE`）
  - `pub(crate) fn encode_jpeg(rgb: &image::RgbImage, quality: u8, embed: &Embed) -> anyhow::Result<Vec<u8>>`
  - `pub(crate) struct EncodedJpeg { pub bytes: Vec<u8>, pub quality: u8, pub within_limit: bool }`（現行と同じ形）と
    `pub(crate) fn encode_within_size_limit(rgb: &image::RgbImage, initial: u8, max_bytes: usize, embed: &Embed) -> anyhow::Result<EncodedJpeg>`。
    lib.rs から移し、**探索は現行のまま**（5 刻み・下限 60）。Task 4 は中身だけを替え、Task 6・7 はこのシグネチャの `embed` に値を入れる

- [ ] **Step 1: テーブルの性質のテストを書く**（`encode/quant.rs` の `#[cfg(test)]`）

  ```rust
  #[test] fn quality_20_is_lrc_20_percent()          // quant_tables(20) の luma/chroma を ZIGZAG で並べた値 == LRC_20_*_ZIGZAG
  #[test] fn quality_100_is_all_ones()               // 両テーブルの 64 要素すべて 1
  #[test] fn every_element_is_a_valid_8bit_step()    // q = 1..=100 × 128 要素すべて 1..=255
  #[test] fn lowering_quality_never_makes_a_step_finer() // q = 2..=100 × 128 要素で quant_tables(q-1)[i] >= quant_tables(q)[i]
  #[test] fn sampling_switches_at_50()               // sampling(49) == F_2_2、sampling(50) == F_1_1、sampling(20) == F_2_2、sampling(90) == F_1_1
  ```

  期待値は spec の LrC 20% の 128 個の数値そのもの（実装の式から作らない）。zigzag の並べ替え表は JPEG 規格（T.81 Figure A.6）から写し、
  「`ZIGZAG` が 0..64 の置換であること」と「`ZIGZAG[1] == 1`・`ZIGZAG[2] == 8`・`ZIGZAG[63] == 63`」を別のテストで固定する

- [ ] **Step 2: 失敗を確かめる**（`cargo test -p picture-tool-core encode::` → コンパイルエラー）

- [ ] **Step 3: `quant.rs` を実装する**（「目盛りの初期形」の式と定数。定数は名前を付けて 1 か所に置き、Task 3 で変えたことが差分で分かるようにする）

- [ ] **Step 4: 出力 JPEG の契約テストを書く**（`core/tests/output_quality.rs`。`process_image` を quality モードで通し、出力ファイルを読む）

  ```rust
  #[test] fn quality_20_writes_the_lrc_20_percent_tables()  // dqt(出力) の表 0 == LRC 20% 輝度、表 1 == 色差（spec の zigzag の値を逐語でテストに書く）
  #[test] fn quality_20_is_subsampled_420()                  // sof_sampling(出力) == [(2,2),(1,1),(1,1)]
  #[test] fn quality_90_is_444()                             // == [(1,1),(1,1),(1,1)]
  #[test] fn huffman_tables_are_optimized()                  // 同じ 64x64 の画像で、DHT の中身が Annex K の標準表と異なる（標準表の BITS 列 `00 01 05 01 01 01 01 01 01 00 ...` を逐語で持ち、一致しないこと。前提条件: DHT が 1 つ以上ある）
  #[test] fn output_is_baseline()                            // SOF0（0xFFC0）があり、SOF2 が無い
  ```

  入力画像はテスト内で作る（既存の `create_incompressible_image` に倣った 256×256 のノイズを PNG に書く）。
  `dqt` / `sof_sampling` は Task 1 の `core/tests/common/jpeg.rs`

- [ ] **Step 5: `encode_jpeg` を実装し、4 つの呼び出し元を置き換える**

  - `Encoder::new(&mut Vec<u8>, quality)` の後、Global Constraints の 4 設定を必ず呼ぶ。`embed.icc` があれば `add_icc_profile`、`embed.exif` があれば `add_exif_metadata`
  - 幅・高さは `u16` に収まらなければ `Err`（`jpeg-encoder` の制約。現行の `image` は 65,535 超を同様に拒否していたので挙動は変わらない）
  - `lib.rs` の `generate_thumbnail_base64`（75）・`generate_exif_frame_preview_base64`（85）・`generate_full_image_base64`（90）は `Embed::NONE`
  - `encode_within_size_limit` と `EncodedJpeg` を lib.rs（750-794 行）から `encode/mod.rs` へ移し、`embed` 引数を足す。`process_image` は今は `&Embed::NONE` を渡す（ICC は Task 6、Exif は Task 7 で入れる）
  - `encode_jpeg_rgb` を削除する。`image` の `jpeg` feature はデコードに要るので外さない

- [ ] **Step 6: テストを通し、既存のテストが通ることを確かめる**（`cargo test -p picture-tool-core`）

  サイズ制限の既存テスト（`size_limit_*`、lib.rs 1723 行付近〜）はエンコーダが変わっても成り立つ書き方のはず。落ちたら、テストが守ろうとしている契約を読み直してから直す（期待値を実装に合わせて書き換えない）

- [ ] **Step 7: `make check` を通してコミット**（`feat(core): 書き出し品質の改善: JPEG エンコーダを jpeg-encoder に替え、LrC 20% を基準にした目盛りにする`）

## Task 3: 目盛りを実写真で調整し、サイズ制限の下限を決める

**Files:**
- Modify: `core/src/encode/quant.rs`（定数だけ）、実施メモ

**Interfaces:**
- Produces: サイズ制限の下限の値（実施メモ「Task 3」に書く。定数 `MIN_QUALITY` は Task 4 で作る。ここで作ると Task 4 まで未使用になり clippy が落ちる）

性質 4・5 と「品質 100 の直前で跳ねない」を、定数 `G`・`A_LO`・`A_HI`・`BETA`・`CAP`（と必要ならサンプリングの閾値 50）で合わせる。
**式の形を変えるのは、定数だけではどうしても合わないときに限る。** 変えた場合も Task 2 の性質テストはそのまま通ること。

- [ ] **Step 1: 品質 90 を測る**（Task 1 の写真すべて。spike の `PICTURE_TOOL_SPIKE_QUALITIES=90`）

  合否: 写真ごとに「新しい品質 90 のバイト数 ＋ 埋め込む分 ≦ 現行の品質 90」かつ「PSNR ≧ 現行の品質 90」（spec 性質 5）。
  この時点の出力には ICC も Exif も入らないが、性質 5 は Task 6・7 の後の出力で成り立つ必要があるので、埋め込む分を先に足して判定する。
  埋め込む分 = `ColorProfile::new_srgb().encode()` の長さ（ここで測る）＋ その写真の入力の Exif の長さ（`magick <写真> exif:- | wc -c`。`"Exif\0\0"` 込み）＋ セグメント見出し（APP2 の `"ICC_PROFILE\0"`＋2 バイト、各マーカーと長さ）。
  Exif は組み立て直すと位置情報などを除くので、入力の長さを上限の見積もりとして使う。それぞれの値を実施メモに残す

  調整の手がかり（計画時の試作から。実施メモ「計画時の試作」）:
  - 滑らかな写真（サンプル写真・夕景）は **PSNR** で、細部・ノイズの多い写真は**サイズ**で外れやすい
  - 低周波（`A_LO`・`G`）を細かくすると滑らかな写真の PSNR が上がり、高周波（`A_HI`）を粗くすると細部の多い写真のサイズが下がる
  - 4:2:0 を品質 90 まで広げると細部の多い写真は小さくなるが、滑らかな写真の PSNR が下がる

- [ ] **Step 2: 品質 100 の直前で跳ねないことを確かめる**

  サンプル写真と細部の多い写真で品質 `90..=100` を 1 刻みで測り、隣り合う品質のバイト数の比の最大を実施メモに残す。
  **合否（本計画の判断）: `size(100) / size(99)` が 1.5 以下で、かつ 90〜99 の隣り合う比の最大の 2 倍以下**。
  spec の「高周波をある品質まで据え置いてから品質 100 で一気に 1 にする形を避ける」を数字にしたもの

- [ ] **Step 3: 品質 1 で色が破綻しないことを確かめる**（spec 性質 4）

  サンプル写真の切り抜き `400x300+500+2800` を新しい品質 `1,5,10,20,40,60,90,100` で書き出し、元画像・Task 1 の差し替え前と並べ、彩度強調版も作る
  （`docs/tmp/output-quality/after/` と、横に並べた `docs/tmp/output-quality/scale-compare.png`）。各品質のバイト数・PSNR・テーブルの最大要素を実施メモに表で残す

- [ ] **Step 4: 4 つの条件（性質 4・性質 5・跳ねない・性質 1〜3）が両立しないときは止まる**

  spec の優先順位に従い、**性質 4 と 5 のどちらも勝手に緩めない**。試した定数と実測値を実施メモに残し、利用者に判断を仰ぐ。
  「跳ねない」の合否は本計画が置いた数字なので、これだけが合わないときも、緩めるかどうかを利用者に聞く

- [ ] **Step 5: 利用者に目盛りの比較画像を見せる**（完了条件の 1 つ目。了承を実施メモに残す）

- [ ] **Step 6: サイズ制限の下限を決める**

  写真ごとに、新しい目盛りで「PSNR が現行の品質 60 以上」になる最小の品質を探し（品質 15〜40 を 1 刻み）、**そのうち最も高い値**をサイズ制限の下限にする（spec「7」）。
  計画時の試作では、新しい品質 20 は 4 枚すべてで現行の品質 60 にわずかに届かなかった（0.2〜0.9dB）ので、20 より上になる見込み。
  写真ごとの値を実施メモに残す

- [ ] **Step 7: 参考比較**（合否にしない）: 新しい品質 85 のテーブルを LrC 85% と並べ、要素ごとの比を実施メモに残す

- [ ] **Step 8: `make check` を通してコミット**（`feat(core): 書き出し品質の改善: 目盛りの定数を実写真で調整し、サイズ制限の下限を決める`）

## Task 4: サイズ制限を二分探索にする

**Files:**
- Modify: `core/src/encode/mod.rs`, `core/src/lib.rs`（README は Task 10）

**Interfaces:**
- Consumes: `encode_jpeg`・`EncodedJpeg`・`encode_within_size_limit`（Task 2。シグネチャは変えない）、下限の値（Task 3 の実施メモ）
- Produces:
  - `pub(crate) const MIN_QUALITY: u8`（`encode/mod.rs`。値は Task 3 で決めたもの）
  - `pub(crate) fn search_quality(initial: u8, floor: u8, max_bytes: usize, encode: impl FnMut(u8) -> anyhow::Result<Vec<u8>>) -> anyhow::Result<EncodedJpeg>`
  - `encode_within_size_limit` の中身を `search_quality(initial, MIN_QUALITY, max_bytes, |q| encode_jpeg(rgb, q, embed))` に替える

- [ ] **Step 1: 探索のテストを書く**（`encode/mod.rs`。`encode` には「品質 q で `size(q)` バイトの Vec を返す」偽物を渡し、呼ばれた品質を記録する）

  ```rust
  #[test] fn initial_quality_that_fits_is_used_with_one_encode()  // size = q*1000, max = 90_000, initial 90 → quality 90, within, 呼び出し [90]
  #[test] fn picks_the_highest_quality_that_fits()                // size = q*1000, max = 73_500, initial 90, floor 20 → quality 73, within
  #[test] fn encode_count_is_logarithmic()                        // 上と同じ条件で呼び出し回数 <= 1 + ceil(log2(90 - 20)) = 8
  #[test] fn floor_result_is_written_when_nothing_fits()          // max = 1 → quality == floor, within_limit == false, bytes は size(floor)
  #[test] fn initial_below_floor_encodes_once()                   // initial 10, floor 20, max = 1 → quality 10, within false, 呼び出し [10]
  #[test] fn result_fits_even_if_sizes_are_not_monotonic()        // size が単調でない列（例: q=50 だけ大きい）でも、within のときは bytes.len() <= max
  ```

  期待値は spec「7」の文言から（「上限以下に収まる最大の品質」「最悪 `1 + ceil(log2(初期 − 下限))` 回」「初期品質が下限より低いときは初期品質だけ」）

- [ ] **Step 2: 失敗を確かめる → `search_quality` を実装する**

  初期品質を 1 回試し、超えたら `[floor, initial − 1]` を二分探索する。最後に収まった結果を保持し、1 つも収まらなければ下限の結果を返す（下限は探索中に試していなければ 1 回エンコードする）。
  「単調性が崩れると最大とは限らないが、上限以下に収まることは常に守る」ことを関数のコメントに書く（spec の要求）

- [ ] **Step 3: `encode_within_size_limit` の旧探索（`MIN_QUALITY = 60`・`QUALITY_STEP = 5`）を消し、`search_quality` を呼ぶ形にする**

  `final_quality` と警告 `Size limit not met: {:.1} MB > {} MB even at minimum quality {}` の形は変えない。
  既存の `size_limit_failure_is_accompanied_by_a_warning` は「最低品質」の数値を 60 と仮定していないか確かめる（仮定していたら `MIN_QUALITY` を使う形に直す）

- [ ] **Step 4: `make check` を通してコミット**（`feat(core): 書き出し品質の改善: サイズ制限の探索を二分探索にする`）

## Task 5: 透過を背景色で合成し、背景色を全モードで選べるようにする

**Files:**
- Modify: `core/src/lib.rs`, `cli/src/main.rs`, `gui-frontend/src/lib/panels/ConvertPanel.svelte`, `gui-frontend/e2e/convert.spec.ts`, `README.md`（変換モードの表・`--bg-color` の行）
- Test: `core/tests/output_quality.rs`

**Interfaces:**
- Produces: `fn flatten_alpha(img: DynamicImage, bg: BackgroundColor) -> DynamicImage`（lib.rs の private。アルファが無ければそのまま返す。あれば `ImageRgb8`）

- [ ] **Step 1: 統合テストを書く**（フィクスチャはテスト内で作る: 100×80 の RGBA8 PNG を 2 種類）

  - `hidden.png`: 全画素 `(255, 0, 0, 0)`（透けている部分の裏が赤）
  - `half.png`: 全画素 `(0, 255, 0, 128)`（半透明の緑）

  ```rust
  // crop / quality / pad × white / black の全組み合わせ。pad は 100×80（横長 → 余白あり）と 80×100（厳密な 4:5 → 早期 return の経路）の両方
  #[test] fn transparent_pixels_become_the_background_color()   // hidden: 出力中央の画素が white なら (255,255,255)±6、black なら (0,0,0)±6（赤が出ない）
  #[test] fn half_transparent_pixels_are_blended_with_the_background() // half: white → (127,255,127)±6、black → (0,128,0)±6
  ```

  期待値は spec のストレートアルファの over（`c·a + bg·(1−a)`、a = 128/255）を手で計算した定数。±6 は JPEG の誤差の見込み（本計画の判断）。
  修正前のコードで、少なくとも quality と厳密な 4:5 の pad が落ちることを確かめる

- [ ] **Step 2: `flatten_alpha` を実装し、`process_image` の `apply_orientation` の直後で呼ぶ**（サムネイル・プレビューの経路には入れない）

- [ ] **Step 3: CLI と README を直す**

  - `cli/src/main.rs:28` のヘルプを `/// 背景色 (white, black)。pad の余白と、透過部分の塗りに使う` にする
  - README の「変換モード」の表の下に「透過のある画像は、どのモードでも背景色（`--bg-color`）で塗ってから書き出す」を足し、CLI オプション表の `--bg-color` の説明も同じ意味にする

- [ ] **Step 4: GUI を直す**

  `ConvertPanel.svelte` の `{#if config.mode === "pad"}`（94-105 行）を外して全モードで出す。`config.mode !== "pad"` のときだけ、選択の下に「透過部分をこの色で塗ります」を添える（色・余白はトークン経由）

- [ ] **Step 5: e2e を足す**（`convert.spec.ts`）

  ```ts
  test("crop と quality でも背景色を選べる", ...)  // 各モードへ切り替え、変換パネルの容器にスコープした「背景色」の radio「白」「黒」が見え、「透過部分をこの色で塗ります」が見える。pad では注記が見えない（前提: pad で背景色が見えている）
  ```

  `selectSegment` は page 全体を探すので、背景色の radio は容器（変換パネルの `data-region`）で絞ってから引く。
  既存の `frame.spec.ts`「背景色は変換設定と同じ値を指す」が通り続けることも確かめる

- [ ] **Step 6: `make check` と e2e を通してコミット**（`feat: 書き出し品質の改善: 透過を背景色で合成し、背景色を全モードで選べるようにする`）

## Task 6: 入力の ICC を sRGB に変換し、出力に sRGB の ICC を埋め込む

**Files:**
- Create: `core/src/color.rs`, `core/tests/fixtures/make_output_quality.py`, `core/tests/fixtures/icc/*`, `core/tests/fixtures/over_limit.png`
- Modify: `core/src/decode/mod.rs`, `core/src/lib.rs`, `core/Cargo.toml`
- Test: `core/src/color.rs` の単体テスト、`core/tests/output_quality.rs`

**Interfaces:**
- Produces（`color.rs`、`pub(crate)`）:
  - `fn srgb_icc() -> &'static [u8]`
  - `fn to_srgb(img: DynamicImage, icc: Option<&[u8]>, path: &Path, warnings: &mut Vec<String>) -> DynamicImage`
  - `fn is_srgb(profile: &moxcms::ColorProfile) -> bool`
- Produces（`decode`）: `pub(crate) fn open_with_warnings(path: &Path, format: Format) -> Result<(DynamicImage, Vec<String>)>`。`open` はこれを呼んで警告を捨てる

- [ ] **Step 1: フィクスチャを作る**（`make_output_quality.py`。Pillow と ImageMagick。生成手順と期待値の出どころをスクリプト冒頭に書く）

  - パッチ: 4 色（`200,60,60` / `60,160,90` / `90,110,200` / `210,170,120`）を 8×8 ずつ横に並べた 32×8。**この値を画素値のまま ICC を割り当てる**（`magick patch.png -profile /usr/share/color/icc/colord/AdobeRGB1998.icc`。監査と同じ作り方）
  - `adobe.png`（8bit）、`adobe16.png`（16bit・アルファ 255）、`adobe.jpg`（`-quality 100 -sampling-factor 1x1`）、`adobe.webp`（Pillow の `lossless=True, icc_profile=...`）、`prophoto.png`
  - `srgb_lrc.jpg`: サンプル写真から ICC だけを取り出し（`magick docs/tmp/20260328-DSC05024.jpg lrc_srgb.icc`）、パッチに割り当てたもの。写真の画素はリポジトリに入れない。
    ICC の説明が `sRGB IEC61966-2.1` であることをスクリプトで確かめる
  - `broken_icc.png`: パッチに `icc_profile=b"not an icc profile"` を付けたもの
  - `over_limit.png`: 12000×12000 の RGBA8 で全画素 0（出力 576MB > 512MB。中身は正当な PNG。memory: bomb-fixtures-must-be-otherwise-valid）。
    Pillow の `optimize=True` で約 546KB になる（deflate の圧縮比の上限のため、これより小さくならない）。テストの実行時に作ると 576MB のバッファが要るので、コミットする

  正解値（ImageMagick 7.1.2 の `-intent relative -profile /usr/share/color/icc/colord/sRGB.icc` で 8bit PNG を変換。計画時に測った値）:

  | パッチ | AdobeRGB → sRGB | ProPhoto → sRGB |
  |---|---|---|
  | 200,60,60 | 232,57,57 | 255,0,74 |
  | 60,160,90 | 0,161,85 | 0,190,94 |
  | 90,110,200 | 79,110,204 | 0,133,220 |
  | 210,170,120 | 225,172,118 | 255,176,129 |

  ProPhoto は 3 色が sRGB の色域外（0 か 255 に張り付く）。moxcms と LCMS で色域外の扱いが違うと合わないので、
  ProPhoto のテストは「現行値（画素値そのまま）から正解の方向へ動いた」ことだけを見る（下の Step 2）

- [ ] **Step 2: テストを書く**

  `core/src/color.rs`:
  ```rust
  #[test] fn lightroom_srgb_profile_is_recognized_as_srgb()  // srgb_lrc.jpg の ICC → is_srgb == true
  #[test] fn adobe_and_prophoto_are_not_srgb()
  #[test] fn embedded_srgb_profile_parses_as_srgb()          // srgb_icc() を moxcms で読み直し is_srgb == true
  #[test] fn gray_profile_on_gray_pixels_is_converted()      // テスト内で作る Gray の ICC（moxcms、TRC = ガンマ 1.0）付きの Luma8 画素 128 → 188±2（線形 0.502 の sRGB 符号化）
  ```

  `core/tests/output_quality.rs`（`process_image` の quality モード越し。出力の中央の画素を見る）:
  ```rust
  #[test] fn adobe_rgb_png_is_converted_to_srgb()          // 4 色が正解表の ±3
  #[test] fn adobe_rgb_jpeg_and_webp_are_converted_to_srgb()  // 同 ±6（JPEG 入力の誤差込み）
  #[test] fn sixteen_bit_png_is_converted_before_flattening()  // adobe16.png → ±3（Review Focus 3）
  #[test] fn prophoto_png_moves_toward_the_srgb_rendering()   // 色域内の 4 色目 210,170,120 → 255,176,129 の ±6、他 3 色は R/G/B の大小関係が正解と同じ
  #[test] fn srgb_input_is_not_converted()                 // srgb_lrc.jpg: 出力と、ICC を取り除いた同じ画素の入力からの出力が画素単位で一致（前提: 入力に ICC がある）
  #[test] fn output_carries_an_srgb_icc_profile()          // 出力の APP2 "ICC_PROFILE\0" を連結して moxcms で読み、is_srgb
  #[test] fn unreadable_icc_is_ignored_with_a_warning()    // broken_icc.png: 成功、警告 1 件が "Ignoring unreadable ICC profile" で始まる、画素は割り当て前の値 ±6
  #[test] fn icc_that_does_not_match_the_pixels_is_ignored_with_a_warning() // RGB の PNG に Gray の ICC（テスト内で作る）: 成功、"Ignoring ICC profile that cannot be converted" の警告（Review Focus 2）
  #[test] fn png_whose_output_exceeds_the_memory_limit_is_rejected() // over_limit.png → process_image が Err
  ```

  正解値は上の表（ImageMagick の実測）の定数で書く。許容差（±3 / ±6）は本計画の判断で、実測の最大誤差を実施メモに残す。
  CPU の SIMD 経路で 1 階調変わりうる（spec）ので、±3 未満にはしない

- [ ] **Step 3: 失敗を確かめる**（`adobe_rgb_png_is_converted_to_srgb` は現行で 200,60,60 付近を返して落ちる。`png_whose_output_exceeds_the_memory_limit_is_rejected` は**現行では通る**（`decode()` が `reserve` する）ことを確かめる）

- [ ] **Step 4: `color.rs` を実装する**

  - 対応: `Rgb8`/`Rgba8` は 8bit、`Rgb16`/`Rgba16` は 16bit のまま、`Luma*` に Gray の ICC はグレーとして変換。アルファはそのまま
  - それ以外の組み合わせ（`Rgb32F` 等、または画素形式と ICC の色空間の不一致）は変換せず「cannot be converted」の警告
  - `TransformOptions { rendering_intent: RenderingIntent::RelativeColorimetric, ..Default::default() }`
  - **1 行ずつ変換する**: 1 行分の作業バッファで `transform(&row, &mut buf)` し、元の行へ書き戻す。画像 1 枚分のバッファを確保しない
  - Gray の変換先は `new_srgb()` を元に `color_space = Gray`・`gray_trc` を sRGB の TRC にしたもの（作れなければ「cannot be converted」）

- [ ] **Step 5: `decode::open_with_warnings` を実装する**

  `Format::Other` の経路を `ImageReader::open(path)?.with_guessed_format()?.into_decoder()?` に替え、
  `icc_profile()`（`Err` は「unreadable」の警告）→ `image::Limits::default().reserve(decoder.total_bytes())?` → `DynamicImage::from_decoder` → `color::to_srgb` の順。
  JPEG XL / AVIF の経路は今のまま（警告は空）。`guarded` と `with_context` の包み方は現行の `open` と同じにする

- [ ] **Step 6: `reserve` を外して結果が変わることを確かめる**

  `reserve` の行を一時的に消し、`png_whose_output_exceeds_the_memory_limit_is_rejected` が落ちる（`Ok` になる）ことを確かめて戻す。実施メモに残す

- [ ] **Step 7: `process_image` を `open_with_warnings` に替え、本番の `Embed` に `icc: Some(color::srgb_icc())` を入れる**（サムネイル・プレビューは `Embed::NONE` のまま）

- [ ] **Step 8: `make check` を通してコミット**（`feat(core): 書き出し品質の改善: 入力の ICC を sRGB に変換し、出力に sRGB の ICC を埋め込む`）

## Task 7: 出力に Exif を載せる（位置情報と個体識別タグを除く）

**Files:**
- Create: `core/src/output_exif.rs`
- Modify: `core/src/lib.rs`
- Test: `core/src/output_exif.rs` の単体テスト、`core/tests/output_quality.rs`

**Interfaces:**
- Produces:
  - `pub(crate) fn load(path: &Path, format: decode::Format) -> anyhow::Result<Option<exif::Exif>>`（JPEG XL は `decode::jxl_raw_exif` → `read_raw`、ほかは `read_from_container`。`exif::Error::NotFound` は `Ok(None)`）
  - `pub(crate) fn build(src: &exif::Exif, width: u32, height: u32) -> anyhow::Result<Option<Vec<u8>>>`（TIFF 本体。書くべきタグが残らなければ `None`）
  - `pub(crate) const MAX_EXIF_TIFF_LEN: usize = 65_527`

- [ ] **Step 1: 入力の Exif をテスト内で作る道具を書く**（`core/tests/common/exif.rs`）

  「TIFF のバイト列を作る」と「コンテナに入れる」を分ける。
  - TIFF: kamadak-exif の `experimental::Writer` で作る
  - JPEG: APP1（`"Exif\0\0"` + TIFF）を SOI の直後に差し込む。APP1 の長さは u16 なので、TIFF は 65,527 バイトまで
  - PNG: IHDR の直後に `eXIf` チャンク（長さは u32、CRC 付き）を差し込む。65,527 バイトを超える Exif の入力はこちらで作る
  載せるタグ: `Make`・`Model`・`ExposureTime`・`DateTimeOriginal`・`Copyright`・`Orientation`（引数）・`ColorSpace = 1`・`PixelXDimension`/`PixelYDimension`（元の寸法）・
  `GPSLatitudeRef`/`GPSLatitude`・`BodySerialNumber`・`LensSerialNumber`・`CameraOwnerName`・`Artist`・`XPAuthor`（0x9C9D、`Tag(Context::Tiff, 0x9C9D)`）・`MakerNote`（`Undefined`）・
  IFD1 のサムネイル（`set_jpeg(小さな JPEG, In::THUMBNAIL)`）。
  `Writer` は `Value::Unknown` を書けない（`Cannot write unknown field types`）。`Unknown` を含む入力は、`Writer` で書いた TIFF の中で
  対象タグ（例 `Software`）の IFD エントリをタグ番号で探し、型コード（エントリの 3〜4 バイト目）を未定義の値 99 に書き換えて作る
  （kamadak-exif は未知の型コードを `Value::Unknown` のまま返す）。`UserComment` に 70,000 バイトを入れた入力は PNG で作る

- [ ] **Step 2: テストを書く**

  `core/tests/output_quality.rs`（`process_image` の crop モード越し。出力を `read_from_container` で読み直す。読み直しが警告なしで成功することが全テストの前提）:
  ```rust
  #[test] fn location_and_identifying_tags_are_removed()  // GPS の Context のフィールドが 0 件、BodySerialNumber / LensSerialNumber / CameraOwnerName / Artist / XPAuthor が無い（前提: 入力には全部ある）
  #[test] fn shooting_information_is_kept()              // Make / Model / ExposureTime / DateTimeOriginal / Copyright が入力と同じ値
  #[test] fn orientation_is_reset_to_1()                 // Orientation 6 の入力 → 出力 Orientation == 1、かつ出力の画素は回転済み（縦横が入れ替わっている）
  #[test] fn pixel_dimensions_are_the_output_size()      // PixelXDimension / PixelYDimension == 出力の幅・高さ
  #[test] fn thumbnail_and_makernote_are_removed()       // In::THUMBNAIL のフィールドが 0 件、MakerNote が無い（前提: 入力にはある）
  #[test] fn unknown_values_are_dropped_and_the_rest_is_kept() // Unknown を含む入力 → 警告なし、Make が残る
  #[test] fn oversized_exif_is_dropped_with_a_warning()  // UserComment 70,000 バイトの PNG → 成功、出力に Exif が無い、"Writing without EXIF: EXIF of" の警告 1 件（Review Focus 1）
  #[test] fn input_without_exif_writes_no_exif_and_no_warning()
  #[test] fn broken_exif_is_dropped_with_a_warning()     // APP1 の TIFF ヘッダを壊した入力 → 成功、"Writing without EXIF: failed to read" の警告
  #[test] fn jxl_and_avif_exif_is_carried_over()         // 既存の exif_o6.jxl / exif_o6.avif → 出力に Exif があり Orientation == 1、Make == "TestMake"
  ```

  `core/src/output_exif.rs`（`exif::Exif` には公開のコンストラクタが無く、`src` のテストから `core/tests/common/` は使えない。
  テストモジュールの中に「`Field` の列を `Writer` で書き、`Reader::read_raw` で読む」小さな関数を置く）:
  ```rust
  #[test] fn only_structural_tags_left_means_no_exif()   // Orientation / ColorSpace / PixelX/YDimension / ExifVersion / XResolution ... だけの Exif → build が None
  #[test] fn one_descriptive_tag_is_enough_to_write()    // 上に Software を 1 つ足す → Some
  #[test] fn offset_tags_are_removed()                   // SubIFDs 0x014A・DNGPrivateData 0xC634・OffsetSchema 0xEA1D を持つ入力 → 出力に無い（前提: 入力にはある）
  ```

- [ ] **Step 3: 失敗を確かめる → `output_exif.rs` を実装する**

  spec「Exif」の表を上から順にそのまま実装する（消す: GPS の Context すべて・In::THUMBNAIL すべて・`MakerNote`・個体識別 5 タグ・オフセット系、
  書き換える: `Orientation`→1・`ColorSpace`→1・`PixelX/YDimension`→出力の寸法（元に無ければ足さない）、`Value::Unknown` は落とす）。
  「書かない場合」の判定に使う構造タグの集合は spec の列挙を逐語で定数にする。
  書き出しは `experimental::Writer`（`little_exif` は使わない。SHORT→LONG の正規化が起きることが分かっているため）。エンディアンは元の Exif に合わせる

- [ ] **Step 4: `process_image` に組み込む**

  `output_exif::load` は向きの適用より前に 1 回（`Err` は「failed to read」の警告）。`build` は最終の寸法が決まった後（最終縮小の後）。
  `build` の `Err` は「failed to rebuild」、`MAX_EXIF_TIFF_LEN` 超は「does not fit」の警告にして `exif: None`

- [ ] **Step 5: 実写真で確かめる**（spec「検証（Exif）」）

  撮って出しの JPEG（Task 1 Step 2 で頼んだもの）と LrC の書き出し（サンプル写真）を変換し、
  出力を kamadak-exif で読み直して警告なく読めること・残ったタグの一覧を実施メモに残す。Orientation 6 の写真を Exif の向きに従うビューアで開き、二重に回らないことを目で確かめる

- [ ] **Step 6: `make check` を通してコミット**（`feat(core): 書き出し品質の改善: 位置情報と個体識別タグを除いた Exif を出力に載せる`）

## Task 8: Exif フレームのロゴを表示する寸法で直接ラスタライズし、demultiply する

**Files:**
- Modify: `core/src/exif_frame/logo.rs`, `core/src/exif_frame/mod.rs`, `docs/superpowers/specs/2026-08-12-output-width-limit-design.md`

**Interfaces:**
- Produces（`logo.rs`）: 寸法引数を `target_size: u32`（長辺）から `max: (u32, u32)`（最大幅, 最大高さ。アスペクト比を保って収める）に変える。対象は `load_bundled_logo`・`load_logo_file`・`resolve_lens_brand_logo`・`resolve_and_load_logo`
- Produces（`mod.rs`）: `fn bar_font_sizes(buf_h: u32, photo_short_side: u32, font: &FontConfig) -> (f32, f32)`（現行 430-436 行の `primary_size_base` / `secondary_size_base` の計算を切り出したもの）と
  `fn maker_logo_height(buf_h: u32) -> u32`（`((buf_h as f32 * 0.45) as u32).max(1)`）。`render_exif_frame` と `render_exif_bar` の両方がこれを使う

- [ ] **Step 1: テストを書く**（`logo.rs` / `mod.rs` の `#[cfg(test)]`）

  ```rust
  #[test] fn wide_svg_logo_is_rasterized_at_the_requested_height()   // 同梱 sony.svg を max (10_000, 19) で読む → 高さ 19、幅は 19 × 5.68 の ±2
  #[test] fn extremely_wide_svg_is_bounded_by_the_bar_length()       // テスト内の 1000:1 の SVG を max (500, 40) → 幅 <= 500（Review Focus 4）
  #[test] fn white_logo_edges_are_not_darkened()                     // テスト内の SVG（白い矩形、x = 0.5 から始まる）を読む → 0 < α < 255 の画素の RGB がすべて 255±1（前提: そういう画素が 1 つ以上ある）
  // mod.rs の #[cfg(test)]（maker_logo_height が非公開のため。ExifInfo は Make = "SONY"、ExifAssets は利用者ディレクトリなしで作る）
  #[test] fn maker_logo_is_drawn_at_its_loaded_size()                // render_exif_frame（写真 1080×720 相当）で、バーの左余白からセパレーターまでの列で背景色と異なる画素の行の範囲 == maker_logo_height(バーの太さ) の ±1
  ```

  `white_logo_edges_are_not_darkened` と `wide_svg_logo_is_rasterized_at_the_requested_height` は、直す前のコードで落ちることを確かめる
  （前者は RGB が α と同じ値に下がる、後者は長辺 = 幅で読まれて高さが約 1/5.68 になる）

- [ ] **Step 2: `load_svg_from_bytes` を直す**

  `scale = min(max_w / orig_w, max_h / orig_h)`。pixmap は `pixmap.pixels()` の各画素を `demultiply()` して `RgbaImage` に詰める。
  PNG の経路は `img.resize(max_w, max_h, Lanczos3)`（収める。縮小 1 回だけ）

- [ ] **Step 3: 読み込みの順序を入れ替える**

  `render_exif_frame` で、バーの太さ `buf_h = min(exif_area_height, exif_area_width)` とバーの長さ `buf_w = max(...)` を先に求め、
  メーカーロゴは `(buf_w, maker_logo_height(buf_h))`、レンズロゴは `(buf_w, ((secondary_size_base * 1.2) as u32).max(1))` で読む（`secondary_size_base` は `bar_font_sizes`）。
  `render_exif_bar` の `logo.resize(...)`（403-407 行）と `llogo.resize(...)`（442-448 行）を消し、渡されたロゴをそのまま使う。
  既存の `lens_logo_never_overlaps_the_exif_text` が通ることを確かめる（このテストは 120×40 のロゴを直接渡しているので、再リサイズが無くなった分、期待する配置が変わりうる。変わったら、テストが守る「重ならない」が今も検査できているかを前提条件の assert で確かめる）

- [ ] **Step 4: 出力幅上限 spec の誤記を訂正する**

  `2026-08-12-output-width-limit-design.md` の 184-186 行の引用の直後に、
  「**訂正（2026-10-20）**: 横長の SVG（SONY 5.68:1、FUJIFILM 6:1）では長辺が幅になり、読み込み時の高さが表示高さより小さくなるため、約 4.3〜4.5 倍の拡大が起きていた。
  [書き出し品質の改善 spec](2026-10-20-output-quality-design.md) の 2 の節で直した」を足す

- [ ] **Step 5: 比較画像を作り、利用者に見せる**（完了条件の 2 つ目）

  サンプル写真を pad + Exif フレームで、原寸と `--max-width 1080` × 白背景と黒背景の 4 通り書き出す（差し替え前のものは `git stash` か Task 7 のコミットの CLI で作る）。
  基準画像として、同じ高さで `magick -background none -density ... sony.svg` から直接描いたものを並べる。了承を実施メモに残す

- [ ] **Step 6: `make check` を通してコミット**（`fix(core): 書き出し品質の改善: Exif フレームのロゴを表示する寸法で直接描き、縁を demultiply する`）

## Task 9: Exif の文字の合成式を直す

**Files:**
- Modify: `core/src/exif_frame/text.rs`

- [ ] **Step 1: テストを書く**

  ```rust
  #[test] fn text_drawn_on_a_transparent_layer_composites_like_direct_drawing()
  ```
  同じ文字列（例 `"ILCE-7RM5  35mm"`、サイズ 24、色 `#333333`）を 2 通りに描く。
  (a) 不透明な白（255,255,255,255）の 400×60 に直接 `draw_text_on_image`、
  (b) 透明（0,0,0,0）の 400×60 に描いてから、白の 400×60 に `image::imageops::overlay`。
  全画素で (a) と (b) の RGB 差が ±2 以内（前提: 0 < coverage < 1 の縁の画素が 1 つ以上ある＝(a) で 51 < R < 255 の画素がある）。
  修正前のコードで縁の画素が食い違って落ちることを確かめる（spec「4」）

- [ ] **Step 2: 合成をストレートアルファの over にする**

  `out_a = a + ea·(1 − a)`、`rgb = (c·a + e·ea·(1 − a)) / out_a`（`out_a == 0` なら画素は (0,0,0,0) のまま）。a は `coverage × 色のα`。
  Right / Left の配置は `render_exif_bar` の結果を `rotate90` してから同じ `overlay` を通るので、文字の描画関数はこの 1 つで足りる

- [ ] **Step 3: `draw_text_rotated_90` を消す**

  `grep -rn draw_text_rotated_90 --include=*.rs` で本番の呼び出し元が無いことを確かめ（計画時: 定義・コメント（mod.rs 427 行付近）・テスト 2 件だけ）、定義・そのテスト・コメントの言及を消す

- [ ] **Step 4: `make check` を通してコミット**（`fix(core): 書き出し品質の改善: Exif の文字の縁でアルファが二重に掛かるのを直す`）

## Task 10: 文書を更新し、完了条件を確かめる

**Files:**
- Modify: `README.md`, `docs/README.md`, 本計画の実施メモ

- [ ] **Step 1: README を直す**（spec「文書」の 6 項目）

  - 「機能」: 出力が sRGB の ICC と Exif（位置情報・シリアル番号・所有者名・作者名を除く）を持つ
  - CLI オプション表の `--quality`: 目盛りが LrC 寄りになったこと（品質 20 ≒ LrC 20%）。**以前の設定値は数値のまま引き継がれ、意味が変わる**こと
  - 「サイズ圧縮」: 「初期品質で収まればそれを使う → 収まらなければ下限（`MIN_QUALITY` の実際の値）までの間で、収まる最も高い品質を二分探索 → 下限でも超えればそのまま保存して警告」
  - 「出力」: JPEG / PNG / WebP の入力は ICC を見て sRGB に変換する（JPEG XL / AVIF は既知の制限のまま）。透過は背景色で塗る
  - 「ライセンス」: `jpeg-encoder` の IJG ライセンスの一文「this software is based in part on the work of the Independent JPEG Group」
  - Task 5 で直した「変換モード」と `--bg-color` が残っていることを確かめる

- [ ] **Step 2: `docs/README.md` の本計画の索引行を更新する**（計画時に足した行へ、目盛りの調整結果・サイズ制限の下限・各検証の実測値を持つことを書き足す）

- [ ] **Step 3: 完了条件の 3 つ目を見せる**

  LrC 20% の書き出し（利用者の `261011` フォルダーの 9 枚のうち 1 枚以上）と、同じ写真の本ツールの品質 20 を並べて見せる。
  元の写真は Task 1 Step 2 で頼んだもの（LrC 20% の書き出しは元ではない。元の RAW から LrC で書き出した高品質の JPEG / JPEG XL）。
  両者のサイズと、DQT が一致していることを実施メモに残す

- [ ] **Step 4: 性質 5 を測り直す**

  Task 3 は埋め込む分を見積もって足した値で合わせた。その見積もりが正しかったことを、Task 6・7 の後の出力（sRGB の ICC と Exif 込み）で
  Task 1 の写真すべての品質 90 を測り直して確かめ、実施メモに残す。
  外れた写真があれば Task 3 Step 4 と同じく止まって利用者に判断を仰ぐ。定数を変えることになったら、Task 3（Step 1 からやり直し、Step 6 の下限も換算し直す）
  → Task 4（`MIN_QUALITY`）→ 本タスク Step 1（README「サイズ圧縮」の下限の値）の順に直す

- [ ] **Step 5: 完了条件を確かめる**

  `make check`、`cd gui-frontend && bun run e2e`、spec の各「検証」の実測値が実施メモにあること、利用者の了承 3 つ（目盛り・ロゴ・LrC 20% との比較）が実施メモにあること

- [ ] **Step 6: コミット**（`docs: 書き出し品質の改善: README と索引を更新し、実施メモを残す`）

---

## セルフレビュー（spec との突き合わせ）

| spec の節 | タスク |
|---|---|
| 1 エンコーダ（設定・サムネイルとプレビュー・並び・ライセンス） | Task 2、ライセンスは Task 10 |
| 1 目盛り（性質 1〜3a） | Task 2 の単体テストと DQT の統合テスト |
| 1 目盛り（性質 4・5・優先順位・サンプリングの閾値・LrC 85% の参考比較） | Task 3 |
| 1 既存の設定値 | Task 10（README）。移行処理はしない |
| 2・3 ロゴ（レンズロゴを含む）・誤記の訂正 | Task 8 |
| 4 文字の合成・`draw_text_rotated_90` の削除 | Task 9 |
| 5 ICC（対象・`reserve`・相対的色域維持・画素形式・行単位・警告・sRGB 判定・出力の ICC） | Task 6 |
| 6 透過・背景色の全モード表示 | Task 5 |
| 7 サイズ制限（動き・下限の換算・回数・単調性のコメント・警告文） | Task 3（下限）・Task 4 |
| Exif（表・XMP/IPTC を写さない・書かない場合・入力・書き出し） | Task 7 |
| 付随処理の失敗 | Task 6・7 の警告テスト。例外の `reserve` は Task 6 |
| 文書・完了条件 | Task 10 |

### spec から意図的に外したこと・テストを置かなかったこと

- **ICC を持つ JPEG XL / AVIF**: spec の「やらないこと」。`open_with_warnings` は JPEG XL / AVIF の経路に ICC 変換を入れない
- **PNG の gAMA / cHRM / sRGB チャンク・16bit → 8bit のディザ・XMP / IPTC**: spec の「やらないこと」
- **CMYK の JPEG＋CMYK の ICC の実ファイル**: 手元に CMYK の ICC が無い。同じ分岐（画素形式と ICC の色空間の不一致）を、RGB の画素＋Gray の ICC で通す（Review Focus 2）
- **品質 1 の色の破綻**: 機械的な合否にせず、比較画像を利用者が目で見る（spec の検証手順どおり）
- **CPU による 1 階調の差**: 許容差（±3 以上）で吸収する。確かめる手段（AVX の有無で走らせ分ける）は持たない

---

## 実施メモ

### 計画時の試作（2026-10-20。参考値。合否には Task 1 の数値を使う）

Pillow 12.3 で、`qtables`（自然順）・`subsampling`・`optimize=True` を指定して、目盛りの候補をエンコードした。
「現行」は Pillow の `quality=q, subsampling=0`（4:4:4・ハフマン最適化なし。spec の「現行は同じ品質値の libjpeg 4:4:4 と一致」による）。
PSNR は Global Constraints の定義。写真はすべて原寸（サンプル写真 7008×4672、JPEG XL は 66MP 前後を `magick -depth 8` で PNG にしたもの）。

- LrC 20% の 9 枚の DQT は 9 枚とも spec の値と一致した（Python でマーカーを走査して確認）
- 候補の品質 20（LrC 20% のテーブル・4:2:0・最適化）は、サンプル写真で **607KB 42.64dB** となり、spec の試作（`jpeg-encoder`）と一致した

現行（Pillow）:

| 写真 | q20 | q60 | q90 |
|---|---|---|---|
| サンプル写真 | — | 1217KB 42.84dB | 4209KB 47.78dB |
| DSC05479（杉林） | — | 9640KB 36.93dB | 19781KB 43.03dB |
| DSC05529（夕景） | — | 4063KB 40.53dB | 10388KB 45.35dB |
| result2（星景） | — | 5138KB 38.54dB | 11946KB 43.06dB |

候補の品質 20（全候補で共通）: サンプル写真 607KB 42.64dB、DSC05479 9588KB 36.06dB、DSC05529 2768KB 40.08dB、result2 4274KB 38.35dB。
**4 枚とも現行の品質 60 の PSNR に 0.2〜0.9dB 届かない**ので、サイズ制限の下限は 20 より上になる見込み（Task 3 Step 6）。

候補の品質 90（形 `v = B^((1−u)^a)`、`u = ((q−20)/80)^G`。`BETA = 1`・`CAP = 40`。現行の品質 90 と比べた合否は「サイズ ≦ かつ PSNR ≧」）:

| G | A_LO | A_HI | 4:2:0 の上限 | サンプル写真 | DSC05479 | DSC05529 | result2 |
|---|---|---|---|---|---|---|---|
| 1 | 1 | 0.3 | 50 | 6249KB 56.35dB（サイズ×） | 33044KB 49.28dB（×） | 20927KB 49.86dB（×） | 24468KB 49.88dB（×） |
| 1 | 1 | 0.1 | 50 | 6113KB 55.85dB（×） | 31968KB 49.20dB（×） | 19802KB 49.66dB（×） | 23236KB 49.50dB（×） |
| 1 | 0.6 | 0.1 | 50 | 4761KB 52.99dB（×） | 25112KB 47.60dB（×） | 13906KB 48.12dB（×） | 17375KB 47.95dB（×） |
| 1 | 0.4 | 0.05 | 50 | 3520KB 48.66dB（○） | 20976KB 46.42dB（サイズ×） | 10237KB 46.33dB（○） | 13652KB 46.25dB（サイズ×） |
| 2 | 0.5 | 0.1 | 50 | 3190KB 47.86dB（○） | 20176KB 45.69dB（サイズ×） | 9610KB 45.87dB（○） | 13018KB 45.63dB（サイズ×） |
| 3 | 0.5 | 0.1 | 50 | 2561KB 46.69dB（PSNR×） | 18469KB 43.85dB（○） | 8138KB 44.63dB（PSNR×） | 11437KB 43.96dB（○） |
| 3 | 1 | 0.2 | 50 | 4517KB 51.92dB（×） | 23861KB 47.21dB（×） | 12780KB 47.63dB（×） | 16143KB 47.60dB（×） |
| 2 | 0.8 | 0.05 | 50 | 4484KB 51.28dB（×） | 23646KB 47.16dB（×） | 12479KB 47.44dB（×） | 15839KB 47.42dB（×） |
| 2.3 | 0.6 | 0.05 | 50 | 3244KB 48.04dB（○） | 20502KB 45.79dB（サイズ×） | 9777KB 45.94dB（○） | 13134KB 45.64dB（サイズ×） |
| 2.3 | 0.6 | 0.05 | 91 | 2807KB 47.17dB（PSNR×） | 18977KB 44.16dB（○） | 9030KB 45.71dB（○） | 12101KB 45.29dB（サイズ×） |
| 2 | 0.8 | 0.05 | 91 | 3954KB 49.83dB（○） | 21650KB 45.36dB（サイズ×） | 11441KB 47.19dB（○） | 14413KB 46.93dB（サイズ×） |

別の形 `v = B^(1 − u^k)`（`u = (q−20)/80`、`k` を低周波 4.4〜6・高周波 20）も試したが、品質 20〜60 のテーブルがほぼ変わらず、品質 90 で 4 枚とも PSNR が現行を下回った
（k = 4.4/20: 2592KB 46.29dB / 17911KB 42.53dB / 7561KB 43.89dB / 10585KB 42.78dB）。

読み取れること:
- 線形の倍率（G = 1）では品質 90 が細かすぎ、現行より 1.5〜1.7 倍大きい。品質 90 は LrC 85% に近い粗さ（DC 2〜3、高周波 8〜12）まで下げる必要がある
- 滑らかな写真は PSNR、細部・ノイズの多い写真はサイズで外れ、**この形の中では 4 枚同時に満たす定数をまだ見つけていない**（最も近いのは G = 2.3 / 0.6 / 0.05 で、DSC05479 が 3.6%、result2 が 9.9% 大きい）
- `A_HI = 0.05` では高周波が品質 99 の 8 から 100 の 1 へ跳ぶ（Task 3 Step 2 の合否に掛かる見込み）
- 試作は Pillow（libjpeg）で、`jpeg-encoder` とはハフマン最適化と色差の平均の実装が違う。数値は Task 1〜3 で測り直す

### Task 1: 差し替え前の数値

**計測の写真の組**（利用者が確定。LrC 品質 92 の書き出しで足りると了承済み。ファイル名のみ）:

- `20260328-DSC05024.jpg`: サンプル写真
- `20261010-DSC05479.jxl`: 杉林と社殿。細部が多い。ISO 100
- `20261010-result2.jxl`: 星景。ノイズが多い。ISO 3200・198 秒
- `20261010-DSC05529.jxl`: 夕景のグラデーション。滑らか

JPEG XL の 66MP も 512MB の予算内で開けた（PNG への変換は不要だった）。

**差し替え前のコード（`jpeg-encoder` 既定・4:4:4・Annex K）での計測**。
`cargo test -p picture-tool-core --release --test output_quality_spike -- --ignored measure`
（所要 151.58s）。`PICTURE_TOOL_SPIKE_IMAGES` は**絶対パス**で渡す（相対パスはテストの cwd = `core/` からの解決になる）。
`luma_max`・`chroma_max` は DQT のテーブル 0・1 の最大要素、`sampling` は SOF の成分ごとの H×V。

```tsv
file	quality	bytes	psnr_db	luma_max	chroma_max	sampling
20260328-DSC05024.jpg	1	905875	23.73	255	255	1x1/1x1/1x1
20260328-DSC05024.jpg	5	912864	27.49	255	255	1x1/1x1/1x1
20260328-DSC05024.jpg	10	932236	33.74	255	255	1x1/1x1/1x1
20260328-DSC05024.jpg	20	975073	38.34	255	248	1x1/1x1/1x1
20260328-DSC05024.jpg	40	1070380	41.70	151	124	1x1/1x1/1x1
20260328-DSC05024.jpg	60	1232869	42.85	97	79	1x1/1x1/1x1
20260328-DSC05024.jpg	90	4003385	47.57	24	20	1x1/1x1/1x1
20260328-DSC05024.jpg	100	8125370	55.55	1	1	1x1/1x1/1x1
20261010-DSC05479.jxl	1	2006313	22.57	255	255	1x1/1x1/1x1
20261010-DSC05479.jxl	5	2432373	25.05	255	255	1x1/1x1/1x1
20261010-DSC05479.jxl	10	3372048	28.44	255	255	1x1/1x1/1x1
20261010-DSC05479.jxl	20	4992974	31.76	255	248	1x1/1x1/1x1
20261010-DSC05479.jxl	40	7610521	34.94	151	124	1x1/1x1/1x1
20261010-DSC05479.jxl	60	9765085	36.92	97	79	1x1/1x1/1x1
20261010-DSC05479.jxl	90	19917269	42.97	24	20	1x1/1x1/1x1
20261010-DSC05479.jxl	100	42808323	49.92	1	1	1x1/1x1/1x1
20261010-result2.jxl	1	1846390	26.81	255	255	1x1/1x1/1x1
20261010-result2.jxl	5	1892814	29.66	255	255	1x1/1x1/1x1
20261010-result2.jxl	10	2027872	32.35	255	255	1x1/1x1/1x1
20261010-result2.jxl	20	2454962	34.74	255	248	1x1/1x1/1x1
20261010-result2.jxl	40	3796355	37.28	151	124	1x1/1x1/1x1
20261010-result2.jxl	60	5147459	38.52	97	79	1x1/1x1/1x1
20261010-result2.jxl	90	11854285	43.02	24	20	1x1/1x1/1x1
20261010-result2.jxl	100	32077359	50.47	1	1	1x1/1x1/1x1
20261010-DSC05529.jxl	1	1840376	25.94	255	255	1x1/1x1/1x1
20261010-DSC05529.jxl	5	1869962	29.27	255	255	1x1/1x1/1x1
20261010-DSC05529.jxl	10	1985308	32.85	255	255	1x1/1x1/1x1
20261010-DSC05529.jxl	20	2229158	36.68	255	248	1x1/1x1/1x1
20261010-DSC05529.jxl	40	2940902	39.15	151	124	1x1/1x1/1x1
20261010-DSC05529.jxl	60	4055627	40.52	97	79	1x1/1x1/1x1
20261010-DSC05529.jxl	90	10257385	45.27	24	20	1x1/1x1/1x1
20261010-DSC05529.jxl	100	25525855	50.33	1	1	1x1/1x1/1x1
```

- 現行は**全品質で 4:4:4**（`1x1/1x1/1x1`）。品質 90 未満で 4:2:0 になる `Encoder::new` の既定は `process_image` の経路では効いていない。
- サンプル写真の品質 20 は 975,073 B・38.34 dB、品質 60 は 1,232,869 B・42.85 dB で、spec の表（975KB 38.3dB / 1,233KB 42.8dB）と一致した。
- 比較画像（サンプル写真を現行の品質 1,5,10,20,40,60,90,100 で書き出し、切り抜き `400x300+500+2800` と `-modulate 100,300` の彩度強調）は `docs/tmp/output-quality/before/`（`q<品質>.png` と `q<品質>-sat.png`、計 16 枚。コミットしない）。
  CLI（`-m quality --max-size 1024`）で書き出した各品質のバイト数は上の表と一致した。

### Task 2: エンコーダの差し替え

- `jpeg-encoder` 0.7.1 の API は計画どおりの名前で存在した（`set_quantization_tables`・`set_sampling_factor`・`set_chroma_subsampling_method`・
  `set_optimized_huffman_tables`・`add_icc_profile`・`add_exif_metadata`）。`SamplingFactor` は `PartialEq` を持つのでテストで直接比べられる。
  `simd` feature は既定で off で、スカラー経路だけが使われる（CPU で出力が変わらない）。`Custom` の値は内部で `clamp(1, 2048)` されるだけで、255 超は検査されない
- 依存は `core/Cargo.toml` にだけ足した。ワークスペースの `[workspace.dependencies]` は複数クレートが使う依存の置き場で、`jpeg-encoder` は core だけが使う
- 計画の 5 つの契約テストのうち、品質 20（4:2:0）と 90（4:4:4）は `Encoder::new` の既定（90 未満で 4:2:0）と結果が一致するので、
  `set_sampling_factor` を消しても通ってしまう。食い違う品質 60 で 4:4:4 を確かめる `quality_60_is_444_although_the_encoder_default_is_420` を足した。
  `set_sampling_factor` を消すとこれだけが落ち、`set_optimized_huffman_tables(false)` にすると `huffman_tables_are_optimized` だけが落ちることを確かめた
- 差し替え前のエンコーダで `output_quality.rs` を走らせると、LrC テーブル・4:2:0・ハフマン最適化の 3 件が落ち、品質 90 の 4:4:4 とベースラインの 2 件は通った（現行も満たしている性質）
- `ChromaSubsamplingMethod::Average` を外しても落ちるテストは無い（ヘッダーに現れない。計画にもテストは無い）
- `core/tests/common/jpeg.rs` に `markers` と `dht` を足した（`output_is_baseline` と `huffman_tables_are_optimized` 用）

### Task 3: 目盛りの調整と下限

**状態（2026-10-20）**: 定数だけでは性質 5 を満たせず Step 4 で一度止めた（下の「定数での探索の結果」まで）。
利用者の判断で、式を「Annex K の形を LrC 20% で上から抑える」に変え、性質 5 に許容幅（サイズ +1%・PSNR −0.1dB）を入れた（spec 性質 5 に書き足した）。
その後の実装と Step 1〜3・6・7 は下の「式を変えた後」。
**2026-10-20 利用者が了承**: (1) 目盛り（scale-compare.png）、(2) `CAP` を 40 から 30 に下げる、(3) 20〜90 の補間の形と下限 28。

**Step 1: 埋め込む分**（バイト）

- sRGB の ICC: `moxcms` 0.8.1 の `ColorProfile::new_srgb().encode()` の長さ = **612**（スクラッチの使い捨てクレートで測った。core には足していない）。
  APP2 の見出しは マーカー 2 ＋ 長さ 2 ＋ `"ICC_PROFILE\0"` 12 ＋ 通し番号と総数 2 = 18。計 **630**
- 入力の Exif（`magick <写真> exif:- | wc -c`。先頭は `Exif\0\0II` だった）。APP1 のマーカー 2 ＋ 長さ 2 を足す

| 写真 | Exif | Exif＋4 | 埋め込む分の合計 | 現行の品質 90 | 新しい品質 90 に許されるバイト数 |
|---|---|---|---|---|---|
| 20260328-DSC05024.jpg | 11589 | 11593 | 12223 | 4003385 | 3991162 |
| 20261010-DSC05479.jxl | 916 | 920 | 1550 | 19917269 | 19915719 |
| 20261010-result2.jxl | 490 | 494 | 1124 | 11854285 | 11853161 |
| 20261010-DSC05529.jxl | 916 | 920 | 1550 | 10257385 | 10255835 |

**Step 1: Task 2 の定数（G 2.3・A_LO 0.6・A_HI 0.05・BETA 1・CAP 40・閾値 50）の品質 90**（spike。所要 31.41s）

```tsv
file	quality	bytes	psnr_db	luma_max	chroma_max	sampling
20260328-DSC05024.jpg	90	3322326	48.04	10	10	1x1/1x1/1x1
20261010-DSC05479.jxl	90	20994498	45.79	10	10	1x1/1x1/1x1
20261010-result2.jxl	90	13448749	45.63	10	10	1x1/1x1/1x1
20261010-DSC05529.jxl	90	10012036	45.94	10	10	1x1/1x1/1x1
```

| 写真 | バイト数＋埋め込む分 ≦ 現行 | PSNR ≧ 現行 |
|---|---|---|
| DSC05024 | 3334549 ≦ 4003385 ○ | 48.04 ≧ 47.57 ○ |
| DSC05479 | 20996048 > 19917269 **×（+5.4%）** | 45.79 ≧ 42.97 ○ |
| result2 | 13449873 > 11854285 **×（+13.5%）** | 45.63 ≧ 43.02 ○ |
| DSC05529 | 10013586 ≦ 10257385 ○ | 45.94 ≧ 45.27 ○ |

**探索の道具**: spike は 1 回ごとに JPEG XL を開き直すので、`open_image_oriented` の RGB8 を一度だけ書き出し、
`quant.rs` と同じ式・同じエンコーダ設定で品質 90 を並列にエンコードする使い捨てのクレートを作った（`docs/tmp/output-quality/task3-tuning/tuner-main.rs`。コミットしない）。
Task 2 の定数で 4 枚とも spike とバイト数・PSNR が一致することを確かめてから使った。
全行の TSV は `docs/tmp/output-quality/task3-tuning/` の `g1.tsv`（格子 96 組）・`g2.tsv`（乱択 300 組）・`g3.tsv`（4:2:0 を 90 まで広げた乱択 120 組）・`c1〜c3.tsv`（式の外のテーブル）にある。
判定は `judge.py`。列は `定数(G,A_LO,A_HI,BETA,CAP,閾値)	写真	品質	バイト数	PSNR`。

**定数での探索の結果**: 516 組のどれも 4 枚同時には通らなかった。result2 のサイズ（埋め込む分込み）が現行以下になる組の中で、サンプル写真の PSNR が最も高いのは `4.61,0.6,0.378,1,40,50` の 46.78dB（現行より −0.79dB）。代表の行（逐語）:

```tsv
3,0.6,0.15,1,40,50	20260328-DSC05024	90	3102971	47.60
3,0.6,0.15,1,40,50	20261010-DSC05479	90	20288620	45.36
3,0.6,0.15,1,40,50	20261010-DSC05529	90	9556924	45.61
3,0.6,0.15,1,40,50	20261010-result2	90	13056702	45.33
3,0.6,0,1,40,50	20260328-DSC05024	90	2799934	46.98
3,0.6,0,1,40,50	20261010-DSC05479	90	19676028	44.25
3,0.6,0,1,40,50	20261010-DSC05529	90	8863318	44.92
3,0.6,0,1,40,50	20261010-result2	90	12201282	44.10
2.3,0.6,0,1,40,50	20260328-DSC05024	90	3322189	48.01
2.3,0.6,0,1,40,50	20261010-DSC05479	90	20895226	45.79
2.3,0.6,0,1,40,50	20261010-DSC05529	90	9897636	45.87
2.3,0.6,0,1,40,50	20261010-result2	90	13304220	45.45
4.61,0.6,0.378,1,40,50	20260328-DSC05024	90	2665158	46.78
4.61,0.6,0.378,1,40,50	20261010-DSC05479	90	18767435	43.99
4.61,0.6,0.378,1,40,50	20261010-DSC05529	90	8371348	44.74
4.61,0.6,0.378,1,40,50	20261010-result2	90	11830336	44.31
3.7,0.87,0.133,1,40,91	20260328-DSC05024	90	3145553	47.70
3.7,0.87,0.133,1,40,91	20261010-DSC05479	90	19889912	44.66
3.7,0.87,0.133,1,40,91	20261010-DSC05529	90	9679512	46.09
3.7,0.87,0.133,1,40,91	20261010-result2	90	12878820	45.86
4.34,0.82,0.116,1,40,91	20260328-DSC05024	90	2543446	46.52
4.34,0.82,0.116,1,40,91	20261010-DSC05479	90	18662300	43.40
4.34,0.82,0.116,1,40,91	20261010-DSC05529	90	8597143	45.11
4.34,0.82,0.116,1,40,91	20261010-result2	90	11793201	44.54
```

（4:2:0 の行の PSNR は `jpeg-decoder` 0.3 で復号して測った。理由は下の「zune-jpeg」）

**式の外のテーブルで確かめたこと**（`c1.tsv`。テーブルは `t1.txt`、自然順）:

```tsv
annexK90	20260328-DSC05024	90	3763862	47.79
annexK90	20261010-DSC05479	90	19847095	43.03
annexK90	20261010-DSC05529	90	10013604	45.35
annexK90	20261010-result2	90	11685938	43.06
annexK90_420	20260328-DSC05024	90	3293268	46.89
annexK90_420	20261010-DSC05479	90	18227957	41.40
annexK90_420	20261010-DSC05529	90	9293924	45.07
annexK90_420	20261010-result2	90	10876172	42.84
minK90B	20260328-DSC05024	90	3797092	47.84
minK90B	20261010-DSC05479	90	20539647	43.35
minK90B	20261010-DSC05529	90	10123552	45.43
minK90B	20261010-result2	90	12218467	43.28
minK90B_420	20260328-DSC05024	90	3325542	46.94
minK90B_420	20261010-DSC05479	90	18947471	41.75
minK90B_420	20261010-DSC05529	90	9403089	45.16
minK90B_420	20261010-result2	90	11394432	43.06
```

- `annexK90` は**現行と同じテーブル**（Annex K の品質 90、最大 24/20）を 4:4:4・ハフマン最適化で書いたもの。現行より小さいのは **0.4〜6.0%** だけだった
  （DSC05024 −6.0%、DSC05479 −0.4%、DSC05529 −2.4%、result2 −1.4%）。spec の「ハフマン最適化だけで約 35% 小さくなる」は品質 60 のサンプル写真の値で、品質 90 の大きなファイルでは成り立たない
- 性質 1 と 3 から、品質 90 のテーブルはどの要素も LrC 20% 以下になる（高周波は 12 以下）。現行の品質 90 は高周波が 20〜24 なので、
  これより粗くできない分だけ細部・ノイズの多い写真が大きくなる。`minK90B`（現行の品質 90 と LrC 20% の要素ごとの小さい方。性質 1・3 の下で現行に最も近いテーブル）は
  DSC05479 が +3.1%、result2 が +3.1% 大きく、サイズで外れる
- つまり性質 5 は「ハフマン最適化の利得（0.4〜6%）」と「高周波を 12 で抑える損（約 3%）」の差で決まり、ほとんど余裕がない。
  今の式 `B^((1−u)^a)` は Annex K の形（低周波 2〜3 から中周波で急に粗くなる）を表せないので、さらに外れる

Annex K の形を LrC 20% で抑えたテーブル（`min(B, round(K·0.2·s))`、輝度の倍率 `s_l`・色差の倍率 `s_c`）も試した（`c2.tsv`・`c3.tsv`）。最も近いもの（逐語）:

```tsv
K_1.04_1.0,-,-,-,-,-	20260328-DSC05024	90	3746364	47.60
K_1.04_1.0,-,-,-,-,-	20261010-DSC05479	90	20247881	43.16
K_1.04_1.0,-,-,-,-,-	20261010-DSC05529	90	9912734	45.29
K_1.04_1.0,-,-,-,-,-	20261010-result2	90	11973846	43.18
K_1.06_1.0,-,-,-,-,-	20260328-DSC05024	90	3617385	47.49
K_1.06_1.0,-,-,-,-,-	20261010-DSC05479	90	19968875	43.11
K_1.06_1.0,-,-,-,-,-	20261010-DSC05529	90	9671495	45.23
K_1.06_1.0,-,-,-,-,-	20261010-result2	90	11718629	43.14
```

`K_1.04_1.0` は PSNR が 4 枚とも現行以上で、サイズが DSC05479 +1.7%・result2 +1.0%。`K_1.06_1.0` はサイズが DSC05479 だけ +0.3% で、PSNR がサンプル写真 −0.08dB・DSC05529 −0.04dB。
どちらも外れる。この形はまだ品質 90 の 1 点で比べただけで、品質の目盛り全体（品質 20 で LrC 20%、単調、品質 100 の直前）は作っていない。

**zune-jpeg 0.5.15 が本ツールの 4:2:0 の出力を緑一色に復号する**（目盛りとは別の発見。Step 3・6 の計測に影響する）

- `jpeg-encoder` 0.7.1 はハフマン最適化を指定すると成分ごとに別のスキャン（非インターリーブ）で書く（`encoder.rs` の `encode_image_internal`）。
  `image` 0.25.10 が使う `zune-jpeg` 0.5.15 は、これが 4:2:0 のとき全画素 `(0,255,0)` に復号する。4:4:4 は正しく復号する。libjpeg の 4:2:0（インターリーブ）は正しく復号する
- spike の品質 20 の行は `607292	4.18	34	27	2x2/1x1/1x1`（PSNR 4.18dB）になった。同じファイルを ImageMagick（libjpeg）で比べると 42.64dB、`jpeg-decoder` 0.3 でも 42.64dB で、spec の 42.6dB と一致する
- `zune-jpeg` 0.5.16-rc3 では正しく復号した（中央の画素が ImageMagick と同じ値）
- 影響: (1) spike の PSNR は品質 50 未満で使えない（Step 3・6 は別の復号器で測る必要がある）。(2) 本ツール自身や `image` 0.25 を使うソフトに品質 50 未満の出力を読ませると緑になる。
  サイズ制限の下限（Step 6）が 50 未満になると、上限に収まらない写真の出力が 4:2:0 になる

#### 式を変えた後（利用者の判断: 案 B・許容幅 サイズ +1%・PSNR −0.1dB）

**式**（`core/src/encode/quant.rs`。定数は名前付きで同じファイルの 1 か所に置いた）: `B` は LrC 20%、`K` は Annex K、`S` は `LUMA_SCALE = 1.05`・`CHROMA_SCALE = 1.0`、`A` は `ANCHOR_QUALITY = 90` で、
`k(q) = min(B, K·S·(100 − q)/50)` とする。

- q ≥ 90: `k(q)`
- 20 ≤ q < 90: `B^(1−w)·k(90)^w`（`w = (q−20)/70`）
- q < 20: 前と同じ `min(B·(20/q)^BETA, max(B, CAP))`（`BETA = 1`・`CAP = 30`。利用者の判断で 40 から下げた。下の「CAP 30」）
- サンプリングの閾値 50 は変えていない

20〜90 を補間にした理由: 最初は q ≥ 20 をすべて `k(q)` にしていた。その形では、Annex K よりずっと細かい `B` の要素（低周波）が品質 60 前後まで `B` のまま残った。
そのためサンプル写真の品質 20 と 40 が 607292 B と 608183 B でほぼ同じになり（PSNR は 42.64 と 42.65。`jpeg-decoder` で測定）、
現行の品質 60 の PSNR に届く最小の品質が 4 枚とも 15〜40 の外（47〜55。`docs/tmp/output-quality/task3-tuning/s6.tsv`）になった。
品質 90 以上は補間の前後で同じテーブルなので、Step 1・2 の数値は変わらない。

Task 2 の性質テスト（ZIGZAG・品質 20 = LrC 20%・品質 100 = 全要素 1・1〜255・単調・サンプリングの閾値）は変えずに通った。

**PSNR の測り方**: 4:4:4（品質 50 以上）は spike の値（`image`＝zune-jpeg で復号）。
4:2:0（品質 50 未満）は zune-jpeg 0.5.15 で正しく復号できないので、使い捨てのクレートで `jpeg-decoder` 0.3 を使って復号して測った（下の行の先頭の `-2,1.05,1,1,40,50` はその道具の設定名）。
その道具のバイト数は、同じ品質の spike のバイト数と 4 枚とも一致した。

**Step 1**（spike、所要 30.57s。このあと補間を入れたが、品質 90 は変わっていない）

```tsv
file	quality	bytes	psnr_db	luma_max	chroma_max	sampling
20260328-DSC05024.jpg	90	3635163	47.55	15	20	1x1/1x1/1x1
20261010-DSC05479.jxl	90	19988939	43.11	15	20	1x1/1x1/1x1
20261010-result2.jxl	90	11735598	43.15	15	20	1x1/1x1/1x1
20261010-DSC05529.jxl	90	9694999	45.25	15	20	1x1/1x1/1x1
```

| 写真 | バイト数＋埋め込む分 ≦ 現行 × 1.01 | PSNR ≧ 現行 − 0.1 |
|---|---|---|
| DSC05024 | 3647386 ≦ 4043418 ○（現行比 −8.9%） | 47.55 ≧ 47.47 ○（−0.02dB） |
| DSC05479 | 19990489 ≦ 20116441 ○（+0.37%） | 43.11 ≧ 42.87 ○（+0.14dB） |
| result2 | 11736722 ≦ 11972827 ○（−1.0%） | 43.15 ≧ 42.92 ○（+0.13dB） |
| DSC05529 | 9696549 ≦ 10359958 ○（−5.5%） | 45.25 ≧ 45.17 ○（−0.02dB） |

倍率の選び方（品質 90。`n1.tsv`。列は `-1,LUMA_SCALE,CHROMA_SCALE,BETA,CAP,閾値`）。許容幅の中に入ったのは 1.03/1.06・1.03/1.12・1.04/1.06・1.04/1.12・1.05/1.0・1.06/1.0 の 6 組だった。
1.05/1.0 を選んだ。4 枚の中で最も悪い値は、サイズ +0.37%（許容幅の 37% を使う）と PSNR −0.02dB（同 20%）。6 組の中で、許容幅の使い方が大きい方の軸で最も小さかった（1.06/1.0 はサイズ +0.27%・PSNR −0.08dB で、PSNR の軸が 80%）:

```tsv
-1,1.04,1.06,1,40,50	20260328-DSC05024	90	3717182	47.50
-1,1.04,1.06,1,40,50	20261010-DSC05479	90	20100442	43.12
-1,1.04,1.06,1,40,50	20261010-DSC05529	90	9838947	45.24
-1,1.04,1.06,1,40,50	20261010-result2	90	11895281	43.14
-1,1.05,1,1,40,50	20260328-DSC05024	90	3635163	47.55
-1,1.05,1,1,40,50	20261010-DSC05479	90	19988939	43.11
-1,1.05,1,1,40,50	20261010-DSC05529	90	9694999	45.25
-1,1.05,1,1,40,50	20261010-result2	90	11735598	43.15
-1,1.06,1,1,40,50	20260328-DSC05024	90	3617385	47.49
-1,1.06,1,1,40,50	20261010-DSC05479	90	19968875	43.11
-1,1.06,1,1,40,50	20261010-DSC05529	90	9671495	45.23
-1,1.06,1,1,40,50	20261010-result2	90	11718629	43.14
```

**Step 2**（spike、品質 90〜100、所要 85.05s）

```tsv
file	quality	bytes	psnr_db	luma_max	chroma_max	sampling
20260328-DSC05024.jpg	90	3635163	47.55	15	20	1x1/1x1/1x1
20260328-DSC05024.jpg	91	3893419	48.10	14	18	1x1/1x1/1x1
20260328-DSC05024.jpg	92	4125452	48.80	12	16	1x1/1x1/1x1
20260328-DSC05024.jpg	93	4748213	49.83	12	14	1x1/1x1/1x1
20260328-DSC05024.jpg	94	5149491	51.38	12	12	1x1/1x1/1x1
20260328-DSC05024.jpg	95	5789339	53.59	12	10	1x1/1x1/1x1
20260328-DSC05024.jpg	96	6374244	55.68	10	8	1x1/1x1/1x1
20260328-DSC05024.jpg	97	6647328	56.80	8	6	1x1/1x1/1x1
20260328-DSC05024.jpg	98	7170752	58.18	5	4	1x1/1x1/1x1
20260328-DSC05024.jpg	99	8639600	59.21	3	2	1x1/1x1/1x1
20260328-DSC05024.jpg	100	10631055	60.09	1	1	1x1/1x1/1x1
20261010-DSC05479.jxl	90	19988939	43.11	15	20	1x1/1x1/1x1
20261010-DSC05479.jxl	91	21044513	43.52	14	18	1x1/1x1/1x1
20261010-DSC05479.jxl	92	22015193	44.13	12	16	1x1/1x1/1x1
20261010-DSC05479.jxl	93	23349621	44.96	12	14	1x1/1x1/1x1
20261010-DSC05479.jxl	94	24696424	45.83	12	12	1x1/1x1/1x1
20261010-DSC05479.jxl	95	26756609	46.94	12	10	1x1/1x1/1x1
20261010-DSC05479.jxl	96	29213178	48.08	10	8	1x1/1x1/1x1
20261010-DSC05479.jxl	97	31319834	48.69	8	6	1x1/1x1/1x1
20261010-DSC05479.jxl	98	34342822	49.47	5	4	1x1/1x1/1x1
20261010-DSC05479.jxl	99	41701036	50.42	3	2	1x1/1x1/1x1
20261010-DSC05479.jxl	100	54763642	50.80	1	1	1x1/1x1/1x1
```

- DSC05024: 隣り合う比 1.071, 1.060, 1.151, 1.085, 1.124, 1.101, 1.043, 1.079, 1.205（90〜99 の最大 1.205）。`size(100)/size(99)` = **1.231** ≦ 1.5 かつ ≦ 2 × 1.205 ○
- DSC05479: 1.053, 1.046, 1.061, 1.058, 1.083, 1.092, 1.072, 1.097, 1.214（最大 1.214）。`size(100)/size(99)` = **1.313** ≦ 1.5 かつ ≦ 2 × 1.214 ○

**Step 3**（spike、4 枚 × 品質 1,5,10,20,23,27,28,40,60,90,100、所要 200.84s。品質 50 未満の `psnr_db` 列は zune-jpeg の誤復号の値なので使わない）

```tsv
file	quality	bytes	psnr_db	luma_max	chroma_max	sampling
20260328-DSC05024.jpg	1	264232	4.18	40	40	2x2/1x1/1x1
20260328-DSC05024.jpg	5	270187	4.18	40	40	2x2/1x1/1x1
20260328-DSC05024.jpg	10	341297	4.18	40	40	2x2/1x1/1x1
20260328-DSC05024.jpg	20	607292	4.18	34	27	2x2/1x1/1x1
20260328-DSC05024.jpg	23	628337	4.18	32	26	2x2/1x1/1x1
20260328-DSC05024.jpg	27	679905	4.18	30	24	2x2/1x1/1x1
20260328-DSC05024.jpg	28	689464	4.18	30	24	2x2/1x1/1x1
20260328-DSC05024.jpg	40	871645	4.18	24	20	2x2/1x1/1x1
20260328-DSC05024.jpg	60	1800412	45.33	18	20	1x1/1x1/1x1
20260328-DSC05024.jpg	90	3635163	47.55	15	20	1x1/1x1/1x1
20260328-DSC05024.jpg	100	10631055	60.09	1	1	1x1/1x1/1x1
20261010-DSC05479.jxl	1	3678560	8.21	40	40	2x2/1x1/1x1
20261010-DSC05479.jxl	5	3814630	8.21	40	40	2x2/1x1/1x1
20261010-DSC05479.jxl	10	5332568	8.22	40	40	2x2/1x1/1x1
20261010-DSC05479.jxl	20	9792880	8.22	34	27	2x2/1x1/1x1
20261010-DSC05479.jxl	23	10118295	8.22	32	26	2x2/1x1/1x1
20261010-DSC05479.jxl	27	10607759	8.22	30	24	2x2/1x1/1x1
20261010-DSC05479.jxl	28	10714385	8.22	30	24	2x2/1x1/1x1
20261010-DSC05479.jxl	40	11959558	8.22	24	20	2x2/1x1/1x1
20261010-DSC05479.jxl	60	15874218	41.00	18	20	1x1/1x1/1x1
20261010-DSC05479.jxl	90	19988939	43.11	15	20	1x1/1x1/1x1
20261010-DSC05479.jxl	100	54763642	50.80	1	1	1x1/1x1/1x1
20261010-result2.jxl	1	961104	9.90	40	40	2x2/1x1/1x1
20261010-result2.jxl	5	1011512	9.90	40	40	2x2/1x1/1x1
20261010-result2.jxl	10	1739954	9.91	40	40	2x2/1x1/1x1
20261010-result2.jxl	20	4367382	9.90	34	27	2x2/1x1/1x1
20261010-result2.jxl	23	4556369	9.90	32	26	2x2/1x1/1x1
20261010-result2.jxl	27	4888769	9.90	30	24	2x2/1x1/1x1
20261010-result2.jxl	28	4962468	9.91	30	24	2x2/1x1/1x1
20261010-result2.jxl	40	5899017	9.90	24	20	2x2/1x1/1x1
20261010-result2.jxl	60	8432886	41.60	18	20	1x1/1x1/1x1
20261010-result2.jxl	90	11735598	43.15	15	20	1x1/1x1/1x1
20261010-result2.jxl	100	40967448	51.46	1	1	1x1/1x1/1x1
20261010-DSC05529.jxl	1	753332	6.41	40	40	2x2/1x1/1x1
20261010-DSC05529.jxl	5	788906	6.41	40	40	2x2/1x1/1x1
20261010-DSC05529.jxl	10	1192285	6.41	40	40	2x2/1x1/1x1
20261010-DSC05529.jxl	20	2806612	6.41	34	27	2x2/1x1/1x1
20261010-DSC05529.jxl	23	2946549	6.41	32	26	2x2/1x1/1x1
20261010-DSC05529.jxl	27	3195781	6.41	30	24	2x2/1x1/1x1
20261010-DSC05529.jxl	28	3245178	6.41	30	24	2x2/1x1/1x1
20261010-DSC05529.jxl	40	4070262	6.41	24	20	2x2/1x1/1x1
20261010-DSC05529.jxl	60	6445074	43.29	18	20	1x1/1x1/1x1
20261010-DSC05529.jxl	90	9694999	45.25	15	20	1x1/1x1/1x1
20261010-DSC05529.jxl	100	38762898	51.15	1	1	1x1/1x1/1x1
```

サンプル写真の表（`CAP = 30`。品質 1〜10 は下の「CAP 30」で測り直した値。バイト数とテーブルの最大要素は spike。PSNR は品質 50 未満が `jpeg-decoder`、50 以上が spike）:

| 品質 | バイト数 | PSNR | 輝度の最大 | 色差の最大 | サンプリング |
|---|---|---|---|---|---|
| 1 | 293488 | 39.54 | 34 | 30 | 4:2:0 |
| 5 | 293488 | 39.54 | 34 | 30 | 4:2:0 |
| 10 | 343495 | 40.81 | 34 | 30 | 4:2:0 |
| 20 | 607292 | 42.64 | 34 | 27 | 4:2:0 |
| 40 | 871645 | 43.65 | 24 | 20 | 4:2:0 |
| 60 | 1800412 | 45.33 | 18 | 20 | 4:4:4 |
| 90 | 3635163 | 47.55 | 15 | 20 | 4:4:4 |
| 100 | 10631055 | 60.09 | 1 | 1 | 4:4:4 |

比較画像
- `docs/tmp/output-quality/after/`: `original.png`・`q<品質>.png` と、それぞれの `-sat.png`
- `docs/tmp/output-quality/scale-compare.png`: 1 行が 1 つの品質。左から 元・差し替え前・差し替え後、その彩度強調版
- 書き出しは Task 1 と同じ CLI（`-m quality --max-size 1024`）。切り抜きと彩度強調は ImageMagick（libjpeg で復号）

見たこと（判定は利用者）:
- 差し替え後はどの品質でもグレーにならない（差し替え前は品質 1・5 でグレー、10 で色相が帯状に飛ぶ）
- 品質 1・5 の彩度強調版には、16×16 画素単位の黄・橙のまだらが出る（4:2:0 の色差の DC の刻みによる）。差し替え前の品質 20 の彩度強調版に近い程度
- 参考に、品質 20 未満の上限 `CAP` を 40・30・24・20 にした品質 1・5 の彩度強調版を `docs/tmp/output-quality/cap-compare.png` に置いた。この比較は品質 20 以上を `k(q)` にしていた時点のものだが、品質 20 未満の式は同じ
  - サンプル写真の品質 1 は、CAP 30 で 293488 B・39.54dB、CAP 24 で 322540 B・40.91dB、CAP 20 で 352891 B・41.37dB（`jpeg-decoder`）
  - CAP 30 がいちばんまだらが少なく見えた
- 利用者の判断で `CAP = 30` にした（下の「CAP 30」）。上の spike の TSV の品質 1・5・10 の行と、この 3 行は `CAP = 40` の時点の値

**Step 6: サイズ制限の下限**（`jpeg-decoder`、品質 15〜40 の全行。全行は `s6b.tsv`、品質 15〜70）

```tsv
-2,1.05,1,1,40,50	20260328-DSC05024	15	445147	42.13
-2,1.05,1,1,40,50	20260328-DSC05024	16	483439	42.21
-2,1.05,1,1,40,50	20260328-DSC05024	17	528805	42.16
-2,1.05,1,1,40,50	20260328-DSC05024	18	557431	42.51
-2,1.05,1,1,40,50	20260328-DSC05024	19	588561	42.56
-2,1.05,1,1,40,50	20260328-DSC05024	20	607292	42.64
-2,1.05,1,1,40,50	20260328-DSC05024	21	607842	42.65
-2,1.05,1,1,40,50	20260328-DSC05024	22	612208	42.66
-2,1.05,1,1,40,50	20260328-DSC05024	23	628337	42.77
-2,1.05,1,1,40,50	20260328-DSC05024	24	662123	42.82
-2,1.05,1,1,40,50	20260328-DSC05024	25	663857	42.82
-2,1.05,1,1,40,50	20260328-DSC05024	26	665474	42.83
-2,1.05,1,1,40,50	20260328-DSC05024	27	679905	43.07
-2,1.05,1,1,40,50	20260328-DSC05024	28	689464	43.07
-2,1.05,1,1,40,50	20260328-DSC05024	29	695228	43.08
-2,1.05,1,1,40,50	20260328-DSC05024	30	696098	43.08
-2,1.05,1,1,40,50	20260328-DSC05024	31	708016	43.10
-2,1.05,1,1,40,50	20260328-DSC05024	32	743865	43.36
-2,1.05,1,1,40,50	20260328-DSC05024	33	780601	43.48
-2,1.05,1,1,40,50	20260328-DSC05024	34	781776	43.48
-2,1.05,1,1,40,50	20260328-DSC05024	35	790393	43.50
-2,1.05,1,1,40,50	20260328-DSC05024	36	791669	43.50
-2,1.05,1,1,40,50	20260328-DSC05024	37	815898	43.51
-2,1.05,1,1,40,50	20260328-DSC05024	38	848745	43.55
-2,1.05,1,1,40,50	20260328-DSC05024	39	864332	43.64
-2,1.05,1,1,40,50	20260328-DSC05024	40	871645	43.65
-2,1.05,1,1,40,50	20261010-DSC05479	15	7187732	34.56
-2,1.05,1,1,40,50	20261010-DSC05479	16	7613320	34.89
-2,1.05,1,1,40,50	20261010-DSC05479	17	8135909	35.17
-2,1.05,1,1,40,50	20261010-DSC05479	18	8781365	35.50
-2,1.05,1,1,40,50	20261010-DSC05479	19	9086205	35.76
-2,1.05,1,1,40,50	20261010-DSC05479	20	9792880	36.06
-2,1.05,1,1,40,50	20261010-DSC05479	21	9849830	36.15
-2,1.05,1,1,40,50	20261010-DSC05479	22	9964181	36.25
-2,1.05,1,1,40,50	20261010-DSC05479	23	10118295	36.41
-2,1.05,1,1,40,50	20261010-DSC05479	24	10274362	36.49
-2,1.05,1,1,40,50	20261010-DSC05479	25	10389676	36.61
-2,1.05,1,1,40,50	20261010-DSC05479	26	10470524	36.70
-2,1.05,1,1,40,50	20261010-DSC05479	27	10607759	36.87
-2,1.05,1,1,40,50	20261010-DSC05479	28	10714385	36.99
-2,1.05,1,1,40,50	20261010-DSC05479	29	10800526	37.09
-2,1.05,1,1,40,50	20261010-DSC05479	30	10841812	37.15
-2,1.05,1,1,40,50	20261010-DSC05479	31	10969558	37.27
-2,1.05,1,1,40,50	20261010-DSC05479	32	11179985	37.43
-2,1.05,1,1,40,50	20261010-DSC05479	33	11311921	37.54
-2,1.05,1,1,40,50	20261010-DSC05479	34	11376477	37.62
-2,1.05,1,1,40,50	20261010-DSC05479	35	11479337	37.74
-2,1.05,1,1,40,50	20261010-DSC05479	36	11521663	37.78
-2,1.05,1,1,40,50	20261010-DSC05479	37	11653355	37.92
-2,1.05,1,1,40,50	20261010-DSC05479	38	11790066	38.02
-2,1.05,1,1,40,50	20261010-DSC05479	39	11902931	38.13
-2,1.05,1,1,40,50	20261010-DSC05479	40	11959558	38.20
-2,1.05,1,1,40,50	20261010-DSC05529	15	1886978	39.29
-2,1.05,1,1,40,50	20261010-DSC05529	16	2070185	39.55
-2,1.05,1,1,40,50	20261010-DSC05529	17	2259552	39.66
-2,1.05,1,1,40,50	20261010-DSC05529	18	2468969	39.85
-2,1.05,1,1,40,50	20261010-DSC05529	19	2587546	39.94
-2,1.05,1,1,40,50	20261010-DSC05529	20	2806612	40.07
-2,1.05,1,1,40,50	20261010-DSC05529	21	2819679	40.09
-2,1.05,1,1,40,50	20261010-DSC05529	22	2870070	40.15
-2,1.05,1,1,40,50	20261010-DSC05529	23	2946549	40.29
-2,1.05,1,1,40,50	20261010-DSC05529	24	3049231	40.35
-2,1.05,1,1,40,50	20261010-DSC05529	25	3095653	40.40
-2,1.05,1,1,40,50	20261010-DSC05529	26	3126454	40.44
-2,1.05,1,1,40,50	20261010-DSC05529	27	3195781	40.58
-2,1.05,1,1,40,50	20261010-DSC05529	28	3245178	40.66
-2,1.05,1,1,40,50	20261010-DSC05529	29	3287986	40.70
-2,1.05,1,1,40,50	20261010-DSC05529	30	3300270	40.72
-2,1.05,1,1,40,50	20261010-DSC05529	31	3382897	40.79
-2,1.05,1,1,40,50	20261010-DSC05529	32	3519852	40.91
-2,1.05,1,1,40,50	20261010-DSC05529	33	3614299	40.98
-2,1.05,1,1,40,50	20261010-DSC05529	34	3651729	41.02
-2,1.05,1,1,40,50	20261010-DSC05529	35	3724389	41.09
-2,1.05,1,1,40,50	20261010-DSC05529	36	3751362	41.12
-2,1.05,1,1,40,50	20261010-DSC05529	37	3848519	41.29
-2,1.05,1,1,40,50	20261010-DSC05529	38	3949306	41.36
-2,1.05,1,1,40,50	20261010-DSC05529	39	4031089	41.45
-2,1.05,1,1,40,50	20261010-DSC05529	40	4070262	41.50
-2,1.05,1,1,40,50	20261010-result2	15	2895363	37.50
-2,1.05,1,1,40,50	20261010-result2	16	3138128	37.66
-2,1.05,1,1,40,50	20261010-result2	17	3396956	37.77
-2,1.05,1,1,40,50	20261010-result2	18	3754067	37.96
-2,1.05,1,1,40,50	20261010-result2	19	3913413	38.08
-2,1.05,1,1,40,50	20261010-result2	20	4367382	38.34
-2,1.05,1,1,40,50	20261010-result2	21	4394565	38.39
-2,1.05,1,1,40,50	20261010-result2	22	4464527	38.45
-2,1.05,1,1,40,50	20261010-result2	23	4556369	38.58
-2,1.05,1,1,40,50	20261010-result2	24	4680788	38.64
-2,1.05,1,1,40,50	20261010-result2	25	4752695	38.72
-2,1.05,1,1,40,50	20261010-result2	26	4798712	38.77
-2,1.05,1,1,40,50	20261010-result2	27	4888769	38.99
-2,1.05,1,1,40,50	20261010-result2	28	4962468	39.07
-2,1.05,1,1,40,50	20261010-result2	29	5025734	39.14
-2,1.05,1,1,40,50	20261010-result2	30	5051131	39.18
-2,1.05,1,1,40,50	20261010-result2	31	5156351	39.27
-2,1.05,1,1,40,50	20261010-result2	32	5310589	39.43
-2,1.05,1,1,40,50	20261010-result2	33	5426523	39.50
-2,1.05,1,1,40,50	20261010-result2	34	5467069	39.56
-2,1.05,1,1,40,50	20261010-result2	35	5555231	39.66
-2,1.05,1,1,40,50	20261010-result2	36	5583345	39.69
-2,1.05,1,1,40,50	20261010-result2	37	5678027	39.83
-2,1.05,1,1,40,50	20261010-result2	38	5774775	39.90
-2,1.05,1,1,40,50	20261010-result2	39	5858934	40.01
-2,1.05,1,1,40,50	20261010-result2	40	5899017	40.07
```

| 写真 | 現行の品質 60 の PSNR | それ以上になる最小の品質 |
|---|---|---|
| DSC05024 | 42.85 | 27（43.07） |
| DSC05479 | 36.92 | 28（36.99。27 は 36.87） |
| DSC05529 | 40.52 | 27（40.58） |
| result2 | 38.52 | 23（38.58） |

**下限は 28**（4 枚の最大）。Task 4 の `MIN_QUALITY` にする。品質 28 は 4:2:0 になる（閾値 50 未満）。

**Step 7: 参考比較**（合否にしない）。新しい品質 85 のテーブル（zigzag 順）:

```text
輝度: 4,3,3,3,3,2,4,3,3,3,4,4,4,4,6,9,6,6,5,5,6,11,8,8,7,9,13,12,13,12,12,11,12,12,12,15,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12
色差: 4,4,4,5,5,5,10,6,6,10,20,13,11,13,20,20,14,14,14,14,20,17,12,12,12,12,12,17,17,12,12,12,12,12,12,17,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12,12
```

LrC 85% との要素ごとの比（新 / LrC。zigzag 順）:

```text
輝度: 2.00,1.50,1.50,1.00,1.50,0.67,1.00,1.50,1.50,0.75,0.80,1.00,1.33,1.00,1.20,1.50,1.20,1.20,1.00,1.00,1.00,1.38,1.14,1.14,1.00,1.29,1.86,1.50,1.18,1.33,1.33,1.22,1.33,1.33,1.33,1.36,1.09,1.09,1.09,1.09,1.09,1.09,1.09,1.00,1.00,1.00,1.00,1.00,1.00,1.00,1.00,1.00,1.00,1.00,1.00,1.00,1.00,1.00,1.00,1.00,1.00,1.00,1.00,1.00
色差: 1.33,1.33,1.33,0.71,1.25,0.71,0.77,0.86,0.86,0.77,1.33,1.00,0.85,1.00,1.33,1.33,1.00,1.00,1.00,1.00,1.33,1.13,1.00,1.00,1.00,1.00,1.00,1.13,1.13,1.00,1.00,1.00,1.00,1.00,1.00,1.13,1.00,1.00,1.00,1.00,1.00,1.00,1.00,1.00,1.00,1.00,1.00,1.00,1.00,1.00,1.00,1.00,1.00,1.00,1.00,1.00,1.00,1.00,1.00,1.00,1.00,1.00,1.00,1.00
```

- 輝度: 最小 0.67、最大 2.00、平均 1.15
- 色差: 最小 0.71、最大 1.33、平均 1.03
- 新しい品質 85 は、輝度の低〜中周波が LrC 85% より粗い（DC 4 と 2）

**CAP 30**（2026-10-20 利用者の判断。`CAP` だけを 40 から 30 に変えた）

- 品質 20〜100 は、すべての品質でテーブルが CAP 40 のときと同じだった（使い捨ての道具で、テーブルを品質ごとに比べた）。違うのは品質 19 以下だけ。
  品質 90 の spike の行も前と同じだった（3635163 B・47.55dB）ので、Step 1・2 の結果は変わらない
- Task 2 の性質テストは無変更で通った（`make check`）
- サンプル写真（spike、所要 4.00s。品質 50 未満の `psnr_db` は zune-jpeg の誤復号の値）:

```tsv
file	quality	bytes	psnr_db	luma_max	chroma_max	sampling
20260328-DSC05024.jpg	1	293488	4.19	34	30	2x2/1x1/1x1
20260328-DSC05024.jpg	5	293488	4.19	34	30	2x2/1x1/1x1
20260328-DSC05024.jpg	10	343495	4.18	34	30	2x2/1x1/1x1
20260328-DSC05024.jpg	20	607292	4.18	34	27	2x2/1x1/1x1
20260328-DSC05024.jpg	90	3635163	47.55	15	20	1x1/1x1/1x1
```

- PSNR（`jpeg-decoder`）:

```tsv
-2,1.05,1,1,30,50	20260328-DSC05024	1	293488	39.54
-2,1.05,1,1,30,50	20260328-DSC05024	5	293488	39.54
-2,1.05,1,1,30,50	20260328-DSC05024	10	343495	40.81
-2,1.05,1,1,30,50	20260328-DSC05024	20	607292	42.64
```

- 品質 1 と 5 は同じテーブルになる（どちらも全要素が `max(B, 30)` で頭打ち）
- `scale-compare.png` と `after/` は CAP 30 で作り直した
- Step 6: 品質 20〜40 の行は CAP 40 のときと 4 枚とも同じだった（`s6c.tsv` と `s6b.tsv` を比べた）。
  品質 15〜19 では、どの写真も現行の品質 60 の PSNR に届かない（下の行）。よって**下限は 28 のまま**

```tsv
-2,1.05,1,1,30,50	20260328-DSC05024	15	445459	42.13
-2,1.05,1,1,30,50	20260328-DSC05024	16	483729	42.21
-2,1.05,1,1,30,50	20260328-DSC05024	17	529125	42.17
-2,1.05,1,1,30,50	20260328-DSC05024	18	557474	42.51
-2,1.05,1,1,30,50	20260328-DSC05024	19	588561	42.56
-2,1.05,1,1,30,50	20261010-DSC05479	15	7327903	34.81
-2,1.05,1,1,30,50	20261010-DSC05479	16	7701990	35.05
-2,1.05,1,1,30,50	20261010-DSC05479	17	8176780	35.25
-2,1.05,1,1,30,50	20261010-DSC05479	18	8789675	35.52
-2,1.05,1,1,30,50	20261010-DSC05479	19	9091091	35.77
-2,1.05,1,1,30,50	20261010-DSC05529	15	1902026	39.33
-2,1.05,1,1,30,50	20261010-DSC05529	16	2080213	39.58
-2,1.05,1,1,30,50	20261010-DSC05529	17	2264612	39.67
-2,1.05,1,1,30,50	20261010-DSC05529	18	2469714	39.85
-2,1.05,1,1,30,50	20261010-DSC05529	19	2587939	39.94
-2,1.05,1,1,30,50	20261010-result2	15	2945542	37.58
-2,1.05,1,1,30,50	20261010-result2	16	3168764	37.72
-2,1.05,1,1,30,50	20261010-result2	17	3411087	37.80
-2,1.05,1,1,30,50	20261010-result2	18	3755600	37.96
-2,1.05,1,1,30,50	20261010-result2	19	3914444	38.09
```

**zune-jpeg の件（利用者の判断）**: 既知の制限として受け入れ、0.5.16 の安定版が出たら上げる。この Task では依存を変えない。README への記載は Task 10 で行う。
spike は直していない。品質 50 未満の PSNR は上のとおり `jpeg-decoder` で測った。
下限 28 は 4:2:0 なので、サイズ制限で下限まで下がった出力は `image` 0.25（zune-jpeg 0.5.15）で読むと緑になる。

### Task 4: サイズ制限の探索

- `search_quality`（初期 → `[floor, initial-1]` の二分探索）に替え、`MIN_QUALITY = 28`。旧 `QUALITY_STEP` は削除。
- spec 7 の最悪回数 `1 + ceil(log2(初期 − 下限))` は、差が 2 の冪のとき 1 回足りない（結果が「差」通り + 「収まらない」で `差 + 1` 通り）。実装とコメントは `1 + ceil(log2(初期 − 下限 + 1))`。初期 90・下限 26（差 64）で 8 回以内になることをテストで固定した。
- `save_with_size_limit_actually_reduces_quality` の `Some(60)` は `MIN_QUALITY` に置き換えた。

### Task 5: 透過の合成

### Task 6: ICC（許容差の実測・`reserve` を外したとき）

- 依存: `moxcms = "=0.8.1"`（`image` 0.25.10 と同じ版）。`srgb_icc()`（`new_srgb().encode()`）は 612 バイト（Task 3 の実測と同じ）。
- 変換は `decode::open_with_warnings` の中で行うので、`process_image` の `flatten_alpha` より前（アルファを持ったまま変換してから合成）。
- **`adobe16.png` はアルファ 128**（brief の「アルファ 255」から変更。255 だと変換と合成の順序が結果に出ない）。
  正解値は ImageMagick で変換 →（`-clamp`）→ 白で `-flatten`: `[243,156,156] / [127,208,170] / [167,182,229] / [240,213,186]`。
  `-clamp` を挟まないと Q16-HDRI の ImageMagick は緑のパッチの R（-109.834%）を切り詰めずに合成し、R が 0 になる（`[0,208,170]`）。
  整数で画素を持つ本ツールは変換で 0 に切り詰めるので、合成後は 127 が正しい。誤った順序（合成してから変換）の ImageMagick の値は
  `[249,159,159] / [132,209,172] / [169,184,230] / [240,213,188]`。
- **順序を入れ替えて確かめた**: decode での変換を外し、`flatten_alpha` の後で `to_srgb` を呼ぶと
  `sixteen_bit_png_is_converted_before_flattening` が落ちる（パッチ 0 が `[248,158,158]`、パッチ 1 が `[132,208,172]`。期待 ±3 の外）。
- 許容差に対する実測の最大誤差（許容差を 0 にして測った。この機械の SIMD 経路）:

  | テスト | 許容差 | 実測の最大誤差 |
  |---|---|---|
  | adobe.png | ±3 | 1（`[231,57,56]` / `[79,110,203]`） |
  | adobe.jpg / adobe.webp | ±6 | 1 |
  | adobe16.png | ±3 | 1（`[242,156,155]` / `[127,207,170]` / `[168,182,229]`） |
  | prophoto.png 4 色目 | ±6 | 1（`[254,176,128]`） |
  | broken_icc.png・Gray の ICC 付き RGB（割り当て前の値） | ±6 | 1 |

- **`srgb_input_is_not_converted` は `is_srgb` を壊しても落ちない**: `is_srgb` を常に偽にしても通った。
  LrC の sRGB → moxcms の sRGB の変換は、R・G・B 各 0〜255 を網羅した 256×256 のグラデーション（196,608 サンプル）でも
  差が 0 だった（一時的なテストで実測）。画素では観測できないので、無駄な変換をしないことは `is_srgb` の単体テスト
  （`lightroom_srgb_profile_is_recognized_as_srgb`）で固定する。統合テストは「sRGB の入力の画素が変わらない」契約として残す。
- **`reserve` を外したとき**: `png_whose_output_exceeds_the_memory_limit_is_rejected` が落ちた（`Ok` になる）。
  `over_limit.png`（12000×12000）が 36 秒かけて変換まで通り、最大 RSS は 2.1GB。戻すと `Err` で即座に返る。
  `reserve` を入れる前（現行の `decode()`）でもこのテストは通ることを Step 3 で確かめた。
- `over_limit.png` は 559,177 バイト（Pillow 12.3 の `optimize=True`）。
- `decode::open`（`open_image_oriented` 経由のサムネイル・プレビュー）にも ICC 変換がかかるようになった（spec「5」が意図した変化。警告は捨てる）。
- moxcms の解析と変換の作成は `catch_unwind` で包み、panic を警告にして画素を変換せずに返す。行の変換の途中の panic は包まず、
  `Format::Other` の経路全体を `guarded` で包んで 1 枚の `Err` にする。panic する ICC は見つかっていないのでテストは無い。
- ProPhoto の色域外 3 色は大小関係だけを見る。パッチ 1・2 は変換前の値でも大小関係が正解と同じで、
  この検査で差が出るのはパッチ 0（変換前 `[200,60,59]`）と 4 色目の値の比較。

### Task 7: Exif（実写真で残ったタグ）

- 書き出しは kamadak-exif 0.6.1 の `experimental::Writer`。エンディアンは元の Exif に合わせる。IFD は本体（`In::PRIMARY`）だけを書く。
- **IFD へのポインタ（`ExifIFDPointer`・`GPSInfoIFDPointer`・`InteropIFDPointer`）は kamadak-exif の `Reader` がフィールドとして返さない**
  （`tiff.rs` の `parse_ifd` が子 IFD を辿るだけで、エントリには積まない）。`Writer` も中身から作り直す。
  そのため GPS は `Context::Gps` で消せば足り、構造タグの定数にある 2 つのポインタは実際には現れない（spec の列挙に合わせて置いてある）。
  単体テストの前提もポインタではなく「Exif / Interop の Context のフィールドがある」で書いた。
- **本計画の判断**: TIFF のタグに入った XMP（0x02BC）・IPTC-NAA（0x83BB）・Photoshop の画像リソース（0x8649。IPTC を含む）も消す。
  spec「XMP と IPTC は写さない」が Exif の中に入った XMP / IPTC にも及ぶと読んだ（LrC の XMP は位置情報を持ちうる）。
  オフセット系は spec の 3 つに `StripOffsets`/`StripByteCounts`/`TileOffsets`/`TileByteCounts`/`FreeOffsets`/`FreeByteCounts`/
  `JPEGInterchangeFormat`/`JPEGInterchangeFormatLength` を足し、本体 IFD・Exif IFD のどちらでも番号で照合する。
- `PixelX/YDimension` は元が SHORT で値が収まれば SHORT のまま、そうでなければ LONG。
- **JPEG XL**: `decode::jxl_exif`（`Result<Option<Vec<u8>>>`）を足し、`output_exif::load` はこれを使う。`jxl_raw_exif` は
  `jxl_exif(..).ok().flatten()` の包みとして残した（`read_exif_info` が使う）。`broken_brob.jxl` は「failed to read」の警告 1 件になる
  （`broken_jxl_exif_is_dropped_with_a_warning`。`load` を `jxl_raw_exif` に戻すと落ちることを確かめた）。
  **`exif_bomb.jxl`（展開後 8MiB）は `jxl::raw_exif` が上限超えを `Ok(None)` にするので、警告なしで Exif なしになる**（未対応）。
- `MAX_EXIF_TIFF_LEN = 65_527` は `jpeg-encoder` の実際の判定と一致することを単体テスト（`max_exif_tiff_len_is_the_encoder_limit`。
  65,527 は `Ok`、65,528 は `Err`）で固定した。
- **RED**: `output_quality.rs` の Exif のテスト 11 件のうち、実装前に 10 件が落ちた（前提の assert はすべて通った上で、出力に Exif が無い／警告が 0 件）。
  `input_without_exif_writes_no_exif_and_no_warning` は実装前から通る（現行も Exif を書かないため。回帰の固定として残す）。
- **実装を壊して落ちることを確かめた**（1 つずつ戻して確認）:

  | 壊したところ | 落ちたテスト |
  |---|---|
  | GPS を消さない | `location_and_identifying_tags_are_removed` |
  | 個体識別 5 タグを消さない | `location_and_identifying_tags_are_removed` |
  | IFD1 を消さない | `thumbnail_and_makernote_are_removed` |
  | `MakerNote` を消さない | `thumbnail_and_makernote_are_removed` |
  | `Value::Unknown` を落とさない | `unknown_values_are_dropped_and_the_rest_is_kept` |
  | `Orientation` を 1 にしない | `orientation_is_reset_to_1`・`jxl_and_avif_exif_is_carried_over` |
  | `PixelXDimension` を書き換えない | `pixel_dimensions_are_the_output_size` |
  | `MAX_EXIF_TIFF_LEN` の判定を外す | `oversized_exif_is_dropped_with_a_warning`（エンコーダの `Err` で 1 枚が失敗する） |
  | JPEG XL を `jxl_raw_exif` で読む | `broken_jxl_exif_is_dropped_with_a_warning` |
  | 「書かない場合」の判定を外す | `only_structural_tags_left_means_no_exif`・`removed_tags_do_not_count_as_something_to_write` |
  | Interop の中身を構造に数えない | 同上 |
  | オフセット系を消さない | `offset_tags_are_removed` |
  | XMP / IPTC を消さない | `xmp_and_iptc_in_tiff_tags_are_removed` |
  | `ColorSpace` を 1 にしない | `color_space_becomes_srgb` |
  | エンディアンを固定する | `byte_order_follows_the_source` |

- kamadak-exif の読み取りは `catch_unwind` で包んでいない。同じ入力を既存の `read_exif_info` が包まずに読んでいるので、新しい危険は増えていない。
- **実写真**（crop モード・品質 90 の CLI。出力を kamadak-exif 0.6.1 の `read_from_container` で読み直し、どちらも `Ok`。exiv2 0.28 でも stderr 0 行）:
  - `DSC00097.JPG`（撮って出し。ILCE-7CM2）: 入力 63 フィールド → 出力 52 フィールド。消えたのは `MakerNote` と IFD1 の 10 個
    （`Compression`・`ImageDescription`・`JPEGInterchangeFormat`・`JPEGInterchangeFormatLength`・`Orientation`・`ResolutionUnit`・`Software`・
    `XResolution`・`YCbCrPositioning`・`YResolution`）。GPS とシリアル番号は入力に無かった。`PixelXDimension` は 3504 → 1869、`PixelYDimension` は 2336（出力 1869×2336）。
    残ったタグ: Tiff/Make・Model・Orientation（1）・XResolution・YResolution・ResolutionUnit・Software・DateTime・YCbCrPositioning・PrintIM（0xC4A5）、
    Exif/ExposureTime・FNumber・ExposureProgram・PhotographicSensitivity・SensitivityType・RecommendedExposureIndex・ExifVersion・DateTimeOriginal・
    DateTimeDigitized・OffsetTime・OffsetTimeOriginal・OffsetTimeDigitized・ComponentsConfiguration・CompressedBitsPerPixel・BrightnessValue・
    ExposureBiasValue・MaxApertureValue・MeteringMode・LightSource・Flash・FocalLength・SubSecTime・SubSecTimeOriginal・SubSecTimeDigitized・
    FlashpixVersion・ColorSpace・PixelXDimension・PixelYDimension・CustomRendered・ExposureMode・WhiteBalance・DigitalZoomRatio・
    FocalLengthIn35mmFilm・SceneCaptureType・Contrast・Saturation・Sharpness・LensSpecification・LensModel・CompositeImage、
    Interop/InteroperabilityIndex・InteroperabilityVersion。
  - `20260328-DSC05024.jpg`（LrC の書き出し）: 入力 55 フィールド → 出力 48 フィールド。消えたのは `Artist` と IFD1 の 6 個
    （`Compression`・`JPEGInterchangeFormat`・`JPEGInterchangeFormatLength`・`ResolutionUnit`・`XResolution`・`YResolution`）。
    入力に `Orientation`・`PixelX/YDimension`・GPS は無く、出力にも足されていない。`Copyright` は残る。
    残ったタグ: Tiff/Make・Model・XResolution・YResolution・ResolutionUnit・Software・DateTime・Copyright、
    Exif/ExposureTime・FNumber・ExposureProgram・PhotographicSensitivity・SensitivityType・RecommendedExposureIndex・ExifVersion・DateTimeOriginal・
    DateTimeDigitized・OffsetTime・OffsetTimeOriginal・OffsetTimeDigitized・ShutterSpeedValue・ApertureValue・BrightnessValue・ExposureBiasValue・
    MaxApertureValue・MeteringMode・LightSource・Flash・FocalLength・SubSecTimeOriginal・SubSecTimeDigitized・ColorSpace・FocalPlaneXResolution・
    FocalPlaneYResolution・FocalPlaneResolutionUnit・FileSource・SceneType・CustomRendered・ExposureMode・WhiteBalance・DigitalZoomRatio・
    FocalLengthIn35mmFilm・SceneCaptureType・Contrast・Saturation・Sharpness・LensSpecification・LensModel。
  - 型は保たれる（exiv2 で `ISOSpeedRatings` は Short のまま、`PixelXDimension` は入力どおり Long）。
- **Orientation 6 の実写真は手元に無い**ので、`DSC00097.JPG` の写しに exiv2 で `Exif.Image.Orientation 6` を書いて作った。
  quality モードの出力は 2336×3504・Orientation 1。Exif の向きに従う表示（`magick -auto-orient`）でも 2336×3504 のままで、
  縮小して目で見ると 90° 時計回りに 1 回だけ回った絵（空が右）だった。二重には回らない。

### Task 8: ロゴ（直す前に戻したとき・利用者の了承）

### Task 9: 文字の合成（直す前に戻したとき）

### Task 10: 完了条件
