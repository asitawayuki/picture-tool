# AVIF デコーダの差し替え 設計

**日付**: 2026-10-14
**対象**: `core/src/decode/`（AVIF のデコード経路）、`core/Cargo.toml`、CI、README のビルド要件と既知の制限
**前提**: JPEG XL / AVIF 入力対応（`f1c88fd`〜`f3f8d26`、マージ `593a630`）。同機能には spec がなく、
設計意図はコミット本文と `core/src/decode/mod.rs` 冒頭のモジュールコメントにある
**姉妹 spec（未作成）**: メモリ予算（JPEG XL の作業領域上限が実効的でない・一括変換の並列数が無制限）。
本 spec の範囲外。理由は「スコープ」

## 背景と目的

JPEG XL / AVIF 入力対応のマージ後レビュー（2026-10-14）で、**壊れた AVIF 1 枚でプロセスごと abort する**ことが
実測で確認された。

- 現行は avif-parse 2 で箱を読み、avif-decode 3.0.0（内部で rav1d 1.1.0。dav1d の Rust 移植）で AV1 をデコードする
- 正常な 317 バイトの AVIF の frame OBU ペイロード先頭 2 バイトを `0xFF` にすると、
  rav1d-1.1.0 `src/decode.rs:4997` の `try_write().unwrap()` が panic する。呼び出し元
  `dav1d_send_data` が `extern "C"` なので巻き戻せず、`thread caused non-unwinding panic. aborting.`
  （終了コード 134）。**呼び出したスレッドでの panic で、`catch_unwind`（`guarded`）は効かない**
- 影響: 一括変換は他のファイルの結果を 1 件も返さずに終わる。GUI はサムネイル表示だけで落ちる
- 頻度（実測）: 正常 AVIF 10 種 × 130 件をランダムに 1〜4 バイト破壊した 1300 件で、現行 core は
  **abort 209 / Err 766 / Ok 325**。同じ方法の JPEG XL 300 件では abort 0
- rav1d 1.1.0・avif-decode 3.0.0 とも 2026-10-14 時点で crates.io の最新版。更新では直らない
- `decode/mod.rs` の `guarded` のコメント「rav1d は…入力から panic に至る経路は確認できていない」は
  この実測と食い違う。前回のマージ時に受け入れた残リスク（「ワーカースレッドの panic」）とも前提が違う

**目的**: 壊れた・細工された AVIF を、プロセスを落とさず **1 枚の失敗（`Err`）** として扱う。
正常な AVIF の出力は現行と**画素単位で一致**させる。

## 決定事項と、捨てた案

| 論点 | 決定 | 捨てた案と理由 |
|---|---|---|
| デコーダ | **C 版 libdav1d を `dav1d` クレート（0.11）経由で使う**。avif-parse は残し、取り出した AV1 データを直接渡す（以下「方式 b」） | (a) `image` の `avif-native` feature: 壊れた入力で**無限ループ**（1300 件中 4 件が CPU 99% のまま終わらない。image 0.25.10 の `read_until_ready` がフレームが出ないと `get_picture`→Again→`send_pending_data` を永遠に繰り返す）。画素も一致しない（常に RGBA、未指定行列を BT.709 扱いで最大 53 階調ずれ、16bit 拡張が `rotate_left` で最大値 65472、アルファの範囲伸長・プリマルチプライド解除なし）。メモリ上限の確認がデコード後。/ 子プロセス隔離: abort・OOM・stderr をまとめて閉じ込められるが、CLI と GUI の両方に子プロセスモードと画素の受け渡しが要り、規模に見合わない。/ AVIF 非対応に戻す: 利用者の要望に反する |
| stderr | libdav1d が壊れた入力で stderr に 1 行書くことを**既知の制限として許容**（利用者判断 2026-10-14） | `dav1d-sys` を直接叩く自前ラッパーで logger を NULL にする: プロジェクトに初めて unsafe FFI（約 100 行）を持ち込み、解放漏れ・ポインタ寿命の正しさを自前で負う。`dav1d::Settings` の内部をキャストする方法は `#[repr(C)]` のない構造体のレイアウト依存のハックで不可 |
| 対象 OS | Linux と **Windows を実用対象**（利用者判断 2026-10-14）。macOS は README に手順を書くだけで検証しない | — |
| libdav1d の入手 | Linux はシステムの libdav1d（1.3 以上）を動的リンク。**Windows は `SYSTEM_DEPS_DAV1D_BUILD_INTERNAL=always` で静的ビルド**（配布物に DLL を同梱しない）。**CI は ubuntu-22.04 のまま内部ビルド** | vcpkg: 環境構築が重く、DLL 同梱か静的 triplet の選択が要る。/ CI を ubuntu-24.04 に上げて apt の libdav1d を使う: 22.04 は「webkit2gtk-4.1 が標準で手に入る最古の LTS」として固定している（`ci.yml` のコメント）。内部ビルドなら Windows と同じ仕組みを CI が毎回通す |
| スレッド | `n_threads = 1`、`max_frame_delay = 1` | 自動: 1 枚は速い（photo.avif 4000×3000 で 78〜90ms、スレッド 1 は 229〜239ms）が、一括変換は rayon でファイル単位に並列なので過剰生成になる（現行 rav1d で 40 枚のバッチに最大 421 スレッド）。JPEG XL もプールを使っていない（`jxl_decoder`）。なお **`max_frame_delay` を既定のままスレッド自動にするとフレームが出ず `no frame decoded` になる**（実測）ので、どの設定でも 1 は必須 |

## スコープ

### やること

1. AVIF の AV1 デコードを avif-decode（rav1d）から dav1d クレート（libdav1d）へ差し替える
2. avif-decode が担っていた YUV→RGB 変換を core に移植する
3. AVIF のメモリ見積もり（`reserve_avif`）を作り直す
4. 出力の丸ごとコピー（`avif_to_dynamic`）をなくす
5. 回帰テスト（abort・寸法上限・画素一致・シーケンスヘッダ複数）
6. ビルド要件・CI・README・コメントの更新
7. マージ後レビューの低優先度指摘（「低優先度の修正」の節）

### やらないこと

- **メモリ予算**（姉妹 spec）: JPEG XL の作業領域上限（`JXL_WORKING_BYTES_PER_PIXEL = 64`）が 39KB の
  ファイルで RSS 4.5GB を許す問題、一括変換（`process_batch` の `par_iter`）の並列数が無制限で
  24MP の JPEG XL 20 枚が 12.3GB になる問題。JPEG を含めた並列数・メモリ予算の設計問題で、AVIF の
  abort とは独立に完結するため分けた（利用者判断 2026-10-14）。本 spec 後も AVIF のデコーダ内部は
  「宣言寸法 × 定数」まで使いうる（「メモリの上限」の節）ので、その定数は姉妹 spec の予算に含める
- **libdav1d の stderr 出力を止めること**（上表）
- AVIF の切り抜き（`clap`）・グリッド・アニメーション（`avis`）への対応（現行どおり非対応）
- 主ブランドが `avif` 以外（`mif1` 等）で互換ブランドにだけ `avif` を持つファイルへの対応。
  現行どおり `sniff_bytes` は AVIF と判定せず、`image` 側で失敗する。README に制限として書くだけ
- ★レーティングを JPEG XL / AVIF から読むこと。grid-filtering spec の「非 JPEG は意図的に非対称にする」の
  範囲内。README に制限として書くだけ
- macOS の実機検証

## 設計

### モジュール構成

| ファイル | 役割 | 変更 |
|---|---|---|
| `core/src/decode/mod.rs` | 形式の振り分け、`guarded`、メモリの上限 | `Format::Avif` の経路を新しい 2 モジュールに繋ぎ替え。`reserve_avif` と `avif_to_dynamic` を置き換え |
| `core/src/decode/avif.rs` | ISOBMFF の、avif-parse が扱わない部分（`ispe`・`irot`/`imir`） | `max_frame_pixels` / `sequence_header_bound` を削除し、**シーケンスヘッダの数とその宣言寸法・ビット深度を返す関数**に置き換える（下記） |
| `core/src/decode/av1.rs`（新規） | dav1d で AV1 データを 1 フレームデコードする | — |
| `core/src/decode/yuv.rs`（新規） | dav1d の `Picture`（Y/U/V と別アイテムのアルファ）を `DynamicImage` に変換する | — |

`av1.rs` は「AV1 のバイト列 → dav1d の `Picture`」だけを持ち、色変換を知らない。`yuv.rs` は
`Picture` から `DynamicImage` を作るだけで、ファイルも dav1d の設定も知らない。どちらも
`decode` モジュールの外へは公開しない（`pub(super)`）。

### AV1 のデコード（`av1.rs`）

```text
fn decode(av1: &[u8], frame_size_limit: u32) -> Result<dav1d::Picture>
```

- `dav1d::Settings`: `set_n_threads(1)`、`set_max_frame_delay(1)`、`set_apply_grain(true)`（avif-decode と同じ）、
  `set_frame_size_limit(frame_size_limit)`
- `send_data` → `get_picture` を回し、**最初に出てきた 1 枚**を返す。`Error::Again` は「データをさらに送る／
  もう一度取る」の合図で、送るデータが尽きてもフレームが出なければ `Err("no frame decoded")`。
  **ループは送ったデータが尽きた時点で必ず終わる形にする**（方式 a の無限ループを繰り返さない）
- dav1d が返す他のエラーはすべて `Err`

`frame_size_limit` は「フレームの画素数（幅×高さ）」の上限で、超えるフレームは確保前に Err になる（実測:
13000×13000 の big169.avif に 169,000,000 なら Ok、168,999,999 なら即 Err、ピーク RSS 3.4MB）。
値は次節の「宣言寸法」から決める。

### メモリの上限

現行の `reserve_avif` は、宣言寸法での 512MB 判定に加え、全シーケンスヘッダから取りうる最大寸法を求めて
「4 倍まで」で判定していた。レビューでこれが実際の消費を表していないと指摘された:

- ビット深度は**先頭の**シーケンスヘッダの値なのに、寸法の最大は全シーケンスヘッダから取っている。
  先頭を 8bit、後続を 12bit・4:4:4 にすると出力が 2 倍に過小評価される
- 見積もりが出力 RGB だけで、デコーダ内部（ピクチャ本体、フィルムグレインの複製、参照フレーム最大 8 枚）を
  数えていない（コード読解。細工ファイルは作れていない）

**新しい形**: 判定を 2 段にし、フレームの寸法の上限はデコーダ自身に強制させる。

1. **デコード前**（`avif.rs` の AV1 データ解析）
   - 主画像とアルファの AV1 データに含まれるシーケンスヘッダ OBU を数える。**ちょうど 1 個でなければ Err**。
     根拠: av1-avif 仕様 §2.1「The AV1 Image Item Data shall have exactly one Sequence Header OBU.」
     （2026-10-14 に <https://aomediacodec.github.io/av1-avif/> で確認）。これで「後続のヘッダで
     ビット深度や寸法を引き上げる」細工の入口がなくなる
   - そのシーケンスヘッダの `max_frame_width × max_frame_height` を**宣言寸法**とし、
     ビット深度（8 / 10 / 12）とアルファの有無から出力バイト数を出して `Limits::default()`（512MB）で判定する
     （現行の 1 段目と同じ。ただし値の出どころを avif-parse の `primary_item_metadata` から自前の解析に寄せるかは
     実装で決めてよい。**どちらでも「ヘッダがちょうど 1 個」の確認は自前で行う**）
   - 宣言寸法を `frame_size_limit` として dav1d に渡す。AV1 の仕様上、`frame_size_override_flag` で書ける
     フレーム寸法はシーケンスヘッダの `max_frame_*` 以下なので、正常なファイルを落とさない
2. **変換前**（`yuv.rs` の入口）
   - 実際にデコードされた `Picture` のビット深度と、アルファの有無から出力バイト数を出し、もう一度
     `Limits::default()` で判定する。1 で弾けない取りこぼしがあっても、RGB バッファの確保前に止める

**残る上限**: デコーダ内部は「宣言寸法 × ビット深度 × 参照フレーム数」まで使いうる。av1-avif 仕様は
非表示フレームの数を制限していないので、正常なファイルでも理論上は参照フレームを持てる。これは
姉妹 spec（メモリ予算）の扱いとする。big169.avif（13000×13000、出力 507MB）のピーク RSS は方式 b で
808MB（現行 996MB）だった。

### YUV→RGB 変換（`yuv.rs`）

avif-decode 3.0.0 の `src/image.rs` の変換規則を移植する。**ライセンスは BSD-3-Clause なので、移植した
ファイルの冒頭に原著作権表示とライセンス文を残す**。変換本体は avif-decode も使っている `yuv` クレート
（0.8 系）を呼ぶ。

移植する規則（spike で現行と出力がバイト一致した規則）:

- 行列係数が未指定（2）のときは BT.601 として扱う
- dav1d クレートの `MatrixCoefficients` は値 4（FCC）を `BT470M`、値 5・6 を `BT470BG` という名前で返す。
  **名前ではなく値の意味で対応づける**（4 → FCC、5/6 → BT.601）
- YCgCo・Identity（Identity はクロマの間引きがあれば Err）・BT.2020 NCL/CL・SMPTE 240 を avif-decode と同じに扱う
- 10/12bit を 16bit に広げるときはビット複製（最大値が 65535 になる）
- グレー（モノクロ）とアルファは、限定レンジなら全レンジへ伸長する
- アルファがプリマルチプライドなら解除する
- 出力の画素形式は現行と同じ（8bit: `Rgb8`/`Rgba8`/`Luma8`、10/12bit: `Rgb16`/`Rgba16`/`Luma16`）

出力は `ImageBuffer` に直接書く。現行の `avif_to_dynamic` が `flat_map(..).collect()` で
avif-decode の RGB バッファを丸ごと複製していた分（出力 507MB でピーク 997MB）はなくなる。

**挙動が変わる点**: 行列係数が予約値 3 の AVIF は、dav1d クレートの `matrix_coefficients()` が
`unreachable!()` で panic する（ソース読解と実測）。呼び出したスレッドでの panic なので `guarded` が `Err` にする。
現行はこれを BT.601 として開いていたが、不正な値なので Err で良い。

### 依存関係

- `core/Cargo.toml`: `avif-decode` を外し、`dav1d = "0.11"` と `yuv = "0.8"` を足す。avif-parse は残す
- **avif-decode と rav1d は dev-dependencies を含めて完全に外す。** rav1d は `dav1d_open` などを同じ名前の
  C ABI で公開しており、同居させるとリンカーが黙って rav1d 側を使う（実測: そのビルドには `libdav1d.so` の
  NEEDED がなく、`nm` で `dav1d_open` がバイナリ内に定義されていた）。**完了条件として
  `cargo tree -i rav1d` が空であることを確かめる**
- コメント「入力専用の JPEG XL / AVIF デコーダ。どちらも純 Rust（C ライブラリ不要）」と、
  nasm を rav1d のためとする記述を直す。nasm は libdav1d の内部ビルド（x86/x86_64 のアセンブリ）で引き続き要る
- README の「Rust 1.98 以降（avif-decode の要件）」は、外した後の依存の要件で見直す

### ビルドと CI

| 環境 | libdav1d | ビルド時に要るもの |
|---|---|---|
| Linux（開発機・配布） | システムの libdav1d を動的リンク（`dav1d-sys` が pkg-config で 1.3 以上を要求） | `libdav1d-devel`（Fedora）/ `libdav1d-dev`（Debian/Ubuntu 24.04 以降） |
| Windows | `SYSTEM_DEPS_DAV1D_BUILD_INTERNAL=always` で dav1d を git clone して meson/ninja で静的ビルド（`dav1d-sys` 0.8.3 は tag `1.5.0` を取る） | meson、ninja、git、nasm、C コンパイラ（MSVC）、ビルド時のネットワーク（code.videolan.org） |
| CI（ubuntu-22.04） | Windows と同じ内部ビルド。22.04 の apt の libdav1d は 1.3 未満で使えない | apt で `meson`、`ninja-build` を追加。ジョブに `SYSTEM_DEPS_DAV1D_BUILD_INTERNAL: always` |
| macOS | `brew install dav1d`（動的リンク）。**未検証**。配布物への dylib 同梱は扱わない | — |

- Linux での内部ビルドは spike で成功済み（meson/ninja を uv の venv で導入。crash.avif が Err、photo.avif が現行と一致）
- **Windows のビルドは手元で検証できない。** 実装の最後に利用者が Windows でビルドし、
  `tests/jxl_avif_input.rs` が通ることを確認する。meson が MSVC で失敗する場合の代替（vcpkg の静的 triplet）は、
  そのとき別途判断する
- 内部ビルドはビルドのたびに外部の git から取る。`dav1d-sys` が tag を固定しているので版は揺れない

### エラー処理

| 事象 | 扱い |
|---|---|
| dav1d がエラーを返す | `Err`（1 枚の失敗） |
| データを送り切ってもフレームが出ない | `Err("no frame decoded")` |
| dav1d クレートや avif-parse が呼び出したスレッドで panic | `guarded` が `Err` に変える（現行どおり） |
| シーケンスヘッダが 0 個・2 個以上 | `Err`（デコード前） |
| 宣言寸法・実際の画像が 512MB を超える | `Err`（それぞれデコード前・変換前） |
| フレームが宣言寸法より大きい | dav1d の `frame_size_limit` で `Err` |
| libdav1d が stderr に書く | 許容（既知の制限） |

`guarded` のコメントを書き直す: rav1d の記述を消し、「libdav1d のワーカーは C のスレッドで Rust の panic は
起きない。dav1d クレート側の panic は呼び出したスレッドで起き、ここで捕まえる」とする。
`core/src/lib.rs` 冒頭の「core は `eprintln!` しない」には、外部の C ライブラリ（libdav1d）が壊れた入力で
stderr に書くことは止められない旨を例外として書く。

## テスト

**テストを書く・設計する前に `test-integrity` スキルを起動する**（グローバル指示）。

| テスト | 何を保証するか | ガードを外して落ちることの確認 |
|---|---|---|
| abort の回帰 | `crash.avif`（317 バイト）と、ランダム破壊で現行が abort したファイルのうち**壊れ方の違うもの数件**を開くと `Err` が返る | 現行（avif-decode）のまま走らせると、テストバイナリごと abort して落ちることを実測して記録する |
| 寸法上限 | 13000×13000 の AVIF（libaom で約 5.5KB）が、確保前に `Err` になる | 1 段目の判定を外すと、`frame_size_limit` も外したうえで落ちる（または RSS が跳ねる）ことを確かめる。**判定 1 段目と `frame_size_limit` のどちらが効いているかを区別できる組み立てにする** |
| シーケンスヘッダ複数 | シーケンスヘッダを 2 個持つ AV1 データの AVIF が `Err` になる | 数の確認を外すと通ってしまうことを確かめる |
| 画素一致 | 下記の正常系が、差し替え前の avif-decode の出力と一致する | 期待値は差し替え前に現行コードで生成して固定する。spike では YUV 変換の規則を 1 つ変えると一致しなくなることを確かめていないので、実装時に少なくとも「未指定行列を BT.709 にする」変更で落ちることを確かめる |
| 既存 | `tests/jxl_avif_input.rs` の 19 件（向き・Exif・切れたファイル等）がそのまま通る | — |

**画素一致の対象**（spike で一致を確かめた範囲。`make_jxl_avif.py` を拡張して生成する）:
8 / 10 / 12bit、4:2:0 / 4:2:2 / 4:4:4、グレー、全レンジ、行列（未指定・709・601・FCC・YCgCo・Identity）、
アルファ 8 / 10bit、奇数サイズ。期待値はフィクスチャ 1 枚につき画素のハッシュ（形式と寸法を含む）で持つ。

**ファズ（CI には入れない）**: 実装後に 1 回だけ、正常 AVIF をランダムに 1〜4 バイト破壊した 1300 件以上を
1 件ずつ子プロセスで開き、**abort・signal・timeout が 0 件**であることを計測して plan の実施メモに残す。
spike での方式 b の結果は abort 0 / panic 0 / Err 896 / Ok 404 で、両方が Ok になった 404 件は rav1d と
libdav1d の出力が全件バイト一致した。

## 文書

- README
  - 前提条件: nasm の理由の書き換え、OS ごとの libdav1d（上の表）、Windows の手順（meson・ninja・git の導入と
    `SYSTEM_DEPS_DAV1D_BUILD_INTERNAL=always`）
  - 既知の制限に追加: 壊れた AVIF で stderr に出力が出ることがある / ★は JPEG のみで、JPEG XL・AVIF は
    常に「未評価」になる / 主ブランドが `avif` 以外の AVIF は開けない
- `core/src/decode/mod.rs` 冒頭のモジュールコメント（メモリの上限の節）と `guarded` のコメント
- `core/src/lib.rs` 冒頭（stderr の例外）
- 古い記述: `docs/superpowers/specs/2026-09-08-grid-filtering-design.md:284` の `is_supported_image`
  （`jpg|jpeg|png|webp`）、`gui/src/security.rs:188` と `gui-frontend/src/App.svelte:140-142` の
  「非 JPEG の例が PNG / WebP だけ」
- `docs/README.md` の索引に本 spec と plan を登録する

## 低優先度の修正

- `core/src/decode/jxl.rs:111-114`: 本体が 4 バイト未満の `brob` で `read_exact` が次の箱のヘッダを食い、
  以降の箱の解析がずれる。影響は Exif が読めないだけ。本体長を先に確かめる
- `core/src/decode/mod.rs` の `jxl_decoder`: `width * height * JXL_WORKING_BYTES_PER_PIXEL` が u64 で
  桁あふれしうる（2^30 × 2^30）。`saturating_mul` にする（上限値そのものは姉妹 spec で見直す）

## 完了条件

- `make check` が通る（Linux、nasm・libdav1d-devel 導入済み）
- CI が通る（内部ビルド）
- `cargo tree -p picture-tool-core -i rav1d` が空
- ファズで abort・signal・timeout が 0 件（実施メモに記録）
- 利用者が Windows でビルドし、`cargo test -p picture-tool-core --test jxl_avif_input` が通る
