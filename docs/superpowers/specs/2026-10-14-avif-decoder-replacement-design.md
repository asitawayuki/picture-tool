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
| デコーダ | **C 版 libdav1d を `dav1d` クレート（0.11）経由で使う**。avif-parse は残し、取り出した AV1 データを直接渡す（以下「方式 b」） | (a) `image` の `avif-native` feature: 壊れた入力で**無限ループ**（1300 件中 4 件が CPU 99% のまま終わらない。image 0.25.10 の `read_until_ready` がフレームが出ないと `get_picture`→Again→`send_pending_data` を永遠に繰り返す）。画素も一致しない（常に RGBA、未指定行列を BT.709 扱いで最大 53 階調ずれ、16bit 拡張が `rotate_left` で最大値 65472、アルファの範囲伸長・プリマルチプライド解除なし）。メモリ上限の確認がデコード後。/ 子プロセス隔離: abort・OOM・stderr をまとめて閉じ込められるが、CLI と GUI の両方に子プロセスモードと画素の受け渡しが要り、規模に見合わない。/ rav1d を fork して直す（純 Rust のまま、C のビルド要件・stderr・CI の内部ビルドが要らない）: rav1d の Rust 側の API（`rav1d_send_data` 等）は `pub(crate)` で外から呼べるのは `extern "C"` の関数だけ。fork の保守に加えて、avif-decode の `rav1d.rs` 相当（約 270 行）の unsafe FFI ラッパーを自前で持つことになり、stderr の件で避けた unsafe を結局抱える。abort 209 件が単一箇所由来かも未確認で、1 箇所直して済む保証がない / AVIF 非対応に戻す: 利用者の要望に反する |
| stderr | 壊れた入力で外部ライブラリが stderr に書くこと（libdav1d のログと、`guarded` が捕まえた panic の既定 hook によるメッセージ）を**既知の制限として許容**（利用者判断 2026-10-14） | `dav1d-sys` を直接叩く自前ラッパーで logger を NULL にする: プロジェクトに初めて unsafe FFI（約 100 行）を持ち込み、解放漏れ・ポインタ寿命の正しさを自前で負う。`dav1d::Settings` の内部をキャストする方法は `#[repr(C)]` のない構造体のレイアウト依存のハックで不可 |
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
- **外部ライブラリの stderr 出力を止めること**（上表）
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
- `send_data` → `get_picture`（残りがあれば `send_pending_data`）を回し、**最初に出てきた 1 枚**を返す。
  `Error::Again` は「データをさらに送る／もう一度取る」の合図
- ループは次のどちらかで必ず終わる形にする（方式 a の無限ループを繰り返さない）
  - 送るデータが尽きてもフレームが出ない → `Err("no frame decoded")`
  - **1 周の間に、送り残りのバイト数が減らず、フレームも出なかった** → `Err`。dav1d クレートの
    `send_pending_data` は、データを一部しか消費できないと残りを保持したまま `Err(Again)` を返す
    （`dav1d-0.11.1/src/lib.rs` `send_pending_data`）。消費が止まったまま `get_picture` も `Again` を返し続けると
    「データが尽きない」ので、前者の条件だけでは終わらない。libdav1d（スレッド 1）が正常な入力でこの状態を
    一時的に通ることがあるかは確認していないが、あれば正常なファイルが Err になり、画素一致のテストとファズの
    Ok 件数（spike では 404 件）の減少で検知できる
- dav1d が返す他のエラーはすべて `Err`

`frame_size_limit` は「フレームの画素数（幅×高さ）」の上限で、超えるフレームは確保前に Err になる（実測:
13000×13000 の big169.avif に 169,000,000 なら Ok、168,999,999 なら即 Err、ピーク RSS 3.4MB）。
値は次節の「宣言寸法」から決める。

### メモリの上限

現行の `reserve_avif` は、宣言寸法での 512MB 判定に加え、全シーケンスヘッダから取りうる最大寸法を求めて
「4 倍まで」で判定していた。マージ後レビューでこれが実際の消費を表していないと指摘された:

- ビット深度は**先頭の**シーケンスヘッダの値なのに、寸法の最大は全シーケンスヘッダから取っている。
  先頭を 8bit、後続を 12bit・4:4:4 にすると出力が 2 倍に過小評価される
- 見積もりが出力 RGB だけで、デコーダ内部（ピクチャ本体、フィルムグレインの複製、参照フレーム最大 8 枚）を
  数えていない（コード読解。細工ファイルは作れていない）

**新しい形**: 判定を 2 段にし、フレームの寸法の上限はデコーダ自身に強制させる。

以下、**アイテム**は主画像とアルファ（あれば）の 2 つを指す。規則はアイテムごとに独立に当てる。

1. **デコード前**（`avif.rs` の AV1 データ解析）。次の順で行う
   1. **アイテムごとに**、その AV1 データに含まれるシーケンスヘッダ OBU を数え、**ちょうど 1 個でなければ Err**
      （合計ではない。アルファ付きの正常なファイルは合計 2 個になる）。
      根拠: av1-avif 仕様 §2.1「The AV1 Image Item Data shall have exactly one Sequence Header OBU.」
      （2026-10-14 に <https://aomediacodec.github.io/av1-avif/> で確認）。これで「後続のヘッダで
      ビット深度や寸法を引き上げる」細工の入口がなくなる
   2. アイテムごとに、そのヘッダの `max_frame_width × max_frame_height`（u64）を**そのアイテムの宣言寸法**とする
   3. **1 つの `Limits::default()`（512MB）に、主画像の出力（宣言寸法 × 3 チャンネル × 主画像のビット深度の
      バイト数）とアルファの出力（アルファの宣言寸法 × 1 チャンネル × アルファのビット深度のバイト数）を
      順に `reserve` する**。どちらかで超えれば Err。アルファを判定から外すと、主画像 64×64・アルファの
      宣言 65536×65536 の数 KB のファイルで、アルファのデコードが最大 4G 画素を確保する
   4. 3 を通過した後で、**各アイテム自身の宣言寸法**を `u32::try_from` で変換し（失敗は Err）、
      そのアイテムのデコードの `frame_size_limit` として dav1d に渡す。順序が重要: `frame_size_limit` は `u32` で
      **0 が無制限**を意味し（dav1d 0.11.1 `set_frame_size_limit(u32)`）、`max_frame_*` は最大 65536 なので
      積は 2^32 になりうる。3 を先に通せば 2^32 には届かない
   - AV1 の仕様上、`frame_size_override_flag` で書けるフレーム寸法はシーケンスヘッダの `max_frame_*` 以下なので、
     正常なファイルを `frame_size_limit` で落とさない
   - 値の出どころ（avif-parse の `primary_item_metadata` / `alpha_item_metadata` か自前の解析か）は実装で決めてよい。
     **どちらでも「ヘッダがちょうど 1 個」の確認は自前で行う**
2. **変換前**（`yuv.rs` の入口）
   - 実際にデコードされた主画像の `Picture` の寸法・ビット深度と、アルファの有無から出力バイト数を出し、もう一度
     `Limits::default()` で判定する。1 で弾けない取りこぼしがあっても、RGB バッファの確保前に止める
   - アルファの `Picture` の寸法が主画像と違えば Err（avif-decode の `invalid alpha size` と同じ。下記）

**残る上限**: デコーダ内部は「宣言寸法 × ビット深度 × 参照フレーム数」まで使いうる。av1-avif 仕様は
非表示フレームの数を制限していないので、正常なファイルでも理論上は参照フレームを持てる。これは
姉妹 spec（メモリ予算）の扱いとする。big169.avif（13000×13000、出力 507MB）のピーク RSS は方式 b で
808MB（現行 996MB）だった。

### YUV→RGB 変換（`yuv.rs`）

avif-decode 3.0.0 の変換を移植する。移植元は `src/lib.rs` の `to_image`、`src/image.rs`、`src/rav1d.rs` の
`matrix_coefficients`。**ライセンスは BSD-3-Clause なので、移植したファイルの冒頭に原著作権表示とライセンス文を残す**。
変換本体は avif-decode も使っている `yuv` クレートを呼ぶ。**`yuv = { version = "=0.8.19", default-features = false }`
とし、SIMD を使わないスカラー経路に固定する。**

- **理由: `yuv` の出力は実行する CPU で変わる**（2026-10-14 実測。同じ入力を既定 feature＝AVX2 経路、`sse` だけ、
  feature なし＝スカラーの 3 通りでビルドして比べた）。AVX2 とスカラーは 8bit で最大 2 階調、10bit を 16bit に
  広げた値で最大 129 違い、画素の 2〜4 割が一致しない。AVX2 と SSE も 8bit で最大 2 階調違う。
  現行の avif-decode も既定 feature で `yuv` を使うので、**現行の出力自体が CPU によって違う**
- スカラーなら Linux・Windows・CI のどの CPU でも同じ写真が同じ画素になり、画素一致のテストを
  ハッシュで厳密に照合できる。代償は速度で、photo.avif（4000×3000）のデコード全体が約 230ms → 約 257ms（約 12%）。
  一括変換はファイル単位で並列なので、全体への影響はこれ以下
- 版を `=` で固定するのは、avif-decode を外すと lockfile 以外に版を縛るものがなくなり、`cargo update` だけで
  丸めが変わってハッシュが変わりうるため

**`yuv.rs` が dav1d の `Picture` から読んでよいのは、寸法・`pixel_layout`・`bit_depth`・`matrix_coefficients`
（下記の扱いで）・`color_range` と各平面だけ。** dav1d 0.11.1 の `color_primaries()`・`transfer_characteristic()`・
`chroma_location()` は予約値で `unreachable!()` に落ちる（`dav1d-0.11.1/src/lib.rs` の各アクセサ）。
avif-decode はこれらを読まないので、参照すると現行で開けたファイルが Err になる。

移植する規則（移植元の関数と、Ok/Err が分かれる条件）:

| 規則 | 移植元 | 内容 |
|---|---|---|
| 行列係数の正規化 | `rav1d.rs` `matrix_coefficients` | 値 0（Identity）・1（709）・4（FCC）・5/6（BT.601 系）・7（240）・8（YCgCo）・9/10（BT.2020 NCL/CL を NCL として）・11〜14 を対応する種別に、**それ以外（2 = 未指定と予約値）は「なし」**にする。dav1d クレートは未指定を `Unspecified` という**値**で返し、値 4 を `BT470M`、5・6 を `BT470BG` という名前で返すので、**名前ではなく値の意味で対応づける**。予約値 3 はアクセサ自体が panic する（「挙動が変わる点」） |
| カラーの変換行列 | `image.rs` `conversion`、`yuv_to_rgb8` / `yuv_to_rgb16` | 「なし」は BT.601。Identity → GBR、YCgCo、709、FCC、BT.601 系 → BT.601、240、BT.2020 NCL。**SMPTE 2085・Chromaticity-derived NCL/CL・ICtCp は Err**（`Unsupported("matrix coefficients")`） |
| Identity と間引き | `image.rs` `convert8` / `convert16` | Identity でクロマの間引き（4:2:0 / 4:2:2）があれば Err |
| カラーの 16bit 化 | `image.rs` `convert16` の後処理 | 10/12bit を 16bit に広げるときはビット複製（最大値が 65535 になる） |
| グレーの値 | `image.rs` `luma8` / `luma16`、`yuv_to_gray8` / `yuv_to_gray16` | 範囲によらず `luma16`: `v' = (min(sat_sub(v, min), span) × 65535 + span/2) / span`（全レンジは min=0・span=2^bits−1、限定レンジは min=16≪(bits−8)・span=219≪(bits−8)）。**ビット複製ではない**。8bit は `luma8` |
| アルファの行列係数の検査 | `lib.rs` `to_image` | 主画像の行列（「なし」なら Identity とみなす）と、アルファの行列が「あり」かつ Identity でないとき、両者が違えば Err（`alpha image has color info`）。**アルファの「なし」（未指定を含む）は検査を素通りする**。素直に `Unspecified` を値として比べると「主画像 709・アルファ未指定」の普通のファイルが Err になる |
| アルファの寸法 | `image.rs` `write_alpha8` / `write_alpha16` | 主画像と寸法が違えば Err（`invalid alpha size`） |
| アルファの値と深度 | `image.rs` `write_alpha8` / `write_alpha16`、`luma16` | 出力形式は**主画像の深度**で決まる。アルファは**アルファ自身の深度と範囲**で変換する: 8bit→8bit 出力は `luma8`、10/12bit→8bit 出力は `luma16(..) >> 8`、10/12bit→16bit 出力は `luma16`、8bit→16bit 出力は `luma16(u16::from(y), Depth8, ..)` |
| プリマルチプライドの解除 | `image.rs` `unpremultiply8` / `unpremultiply16` | a=0 と a=最大値は触らない。それ以外は `min(c × 最大値 / a, 最大値)` の整数式 |
| 出力の画素形式 | `image.rs` | 8bit: `Rgb8`/`Rgba8`/`Luma8`（グレー＋アルファは `Rgba8`）、10/12bit: 同じく 16bit 版。現行の `avif_to_dynamic` と同じ |

出力は `ImageBuffer` に直接書く。現行の `avif_to_dynamic` が `flat_map(..).collect()` で
avif-decode の RGB バッファを丸ごと複製していた分（出力 507MB でピーク 997MB）はなくなる。

### 挙動が変わる点

現行で開けていて、本 spec の後に開けなくなるファイル。どちらも仕様上不正なファイルなので Err で良いが、
README の既知の制限に載せる。

- **行列係数が予約値 3 の AVIF**: dav1d クレートの `matrix_coefficients()` が `unreachable!()` で panic する
  （ソース読解と実測）。呼び出したスレッドでの panic なので `guarded` が `Err` にする。現行は BT.601 として開いていた
- **アイテムの AV1 データにシーケンスヘッダが 2 個以上ある AVIF**: 現行の `max_frame_pixels` は最大値を取って
  受け入れていた。仕様違反（av1-avif §2.1）で、普通のエンコーダは書かない（spike で aom・rav1e・SVT-AV1 の出力を
  確認）。書き出すアプリが実在するかは確かめられないが、実在しても起きるのはそのファイル 1 枚の読み込み失敗で、
  プロセスは落ちない。受け入れると「後のヘッダで深度や寸法を引き上げる」細工への対策を別に要するので、拒否を選ぶ。**0 個のものは現行でも開けない**（avif-parse の `primary_item_metadata` がアイテムのデータを解析して
  失敗し、avif-decode も `av1C` の設定 OBU をデコーダに渡さない）ので、変わるのは 2 個以上だけ

### 依存関係

- `core/Cargo.toml`: `avif-decode` を外し、`dav1d = "0.11"` と `yuv = { version = "=0.8.19", default-features = false }` を足す（理由は「YUV→RGB 変換」）。avif-parse は残す
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
| Linux（開発機・配布） | システムの libdav1d を動的リンク（`dav1d-sys` が pkg-config で 1.3 以上を要求） | `libdav1d-devel`（Fedora）/ `libdav1d-dev`（Ubuntu 24.04 以降。Debian 12 の dav1d 1.0 は要件を満たさない） |
| Windows | `SYSTEM_DEPS_DAV1D_BUILD_INTERNAL=always` で dav1d を git clone して meson/ninja で静的ビルド（`dav1d-sys` 0.8.3 は tag `1.5.0` を取る） | meson、ninja、git、nasm、C コンパイラ（MSVC）、**pkg-config の実行ファイル**、ビルド時のネットワーク（code.videolan.org）。環境変数はビルドのたびに要る |
| CI（ubuntu-22.04） | Windows と同じ内部ビルド。22.04 の apt の libdav1d は 1.3 未満で使えない | apt で `meson`、`ninja-build` を追加。ジョブに `SYSTEM_DEPS_DAV1D_BUILD_INTERNAL: always` |
| macOS | `brew install dav1d`（動的リンク）。**未検証**。配布物への dylib 同梱は扱わない | — |

- Linux での内部ビルドは spike で成功済み（meson/ninja を uv の venv で導入。crash.avif が Err、photo.avif が現行と一致）
- **内部ビルドでも pkg-config の実行ファイルが要る。** `dav1d-sys-0.8.3/build.rs` は meson で install した後に
  `system_deps::Library::from_internal_pkg_config` を呼び、system-deps 7.0.8 の同関数は `PKG_CONFIG_PATH` を差し替えて
  `pkg_config::Config::probe`、つまり pkg-config を起動する。Windows の標準環境にはないので要件に含める。
  CI（ubuntu-22.04）には入っているので、CI ではこの欠落を検知できない
- **`build.rs` は外部コマンドの終了コードを見ない**（`runner!` マクロが `.output().expect(..)` だけ）。git clone や
  meson setup が失敗しても先へ進み、最後に「pkg-config が dav1d を見つけない」という見当違いのエラーで止まる。
  README の Windows 手順に「このエラーが出たらビルドログを遡って git / meson の失敗を探す」と書く
- `SYSTEM_DEPS_DAV1D_BUILD_INTERNAL=always` は `.cargo/config.toml` の `[env]` には置かない（`[env]` はターゲットで
  分岐できず、Linux 開発機の動的リンクと両立しない）。README の Windows 手順でシェルでの設定方法を示す
- **Windows で成り立つかを本体の実装より前に確かめる。** plan の最初の工程は「Windows で `dav1d` クレートだけを
  依存に持つ最小のクレートを内部ビルドし、`dav1d::Decoder::new()` が通る」ことの確認とし、利用者が手元で行う。
  通らなければ、その時点で代替（vcpkg の静的 triplet 等）を判断する。本体を作り終えてから判断すると、
  依存の宣言・README・CI のすべてが手戻りになる
  - この工程で確かめること: meson が作る静的ライブラリを rustc が見つけられるか、meson が Developer Command Prompt の
    外（cargo の build script）から起動されたときに MSVC の環境を自分で整えるか。どちらも Linux からは確かめられない
- **残るリスク: 静的ビルドの libdav1d の版は `dav1d-sys` の tag（0.8.3 は `1.5.0`）に縛られる。** 本 spec は
  壊れた・細工されたファイルを C のデコーダに渡す前提で、C の不具合はメモリ破壊として現れうる。Linux（動的リンク）は
  ディストリの更新で修正が入るが、Windows の配布物と CI は `dav1d-sys` が tag を上げるまで留まる。上流の修正を
  急いで取り込む必要が出たら、`[patch]` で `dav1d-sys` を差し替えるか vcpkg に切り替える。
  1.5.0 以降に重要な修正が出たかは未確認

### エラー処理

| 事象 | 扱い |
|---|---|
| dav1d がエラーを返す | `Err`（1 枚の失敗） |
| データを送り切ってもフレームが出ない | `Err("no frame decoded")` |
| dav1d クレートや avif-parse が呼び出したスレッドで panic | `guarded` が `Err` に変える（現行どおり） |
| アイテムごとのシーケンスヘッダが 0 個・2 個以上 | `Err`（デコード前） |
| アルファの寸法が主画像と違う／アルファの行列係数が主画像と食い違う | `Err`（変換時。avif-decode と同じ） |
| 宣言寸法・実際の画像が 512MB を超える | `Err`（それぞれデコード前・変換前） |
| フレームが宣言寸法より大きい | dav1d の `frame_size_limit` で `Err` |
| libdav1d がログを stderr に書く／`guarded` が捕まえた panic のメッセージが既定の panic hook で stderr に出る | 許容（既知の制限） |
| 送り残りが減らずフレームも出ない周回 | `Err`（無限ループの打ち切り） |

`guarded` のコメントを書き直す: rav1d の記述を消し、「libdav1d のワーカーは C のスレッドで Rust の panic は
起きない。dav1d クレート側の panic は呼び出したスレッドで起き、ここで捕まえる」とする。
`core/src/lib.rs` 冒頭の「core は `eprintln!` しない」には、**外部ライブラリが壊れた入力で stderr に書くこと
（libdav1d のログ、`guarded` が捕まえた panic のメッセージ）は止められない**旨を例外として書く。
panic のメッセージは avif-parse の `debug_assert!` で既に起きうるが、本 spec で予約値 3 の経路が加わる。

## テスト

**テストを書く・設計する前に `test-integrity` スキルを起動する**（グローバル指示）。

| テスト | 何を保証するか | ガードを外して落ちることの確認 |
|---|---|---|
| abort の回帰 | 現行で abort するファイル（選び方は下記）を開くと `Err` が返る | 現行（avif-decode）のまま走らせると、テストバイナリごと abort して落ちることを実測して記録する |
| `frame_size_limit` の配線（`av1::decode` の単体テスト） | 13000×13000 の AV1 データに `frame_size_limit = 169,000,000 − 1` を渡すと即 `Err`、`169,000,000` を渡すとデコードが始まる（spike の実測をそのままテストにする） | `set_frame_size_limit` の呼び出しを外すと前者が `Err` にならないことを確かめる |
| 宣言寸法の取り出し（`avif.rs` の単体テスト） | アイテムごとの宣言寸法として、シーケンスヘッダの `max_frame_width × max_frame_height` が返る。`open` が `frame_size_limit` に渡す値はこれ | — |
| 寸法上限（512MB） | 13000×13000 の AVIF（libaom で約 5.5KB）が、デコード前の判定で `Err` になる | 判定を外すとデコードが走ってしまう（RSS が跳ねる、または変換前の判定で落ちる）ことを確かめる。**`frame_size_limit` はこのファイルでは効かない**（宣言寸法と実フレームが同じなので上限ちょうどで通過する）ので、`frame_size_limit` の確認は上の単体テストが担う |
| アルファの宣言寸法 | 主画像が小さく、アルファのシーケンスヘッダの宣言だけが巨大な AVIF が、デコード前に `Err` になる | アルファを 512MB 判定から外すと通ってしまうことを確かめる |
| シーケンスヘッダ複数 | 主画像の AV1 データにシーケンスヘッダを 2 個持つ AVIF が `Err` になる。アルファ付きの正常なファイル（合計 2 個）は開ける | 数の確認を外すと前者が通ってしまうことを確かめる |
| 画素一致（常設） | 下記の正常系の出力（スカラー経路）が、コミットした期待値のハッシュと厳密に一致する | 「未指定行列を BT.709 にする」「アルファの `Unspecified` を値として比べる」「16bit 化をビット複製でなく左シフトにする」の各変更で落ちることを確かめる |
| 移植の正しさ（1 回だけ・実施メモに記録） | 移植したコードを一時的に `yuv` の既定 feature（AVX2 経路）でビルドすると、同じ機械で差し替え前の avif-decode の出力と**バイト一致**する | — （avif-decode という独立した実装を正解として使う。常設テストの期待値は、この照合を通ったコードのスカラー経路で作る） |
| 既存 | `tests/jxl_avif_input.rs` の 19 件（向き・Exif・切れたファイル等）がそのまま通る | — |

**abort の回帰に使うファイルの選び方**: abort した panic 箇所ごとに 1 件を基本とする。spike では現行の abort は
全件 rav1d-1.1.0 `src/decode.rs:4997` だったので、`crash.avif`（正常な 64×32・317 バイトの AVIF の、frame OBU
ペイロード先頭 2 バイト＝ファイルオフセット 299・300 を `0xFF` にしたもの）を必須とし、加えて破壊位置が異なる
（シーケンスヘッダ・フレームヘッダ・タイルデータ）もので現行が abort するものを各 1 件まで選ぶ。
いずれも現行で abort することを確かめてからコミットする。

**フィクスチャの作り方**:
- 生成スクリプトを `core/tests/fixtures/` にコミットし、生成に使ったツールと版をスクリプト冒頭に書く。
  spike は ffmpeg 8.1.3（libaom-av1）と Pillow 12.3.0 を使った。Pillow の AVIF 保存は 8bit 専用で行列係数も
  10/12bit も指定できないので、それらは ffmpeg（`-pix_fmt`・`-colorspace`・`-color_range`）で作る。
  深度の違うアルファなど、ツールで作れない組み合わせは、主画像とアルファを別々にエンコードした AV1 データを
  スクリプトで AVIF に組む（`make_jxl_avif.py` が JPEG XL の箱を組んでいるのと同じ形）
- **フィクスチャのバイナリと期待値のハッシュをコミットする。** avif-decode を外した後は期待値を作り直す手段がなく、
  エンコーダの版が変わればフィクスチャのバイト列も変わるため
- abort の回帰に使う壊れたファイルもコミットする（元のファイルと破壊位置をスクリプトか README に残す）

**画素一致の対象**（spike で一致を確かめた範囲に、移植の罠を突く組み合わせを足したもの）:
8 / 10 / 12bit、4:2:0 / 4:2:2 / 4:4:4、グレー、全レンジ、行列（未指定・709・601・FCC・YCgCo・Identity）、
アルファ 8 / 10bit、奇数サイズ、**主画像 10bit＋アルファ 8bit・主画像 8bit＋アルファ 10bit、
主画像 BT.709＋アルファ未指定**。期待値はフィクスチャ 1 枚につき画素のハッシュ（形式と寸法を含む）で持つ。

**期待値の作り方**: 常設テストの期待値を avif-decode の出力から直接作らないのは、avif-decode の出力が CPU で
変わるため（「YUV→RGB 変換」）。正しさの根拠は 1 回だけの照合（上表）に置き、常設テストは「その後に変わって
いないこと」を全環境で同じ値で守る。差し替え前に avif-decode の出力（生の画素）を開発機で保存しておき、
照合が済むまで消さない。

**ファズ（CI には入れない）**: 実装後に 1 回だけ、正常 AVIF をランダムに 1〜4 バイト破壊した 1300 件以上を
1 件ずつ子プロセスで開き、**abort・signal・timeout が 0 件**であることを計測して plan の実施メモに残す。
spike での方式 b の結果は abort 0 / panic 0 / Err 896 / Ok 404 で、両方が Ok になった 404 件は rav1d と
libdav1d の出力が全件バイト一致した。

## 文書

- README
  - 前提条件: nasm の理由の書き換え、OS ごとの libdav1d（上の表）、Windows の手順（meson・ninja・git の導入と
    `SYSTEM_DEPS_DAV1D_BUILD_INTERNAL=always`）
  - Windows の手順には pkg-config の導入と、見当違いのエラー（「pkg-config が dav1d を見つけない」）の読み方を含める
  - 既知の制限に追加: 壊れた AVIF で stderr に出力が出ることがある / ★は JPEG のみで、JPEG XL・AVIF は
    常に「未評価」になる / 主ブランドが `avif` 以外の AVIF は開けない / 「挙動が変わる点」の 2 種類のファイルは開けない
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
- 利用者が Windows でビルドし、`cargo test -p picture-tool-core --test jxl_avif_input` が通る（画素一致のテストを含む）
