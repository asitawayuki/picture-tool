# メタデータ編集（タイトル・コメント・レーティング）設計

**状態**: 実装済み（`feature/metadata-editing` ブランチ）。実装計画と実施メモは
[`docs/superpowers/plans/2026-08-26-metadata-editing.md`](../plans/2026-08-26-metadata-editing.md)
**前提**: この機能は **GUI デザイン刷新の後**に実装した（後述「実装順序」）

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

**`WritableRoots` は用途ごとに別の集合を持つ。** `ProcessingState` が
`writable_roots`（バッチ変換の出力先。`pick_output_folder` が足し、`writable_dir` が
見る）と `metadata_roots`（メタデータ編集。`grant_metadata_editing` が足し、
`writable_image` が見る）の 2 本を持ち、**それぞれのダイアログが承認したことだけを
承認する**。

**当初は 1 つの集合を共有していた**（新しい概念を増やさず既存の `WritableRoots` を
再利用する、という判断）。ブランチ最終レビューで撤回した ── 逆向き、すなわち
「変換の出力先の選択が、メタデータ書き込みの承認を兼ねる」が実害を持つため。
`pick_output_folder` のダイアログは「出力先フォルダーを選択」としか言っておらず、
既存写真の書き換えについて一言も触れていない。出力先に自分の写真フォルダーを選ぶのは
ごく普通の操作で、共有していると**その瞬間から webview は追加の同意なしに配下の
全 JPEG のメタデータを書き換えられる**。しかもフロントの承認表示は
`grantMetadataEditing` の戻り値しか持たないので、UI が「まだ許可されていません」と
出したまま Rust 側は許可している、という**表示が実権限より狭い**食い違いになる。
（もう一方の向き ── メタデータ編集の承認が変換の出力先を兼ねる ── の実害は
「`_processed` 付きのファイルが増える」までだったが、集合を分けたのでこちらも消えた。）

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
> **だけを差し替え、他の名前空間のプロパティと値はすべて元のまま残す**。
> 触る必要が無い要素・属性はバイト列のまま写す。
>
> 例外は、本ツールの属性を落とした `rdf:Description` の start タグだけである。
> 組み直しにより属性の区切りが単一スペースへ正規化される（**値は不変**）。
> XML の書き戻しで実装する以上ここは避けられず、守りたいのは
> 「他アプリのデータを壊さないこと」であってバイト列の同一性ではない。

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

**この区別を GUI の入力欄は表現できない。** `None`（タグが無い）と `Some("")`
（空文字のタグがある）は Core API 上は別の値だが、フロントのテキスト入力欄は
どちらも「空欄」としてしか表示できない。そのため `gui-frontend/src/lib/panels/metadataValues.ts`
は空欄を常に `null`（＝タグを削除）として送る。**したがって、他アプリが書いた
`Some("")`（空文字のタグ）を持つ写真を本ツールで開き、何も変更せず保存すると、
その空文字のタグは `null`（タグ削除）に変わる。** これは設計どおりの挙動（前段の
「タグを削除する」がそもそもこの区別を UI に持ち込まない前提で書かれている）だが、
「無変更のまま保存したのに値が変わる」という体感を持ちうるので明記しておく。

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

**レイアウトの現行仕様は [GUI デザイン刷新 spec](./2026-08-19-gui-redesign-design.md) にある。**
本機能は刷新が用意した navigation rail の 3 モード構成のうち、**「情報」モード**に載る。

当初この spec は「右カラムだけを変換 / メタデータで切替」と書いていたが、刷新は
3 モードの rail で着地した（2026-08-19）。rail は「右カラムだけ切替」の上位互換であり、
左 2 カラム（フォルダーツリー・写真グリッド）が変換とメタデータで完全共通、という
本 spec の要求はそのまま成立している。ラベルが「メタデータ」ではなく「情報」なのは、
rail 幅 80px に日本語 3 語を収めるための決着（刷新 spec §8）。

| 項目 | 参照先 |
|---|---|
| 3 カラムと rail の構成 | 刷新 spec §3-1 / §3-3 |
| `editingPath` と `focusedPath` の分離 | 刷新 spec §3-2 |
| 未保存ガードの 4 経路 | 刷新 spec §3-4 |
| メタデータモードのグリッド（単一フォーカス・★・未保存マーク） | 刷新 spec §4-3 |
| メタデータパネルの 7 要素 | 刷新 spec §5-2 |

### 保存の体験

**明示的な保存ボタン。** 星も含めて保存ボタン経由とする。元ファイルを書き換える
操作である以上、意図が必ず介在する形が正しい。連続して付けていく作業が主なので、
「保存して次の写真へ」を主ボタン、「保存」を副ボタンに置く。

離脱経路はすべて `metadataDraft` の `isDirty` を通す。アプリ内の 3 経路
（写真の切替 / rail でのモード移動 / フォルダー変更）は webview 内のダイアログ、
**ウィンドウを閉じるときだけは OS ネイティブのダイアログ**で確認する
（`set_unsaved_state` + `on_window_event`。刷新 spec §3-4 に制約の詳細）。

### 追加する Tauri コマンド

| コマンド | 役割 |
|---|---|
| `read_image_metadata` | 編集対象の現在値と警告を返す |
| `write_image_metadata` | 検証付き原子的置換で書き込み、**保存直後の実際の中身**を返す |
| `grant_metadata_editing` | OS ネイティブ確認ダイアログを出し、承認されたら `WritableRoots` に grant |
| `read_image_ratings` | 可視範囲ぶんの★をまとめて返す（グリッド表示用） |
| `set_unsaved_state` | 未保存の有無を Rust へ通知する（終了時の確認のため） |

### security.rs に追加する検証

`writable_image(roots, raw) -> Result<PathBuf, String>` を追加する。条件は3つ:

1. 実体が **JPEG** の拡張子を持つこと（`readable_image` より強い。メタデータ編集の
   対象は JPEG だけで、PNG / WebP は core の `metadata` が扱わない）
2. 承認済みルート配下であること
3. **既に存在するファイルであること**

新規ファイル作成は一切許可しない。メタデータ編集が新しいファイルを作る理由が無いので、
塞げる分は塞ぐ。

追加する関数は `writable_image` と、その内側で使う `readable_jpeg`（条件1のみを見る）
の2本。読み取り側の `read_image_metadata` / `read_image_ratings` コマンドも
`readable_jpeg` を経由し、同じ「JPEG であること」の条件で通す
（承認済みルートの判定はしない ── 読み取りは既存の `readable_image` と同格の扱い）。

**コマンドを追加するときは必ず `security.rs` を経由させる**という既存の前提は
この機能でも変わらない。

## 検証済みの事実と、未検証のリスク

**結論（実装完了時点）**: 下記「実測で確認済み」は spec 確定時点（2026-08-18）の記述で、
実装計画の Task 1・Task 7 で数字つきの再検証を通した。

- **round-trip 実測（Task 1）**: Lightroom 書き出しの実写真（SONY ILCE-7CM2、約11.8MB /
  12,343,419 bytes）で 2 回連続の書き込みが成功し、issue #93 の
  `failed to fill whole buffer` は発生しなかった。SOS 以降のバイト列は書き込み前後で
  完全一致（ピクセル不変）。Exif フィールドは 1 つも消えず、唯一変化した
  `In(1)/JPEGInterchangeFormat`（IFD1 サムネイルへの相対オフセット、1030 → 1056）は
  IFD0 にタグが増えた分の自然なシフトで、サムネイル本体のバイト列一致で無害と確認した
  （`core/tests/metadata_spike.rs` の `a_real_photo_survives_two_write_cycles`。
  詳細は実装計画の実施メモ「Task 1 の実測」）
- **既存 XMP パケットの保持（Task 7）**: `crs:`（`camera-raw-settings`）を含む
  Lightroom 書き出しの XMP パケット（フィクスチャ `lightroom-xmp.jpg`）に書き込み、
  本ツールが扱う 4 プロパティ以外の属性値が 1 つも変わらないことを
  `core/tests/metadata_integration.rs` の `writing_metadata_preserves_foreign_xmp_namespaces`
  で検証した。`core/src/metadata/xmp.rs` の単体テストも `foreign_content` による意味論比較
  （属性の並び替え・単一引用符・入れ子構造・`rdf:RDF` ルート直下の外部要素の 4 系統）で
  この要件を検査している

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

**この検証は XMP パケットを削除して作り直す方式で行われた**（spec 確定時点の記述）。
上記「既存 XMP パケットは保持する」の要件はこの時点では未実装・未検証だったが、
冒頭の「結論」に書いたとおり Task 7 で満たされたことを確認済み。

### 残リスク: MakerNote 付き JPEG

`little_exif` の issue #93（既存 Exif を読み込んで書き戻すと
`failed to fill whole buffer` で失敗する）は MakerNote が絡む可能性があるが、
**本ツールが実際に書き込む対象は Lightroom 書き出しの JPEG であり、MakerNote を
持たない**（利用者確認済み）。当初は「カメラ直出しの JPEG で必ず 1 度検証する」を
実装の前提条件としていたが、対象に存在しない条件のために着手を止める意味が無い。

代わりに**壊さずに失敗すること**で守る。`core::metadata` の書き込みは、
既存 Exif を解析できない場合に**作り直さず `Err` で止める**。置き換えは
検証付き原子的置換なので元ファイルは必ず無傷で、症状は「保存できませんでした」
であってデータ消失ではない。この保証は
`core/tests/metadata_integration.rs` の
`a_file_whose_exif_cannot_be_parsed_is_refused_without_being_modified` が守る。

**この保証の実際の強度**（ブランチ最終レビュー I-3 の記録を、検証強化
[2026-09-29-metadata-write-verification-design.md](2026-09-29-metadata-write-verification-design.md)
の実装後の実態に書き換えたもの）: `atomic::verify` は置き換えの前に、書き込み前後の
SOS より前を `core/src/metadata/preserve.rs` で比べる。

- **検査 1**: Exif の全フィールド（編集対象 5 タグと `JPEGInterchangeFormat` を除く）が、
  書き込み後に同じ値で残っていること。Exif は kamadak-exif の厳格読みで読み、
  書き込み前後のどちらかを読めなければ保存を拒否する
- **検査 2**: IFD1 サムネイル本体のバイト列が同じであること
- **検査 3**: Exif・XMP 以外のセグメント（ICC・IPTC・拡張 XMP・JFIF・量子化／ハフマン表、
  2 つ目以降の Exif／XMP APP1）が内容も相対順序も同じであること

したがって「解析には成功するが書き戻しで一部を落とす」経路も、GPS・MakerNote・サムネイル・ICC の
消失として検知して拒否する。**それでも検知しないもの**:

- MakerNote 内部のオフセット整合 ── バイト列が同じで位置だけが変わったときは保存を通し、
  警告（「…MakerNote）の位置が変わったため…」）を返すに留める
- 未知の型コードを持つタグの中身（kamadak-exif が中身を返さないため、型と個数だけを比べる）
- XMP 内部の保全（本 spec の「既知の制約」のまま）

MakerNote 付きの JPEG が手に入ったら、`core/tests/metadata_spike.rs` の
`#[ignore]` テストに `PICTURE_TOOL_SPIKE_JPEG` で渡していつでも再検査できる。

### 残リスク: XMP APP1 が Exif APP1 より前にあるファイル

`little_exif` は **JPEG の最初の APP1 セグメントを Exif とみなす**。そのため
XMP の APP1 が Exif の APP1 より前に置かれた JPEG は、Exif 自体が健全でも解析に失敗し、
保存が「既存の Exif を解析できないため書き込みを中止しました」という**誤診の文面**で
止まる（実際に起きているのは「Exif を見つけられなかった」であって「壊れている」では
ない）。壊さずに失敗する側なのでデータは無事だが、利用者には原因が分からない。

本ツール自身の書き込みは常に Exif APP1 を先頭に置くので、自作ファイルでは起きない。
他ツールが書いた JPEG に限った残リスクである。

### 残リスク: 多言語 XMP と Extended XMP

**多言語 Lang Alt が黙って落ちる。** `dc:title` / `dc:description` は XMP 上
Lang Alt（`rdf:Alt` + `rdf:li`）であり、他アプリが `en-US` / `fr-FR` など
複数言語の `rdf:li` を書き込んでいても、本ツールは差し替え時に `x-default` の
1 要素だけに畳む（`core/src/metadata/xmp.rs`）。本ツールが扱う 4 プロパティは
「差し替えてよい」対象という設計そのものなので不具合ではないが、**多言語の値を
持つファイルを本ツールで編集すると、他言語の `rdf:li` が消える**という実害が
利用者に見える形で残る。多言語運用をしている写真には向かない。

**`install()` が既存の Extended XMP セグメントを残置する。** Extended XMP は
64KB を超える XMP パケットを複数の APP1 セグメントに分割して格納する仕様だが、
本ツールはこれに対応しない。標準の XMP パケット（`http://ns.adobe.com/xap/1.0/\0`
前置きの APP1）だけを差し替え、Extended XMP のセグメント
（`http://ns.adobe.com/xmp/extension/` 前置き）が既に存在していてもそのまま残す。
本ツールは Extended XMP を読まないため、標準パケット側の値との整合は保証しない。

### その他の既知リスク

| 項目 | 内容 |
|---|---|
| `little_exif` #104 | `quick-xml` 依存に既知の RUSTSEC 脆弱性が到達可能、オープン中 |
| `little_exif` | APP12 / APP13（Photoshop IRB）セグメントは編集不可（削除のみ） |
| Windows の Title 挙動 | 公式ドキュメントと ExifTool 実装注記が矛盾。実機未検証。上記「`ImageDescription` の扱い」で回避 |
| digiKam の読み取り | 名前空間の優先順位がユーザー設定依存。デフォルトで `acdsee` が先頭に来ることがあり、他ソフトと食い違う場合がある（本ツール側では対処不能） |
| `quick-xml` の版揃え | 「依存クレート」の `quick-xml = "0.37"` 固定で 1 版に揃うのは **Linux / Windows のビルド限定**。`gui` の `Cargo.lock` には Tauri が依存する `plist`（macOS 専用、`cargo tree --target all` でのみ現れる）経由で `quick-xml` 0.38 系も入るが、`plist` は `cfg(target_os = "macos")` 相当のターゲット限定クレートなので Linux / Windows のビルドグラフには乗らない。実測: `cargo tree -p picture-tool-gui -i quick-xml`（この環境のデフォルトターゲット）は 0.37.5 の 1 行のみ、`--target all` を付けると 0.38.4 が `plist` 経由で追加で見える。workspace の S6-M19（同一クレート 2 版のリンク禁止）の趣旨には反しない |

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
  **本ツールが扱う 4 プロパティ以外のプロパティと値が 1 つも変わらないこと**を検証する。
  Lightroom 書き出し画像をフィクスチャに含める
- **原子性** — 書き込み途中で失敗した場合に元ファイルが無傷であること
- **`writable_image` の境界** — 承認していないルート、存在しないファイル、
  非対応拡張子がそれぞれ拒否されること

テストフィクスチャには**実写真由来の JPEG**を含める。合成画像だけでは検査として弱い。
`exif-only.jpg` / `lightroom-xmp.jpg` はどちらも Lightroom 書き出しの実写真から作った
（`core/tests/fixtures/`）。**MakerNote 付きは含まない** ── 本ツールが実際に書き込む対象は
Lightroom 書き出しの JPEG で、MakerNote を持たないため（詳細は
「### 残リスク: MakerNote 付き JPEG」）。その代わり、解析できない Exif に当たった場合に
作り直さず `Err` で止まることを
`a_file_whose_exif_cannot_be_parsed_is_refused_without_being_modified` が守る。

## 実装順序

この機能は**単独では実装しない**。

現行 GUI のデザインに不満があり刷新の意向があること、および星評価・長文テキストエリア・
保存/未保存状態といったコンポーネントが現行 GUI に一切存在しないことから、順序を分ける。

1. ✅ 本 spec を確定させる
2. ✅ **GUI デザイン刷新**。本 spec を入力の一部とし、「変換 ＋ メタデータ」の
   両方を載せるデザインシステムとして設計した（main に統合済み）
3. ✅ 本機能を実装する（新しいデザイン言語の上に、一度だけ）。実装計画と実施メモは
   [`docs/superpowers/plans/2026-08-26-metadata-editing.md`](../plans/2026-08-26-metadata-editing.md)

先に spec を書いておいたことで、デザイン刷新が必要とする部品の全量が判明した状態で
刷新を設計できた。刷新後に部品を継ぎ足す事故と、メタデータ編集 UI を 2 度作る無駄を
どちらも避けられた。

## 参照

- `gui/src/security.rs` モジュールコメント — 信頼境界の設計と脅威モデル
- `docs/superpowers/plans/2026-08-04-full-codebase-review-fixes.md` S6 節 — 境界設計の経緯
- [System.Rating](https://learn.microsoft.com/en-us/windows/win32/properties/props-system-rating) / [System.SimpleRating](https://learn.microsoft.com/en-us/windows/win32/properties/props-system-simplerating) — ★の値変換表とプロパティの分離
- [System.Comment Photo Metadata Policy](https://learn.microsoft.com/en-us/windows/win32/wic/-wic-photoprop-system-comment) — 読み 3 経路・書き `XPComment` のみ
- [ExifTool Exif.pm](https://raw.githubusercontent.com/exiftool/exiftool/master/lib/Image/ExifTool/Exif.pm) — XP\* タグのエンコーディング定義、`ImageDescription` 優先説
- [ExifTool MWG.pm](https://raw.githubusercontent.com/exiftool/exiftool/master/lib/Image/ExifTool/MWG.pm) — MWG の Rating / Description 定義
