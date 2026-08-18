# メタデータ編集（タイトル・コメント・レーティング）設計

**状態**: 設計確定・実装前
**前提**: この機能の実装は **GUI デザイン刷新の後**に行う（後述「実装順序」）

## 背景と目的

手元の写真にコメントと星評価を付けたい。しかし既存の手段が両 OS とも不十分:

- **Windows エクスプローラー**: コメント欄はあるが入力が非常に手間
- **Fedora (GNOME Files / Eye of GNOME)**: そもそも編集手段が無い

picture-tool は既にフォルダーツリー・サムネイルグリッド・EXIF 読み取り・パス検証境界を
持っている。ここにメタデータ編集を載せれば、両 OS で同じ操作感の編集手段が手に入る。

## スコープ

### やること

| 項目 | 内容 |
|---|---|
| 対象ファイル | **手元の元写真**。JPEG のみ |
| 編集項目 | タイトル / コメント / レーティング（★0〜5）の3つ |
| 互換性 | Windows エクスプローラー と MWG 系（digiKam / Lightroom / Bridge）の**両方**から読めること |
| 操作単位 | 選んだ数枚にじっくり書く。明示的な保存 |

### やらないこと

- **RAW は対象外**。RAW は本体を書き換えず `.xmp` サイドカーを置くのが定石だが、
  Windows エクスプローラーはサイドカーを読まない。互換要件と正面から衝突するため今回は扱わない
- **タグ（キーワード）は対象外**。複数値であり、補完 UI と語彙管理が要る。要求されていない
- **変換出力へのメタデータ伝搬は対象外**。付ける対象は元写真だけ。
  （現状の変換処理は出力 JPEG に EXIF を一切書いていない。これは別問題として据え置く）
- **IPTC は書かない**。Windows / Lightroom / digiKam はすべて後述のタグセットで拾える。
  IPTC は digest による整合性チェックが絡んで複雑さが跳ね上がる割に、追加で救える読み手が無い

## 既存の設計方針との衝突と、その解消

この機能は picture-tool の既存ルール2つに正面からぶつかる。両方とも方針を書き換える。

### 衝突1: 「元の画像ファイルは上書きしない」

既存方針の意図は「**変換処理が**元画像を破壊しないこと」。メタデータ編集は上書きが目的
そのものなので、方針を次のように限定して書き換える。

> 変換処理は元ファイルを上書きしない。メタデータ編集は元ファイルの
> **メタデータセグメントのみを差し替える**（ピクセルデータは再エンコードせずバイト単位で不変）。

**ピクセル不変は努力目標ではなく実装の必須条件**。JPEG をデコードして再エンコードする
実装は再圧縮で必ず劣化するため採用できない。この条件が満たせなくなった時点で、
クレート選定ごと設計を見直す。

### 衝突2: 書き込みの信頼境界

`gui/src/security.rs` の現行ルールでは、書き込みは「ネイティブダイアログで許可した
ルート配下」に限られる。しかしメタデータ編集が書く先はフォルダーツリーで自由に選んだ
**入力フォルダ**であり、このモデルに収まらない。元ファイル削除がすでにぶつかったのと
同じ構造の問題。

削除は「実行ごとに OS ネイティブダイアログ」で解いているが、保存のたびにダイアログを
出すのは使い物にならない。既存の2部品を合成して解く。**新しい概念は導入しない。**

1. フォルダーツリーで選んだフォルダに「メタデータ編集を有効にする」操作を置く
2. 押すと Rust 側が **OS ネイティブの確認ダイアログ**を出す
   （`confirm_delete_originals` と同型。webview からは偽装できない）
3. 承認されたらそのフォルダを `WritableRoots` に `grant` する
   （`pick_output_folder` と同じ仕組み。webview から直接 grant はできない）
4. 以降そのフォルダ配下は、アプリ終了までダイアログなしで保存できる

乗っ取られた webview にできる最悪のことは「ユーザーが自分で承認したフォルダの
メタデータを壊す」まで。承認していないフォルダには一切書けない。

## 書き込むタグの仕様

### 前提: Windows は標準の `xmp:Rating` を見ていない

設計上もっとも非自明な事実。Windows エクスプローラーの★列は `System.Rating` という
プロパティで、その実体は **EXIF `RatingPercent`(0x4749) + XMP `MicrosoftPhoto:Rating`**
（パーセント値）である。

Lightroom / digiKam が使う標準の `xmp:Rating`(0〜5) は、Windows では
`System.SimpleRating` という**別プロパティ**に割り当てられており、`IsColumn = false`
すなわちエクスプローラーの列に出ない。**`xmp:Rating` だけを書いた写真は
Windows 上では「評価なし」のまま**になる。

同様に、エクスプローラーのコメント欄は読むときこそ3箇所を辿るが、
**書くときは `XPComment` しか更新しない**。`dc:description` をどれだけ充実させても
エクスプローラーの「コメント」欄には出ない。

Windows 系と MWG 系は根拠タグが完全に分離しているため、**両方に書く**以外に
互換手段は存在しない。

### 書き込み先一覧

| 項目 | 書き込み先 |
|---|---|
| レーティング | XMP `xmp:Rating`(0-5) ＋ EXIF `RatingPercent`(0x4749) ＋ XMP `MicrosoftPhoto:Rating` |
| コメント | EXIF `XPComment`(0x9C9C) ＋ EXIF `UserComment`(0x9286) ＋ XMP `dc:description` |
| タイトル | EXIF `XPTitle`(0x9C9B) ＋ XMP `dc:title` ＋ 条件付きで `ImageDescription`(0x010E) |

### エンコーディングの注意

- **`XPTitle` / `XPComment`**: EXIF 上の型は **BYTE 配列**（文字列型ではない）。
  中身は UCS-2LE（実質 UTF-16LE）で、末尾に 2 バイトの NUL 終端を付ける
- **`UserComment`**: 型は `undef` だが構造が違い、**先頭 8 バイトが文字コード識別子**
  （`UNICODE\0` / `ASCII\0\0\0` 等）＋以降が本文。`XPComment` とは別物なので混同しない
- **XMP パケット**: `http://ns.adobe.com/xap/1.0/\0` を前置きした APP1 セグメント。
  **SOS より前**に挿入する。`MicrosoftPhoto` の名前空間は
  `http://ns.microsoft.com/photo/1.0/`（末尾スラッシュに注意）

### レーティングの値変換

Microsoft 公式の変換表に従う。

| ★ | `xmp:Rating` | パーセント系（`RatingPercent` / `MicrosoftPhoto:Rating`） |
|---|---|---|
| 未評価 | タグ無し | 0 |
| 1 | 1 | 1 |
| 2 | 2 | 25 |
| 3 | 3 | 50 |
| 4 | 4 | 75 |
| 5 | 5 | 99 |

読み取り時のパーセント→★判定も公式表に従う:
1-12=★1, 13-37=★2, 38-62=★3, 63-87=★4, 88-99=★5, **0=未評価**。

**「未評価」と「★0」の区別**: `RatingPercent` では 0 が未評価を意味するため、
★0 という状態はパーセント系に表現できない。したがって本ツールは
**`Some(0)` を自分からは書かない**。UI は★クリックで 1〜5、同じ★を再クリックで
解除（未評価）とする。他アプリが書いた `xmp:Rating = 0` は読み取り時に受け入れる。

### `ImageDescription` の扱い（要注意）

EXIF 仕様上 `ImageDescription`(0x010E) は **ASCII 型**であり、日本語を格納すると
文字化けする（実測確認済み。クレートのバグではなく仕様上の制約）。

一方 ExifTool のソースには実測に基づく
`XPTitle is ignored by Windows Explorer if ImageDescription exists` という注記があり、
Microsoft 公式ドキュメントの記述（XPTitle 優先と読める）と矛盾している。公式側の
該当ページはコメント用のパスが混入した明らかな記述汚染があり、コミュニティからも
指摘されているため、並び順を全面的には信頼できない。**どちらが正しいかは
Windows 実機がないため断定できない。**

両説から確実に導ける結論は「`ImageDescription` と `XPTitle` に矛盾する値を
入れなければ、どちらが優先されても正しい値が出る」ことのみ。これを設計に落とす。

> **タイトル**: `dc:title` と `XPTitle` は常に書く。
> `ImageDescription` は**タイトルが ASCII で表現可能なときだけ**同じ値を書き、
> 不可能なとき（日本語を含むとき）は**既存の `ImageDescription` を削除する**。

削除まで踏み込むのは、日本語タイトルを付けたのに古い ASCII の `ImageDescription` が
残り、Windows でそちらが表示される事故を防ぐため。正しい値は `dc:title` と `XPTitle`
の両方にあるので情報は失われない。

**コメントは `ImageDescription` を一切読み書きしない**。タイトル用途と衝突するため。
`dc:description` を digiKam も Lightroom も読むので実害はない。

### 既存 XMP パケットは保持する（必須要件）

XMP パケットは本ツールの3項目だけが入る場所ではない。Lightroom は現像設定を `crs:`
名前空間に、他のツールも独自の名前空間に大量のデータを書き込む。テスト画像自体が
Lightroom 書き出しであり、実際に他の名前空間のデータを持っていた。

> **既存の XMP パケットを丸ごと削除して書き直してはならない。**
> パケットをパースし、本ツールが扱う 4 プロパティ
> （`xmp:Rating` / `MicrosoftPhoto:Rating` / `dc:title` / `dc:description`）
> **だけを差し替え、他の名前空間のデータはすべて元のまま残す**。

XMP パケットが存在しない場合のみ、新規に生成して挿入する。

`img-parts` はセグメント単位の低レベル操作しか提供しないため、パケット内部の
XML 操作は別途行う必要がある。`little_exif` の `xmp.rs` は EXIF との重複除去専用で
書き込み API を持たないため使えない。XML パーサ（`quick-xml` 等）で
プロパティ単位の差し替えを実装する。

**「削除して作り直す」実装は他アプリのデータを破壊するため、いかなる理由があっても
採用しない。** これはピクセル不変と同格の必須条件として扱う。

## 読み取りの優先順位

書くときは全箇所に同じ値を書くので迷わない。問題は**読むとき**である。
他アプリが書いた写真では箇所ごとに値が食い違う。Windows で編集すれば `XPComment`
だけが新しくなり、Lightroom で編集すれば `dc:description` だけが新しくなる。
**どちらが新しいかを知る手段はファイル内に存在しない。**

優先順位を固定する。XMP を先頭に置くのは、それが業界標準であり MWG も digiKam も
Lightroom もそこを見るため。

| 項目 | 優先順位 |
|---|---|
| コメント | XMP `dc:description` → EXIF `UserComment` → EXIF `XPComment` |
| タイトル | XMP `dc:title` → EXIF `XPTitle` → EXIF `ImageDescription` |
| レーティング | XMP `xmp:Rating` → EXIF `RatingPercent` → XMP `MicrosoftPhoto:Rating` |

### 食い違いは警告として返す

黙って片方を採用し片方を捨てるのが最もまずい挙動。**複数箇所に異なる値を検出したら
警告として呼び出し元へ返す**。既存の `ProcessResult.warnings` と同じ思想で、
core は判断せず事実だけを上に投げる。

UI は編集欄の脇に控えめに「他のアプリが書いた別の値があります」と示す。保存すれば
全箇所が揃うので、警告は自然に消える。

## Core API

`core/src/metadata.rs` を新設する。既存の `read_exif_info`（カメラが書いた撮影情報を
読む）とは目的が違うので分ける。こちらは「人が書くものを読み書きする」。

```rust
/// 人が編集する対象のメタデータ。
/// `None` は「タグが無い」、`Some("")` は「空文字のタグがある」を意味し、区別する。
pub struct EditableMetadata {
    pub title:   Option<String>,
    pub comment: Option<String>,
    /// `None` = 未評価。`Some(0..=5)`。書き込み時に `Some(0)` は生成しない
    pub rating:  Option<u8>,
}

/// 読み取り結果。`warnings` には箇所ごとの値の食い違いが載る
pub struct MetadataRead {
    pub metadata: EditableMetadata,
    pub warnings: Vec<String>,
}

pub fn read_metadata(path: &Path) -> Result<MetadataRead>;
pub fn write_metadata(path: &Path, meta: &EditableMetadata) -> Result<()>;
```

UI で欄を空にして保存した場合（`None`）は**タグを削除**する。空文字のタグが残ると
他アプリで「空のコメントがある」状態になるため。

## 依存クレート

**`little_exif`（EXIF 書き込み）+ `img-parts`（XMP を APP1 セグメントとして挿入）**
の2本立て。どちらも純 Rust・MIT/Apache-2.0 で、Fedora / Windows 両対応、
C ライブラリ依存なし。

検討して除外したもの:

| クレート | 除外理由 |
|---|---|
| `rexiv2` | **GPL-3.0-or-later** が配布物全体に波及。さらに exiv2 の外部ライブラリが必要 |
| `xmp_toolkit`（Adobe 公式） | C++ SDK のビルドが必要（Windows は MSVC 必須）。純 Rust 方針に反する |
| `nom-exif` / `quickexif` | **読み取り専用**。書き込み不可 |
| `xmpkit` | 純 Rust を謳うが 2025-11 作成・実績不足。経過観察 |

`little_exif` に `XPTitle` / `XPComment` 専用の enum バリアントは無いが、
`ExifTag::UnknownINT8U(bytes, tag_id, ExifTagGroup::GENERIC)` で任意タグに生バイトを
書ける。UCS-2LE エンコードは呼び出し側で行う。

**両クレートとも 1.0 未満で更新頻度が緩やか**（`img-parts` は最終更新 2025-08、
issue も長期未クローズ）。API 破壊的変更とメンテナンス停滞のリスクを見込み、
**バージョンを固定する**。

### `img-parts` の落とし穴

`segments_mut().push()` で追加したセグメントは EOI 後に置かれ、再パース時に消失する。
**`insert()` で SOS より前の位置に挿入しなければならない**（実測で確認）。

## 書き込みの安全性

### 検証付きの原子的置換

元写真が対象である以上、ここは厚くする。

1. **同一ディレクトリ**に一時ファイルとしてコピー
2. 一時ファイルに対してメタデータを書き込む
3. **読み戻して検証**する（書いた値が正しく読めるか、既存 EXIF が壊れていないか）
4. 検証を通ったら `rename` で置き換える

一時ファイルを同一ディレクトリに置くのは、`rename` が原子的であるために同じ
ファイルシステム上にある必要があるため。どの段階で失敗しても**元写真は無傷**で、
一時ファイルが残るだけ。

手順 3 を挟むのは、書いた結果が壊れていた場合に元ファイルを置き換えないため。

### panic の封じ込め

`little_exif` は `unwrap()` が多数残っており、壊れた EXIF で panic しうる既知の
issue がある（#77）。core が panic するとアプリごと落ちるため、
**`catch_unwind` で包んで `Err` に変換する**。

core は `eprintln!` しないという既存方針どおり、事象は呼び出し元へ返す。

## GUI

### 構造

左2カラム（フォルダーツリー・サムネイルグリッド）は変換機能と**完全に共通**。
**右カラムだけを「変換」/「メタデータ」でモード切替**する。同じ写真フォルダを
見ながら、やることを切り替える形。

### 保存の体験

**明示的な保存ボタン**。星も含めて保存ボタン経由とする。元ファイルを書き換える
操作である以上、意図が必ず介在する形が正しい。

- 未保存のまま別の写真を選んだら警告する
- 未保存のままアプリを終了しようとしたら警告する

### 追加する Tauri コマンド

| コマンド | 役割 |
|---|---|
| `read_image_metadata` | 編集対象の現在値と警告を返す |
| `write_image_metadata` | 検証付き原子的置換で書き込む |
| `grant_metadata_editing` | OS ネイティブ確認ダイアログを出し、承認されたら `WritableRoots` に grant |

### security.rs に追加する検証

`writable_image(roots, raw) -> Result<PathBuf, String>` を追加する。条件は3つ:

1. 実体が対応画像の拡張子を持つこと（`readable_image` と同じ強さ）
2. 承認済みルート配下であること
3. **既に存在するファイルであること**

新規ファイル作成は一切許可しない。メタデータ編集が新しいファイルを作る理由が無いので、
塞げる分は塞ぐ。

**コマンドを追加するときは必ず `security.rs` を経由させる**という既存の前提は
この機能でも変わらない。

## 検証済みの事実と、未検証のリスク

### 実測で確認済み

実カメラ由来の JPEG（Sony ILCE-7CM2、5.5MB）で `little_exif` + `img-parts` の
round-trip を検証した。kamadak-exif とは独立に書いた TIFF/IFD パーサでクロス検証済み。

| 検証項目 | 結果 |
|---|---|
| 書き込み（1回目・2回目とも） | 成功。issue #93 のエラーは発生せず |
| ピクセル不変 | **完全一致**。スキャンデータの sha256、デコード後 RGB バッファの sha256 とも書き込み前・1回後・2回後で同一 |
| 既存タグの消失 | **0 件**。サムネイル（IFD1、10553 bytes）不変。機種・レンズ・F値・SS・ISO・撮影日時すべて無傷 |
| 日本語の往復 | `XPTitle` / `XPComment` / `UserComment` とも正しくデコード |
| 既存 core との互換 | `read_exif_info` の出力が書き込み前後で完全に同一 |
| 再編集 | XMP セグメントが重複せず 1 個のまま更新される |

**この検証は XMP パケットを削除して作り直す方式で行われた**。上記「既存 XMP パケットは
保持する」の要件は**未実装・未検証**であり、実装時に別途満たす必要がある。

### 未検証（実装時に必ず潰すこと）

**テスト画像は `Software` タグが `Adobe Photoshop Lightroom Classic 15.2.1` であり、
Lightroom 書き出し後の JPEG だった。そのため MakerNote と GPS IFD を持っていない。**

`little_exif` の issue #93（既存 EXIF を読み込んで書き戻すと
`failed to fill whole buffer` で失敗する）は MakerNote が絡む可能性があり、
**再現条件を満たしていないため「通った」とは言えない**。

> **実装計画の最初のタスクとして、Lightroom 等を通していないカメラ直出しの JPEG
> （MakerNote 付き・GPS 付き）で同じ検証を必ず 1 度通す。**
> ここが割れた場合、クレート選定ごと設計を見直す。

その場合の緩和策として、書き込み失敗時は元ファイルをそのまま残して警告を返す
フォールバックを実装しておく（検証付き原子的置換により、これは自動的に成立する）。

### その他の既知リスク

| 項目 | 内容 |
|---|---|
| `little_exif` #104 | `quick-xml` 依存に既知の RUSTSEC 脆弱性が到達可能、オープン中 |
| `little_exif` | APP12 / APP13（Photoshop IRB）セグメントは編集不可（削除のみ） |
| Windows の Title 挙動 | 公式ドキュメントと ExifTool 実装注記が矛盾。実機未検証。上記「`ImageDescription` の扱い」で回避 |
| digiKam の読み取り | 名前空間の優先順位がユーザー設定依存。デフォルトで `acdsee` が先頭に来ることがあり、他ソフトと食い違う場合がある（本ツール側では対処不能） |

## テスト方針

- **ピクセル不変** — 書き込み前後でスキャンデータの sha256 が一致することを検証する。
  これは設計の必須条件であり、回帰したら即座に検知できなければならない
- **既存タグの保存** — 書き込み前後で、追加した項目以外のタグ集合に差分が無いこと
- **日本語の往復** — UCS-2LE エンコードの正しさ。ASCII のみのテストでは通ってしまう
- **食い違い検出** — 複数箇所に異なる値がある画像を用意し、警告が出ることと、
  優先順位どおりの値が採用されることを検証する
- **`ImageDescription` の条件分岐** — ASCII タイトルでは書かれ、日本語タイトルでは
  既存値が削除されること。両方の分岐を検証する
- **既存 XMP の保持** — `crs:` 等の他名前空間を含む XMP パケットを持つ画像に書き込み、
  **本ツールが扱う 4 プロパティ以外が 1 バイトも変わらないこと**を検証する。
  Lightroom 書き出し画像をフィクスチャに含める
- **原子性** — 書き込み途中で失敗した場合に元ファイルが無傷であること
- **`writable_image` の境界** — 承認していないルート、存在しないファイル、
  非対応拡張子がそれぞれ拒否されること

テストフィクスチャには**実カメラ由来の JPEG**（MakerNote 付き）を含める。
合成画像だけでは今回の未検証リスクを踏み抜く。

## 実装順序

この機能は**単独では実装しない**。

現行 GUI のデザインに不満があり刷新の意向があること、および星評価・長文テキストエリア・
保存/未保存状態といったコンポーネントが現行 GUI に一切存在しないことから、順序を分ける。

1. 本 spec を確定させる（← 現在地）
2. **GUI デザイン刷新**。本 spec を入力の一部とし、「変換 ＋ メタデータ」の
   両方を載せるデザインシステムとして設計する
3. 本機能を実装する（新しいデザイン言語の上に、一度だけ）

先に spec を書いておくことで、デザイン刷新が必要とする部品の全量が判明した状態で
刷新を設計できる。刷新後に部品を継ぎ足す事故と、メタデータ編集 UI を 2 度作る無駄を
どちらも避けられる。

## 参照

- `gui/src/security.rs` モジュールコメント — 信頼境界の設計と脅威モデル
- `docs/superpowers/plans/2026-08-04-full-codebase-review-fixes.md` S6 節 — 境界設計の経緯
- [System.Rating](https://learn.microsoft.com/en-us/windows/win32/properties/props-system-rating) / [System.SimpleRating](https://learn.microsoft.com/en-us/windows/win32/properties/props-system-simplerating) — ★の値変換表とプロパティの分離
- [System.Comment Photo Metadata Policy](https://learn.microsoft.com/en-us/windows/win32/wic/-wic-photoprop-system-comment) — 読み 3 経路・書き `XPComment` のみ
- [ExifTool Exif.pm](https://raw.githubusercontent.com/exiftool/exiftool/master/lib/Image/ExifTool/Exif.pm) — XP\* タグのエンコーディング定義、`ImageDescription` 優先説
- [ExifTool MWG.pm](https://raw.githubusercontent.com/exiftool/exiftool/master/lib/Image/ExifTool/MWG.pm) — MWG の Rating / Description 定義
