# AVIF デコーダの差し替え 実装計画

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 壊れた・細工された AVIF でプロセスが abort しないようにし（1 枚の `Err` にする）、正常な AVIF の出力は差し替え前と画素単位で一致させる。

**Architecture:** avif-parse で箱を読むところは残し、取り出した AV1 データを `dav1d` クレート（C 版 libdav1d）で 1 枚ずつデコードする（新規 `core/src/decode/av1.rs`）。avif-decode が持っていた YUV→RGB 変換は `core/src/decode/yuv.rs` に移植し、`yuv` クレートをスカラー経路に固定して呼ぶ。メモリの上限はデコード前（宣言寸法で 512MB 判定＋dav1d の `frame_size_limit`）と変換前（実画像で 512MB 判定）の 2 段にする。

**Tech Stack:** Rust（`picture-tool-core`）、dav1d 0.11.1 / dav1d-sys 0.8.3（libdav1d。Linux はシステムの 1.3 以上、Windows と CI は tag `1.5.0` の内部ビルド）、yuv =0.8.19（`default-features = false`）、avif-parse 2.1.0、bytemuck 1、ffmpeg 8.1.3（libaom-av1）＋ Pillow 12.3.0（フィクスチャ生成）

**Spec:** [`docs/superpowers/specs/2026-10-14-avif-decoder-replacement-design.md`](../specs/2026-10-14-avif-decoder-replacement-design.md)
（前提: JPEG XL / AVIF 入力対応には spec がない。設計意図は `core/src/decode/mod.rs` 冒頭のモジュールコメントとコミット `f1c88fd`〜`f3f8d26` の本文）

## Global Constraints

- **検証は `make check`。** 各タスクの終わりで exit 0 にする。lint は `cargo clippy --workspace --all-targets -- -D warnings`
  （**未使用コードも失敗になる**。削除と置き換えは同じタスクで行う）
- **avif-decode と rav1d を dav1d クレートと同じビルドに同居させない。** rav1d は `dav1d_open` などを同じ名前の C ABI で
  公開しており、リンカーが黙って rav1d 側を使う（spec「依存関係」）。Task 4 で同じコミットの中で入れ替える
- **差し替え前の出力（`docs/tmp/avif-baseline/`）は Task 5 の照合が済むまで消さない。** avif-decode を外した後は作り直せない
- **`yuv = { version = "=0.8.19", default-features = false }`。** SIMD 経路は CPU で出力が変わる（spec「YUV→RGB 変換」）。
  既定 feature にしてよいのは Task 5 の 1 回だけの照合の間だけで、同じタスク内で戻す
- **`dav1d::Settings` に設定するのは `n_threads = 1`・`max_frame_delay = 1`・`apply_grain = true`・`frame_size_limit` の 4 つだけ。**
  他（`all_layers` など）は既定のまま（avif-decode が rav1d に設定していたのもこの 3 つ＋既定値。画素一致のため）
- **`yuv.rs` が `dav1d::Picture` から読んでよいのは、寸法・`pixel_layout`・`bits_per_component`・`matrix_coefficients`・
  `color_range`・`stride`・`plane` だけ。** `color_primaries()`・`transfer_characteristic()`・`chroma_location()` は予約値で
  `unreachable!()` に落ちる
- **`av1.rs`・`yuv.rs` は `pub(super)` で、`decode` モジュールの外へ公開しない**
- **`core` は `eprintln!` しない**（CLAUDE.md）。libdav1d のログが stderr に出るのは既知の制限として許容する（spec「stderr」）
- **移植したファイル（`yuv.rs`）の冒頭に avif-decode の原著作権表示と BSD-3-Clause のライセンス文を残す**
  （`~/.cargo/registry/src/*/avif-decode-3.0.0/LICENSE` から逐語で写す）
- **ガードのテストは `Err` の理由まで文言で照合する**（spec「テスト」）。文言は下の「エラーの文言」の表のとおりにする
- **否定形の検査には前提条件の assert を添える**（memory: negative-assertion-tests-need-a-precondition）。
  ガードのテストでは「ガードを外すと理由が変わる」ことを実測して実施メモに残す
- **テストを書く・直す前に `test-integrity` スキルを起動する**（グローバル指示）
- **この環境の `cargo test` は出力が要約され `--nocapture` の println が見えない。** 出力が要るときは
  テストバイナリを直接実行してスクラッチパッドのファイルへリダイレクトする（memory: cargo-test-output-is-summarized）
- **長いコマンド（フィクスチャ生成・ファズ）はフォアグラウンドで走らせる**（memory: subagents-must-run-long-commands-in-foreground）
- **コミットは `commit-message` スキルに従う。** `<対応の概要>` は直近 commit から逐語継承して **`AVIF デコーダの差し替え`**。
  footer は `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`
- **push・PR 作成は利用者の確認を取ってから**（CI の確認に push が要る。Task 8）

### エラーの文言

core のエラーは `anyhow`。テストは `format!("{err:#}")` に次の文字列が含まれることで理由を照合する。

| 判定 | 文言（`anyhow!` / `context` に含める） | 置き場所 |
|---|---|---|
| デコード前の 512MB 判定 | `AVIF declares a size over the 512MB output limit` | `mod.rs` `frame_size_limits` |
| 変換前の 512MB 判定 | `decoded AVIF exceeds the 512MB output limit` | `yuv.rs` `to_image` の入口 |
| シーケンスヘッダの数 | `AVIF item must have exactly one sequence header (found {n})` | `avif.rs` `declared` |
| フレームが出ない | `no frame decoded` | `av1.rs` |
| 周回の打ち切り | `AV1 decoder made no progress` | `av1.rs` |
| アルファの寸法 | `invalid alpha size` | `yuv.rs` |
| アルファの行列 | `alpha image has color info` | `yuv.rs` |
| 非対応の行列 | `unsupported matrix coefficients` | `yuv.rs` |
| Identity と間引き | `identity matrix with chroma subsampling` | `yuv.rs` |
| dav1d のエラー | `AV1 decoding failed` を `context` に付ける（中身は `dav1d::Error` の Display） | `av1.rs` |

## Review Focus

spec が含意するが、spec の「テスト」表の行だけでは押さえきれず、利用者が最初に踏みうるもの（起きやすい順）。
各行のテストは右端のタスクに入れてある。

1. **プログレッシブ（レイヤー）AVIF** — 1 つのアイテムから複数枚のピクチャが出る。spec の「最初に出てきた 1 枚」では
   低画質の最初のレイヤーを返してしまう（A-1）。期待: 差し替え前と同じ最後の 1 枚
   → Task 5 `two_frames.avif` の画素一致（2 フレームの AV1 を 1 アイテムに入れた代用品）
2. **プリマルチプライドのアルファ**（`prem` 参照）— spec の画素一致の対象に入っていない。期待: 差し替え前と同じく解除済みの値
   → Task 5 `a8_premul.avif` の画素一致
3. **グレー＋アルファ** — 出力が `Rgba8` になる経路。spec の対象に入っていない → Task 5 `a8_on_gray8.avif` の画素一致
4. **データを消費もせずフレームも出さないまま止まるデコーダ** — 期待: 有限回で `Err`
   → Task 4 `av1::tests::a_stalled_decoder_is_cut_off`（偽のデコーダで周回数を数える）
5. **宣言寸法の積が `u32` に収まらない（65536×65536）** — `frame_size_limit` の 0 は「無制限」。期待: 0 を渡さず 512MB 判定で `Err`
   → Task 3 `frame_size_limits_rejects_before_the_limit_would_wrap`

---

## spec を実装者の目で読んで決めたこと

設計の再検討はしていない。spec の記述と実装の間で決める必要があったこと、spec が誤っていたことだけを記す。

| # | spec の記述 | 計画での扱い |
|---|---|---|
| A-1 | `av1.rs`: 「**最初に出てきた 1 枚**を返す」 | **最後の 1 枚を返す（送り切った後、`get_picture` が `Again` を返すまで取り続ける）。** avif-decode 3.0.0 `src/rav1d.rs` の `decode_av1_data` は「レイヤー付きのアイテムは 1 レイヤーごとに 1 枚出て、最後の 1 枚が完全な画像。フィルムグレインも最後のレイヤーにだけ掛かる」として最後を取っている。dav1d の `all_layers` は既定で真（全レイヤーを出す）。「最初」のままだとプログレッシブ AVIF が差し替え前と一致しない。spike の 1300 件は単一フレームなのでこの差が出なかった |
| A-2 | 周回の上限「AV1 データのバイト数 + 16」 | **`2 × バイト数 + 16`。** A-1 で取り続けるため、「フレームが出る」周回で終わらなくなる。正常に進む周回は「1 バイト以上消費」か「1 枚出る」で、どちらもバイト数以下の回数しか起きない。有限で必ず終わる性質は変わらない |
| A-3 | 寸法上限のテスト「13000×13000 の AVIF が 512MB 判定で `Err`」 | **13000×13000 の 8bit RGB は 507,000,000 バイトで、上限 536,870,912（`512 * 1024 * 1024`、image 0.25.10 `Limits::default`）を下回り、通る。** spec 自身が「big169.avif のピーク RSS は方式 b で 808MB」と開けた結果を書いている。フィクスチャは **14000×13000**（546,000,000 バイト。`over_limit.jxl` と同じ寸法）にする |
| A-4 | `avif.rs` に「シーケンスヘッダの数とその宣言寸法・ビット深度を返す関数」 | **数だけを自前で数え、寸法とビット深度は avif-parse の `AV1Metadata::parse_av1_bitstream` から取る**（spec が「実装で決めてよい」とした点）。数が 1 であることを先に確かめるので、avif-parse が読む「最初のヘッダ」は唯一のヘッダと一致する。自前の `sequence_header_bound`・`Bits` とそのテストは消える。avif-parse は `timing_info` 付きのヘッダを `Unsupported` で拒否するが、現行の `reserve_avif` も同じ関数を呼んでいるので挙動は変わらない |
| A-5 | 行列係数を「値の意味で対応づける」 | dav1d クレートは値 5（BT.470BG）と 6（BT.601）を同じ `BT470BG` で返すので区別できない。avif-decode は「主画像 5・アルファ 6」を `alpha image has color info` で拒否していたが、本実装では通る（主画像の変換はどちらも BT.601 で同じ）。仕様上の意味も同じ行列なので受け入れる。README には書かない |
| A-6 | アルファのデコード | **主画像とは別の `Decoder` を作る**（spec のシグネチャ `fn decode(av1, frame_size_limit)` どおり）。avif-decode は 1 つのデコーダを使い回していた。アルファは独立したキーフレームなので結果は変わらない見込みで、Task 5 の照合がこれも確かめる |
| A-7 | 画素一致の期待値「画素のハッシュ（形式と寸法を含む）」 | **FNV-1a 64bit をテストファイル内に書く**（依存を増やさない）。入力は `色型の名前の UTF-8`・`幅の LE 4 バイト`・`高さの LE 4 バイト`・`サンプル列`（16bit は各サンプルを LE 2 バイト）。`DefaultHasher` は版で変わりうるので使わない |
| A-8 | 「abort の回帰」「ファズ」で 1 件ずつ開く手段 | **`core/examples/avif_probe.rs` を足す。** 1 ファイルを `open_image_oriented` で開き、`Ok WxH <色型>` か `Err <理由>` を表示する。`--dump <出力>` で生の画素をファイルに書く（Task 2 の差し替え前の保存と Task 5 の照合に使う）。CLI はフォルダー単位で出力 JPEG を書くので、1 枚の結果と生の画素を取れない |
| A-9 | 新しいフィクスチャの置き場所 | `core/tests/fixtures/avif/` にまとめ、生成スクリプトは `core/tests/fixtures/make_avif_decoder.py`。既存の `make_jxl_avif.py` と既存フィクスチャは触らない |
| A-10 | spec の完了条件「`--test jxl_avif_input` が通る（画素一致のテストを含む）」 | 新しいテストはすべて `core/tests/jxl_avif_input.rs` に足す（Windows での確認がこの 1 コマンドで済むように） |

---

## ファイル構成

### 新規

| ファイル | 責務 |
|---|---|
| `core/src/decode/av1.rs` | AV1 のバイト列 → `dav1d::Picture`（1 アイテム 1 枚）。色変換を知らない |
| `core/src/decode/yuv.rs` | `dav1d::Picture`（主画像と、あればアルファ）→ `DynamicImage`。ファイルも dav1d の設定も知らない。avif-decode からの移植 |
| `core/examples/avif_probe.rs` | 1 ファイルを開いて結果を表示する／生の画素を書く（A-8） |
| `core/tests/fixtures/make_avif_decoder.py` | `core/tests/fixtures/avif/` のフィクスチャを作る |
| `core/tests/fixtures/fuzz_avif.py` | ファズ（CI には入れない。Task 8） |
| `core/tests/fixtures/avif/*.avif` | 下の「フィクスチャ」の表 |

### 変更

| ファイル | 変更 |
|---|---|
| `core/src/decode/mod.rs` | `Format::Avif` の経路を `av1` / `yuv` に繋ぎ替え。`reserve_avif` → `frame_size_limits`、`avif_to_dynamic` を削除。モジュールコメント（メモリの上限）と `guarded` のコメント。`jxl_decoder` の `saturating_mul` |
| `core/src/decode/avif.rs` | `max_frame_pixels`・`sequence_header_bound`・`Bits` とそのテストを削除し、`declared` と `sequence_header_count` を置く |
| `core/src/decode/jxl.rs` | `brob` の本体長の確認（低優先度の修正） |
| `core/src/lib.rs` | 冒頭の「`eprintln!` しない」に stderr の例外を書く |
| `core/Cargo.toml` | `avif-decode` を外し、`dav1d`・`yuv`・`bytemuck` を足す。コメントの書き換え |
| `core/tests/jxl_avif_input.rs` | 回帰・ガード・画素一致のテスト。冒頭の表に新しいフィクスチャを足す |
| `.github/workflows/ci.yml` | `meson`・`ninja-build` の追加、`SYSTEM_DEPS_DAV1D_BUILD_INTERNAL: always`、nasm のコメント |
| `README.md` | 前提条件（OS ごとの libdav1d・Windows の手順・Rust の版）と既知の制限 |
| `gui/src/security.rs:188`、`gui-frontend/src/App.svelte:140-142`、`docs/superpowers/specs/2026-09-08-grid-filtering-design.md:284` | 古い記述（spec「文書」） |
| `docs/README.md` | 索引に本 plan を足す（spec は登録済み） |

### フィクスチャ（`core/tests/fixtures/avif/`）

元画像は特記なしで **33×17 の非単色の図柄**（奇数寸法。全チャンネルが多くの値を取るもの。例: ffmpeg の `testsrc2`）。
アルファの元画像は横方向のグラデーション（0〜最大値を含む）。エンコードは ffmpeg 8.1.3 の libaom-av1。
**行列・範囲・深度はシーケンスヘッダに書かれた値で決まる**ので、ffmpeg の `-colorspace` / `-color_range` / `-pix_fmt` で指定し、
生成後に `ffprobe` かスクリプト内の解析で書かれた値を assert する（ffmpeg が黙って別の値を書くことがあるため）。

| ファイル | 中身 | 用途 |
|---|---|---|
| `base_64x32.avif` | `make_jxl_avif.py` の `quad`（64×32、4 色）を Pillow で Exif なしで保存 | `crash*.avif` の元 |
| `crash.avif` | `base_64x32.avif` の frame OBU ペイロード先頭 2 バイトを `0xFF` にしたもの（オフセットはスクリプトが mdat を解析して求め、表示する。spike では 299・300） | abort の回帰（必須） |
| `crash_seqhdr.avif` / `crash_framehdr.avif` / `crash_tile.avif` | 破壊位置がシーケンスヘッダ・フレームヘッダ・タイルデータのもの。**差し替え前に abort するものだけ残す**（Task 2）。どれも abort しなければ作らない | abort の回帰（任意） |
| `over_limit.avif` | 14000×13000 の単色 8bit 4:2:0（A-3） | 512MB 判定 |
| `alpha_huge.avif` | 主画像 64×64 の単色 8bit、アルファは **23200×23200 で実際にエンコードした**単色 8bit モノクロ（宣言だけ書き換えない） | アルファの宣言寸法 |
| `two_seqhdr.avif` | `p8_420_lim_unspec.avif` の主画像の AV1 データで、シーケンスヘッダ OBU の直後にそのバイト列をそのまま複製したもの | シーケンスヘッダ複数 |
| 画素一致の 20 枚 | 下の表 | 画素一致 |

画素一致の 20 枚（名前は `<主画像>[_<アルファ>]`。`lim`=限定レンジ、`full`=全レンジ）:

| ファイル | 深度 | 間引き | 範囲 | 行列 | アルファ |
|---|---|---|---|---|---|
| `p8_420_lim_unspec.avif` | 8 | 4:2:0 | lim | 2（未指定） | — |
| `p8_422_full_709.avif` | 8 | 4:2:2 | full | 1 | — |
| `p8_444_lim_601.avif` | 8 | 4:4:4 | lim | 6 | — |
| `p8_444_full_fcc.avif` | 8 | 4:4:4 | full | 4 | — |
| `p8_444_lim_ycgco.avif` | 8 | 4:4:4 | lim | 8 | — |
| `p8_444_full_identity.avif` | 8 | 4:4:4 | full | 0（`-pix_fmt gbrp`） | — |
| `p10_420_lim_709.avif` | 10 | 4:2:0 | lim | 1 | — |
| `p10_444_full_unspec.avif` | 10 | 4:4:4 | full | 2 | — |
| `p10_444_full_identity.avif` | 10 | 4:4:4 | full | 0（`gbrp10`） | — |
| `p12_422_lim_601.avif` | 12 | 4:2:2 | lim | 6 | — |
| `gray8_full.avif` | 8 | 4:0:0 | full | 2 | — |
| `gray10_lim.avif` | 10 | 4:0:0 | lim | 2 | — |
| `p8_420_lim_709_a8.avif` | 8 | 4:2:0 | lim | 1 | 8bit モノクロ full |
| `p10_420_lim_709_a10.avif` | 10 | 4:2:0 | lim | 1 | 10bit モノクロ full |
| `p10_420_lim_709_a8.avif` | 10 | 4:2:0 | lim | 1 | 8bit モノクロ full（主 10＋α 8） |
| `p8_420_lim_709_a10.avif` | 8 | 4:2:0 | lim | 1 | 10bit モノクロ lim（主 8＋α 10。限定レンジの `luma16` も通す） |
| `p8_420_lim_709_a420.avif` | 8 | 4:2:0 | lim | 1 | 8bit **4:2:0**（クロマを持つアルファ）、行列 2（主 709＋α 未指定も兼ねる） |
| `a8_on_gray8.avif` | 8 | 4:0:0 | full | 2 | 8bit モノクロ full（Review Focus 3） |
| `a8_premul.avif` | 8 | 4:2:0 | lim | 1 | 8bit モノクロ full、`iref` に `prem`（Review Focus 2） |
| `two_frames.avif` | 8 | 4:2:0 | lim | 1 | — 。**2 フレーム**（1 枚目と 2 枚目で図柄が違う）の AV1 を 1 アイテムに入れたもの（Review Focus 1）。シーケンスヘッダは 1 個 |

アルファ付き・`prem`・2 フレームのものは、主画像とアルファを別々に `-f obu` でエンコードした AV1 データを
スクリプトが AVIF に組む（`ftyp`・`meta`（`hdlr`/`pitm`/`iloc`/`iinf`/`iref`（`auxl`・`prem`）/`iprp`（`ispe`・`av1C`・`pixi`・
`auxC` = `urn:mpeg:mpegB:cicp:systems:auxiliary:alpha`）/`ipma`）・`mdat`）。`make_jxl_avif.py` の `box` と同じ形でよい。
`av1C` の 4 バイトはエンコードしたシーケンスヘッダの値から組む。
**どのフィクスチャも、差し替え前の core で開けることを Task 2 で確かめる**（開けないものは作り方が誤っている）。

---

## Task 1: Windows で libdav1d の内部ビルドが成り立つかを確かめる（利用者が手元で行う）

spec「ビルドと CI」の最初の工程。**Task 4 はこの結果を待つ**（Task 2・3 は先に進めてよい。どちらも dav1d に依存しない）。
通らなければ、その時点で代替（vcpkg の静的 triplet 等）を利用者と判断し、Task 4・6 を見直す。

**Files:** なし（リポジトリの外に作る）

- [ ] **Step 1: 最小のクレートを利用者に渡す**

リポジトリの外（例 `C:\work\dav1d-probe`）に作る:

```toml
# Cargo.toml
[package]
name = "dav1d-probe"
version = "0.1.0"
edition = "2021"

[dependencies]
dav1d = "=0.11.1"
avif-parse = "2"
```

```rust
// src/main.rs。リポジトリの core/tests/fixtures/orient_1.avif（64x32）をこのクレートの直下に写して実行する。
// リンクされた libdav1d が実際に 1 フレームをデコードできることまで確かめる
fn main() {
    let data = std::fs::read("orient_1.avif").unwrap();
    let avif = avif_parse::read_avif(&mut data.as_slice()).unwrap();
    let mut settings = dav1d::Settings::new();
    settings.set_n_threads(1);
    settings.set_max_frame_delay(1);
    let mut dec = dav1d::Decoder::with_settings(&settings).expect("dav1d_open failed");
    match dec.send_data(avif.primary_item.to_vec(), None, None, None) {
        Ok(()) | Err(dav1d::Error::Again) => {}
        Err(e) => panic!("send_data: {e}"),
    }
    for _ in 0..100 {
        match dec.get_picture() {
            Ok(pic) => return println!("ok: {}x{}", pic.width(), pic.height()),
            Err(dav1d::Error::Again) => dec.send_pending_data().expect("send_pending_data"),
            Err(e) => panic!("get_picture: {e}"),
        }
    }
    panic!("no frame decoded");
}
```

`Decoder::get_frame_delay` は使わない（dav1d 0.11.1 は C の `dav1d_get_frame_delay(const Dav1dSettings *)` にコンテキストを渡しており、
正しく動いていても `Err(InvalidArgument)` を返す）。

- [ ] **Step 2: 利用者が Windows（MSVC）で実行する**

前提: Rust（MSVC）、Visual Studio Build Tools、git、nasm（`winget install NASM.NASM`、PATH を通す）、
meson と ninja（例 `pip install meson ninja`）、pkg-config の実行ファイル（例 `winget install bloodrock.pkg-config-lite`）。
ビルドと実行は **Developer Command Prompt ではない普通の PowerShell で**（spec の確かめたいことの 1 つ）:

```powershell
$env:SYSTEM_DEPS_DAV1D_BUILD_INTERNAL = "always"
cargo run
```

静的リンクの確認だけは `dumpbin` が PATH にある **Developer PowerShell** で:

```powershell
dumpbin /dependents target\debug\dav1d-probe.exe
```

Expected: `ok: 64x32` が出る。`dumpbin` の一覧に `dav1d.dll` がない（静的リンク）。

- [ ] **Step 3: 結果を実施メモに記録する**

「実施メモ / Task 1」に、通ったか、導入したもの（実際に要ったものと版）、Developer Command Prompt の外で通ったか、
失敗したときのエラーを書く。**Task 6 の README の Windows 手順はこの記録を出典にする**（推測で書かない）。

---

## Task 2: フィクスチャと probe を作り、差し替え前の出力を保存する

avif-decode がまだある状態で行う。ここで保存した出力が、Task 5 の「移植の正しさ」の正解になる。

**Files:**
- Create: `core/examples/avif_probe.rs`、`core/tests/fixtures/make_avif_decoder.py`、`core/tests/fixtures/avif/*.avif`

**Interfaces:**
- Produces: `cargo run -p picture-tool-core --example avif_probe -- <path> [--dump <out>]`。
  標準出力 1 行: `Ok <w>x<h> <色型>` / `Err <{:#} の文面>`。終了コード: Ok=0、Err=1。
  `--dump` のファイルは「`<色型>\n<w> <h>\n`」の後に生のサンプル列（16bit は LE）。`<色型>` は `DynamicImage::color()` の `Debug`（例 `Rgb8`）
- Produces: `core/tests/fixtures/avif/` の各ファイル（上の表）
- Produces: `docs/tmp/avif-baseline/<名前>.bin`（画素一致の 20 枚の、差し替え前の `--dump`。コミットしない）

- [x] **Step 1: `core/examples/avif_probe.rs` を書く**

`picture_tool_core::open_image_oriented` を呼ぶだけ。引数解析は手書きでよい（依存を足さない）。
`--dump` の書き出しは `img.as_bytes()` ではなく、16bit をサンプルごとに `to_le_bytes` で書く（A-7 と同じ並び）。

- [x] **Step 2: `make_avif_decoder.py` を書く**

冒頭の docstring に、使い方（`uv run --no-project --with pillow python make_avif_decoder.py <出力先>`）、
使ったツールと版（ffmpeg 8.1.3 / libaom-av1、Pillow 12.3.0）、各ファイルの中身の表への参照（`tests/jxl_avif_input.rs` 冒頭）を書く。
`crash*.avif` は元ファイル名と破壊したオフセット・元の値を表示する。中間ファイルは出力先に残さない。

- [x] **Step 3: 生成して、書かれた値を確かめる**

Run: `cd core/tests/fixtures && uv run --no-project --with pillow python make_avif_decoder.py avif`
Expected: 表の全ファイルができ、スクリプト内の assert（行列・範囲・深度・間引き・シーケンスヘッダの数）がすべて通る。
`alpha_huge.avif` のエンコードは数 GB のメモリと数分を要しうる。**作れなかったときは別の寸法・方法に黙って変えず、止めて報告する**
（宣言を書き換えた代用品はガードを外しても別の理由で落ちるので、テストの意味がなくなる。spec「テスト」）

- [x] **Step 4: 差し替え前の core で、正常なファイルが開け、`crash*.avif` が abort することを確かめる**

```bash
cargo build -p picture-tool-core --example avif_probe
P=target/debug/examples/avif_probe
for f in core/tests/fixtures/avif/*.avif; do echo "$f"; $P "$f"; echo "exit=$?"; done
```

Expected:
- 画素一致の 20 枚と `base_64x32.avif`: `Ok`。`two_seqhdr.avif`: `Ok`（現行は受け入れる。spec「挙動が変わる点」）
- `crash.avif`: `thread caused non-unwinding panic. aborting.` で `exit=134`
- `crash_seqhdr` / `crash_framehdr` / `crash_tile`: **`exit=134` でないものは削除し、スクリプトからも外す**
- `over_limit.avif`: `Err`（現行も宣言寸法で判定する）。`alpha_huge.avif`: 結果と所要時間・ピーク RSS（`/usr/bin/time -v`）を記録する

結果をすべて「実施メモ / Task 2」に貼る。

- [x] **Step 5: 差し替え前の出力を保存する**

```bash
mkdir -p docs/tmp/avif-baseline
for f in <画素一致の 20 枚>; do $P core/tests/fixtures/avif/$f --dump docs/tmp/avif-baseline/${f%.avif}.bin; done
grep -o -m1 'avx2' /proc/cpuinfo; grep -m1 'model name' /proc/cpuinfo
```

Expected: 20 個の `.bin`。CPU の型番と AVX2 の有無を実施メモに書く（Task 5 は同じ機械で行う）。

- [x] **Step 6: Commit**

```bash
git add core/examples/avif_probe.rs core/tests/fixtures/make_avif_decoder.py core/tests/fixtures/avif
git commit   # 件名例: test: AVIF デコーダの差し替え: 画素一致と回帰のフィクスチャを作る
```

---

## Task 3: デコード前のメモリの上限を作り直す（シーケンスヘッダの数・宣言寸法・`frame_size_limit` の値）

デコーダはまだ avif-decode のまま。判定はデコードより前で `Err` にするので、ここで完結して試せる。

**Files:**
- Modify: `core/src/decode/avif.rs`（`max_frame_pixels` 以下を削除して置き換え。既存の `obu` / `reduced` / `Writer` のテスト補助は数のテストに流用してよい）
- Modify: `core/src/decode/mod.rs`（`reserve_avif` → `frame_size_limits`）
- Test: 同ファイルの `#[cfg(test)]`、`core/tests/jxl_avif_input.rs`

**Interfaces:**
- Produces（`avif.rs`）:
  - `pub(super) struct Declared { pub pixels: u64, pub bit_depth: u8 }` — `pixels` は `max_frame_width × max_frame_height`
  - `pub(super) fn declared(av1: &[u8]) -> Result<Declared>` — `sequence_header_count(av1)? == 1` を確かめてから（違えば上の文言）、
    `avif_parse::AV1Metadata::parse_av1_bitstream(av1)` の値を返す
  - `fn sequence_header_count(obus: &[u8]) -> Result<usize>` — 現行 `max_frame_pixels` の OBU の歩き方（拡張ヘッダ・サイズ欄の有無・`leb128`）を流用。切れた OBU は `Err`
- Produces（`mod.rs`）:
  - `struct FrameSizeLimits { primary: u32, alpha: Option<u32> }`
  - `fn frame_size_limits(primary: Declared, alpha: Option<Declared>) -> Result<FrameSizeLimits>` — spec「メモリの上限」1.3〜1.4 の順:
    1 つの `image::Limits::default()` に主画像 `pixels × 3 × bytes(primary.bit_depth)` を、続けてアルファ `pixels × 1 × bytes(primary.bit_depth)` を
    `reserve`（失敗は上の文言で `Err`）。通ってから `u32::try_from`。アルファは `min(alpha.pixels, primary.pixels)`。`bytes(d) = if d > 8 {2} else {1}`
- `open` の `Format::Avif` はこの時点では `frame_size_limits(declared(&parsed.primary_item)?, parsed.alpha_item.as_deref().map(declared).transpose()?)?;` を呼んで値を捨てる（`let _limits`）。Task 4 で dav1d に渡す。
  この間はフィールドが lib のビルドで読まれず dead_code で `-D warnings` が落ちるので、`FrameSizeLimits` に
  `#[cfg_attr(not(test), allow(dead_code))]` を付ける（Task 4 Step 8 で外す）

- [x] **Step 1: 単体テストを書く**

`avif.rs`:

```rust
#[test]
fn exactly_one_sequence_header_is_required() {
    assert_eq!(sequence_header_count(&reduced(6, 5, 64, 32)).unwrap(), 1);
    let two = [reduced(6, 5, 64, 32), obu(6, &[0; 4]), reduced(6, 5, 64, 32)].concat();
    assert_eq!(sequence_header_count(&two).unwrap(), 2);
    assert_eq!(sequence_header_count(TEMPORAL_DELIMITER).unwrap(), 0);
    let err = declared(&two).unwrap_err();
    assert!(format!("{err:#}").contains("exactly one sequence header (found 2)"), "{err:#}");
}

#[test]
fn truncated_obu_is_an_error() { /* 現行 missing_or_truncated_sequence_header_is_an_error の truncated 2 件 */ }

#[test]
fn declared_size_is_the_sequence_header_maximum() {
    // 実ファイルの AV1 データ（64x32）
    let data = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/orient_1.avif")).unwrap();
    let parsed = avif_parse::read_avif(&mut data.as_slice()).unwrap();
    let d = declared(&parsed.primary_item).unwrap();
    assert_eq!((d.pixels, d.bit_depth), (64 * 32, 8));
}
```

`mod.rs`:

```rust
fn d(pixels: u64, bit_depth: u8) -> avif::Declared { avif::Declared { pixels, bit_depth } }

#[test]
fn frame_size_limits_passes_the_declared_sizes() {
    let l = frame_size_limits(d(2048, 8), None).unwrap();
    assert_eq!((l.primary, l.alpha), (2048, None));
    // アルファは主画像との小さいほう
    let l = frame_size_limits(d(4096, 8), Some(d(1_000_000, 8))).unwrap();
    assert_eq!(l.alpha, Some(4096));
    let l = frame_size_limits(d(4096, 8), Some(d(1024, 8))).unwrap();
    assert_eq!(l.alpha, Some(1024));
}

#[test]
fn frame_size_limits_counts_alpha_and_depth_against_one_budget() {
    let over = |r: Result<FrameSizeLimits>| format!("{:#}", r.unwrap_err()).contains("over the 512MB output limit");
    // 100M 画素: 8bit なら 300MB で通り、10bit なら 600MB で落ちる
    assert!(frame_size_limits(d(100_000_000, 8), None).is_ok());
    assert!(over(frame_size_limits(d(100_000_000, 10), None)));
    // 主画像 64x64 ＋ アルファ 23200x23200（1 チャンネルでも合計が上限を超える）
    assert!(over(frame_size_limits(d(64 * 64, 8), Some(d(23200 * 23200, 8)))));
    // アルファは主画像の深度で数える: 主 10bit なら 2 バイト
    // 8bit は 300,012,288 バイトで通り、10bit は 600,024,576 バイトで超える
    assert!(frame_size_limits(d(64 * 64, 8), Some(d(300_000_000, 8))).is_ok());
    assert!(over(frame_size_limits(d(64 * 64, 10), Some(d(300_000_000, 8)))));
}

#[test]
fn frame_size_limits_rejects_before_the_limit_would_wrap() {
    // 65536x65536 = 2^32。u32 に入らず、0（無制限）にもならないまま 512MB 判定で落ちる
    let err = frame_size_limits(d(1 << 32, 8), None).unwrap_err();
    assert!(format!("{err:#}").contains("over the 512MB output limit"), "{err:#}");
}
```

- [x] **Step 2: 結合テストを書く（`core/tests/jxl_avif_input.rs`）**

```rust
fn avif(name: &str) -> PathBuf { fixture(&format!("avif/{name}")) }
fn open_err(name: &str) -> String {
    format!("{:#}", open_image_oriented(&avif(name)).expect_err(name))
}

#[test]
fn avif_over_the_output_limit_is_rejected_before_decoding() {
    assert!(open_err("over_limit.avif").contains("AVIF declares a size over the 512MB output limit"));
}

#[test]
fn avif_alpha_is_counted_against_the_output_limit() {
    // 前提: 主画像だけなら上限の内側（64x64）
    assert_eq!(image_dimensions_oriented(&avif("alpha_huge.avif")).unwrap(), (64, 64));
    assert!(open_err("alpha_huge.avif").contains("AVIF declares a size over the 512MB output limit"));
}

#[test]
fn avif_with_two_sequence_headers_in_one_item_is_rejected() {
    assert!(open_err("two_seqhdr.avif").contains("exactly one sequence header (found 2)"));
    // アイテムごとに 1 個（合計 2 個）の正常なアルファ付きは開ける
    assert!(open_image_oriented(&avif("p8_420_lim_709_a8.avif")).is_ok());
}
```

- [x] **Step 3: 落ちることを確かめる**

Run: `cargo test -p picture-tool-core`
Expected: 新しい関数がないのでコンパイルエラー（`declared` / `frame_size_limits` が未定義）

- [x] **Step 4: 実装する**

`avif.rs` のモジュールコメント 2 行目（「AV1 デコーダ（rav1d）が確保しうるフレームの最大寸法」）を
「アイテムの AV1 データのシーケンスヘッダの数（av1-avif §2.1「exactly one」）」に直す。`reserve_avif` のコメントの内容は
`frame_size_limits` のコメントへ、spec「メモリの上限」の理由（数を 1 に限る理由、アルファを主画像の深度で数える理由、
`frame_size_limit` の 0 が無制限なので判定を先に通す理由、アルファに min を渡す理由）を短く書く。

- [x] **Step 5: 通ることを確かめ、ガードを 1 つずつ外して理由が変わることを実測する**

Run: `cargo test -p picture-tool-core` → PASS。続けて次の 3 つを 1 つずつ一時的に外して `--test jxl_avif_input` を走らせ、
落ちるテストと、そのとき返った `Err` の文面（または `Ok`）を実施メモ「Task 3: ガードを外したとき」に記録して戻す:
1. `declared` の数の確認 → `avif_with_two_sequence_headers...` が落ちる（`Ok` になる）
2. アルファの `reserve` → `avif_alpha_is_counted...` が落ちる（理由が変わる。この時点ではまだ avif-decode なのでアルファを実際にデコードしうる。ピーク RSS も記録）
3. 主画像の `reserve` → `avif_over_the_output_limit...` が落ちる（avif-decode が別の理由で落ちるか、デコードが走る）

Run: `make check` → exit 0

- [x] **Step 6: Commit**（件名例: `fix: AVIF デコーダの差し替え: デコード前の上限をアイテムごとの宣言寸法で判定する`）

---

## Task 4: AV1 のデコードを dav1d に差し替え、YUV→RGB 変換を移植する

**Task 1 の結果を待つ。** avif-decode を外して dav1d を入れるのは同じコミット（Global Constraints）。

**Files:**
- Create: `core/src/decode/av1.rs`、`core/src/decode/yuv.rs`
- Modify: `core/Cargo.toml`、`core/src/decode/mod.rs`、`.github/workflows/ci.yml`
- Test: `av1.rs` の `#[cfg(test)]`、`core/tests/jxl_avif_input.rs`

**Interfaces:**
- Consumes: Task 3 の `avif::declared`、`frame_size_limits`、`FrameSizeLimits`
- Produces（`av1.rs`）: `pub(super) fn decode(av1: &[u8], frame_size_limit: u32) -> Result<dav1d::Picture>`
- Produces（`yuv.rs`）: `pub(super) fn to_image(color: &dav1d::Picture, alpha: Option<&dav1d::Picture>, premultiplied: bool) -> Result<DynamicImage>`
- `mod.rs` の `Format::Avif`:

```rust
let limits = frame_size_limits(declared(&parsed.primary_item)?, parsed.alpha_item.as_deref().map(declared).transpose()?)?;
let color = av1::decode(&parsed.primary_item, limits.primary)?;
let alpha = match (&parsed.alpha_item, limits.alpha) {
    (Some(a), Some(limit)) => Some(av1::decode(a, limit)?),
    _ => None,
};
Ok(layout.apply(yuv::to_image(&color, alpha.as_ref(), parsed.premultiplied_alpha)?))
```

- [x] **Step 1: 依存を入れ替える**

`core/Cargo.toml`: `avif-decode` の行とコメントを消し、次を足す（コメントは spec の理由を短く）:
- `dav1d = "0.11"` — C 版 libdav1d。rav1d と同居させない理由、Linux はシステムの libdav1d（1.3 以上）、Windows・CI は `SYSTEM_DEPS_DAV1D_BUILD_INTERNAL=always`、nasm はその内部ビルドのため
- `yuv = { version = "=0.8.19", default-features = false }` — SIMD 経路は CPU で出力が変わる／`=` で固定する理由／**ワークスペースの別クレートが既定 feature で `yuv` を足すと feature の統合で SIMD が黙って有効になり、画素一致のテストが AVX2 の機械で落ちる**
- `bytemuck = "1"` — 16bit の平面を `&[u16]` として読むため（image が既に依存している版）
- jxl-oxide の上のコメント「どちらも純 Rust（C ライブラリ不要）」を JPEG XL だけの記述に直す

Run: `cargo tree -p picture-tool-core -i rav1d` → `error: package ID specification \`rav1d\` did not match any packages`（空）

- [x] **Step 1b: CI を内部ビルドにする**

`ci.yml`: apt に `meson`・`ninja-build` を足し、`nasm` の理由のコメントを「libdav1d の内部ビルド（x86_64 のアセンブリ）」に直す。
ジョブの `env` に `SYSTEM_DEPS_DAV1D_BUILD_INTERNAL: always`。コメントに「22.04 の apt の libdav1d は 1.3 未満で使えない。
内部ビルドなら Windows と同じ仕組みを CI が毎回通す」。runner は ubuntu-22.04 のまま（22.04 の apt の meson 0.61 は dav1d 1.5.0 の `meson_version: '>= 0.49.0'` を満たす）。
依存の入れ替えと同じコミットに入れる（分けると、このタスクのコミットが CI で通らない）。ローカルでの確認は Step 9 で行う。

- [x] **Step 2: 周回のループの単体テストを書く（`av1.rs`）**

ループを、dav1d の `Decoder` に依らない関数に切り出してテストする:

```rust
/// 送り切るまで `send_pending` し、`get` が `Again` を返すまで取り続けて最後の 1 枚を返す
fn drain<P>(
    len: usize,
    mut pending: bool,
    mut get: impl FnMut() -> Result<P, dav1d::Error>,
    mut send_pending: impl FnMut() -> Result<(), dav1d::Error>,
) -> Result<P>
```

アルゴリズム（A-1・A-2。spec の「1 周」の定義どおり）:

```text
last = None
repeat at most 2*len + 16 times:
    match get():
        Ok(p)          => last = Some(p)
        Err(Again)     => if !pending { return last or Err("no frame decoded") }
        Err(e)         => return Err(e) with context "AV1 decoding failed"
    if pending:
        match send_pending():
            Ok(())     => pending = false
            Err(Again) => {}
            Err(e)     => return Err(e) with context
return Err("AV1 decoder made no progress")
```

テスト（偽の `get` / `send_pending` はクロージャで、呼ばれた回数を数える）:

```rust
#[test]
fn the_last_picture_is_returned() {
    let mut out = vec![Ok(2), Ok(1)];          // pop で 1, 2 の順に出る
    let p = drain(10, false, || out.pop().unwrap_or(Err(dav1d::Error::Again)), || Ok(())).unwrap();
    assert_eq!(p, 2);
}

#[test]
fn pending_data_is_sent_until_accepted() {
    // 3 回目の send_pending で受け付けられ、その後に 1 枚だけ出る
    let sends = std::cell::Cell::new(0);
    let mut emitted = false;
    let p = drain(10, true,
        || if sends.get() >= 3 && !emitted { emitted = true; Ok(7) } else { Err(dav1d::Error::Again) },
        || { sends.set(sends.get() + 1); if sends.get() < 3 { Err(dav1d::Error::Again) } else { Ok(()) } }).unwrap();
    assert_eq!((p, sends.get()), (7, 3));
}

#[test]
fn nothing_decoded_is_an_error() {
    let err = drain::<u8>(10, false, || Err(dav1d::Error::Again), || Ok(())).unwrap_err();
    assert!(format!("{err:#}").contains("no frame decoded"));
}

#[test]
fn a_stalled_decoder_is_cut_off() {
    let mut calls = 0;
    let err = drain::<u8>(100, true, || { calls += 1; Err(dav1d::Error::Again) }, || Err(dav1d::Error::Again)).unwrap_err();
    assert!(format!("{err:#}").contains("AV1 decoder made no progress"));
    assert_eq!(calls, 2 * 100 + 16);
}

#[test]
fn decoder_errors_are_returned() { /* get が Err(dav1d::Error::InvalidArgument) → Err で、文面に "AV1 decoding failed" */ }
```


- [x] **Step 3: `frame_size_limit` の配線の単体テストを書く（`av1.rs`）**

```rust
#[test]
fn frame_size_limit_is_enforced_by_the_decoder() {
    let data = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/orient_1.avif")).unwrap();
    let parsed = avif_parse::read_avif(&mut data.as_slice()).unwrap();
    // 64x32 = 2048 画素。上限は「以下なら通す」
    let ok = decode(&parsed.primary_item, 2048).unwrap();
    assert_eq!((ok.width(), ok.height()), (64, 32));
    assert!(decode(&parsed.primary_item, 2047).is_err());
}
```

- [x] **Step 4: abort の回帰テストを書く（`jxl_avif_input.rs`）**

```rust
/// 差し替え前（avif-decode / rav1d）はテストバイナリごと abort した（実施メモ Task 2）
#[test]
fn malformed_avif_is_an_error_not_an_abort() {
    for name in [/* Task 2 で残った crash*.avif をすべて */] {
        assert!(open_image_oriented(&avif(name)).is_err(), "{name}");
    }
}
```

- [x] **Step 5: 落ちることを確かめる**

Run: `cargo test -p picture-tool-core`
Expected: `av1::decode` / `drain` / `yuv::to_image` が未定義でコンパイルエラー

- [x] **Step 6: `av1.rs` を実装する**

`decode`: `Settings` に Global Constraints の 4 つだけを設定して `Decoder::with_settings`。`send_data(av1.to_vec(), None, None, None)`
（`T: 'static` が要るので複製する。AVIF の圧縮データは小さい）の結果で `pending` の初期値を決め（`Ok`=偽、`Again`=真、他は `Err`）、
`Decoder` を `RefCell` に包み、`drain(av1.len(), pending, || dec.borrow_mut().get_picture(), || dec.borrow_mut().send_pending_data())` を返す
（`get_picture` も `send_pending_data` も `&mut self` なので、2 つのクロージャが同時に可変で捕まえるとコンパイルできない）。
モジュールコメントに、周回で打ち切る理由（送り残りのバイト数が公開 API で観測できない）と、最後の 1 枚を取る理由（A-1）を書く。

- [x] **Step 7: `yuv.rs` を移植する**

移植元: avif-decode 3.0.0 の `src/lib.rs` `to_image`、`src/image.rs`、`src/rav1d.rs` の `single_plane_img` / `matrix_coefficients` / `range`
（`~/.cargo/registry/src/*/avif-decode-3.0.0/`）。規則は spec「YUV→RGB 変換」の表がすべて。実装上の決定:

- 内部の行列の種別: `enum Matrix { Identity, Bt709, Fcc, Bt601, Smpte240, YCgCo, Bt2020Ncl, Smpte2085, ChromatNcl, ChromatCl, ICtCp }`。
  `fn matrix(p: &Picture) -> Option<Matrix>` が dav1d の名前を**値の意味で**写す: `Unspecified`→`None`、`BT470M`（値 4）→`Fcc`、
  `BT470BG`（値 5・6）→`Bt601`、`ST240M`→`Smpte240`、`BT2020NonConstantLuminance` と `BT2020ConstantLuminance`→`Bt2020Ncl`（A-5）
- 入口で、主画像の `width × height × チャンネル数 × (bits_per_component > 8 ? 2 : 1)` を `Limits::default().reserve` し、
  失敗は「変換前の 512MB 判定」の文言。チャンネル数: アルファありは 4、なしはカラー 3・グレー 1
- 出力は `ImageBuffer::<P, Vec<_>>::new(w, h)` を作って、その中身へ `yuv` に直接書かせる（`avif_to_dynamic` の複製をなくす）
- 平面: `p.plane(Y|U|V)` の `&[u8]` と `p.stride(..)`（バイト単位）。8bit は `&[u8]` のまま、16bit は
  `bytemuck::try_cast_slice::<u8, u16>` で、整列していなければ `u16::from_ne_bytes` で複製（avif-decode の `imgref_align_to_u16`）。
  `single_plane_img` の検査（stride が正・サンプル幅の倍数・平面の幅以上）を移し、違反は `Err`
- 深度は `bits_per_component()`（`None` は `Err`）。8 / 10 / 12 以外は `Err`
- 16bit 化の後処理（`(v << shift) | (v >> low_shift)`）、`luma8` / `luma16`、`unpremultiply8` / `unpremultiply16`、
  アルファの行列の検査（主画像の「なし」は `Identity` とみなし、アルファが「あり」かつ `Identity` でないときだけ比べる）は移植元のとおり
- `has_alpha` は `alpha.is_some()`
- 冒頭にライセンス文（Global Constraints）と、「移植元と、読んでよいアクセサの制限」のコメント

- [x] **Step 8: `mod.rs` を繋ぎ替える**

`Format::Avif` を Interfaces のとおりにし、`avif_to_dynamic` と `reserve_avif` の残りを消す。`mod av1; mod yuv;` を足す。
Task 3 で `FrameSizeLimits` に付けた `#[cfg_attr(not(test), allow(dead_code))]` を外す。

- [x] **Step 9: 通ることを確かめる**

Run: `cargo test -p picture-tool-core`
Expected: PASS（既存の `jxl_avif_input` 19 件を含む。向きの 8 件は図柄の色で照合しているので、画素のわずかな違いでは落ちない）

Run: `for f in core/tests/fixtures/avif/crash*.avif; do target/debug/examples/avif_probe "$f"; echo "exit=$?"; done`（先に `cargo build -p picture-tool-core --example avif_probe`）
Expected: すべて `Err ...` で `exit=1`

Run: ローカルで CI と同じ内部ビルドを試す（meson/ninja は uv の venv で入れてよい。spike と同じ）:
`SYSTEM_DEPS_DAV1D_BUILD_INTERNAL=always cargo test -p picture-tool-core --target-dir target/internal-dav1d`
Expected: PASS。`target/internal-dav1d/debug/examples/avif_probe` を `ldd` して `libdav1d.so` がないこと
（`target/` を共有したまま並行ビルドしない）

- [x] **Step 10: ガードを外して落ちることを実測する**

1 つずつ外して戻し、実施メモ「Task 4: ガードを外したとき」に記録する:
1. `set_frame_size_limit` の呼び出し → `frame_size_limit_is_enforced_by_the_decoder` が落ちる
2. `drain` の周回の上限（`loop` にする）→ `a_stalled_decoder_is_cut_off` が終わらないこと（タイムアウトで止める）
3. `alpha_huge.avif` で、アルファの `reserve` を外したとき（Task 3 の 2 を dav1d で再実測）→ 理由がアルファの `frame_size_limit` による dav1d のエラーになること

Run: `make check` → exit 0

- [x] **Step 11: Commit**（件名例: `fix: AVIF デコーダの差し替え: AV1 のデコードを libdav1d に替え、壊れた AVIF で abort しないようにする`）

本文に、rav1d と同居させない理由と `cargo tree -i rav1d` が空であることを書く。

---

## Task 5: 移植の正しさを 1 回だけ照合し、画素一致の常設テストを置く

**Files:**
- Test: `core/tests/jxl_avif_input.rs`
- Modify（一時的、コミットしない）: `core/Cargo.toml` の `yuv` の行

- [x] **Step 1: `yuv` を既定 feature にして、差し替え前の出力とバイト一致することを確かめる**

`core/Cargo.toml` の `yuv` を一時的に `yuv = "=0.8.19"` にし、Task 2 と同じ機械で:

```bash
cargo build -p picture-tool-core --example avif_probe
mkdir -p docs/tmp/avif-ported-avx2
for b in docs/tmp/avif-baseline/*.bin; do n=$(basename "$b" .bin)
  target/debug/examples/avif_probe core/tests/fixtures/avif/$n.avif --dump docs/tmp/avif-ported-avx2/$n.bin
  cmp "$b" docs/tmp/avif-ported-avx2/$n.bin && echo "$n same"; done
```

Expected: 20 件すべて `same`。**1 件でも違えば移植の誤り**（A-6 の別デコーダ、A-1 の最後の 1 枚を含む）。原因を突き止めて Task 4 の
コードを直す（期待値を合わせにいかない）。移植のコードに原因が見つからず、デコーダ（rav1d と libdav1d）の YUV 出力の違いと
疑われるときは、そこで止めて利用者に報告する（Task 4 では直せず、spec の目的「画素単位で一致」の扱いを決め直す必要があるため）。結果を実施メモ「Task 5: 移植の照合」に貼る。

- [x] **Step 2: `yuv` の行を `default-features = false` に戻す**

Run: `git diff core/Cargo.toml` → 差分なし

- [x] **Step 3: 画素一致のテストを書く**

```rust
/// 画素一致の期待値（スカラー経路）。**作り直す手段はない**（avif-decode を外したため）。
/// 期待値は、移植したコードを `yuv` の SIMD 経路でビルドして差し替え前とバイト一致を確かめた後に、
/// スカラー経路で作ったもの（plan 2026-10-19 の実施メモ Task 5）。
/// **ワークスペースの別クレートが `yuv` を既定 feature で依存に加えると SIMD 経路が黙って有効になり、
/// AVX2 の機械でここが落ちる**（core/Cargo.toml の `yuv` の行）。
const PIXELS: &[(&str, &str, u32, u32, u64)] = &[
    // (ファイル, 色型, 幅, 高さ, FNV-1a 64)
];

fn fnv1a(img: &image::DynamicImage) -> u64 { /* A-7 */ }

#[test]
fn avif_pixels_match_the_recorded_output() {
    for &(name, color, w, h, hash) in PIXELS {
        let img = open_image_oriented(&avif(name)).unwrap();
        assert_eq!((format!("{:?}", img.color()).as_str(), img.width(), img.height()), (color, w, h), "{name}");
        assert_eq!(fnv1a(&img), hash, "{name}: {:#018x}", fnv1a(&img));
    }
}
```

`PIXELS` には画素一致の 20 枚すべてを並べる。色型の期待（`Rgb8` / `Rgba16` / `Luma8` …）は spec「出力の画素形式」の表から決めて
先に書き、ハッシュは一度走らせて失敗メッセージから写す（Step 1 を通ったコードの出力なので、ここで写してよい）。

- [x] **Step 4: 通ることを確かめ、罠を戻して落ちることを実測する**

Run: `cargo test -p picture-tool-core --test jxl_avif_input` → PASS

1 つずつ入れて戻し、どのファイルで落ちたかを実施メモ「Task 5: 罠を戻したとき」に記録する（spec「画素一致（常設）」の 3 つ＋ A-1）:
1. 行列「なし」を BT.709 にする → `p8_420_lim_unspec.avif`・`p10_444_full_unspec.avif` が落ちる（グレーは行列を使わないので落ちない）
2. アルファの `Unspecified` を値として比べる（「なし」でなく `Some(Unspecified)` 扱い）→ `p8_420_lim_709_a420.avif` が `alpha image has color info` で落ちる
3. 16bit 化をビット複製でなく左シフトだけにする → `p10_*`・`p12_*` が落ちる
4. `drain` が最初の 1 枚で返す → `two_frames.avif` が落ちる

Run: `make check` → exit 0

- [x] **Step 5: Commit**（件名例: `test: AVIF デコーダの差し替え: 移植した色変換の画素一致を固定する`）

---

## Task 6: README・コメントを更新する

**Files:**
- Modify: `README.md`、`core/src/decode/mod.rs`、`core/src/lib.rs`、
  `gui/src/security.rs:188`、`gui-frontend/src/App.svelte:140-142`、`docs/superpowers/specs/2026-09-08-grid-filtering-design.md:284`

- [x] **Step 1: Rust の最低版を求める**

Run: `cargo metadata --format-version 1 | jq -r '[.packages[] | select(.rust_version != null) | .rust_version] | sort_by(split(".") | map(tonumber)) | last'`
Expected: 1 つの版（2026-10-19 時点の調査では avif-parse 1.90 が最大の見込み）。README の「Rust 1.98以降（avif-decode の要件）」を
「Rust <版>以降（<そのクレート> の要件）」に直す

- [x] **Step 2: README**

spec「文書」の README の項目をすべて。Windows の手順は **Task 1 の実施メモにある、実際に要ったもの**で書く。必ず含めるもの:
`SYSTEM_DEPS_DAV1D_BUILD_INTERNAL=always` をビルドのたびにシェルで設定すること（`.cargo/config.toml` に置かない理由も 1 行）、
pkg-config の導入、ビルド時のネットワーク（code.videolan.org）、「pkg-config が dav1d を見つけない」と出たらビルドログを遡って
git / meson の失敗を探すこと。Linux は Fedora `libdav1d-devel` / Ubuntu 24.04 以降 `libdav1d-dev`（Debian 12 は不可）、macOS は
`brew install dav1d`（未検証と明記）。既知の制限 4 項目（spec「文書」）。
**Linux の配布物（`make release` の deb / rpm / AppImage）は実行時にシステムの libdav1d（1.3 以上）を要する**ことも既知の制限に書く
（パッケージの依存には宣言しない。宣言は後日の対応とする。利用者判断 2026-10-19）。

- [x] **Step 3: コメント**

- `mod.rs` 冒頭「メモリの上限」: AVIF は 2 段（デコード前は宣言寸法、変換前は実画像）、`frame_size_limit` をデコーダに強制させること、
  残る上限（デコーダ内部は「宣言寸法 × 深度 × 参照フレーム数」まで。姉妹 spec で扱う）。`[`reserve_avif`]` へのリンクを `[`frame_size_limits`]` に
- `guarded`: rav1d の記述を消し、「libdav1d のワーカーは C のスレッドで Rust の panic は起きない（`n_threads = 1` でワーカーも起こさない）。
  dav1d クレート側の panic（予約値の `unreachable!()` 等）は呼び出したスレッドで起き、ここで捕まえる」
- `lib.rs` 冒頭の「`eprintln!` しない」に例外: 外部ライブラリが壊れた入力で stderr に書くこと（libdav1d のログ、`guarded` が捕まえた
  panic の既定 hook のメッセージ）は止められない
- 古い記述 3 か所: 非 JPEG の例を「PNG / WebP / JPEG XL / AVIF」に、grid-filtering spec の `is_supported_image` の拡張子一覧を現行に
  （`grep -n "fn is_supported_image" -A5 core/src/lib.rs` で現行を確かめて写す）

Run: `make check` → exit 0

- [x] **Step 4: Commit**（件名例: `docs: AVIF デコーダの差し替え: ビルド要件と既知の制限を libdav1d に合わせる`）

---

## Task 7: 低優先度の修正（JPEG XL）

**Files:**
- Modify: `core/src/decode/jxl.rs:111-114`、`core/src/decode/mod.rs`（`jxl_decoder`）
- Test: `jxl.rs` の `#[cfg(test)]`

- [x] **Step 1: テストを書く**

`jxl.rs` の既存テストの組み方に合わせ、**本体が 4 バイト未満の `brob` の後に正しい `Exif` 箱を置いたコンテナ**で
`raw_exif` が後ろの Exif を読めること（現行は `read_exif_exact` が次の箱のヘッダを食って読めない）を確かめるテストを書く。
前提として、同じコンテナから短い `brob` を除くと読めることも assert する。

- [x] **Step 2: 落ちることを確かめる** — Run: `cargo test -p picture-tool-core jxl` → 新しいテストが FAIL

- [x] **Step 3: 実装する** — `brob` の本体長が 4 未満なら種類を読まずにその箱を飛ばす（種類が Exif でない `brob` と同じ扱い）。
  `jxl_decoder` の `width * height * JXL_WORKING_BYTES_PER_PIXEL + JXL_WORKING_BASE` を `saturating_mul` / `saturating_add` にする
  （上限値そのものは姉妹 spec で見直す。コメントに一言）

- [x] **Step 4: 通ることを確かめる** — Run: `make check` → exit 0

- [x] **Step 5: Commit**（件名例: `fix: AVIF デコーダの差し替え: JPEG XL の短い brob で後続の箱を読み違えないようにする`）

---

## Task 8: ファズ・完了条件の確認・索引

**Files:**
- Create: `core/tests/fixtures/fuzz_avif.py`
- Modify: `docs/README.md`、本 plan の「実施メモ」

- [x] **Step 1: ファズを書いて走らせる**

`fuzz_avif.py <probe> <出力dir> <元の AVIF...>`: 元ファイルごとに 130 件以上、ランダムに 1〜4 バイトを書き換えたものを作り
（乱数の種を固定し、引数で表示する）、1 件ずつ probe を子プロセスで開き、**終了コード 0 / 1 以外・シグナル・10 秒のタイムアウト**を数える。
元は画素一致の 20 枚から 10 種以上（深度・間引き・アルファの違うもの）。合計 1300 件以上。

Run: `uv run --no-project python core/tests/fixtures/fuzz_avif.py target/debug/examples/avif_probe <scratchpad>/fuzz core/tests/fixtures/avif/p8_420_lim_unspec.avif ...`
Expected: abort・signal・timeout が **0 件**。Ok / Err の件数と種を実施メモ「Task 8: ファズ」に貼る。
0 件でなければ、そのファイルを残して原因を調べる（完了にしない）

- [ ] **Step 2: 完了条件を確かめる**

- `cargo tree -p picture-tool-core -i rav1d` → 空
- `make check` → exit 0
- `cd gui-frontend && bun run e2e` → PASS（`App.svelte` を触ったため）
- **push して CI が通ること**（push の前に利用者の確認を取る）
- **利用者が Windows で `cargo test -p picture-tool-core --test jxl_avif_input` を通す**（Task 1 の手順と同じ環境変数で）。
  画素一致が Windows の CPU でも同じハッシュになることがここで確かめられる。結果を実施メモに記録

- [x] **Step 3: 索引と実施メモ**

`docs/README.md` の「直近の実装計画」の先頭に本 plan を足す（内容: 全 8 タスク、A-1〜A-10、ガードを外したときの実測、ファズの結果）。
実施メモの「spec と食い違った点」に A-1・A-2・A-3 を要約して書く。

- [x] **Step 4: Commit**（件名例: `docs: AVIF デコーダの差し替え: ファズの結果と実施メモを残す`）

---

## セルフレビュー（spec との突き合わせ）

| spec の項目 | タスク |
|---|---|
| やること 1（dav1d への差し替え）・決定事項（スレッド・設定） | Task 4 |
| やること 2（YUV→RGB の移植、スカラー固定、ライセンス表示、読んでよいアクセサ） | Task 4（Step 7）・Task 5 |
| やること 3（メモリの上限 2 段、数 = 1、アルファの判定、`frame_size_limit` の順序と min） | Task 3（デコード前）・Task 4（変換前・配線） |
| やること 4（`avif_to_dynamic` の複製をなくす） | Task 4 Step 7・8 |
| やること 5（回帰テスト一式、フィクスチャのコミット、ガードを外したときの実測） | Task 2〜5 |
| やること 6（依存・CI・README・コメント・古い記述・`cargo tree -i rav1d`） | Task 4 Step 1・1b（依存・CI）・Task 6・Task 8 |
| やること 7（低優先度の修正 2 件） | Task 7 |
| 「挙動が変わる点」（予約値 3、ヘッダ 2 個以上） | ヘッダ 2 個は Task 3 のテスト。予約値 3 は `guarded` が捕まえる経路で、フィクスチャはない（下記） |
| Windows を本体より先に確かめる | Task 1（Task 4 の前提） |
| ファズ・完了条件 | Task 8 |

### spec から意図的に外したこと・テストを置かなかったこと

- A-1〜A-5（上の表）
- **行列係数が予約値 3 の AVIF のテスト**: libaom は予約値を書けず、シーケンスヘッダの `matrix_coefficients` の 8 ビットを書き換えると
  `color_config` の後続ビットの位置は変わらないが、作ったファイルが dav1d の別の検査で落ちないことを確かめる手段が要る。
  spec のテスト表にない。`guarded` の経路は avif-parse の `debug_assert!` で既に使われている
- **変換前の 512MB 判定のテスト**: デコード前の判定を通り、かつ実画像が上限を超えるファイルは、`frame_size_limit` があるため作れない
  （取りこぼしの保険なので、到達できないことが正しい状態）
- **Identity と 4:2:0 の組み合わせの `Err`**: libaom は `gbrp` 以外で行列 0 を書かない。移植元のとおり分岐を残す

---

## 実施メモ

### Task 1: Windows での内部ビルド

未実施。利用者が Windows で確認できない状況のため、確認なしで進めた（利用者判断 2026-10-19）。README の Windows 手順は spec の記述から書き、未検証と明記した。spec の完了条件「利用者が Windows で `jxl_avif_input` を通す」は未達。

### Task 2: 差し替え前の確認

- 環境: CPU は `13th Gen Intel(R) Core(TM) i9-13900H`（AVX2 あり。Task 5 も同じ機械）。生成は `uv run --no-project --with pillow==12.3.0 python make_avif_decoder.py avif`（約 4 秒の本体、alpha_huge を含めた全体は 6.5 秒、ピーク RSS 4.8GB）。生成は決定的（2 回の生成で全ファイルがバイト単位で一致）。`--with pillow` だけだと今は 13.0.0 になるので 12.3.0 に固定（13.0.0 でも base のバイト列は同一）
- 差し替え前の core で開いた結果: 画素一致の 20 枚・base_64x32・two_seqhdr は `Ok`（two_seqhdr は現行が受け入れる）、over_limit は `Err … Memory limit exceeded`
- **alpha_huge.avif は ffmpeg で作れない。** ffmpeg 8.1.3 の `av_image_check_size2` が `(w+128)*(h+128) >= INT_MAX/8` を固定で拒否し、23200x23200 は `Picture size 23200x23200 is invalid`（`-max_pixels` では外せない。14000x13000 は内側なので作れた）。アルファだけ `aomenc`（libaom 3.13.3、`--allintra --monochrome --cpu-used=9 --end-usage=q --cq-level=30 --limit=1 --obu`、入力は `Cmono XCOLORRANGE=FULL` の y4m）で実際にエンコードし、主画像（64x64、ffmpeg）と組んだ。宣言だけを書き換えたものではない
- alpha_huge の差し替え前の結果: `Err … invalid alpha size`、**3:07.20、ピーク RSS 818,164 KB**（debug ビルドの probe）。主画像の宣言だけで 512MB 判定をし、アルファは「宣言を超えうる最大寸法 ×4」の判定しか受けないので通り、rav1d が 23200x23200 を実際にデコードしてから avif-decode が寸法の不一致で落とす。Task 3 のガードが防ぐ対象
- **crash.avif**: frame OBU ペイロードの中央（タイルデータ）の 340・341（元 0x18,0xEF）を 0xFF にしたもの。差し替え前の core で rav1d `src/decode.rs:4997:33` の `Option::unwrap()` on `None` → `thread caused non-unwinding panic. aborting.`、exit=134
- 外した crash 候補とその理由（Pillow 12.3.0 / libavif 1.4.2 のベースで、2 バイトを 0xFF にする位置を 277〜388 で 1 つずつ走査）:
  - `crash_seqhdr`（シーケンスヘッダ OBU ペイロード先頭 279・280）→ `Err … seq_profile`。abort しない
  - `crash_framehdr`（frame OBU ペイロードの 3・4 バイト目 292・293）→ `Ok`。abort しない
  - frame OBU ペイロード先頭（290・291）と spike の 299・300 → いずれも `Ok`。シーケンスヘッダ・OBU ヘッダの範囲（277-289）と frame ペイロード先頭（290-296）は abort しない。abort するのはタイルデータの範囲（297, 298, 301, 305, … 340, … 384）だけ
  - spike のベースは今のどの版とも一致しない。Pillow 12.0.0 / 11.3.0（libavif 1.3.0）のベースは 342 バイトで中身が違い、そちらでは 299・300 は abort するが、frame ペイロード先頭は `Ok`
- `p8_420_lim_709_a420` と `p8_420_lim_709_a8` は出力が同じ（主画像が同じで、アルファの Y 面も同じになり、クロマはアルファに使われない）。a420 の役割はクロマを持つアルファの経路（YuvPlanes8）を通すことで、別の面を読めば一致しなくなるので意味は保たれる
- 差し替え前の出力 `docs/tmp/avif-baseline/*.bin`（20 個、`.gitignore` 済み）の sha256 先頭 16 桁は task-2-report に残した
- ffmpeg の癖: testsrc2 は寸法を偶数に丸める / RGB→gray は範囲を pc に固定する / `-colorspace ycgco` ではエンコーダが開けないので `-aom-params matrix-coefficients=ycgco` を使う（`p8_444_lim_601` と `_ycgco` は YUV データが同じで行列の値だけ違う） / gbrp では CP=1・TC=13 が自動で付き範囲は full に固定される

### Task 3: ガードを外したとき

1. `declared` の数の確認を外す: `avif_with_two_sequence_headers_in_one_item_is_rejected` が落ちる。`two_seqhdr.avif` が `Ok`（`expect_err` で panic）
2. アルファの `reserve` を外す: `avif_alpha_is_counted_against_the_output_limit` が落ちる。probe は `Err … invalid alpha size`、経過 2:59.39、ピーク RSS 818,296 KB。前提 assert（64x64）は通る。テストの経過は 105.84 秒
3. 主画像の `reserve` を外す: `avif_over_the_output_limit_is_rejected_before_decoding` が落ちる。`over_limit.avif` は `Ok 14000x13000 Rgb8`（実際にデコードされ、23.3 秒、ピーク RSS 1,091,828 KB）
- ガードがあるとき: alpha_huge は 0.00 秒・7.8MB で `AVIF declares a size over the 512MB output limit: Memory limit exceeded`。over_limit も同じ文言。two_seqhdr は `exactly one sequence header (found 2)`
- RED は厳密には踏んでいない（実装を先に書いてからテストを足した）。旧 `short_payload`（OBU サイズ欄は正しく中身だけ足りない）は数え上げでは Err にならない（数は 1）ので、テストを「数は 1 で `declared` が Err」に直した

### Task 4: ガードを外したとき

- `cargo tree -p picture-tool-core -i rav1d`:
  ```
  error: package ID specification `rav1d` did not match any packages

  help: a package with a similar name exists: `dav1d`
  ```
- crash.avif（システムの libdav1d 1.5.4、動的リンク）: `Err Failed to open image: …/crash.avif: AV1 decoding failed: Invalid argument`、exit=1。内部ビルドの probe でも同じ。他のフィクスチャも全件 probe に通した（20 枚・base・orient_1..8・exif_o6 は Ok、over_limit・alpha_huge は 512MB の Err、two_seqhdr は `exactly one sequence header (found 2)`）
- 内部ビルド: meson 1.12.1 / ninja 1.13.2（uv の venv）を PATH に足し、`SYSTEM_DEPS_DAV1D_BUILD_INTERNAL=always cargo test -p picture-tool-core --target-dir target/internal-dav1d` が exit 0（code.videolan.org からの clone も成功）。`ldd` は `libgcc_s` / `libm` / `libc` / `ld-linux` だけで `libdav1d.so` なし（通常ビルドの probe は `libdav1d.so.7 => /lib64/libdav1d.so.7`）。`nm` で `T dav1d_open` がバイナリ内に定義されている
- ガードを外したとき:
  1. `set_frame_size_limit` の呼び出しを外す → `frame_size_limit_is_enforced_by_the_decoder` が FAILED。`decode(.., 2047)` が `Ok(Picture { .. w: 64, h: 32 .. })` を返して `unwrap_err()` で panic。ガードがあるときの 2047 は `AV1 decoding failed` の Err
  2. `drain` の周回の上限を外す（`loop`）→ `a_stalled_decoder_is_cut_off` が FAILED（0.00s、`drain did not stop at 216 rounds`）。初回の測定は偽の `get` に自分で止まる仕組みがなく、i32 のカウンタが 2^31 回で overflow して 14.83s で panic した（上限の欠落ではなくカウンタの overflow で落ちていた）ので、Fix round 1 でテストを直した
  3. アルファの `reserve` を外す（alpha_huge）→ 理由が変わる。probe は `Err … AV1 decoding failed: Unknown error -34`（0.00s、最大 RSS 7.5MB。libdav1d が stderr に `Frame size 23200x23200 exceeds limit 4096`。4096 は `min(アルファの宣言, 主画像の宣言 64x64)`）、`avif_alpha_is_counted_against_the_output_limit` が FAILED。差し替え前は同条件で約 3 分・818MB
- Fix round 1（アルファの寸法）: `write_alpha` が幅と高さの両方を比べる（`invalid alpha size`）。単体テスト `alpha_of_a_different_size_is_rejected`（dav1d の `Picture` は公開コンストラクタがないので、平面の幾何を `Geometry` に分けて dav1d なしで呼べるようにした）。RED は修正前の実装で `unwrap_err()` on an `Ok` value。高さの比較だけを外すと 2x1 で `range end index 2 out of range for slice of length 0` の panic になる（範囲外読み出しの防止にもなっている）
- 画素一致の予備確認: スカラー（コミットする設定）は 20 件中 7 件が差し替え前と一致、13 件は違う。`yuv` を一時的に既定 feature（SIMD）にすると 20/20 がバイト単位で一致（Task 5 で正式に確認）
- avif-decode との順序の違い: 主画像の変換とアルファのデコードが両方失敗するファイルでは、出る理由が違いうる（本実装はアルファを先にデコードする）。アルファは Y 平面だけを読む（avif-decode は U/V まで検査していたが、libdav1d の出力で落ちることはない）。深度 16 は avif-decode が受け入れていたが 8/10/12 以外は Err（AV1 は 12bit まで）

### Task 5: 移植の照合

- `core/Cargo.toml` の `yuv` を一時的に `yuv = "=0.8.19"`（既定 feature。`cargo tree -e features -i yuv` で `avx` が有効なことを確認）にして `avif_probe --dump` を 20 件走らせ、`docs/tmp/avif-baseline/*.bin` と `cmp`: **20/20 すべて同一**。A-6（アルファ別デコーダ）と A-1（two_frames）も一致。`git checkout core/Cargo.toml` で戻し、差分は空
- 参考（スカラー経路）: 同じ 20 件を差し替え前と比べると **13 件が不一致、7 件（グレー・identity・ycgco・p12 など）が一致。** 不一致の大きさは 8bit が最大 ±2、two_frames は ±1、10bit は 16bit 値で最大 192、a8_premul は最大 128。SIMD とスカラーの丸めの違いで、spec が「SIMD は CPU で出力が変わるのでスカラーに固定」とした前提どおり。常設ハッシュはスカラー出力を固定する。スカラー側が差し替え前と一致するとは主張しない
- 色型の Debug 名はグレーが `L8` / `L16`（`Luma8` ではない）。テストの色型文字列と FNV 入力はこの名前。全 20 件が先に書いた色型・33x17 で通った。a8 と a420 のハッシュが同一（0xe33cd2ae824fa3c0）なのは Task 2 のとおり

### Task 5: 罠を戻したとき

各罠は `yuv.rs` / `av1.rs` に 1 箇所だけ入れ、`avif_probe --dump` で全 20 件の失敗集合を取り、テストの最初の失敗メッセージも取った。入れた変更は戻し済み。

1. 行列「なし」を BT.709（`matrix()` の `Unspecified | Reserved => Matrix::Bt709`）: 落ちたのは `p8_420_lim_unspec.avif` と `p10_444_full_unspec.avif` のみ（予測どおり。グレーは落ちない）。`p8_420_lim_unspec.avif: 0xf44d45f0329eb1b2`
2. アルファの未指定を値として比べる（アルファ側を `unwrap_or(Bt601)` で `Some` 扱い）: 予測は a420 の 1 件だったが、実際は **7 件**（a8_on_gray8, a8_premul, p10_420_lim_709_a10, p10_420_lim_709_a8, p8_420_lim_709_a10, p8_420_lim_709_a420, p8_420_lim_709_a8）が `alpha image has color info` で Err。アルファ付きの 7 件はいずれもアルファの行列が未指定（2）で、主画像が 709 または未指定のため、「なし」を値扱いすると全部が拒否される（a420 だけが未指定アルファではない）。テストは `open_image_oriented(..).unwrap()` の panic として最初の 1 件で落ちる
3. 16bit 化を左シフトだけ（`*v <<= shift`）: 落ちたのは p10_420_lim_709, p10_420_lim_709_a10, p10_420_lim_709_a8, p10_444_full_unspec, p10_444_full_identity, p12_422_lim_601（予測どおり。gray10_lim は `luma16`（全域スケール）を通るので落ちない）。`p10_420_lim_709.avif: 0x02d843eb0d47ceb4`
4. `drain` が最初の 1 枚で返す（`Ok(picture) => return Ok(picture)`）: 落ちたのは `two_frames.avif` のみ。`two_frames.avif: 0xe5fd19e577827d85`。0 件の罠はなし

### Task 8: ファズ

- スクリプト: `core/tests/fixtures/fuzz_avif.py`。種は `--seed`（既定 20261019、実行時に表示）。元ファイルごとに独立の乱数列（種 XOR ファイル名の crc32）なので、引数の順序や個数で結果が変わらない
- 実行（ローカル、debug の probe、約 6 秒）: `uv run --no-project python core/tests/fixtures/fuzz_avif.py target/debug/examples/avif_probe <scratch>/fuzz <元 13 枚>`
- 元ファイル 13 枚（深度・間引き・アルファ・フレーム数を散らした）: p8_420_lim_unspec, p8_444_lim_ycgco, p8_422_full_709, p10_420_lim_709, p10_444_full_identity, p12_422_lim_601, gray8_full, gray10_lim, p8_420_lim_709_a8, p10_420_lim_709_a10, a8_premul, two_frames, p8_444_full_fcc。各 130 件、1〜4 バイトをランダムに書き換え
- 結果: **`TOTAL ok=591 err=1099 abort=0 signal=0 timeout=0 n=1690`**。abort・signal・timeout は 0 件。ファイルごとの Ok / Err（61/69, 69/61, 27/103, 64/66, 47/83, 12/118, 44/86, 41/89, 45/85, 53/77, 53/77, 10/120, 65/65）
- 参考: 元ファイルは 500〜1000 バイトの小さなもので、ランダムな書き換えでは Err が 65% を占める。abort を再現した書き換え（crash.avif）は Task 4 以降 `Err … AV1 decoding failed: Invalid argument` になる（ファズが abort を検出できる条件の確認は Task 2 の差し替え前の abort による）

### 完了条件の確認（Task 8 Step 2、ローカル分）

- `cargo tree -p picture-tool-core -i rav1d` → `error: package ID specification 'rav1d' did not match any packages`（rav1d はどこからも引かれていない）
- `make check` → exit 0
- `cd gui-frontend && bun run e2e` → 110 passed（2.0m）
- **未実施**: push して CI（内部ビルドを含む）が通ること / 利用者が Windows で `jxl_avif_input` を通すこと（Task 1 と同じ理由で未実施）。画素一致が Windows の CPU でも同じハッシュになるかは未確認

### spec と食い違った点

- **A-1（最後の 1 枚）**: spec は「最初に出てきた 1 枚」。avif-decode は最後の 1 枚（レイヤー付きは最後が完全な画像で、フィルムグレインも最後にだけ掛かる）。最後を返す。Task 5 の罠 4（最初の 1 枚で返す）で `two_frames.avif` だけが落ちることを実測
- **A-2（周回の上限）**: 「バイト数 + 16」ではなく `2 × バイト数 + 16`。最後まで取り続けるため、「フレームが出る」周回でも終わらなくなる。正常な周回は「1 バイト以上消費」か「1 枚出る」のどちらかで、どちらもバイト数以下の回数。有限で必ず終わる性質は同じ
- **A-3（寸法上限のテスト）**: 13000×13000 の 8bit RGB は 507,000,000 バイトで上限 536,870,912 を下回り通る。フィクスチャは 14000×13000（546,000,000 バイト）
- **crash.avif の位置**: spec は frame OBU ペイロード先頭の 2 バイト（spike では 299・300）。今のベース（Pillow 12.3.0 / libavif 1.4.2）ではどちらも `Ok` で、abort するのはタイルデータの書き換え（340・341、元 0x18,0xEF）。同じ rav1d `decode.rs:4997` の箇所で落ちる。crash.avif はタイルデータにした
- **アルファの寸法の検査**: spec どおり幅と高さの両方を比べる。avif-decode は行の幅だけを比べていた（高さが違っても Err にならず残りの行は仮置きのアルファ）。意図した違いで、正常なファイルには影響しない
- **`drain` の後段エラー**: 既に 1 枚取れた後に `get_picture` が `Again` 以外のエラーを返すと `Err` にする（avif-decode は取れていた最後の 1 枚を `Ok` で返していた）。spec「dav1d が返す他のエラーはすべて `Err`」どおりで、壊れた後段レイヤーを黙って返さないため意図どおり
- **Windows の確認（Task 1）は未実施**（利用者判断）。spec の完了条件は未達
