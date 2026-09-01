# メタデータ編集 実装計画

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to implement this plan task-by-task.
> Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 手元の JPEG にタイトル・コメント・★を、Windows エクスプローラーと
MWG 系（digiKam / Lightroom）の**両方**から読める形で、ピクセルを一切変えずに書き込む。

**Architecture:** core に `metadata` モジュールを新設し、EXIF は `little_exif`、
XMP は `img-parts` でセグメントを差し替え・`quick-xml` でパケット内をプロパティ単位に
パッチする。書き込みは「同一ディレクトリの一時ファイル → 読み戻し検証 → `rename`」の
原子的置換に閉じる。GUI は刷新済みの 3 モード rail の「情報」モードに載せ、
書き込み許可は OS ネイティブの確認ダイアログ経由で `WritableRoots` に grant する。

**Tech Stack:** Rust（`little_exif` 0.6.23 / `img-parts` 0.4.0 / `quick-xml` 0.37）、
Tauri v2、Svelte 5（runes）、Playwright

**Spec:** [`docs/superpowers/specs/2026-08-18-metadata-editing-design.md`](../specs/2026-08-18-metadata-editing-design.md)
（本計画の Task 18 で §GUI と §実装順序を書き換える。**書き換え後の spec が正**）

**関連 spec:** [`docs/superpowers/specs/2026-08-19-gui-redesign-design.md`](../specs/2026-08-19-gui-redesign-design.md)
── 刷新は §8 で 3 項目を**本工程へ明示的に持ち越している**。本計画はそれも引き受ける:

| 持ち越し | 出典 | 本計画での担当 |
|---|---|---|
| `set_unsaved_state` の実装（終了時の未保存ガード） | 刷新 spec §3-4 | Task 16 |
| ★・未保存マークのグリッド表示（読み取りコストの実測とセット） | 刷新 spec §4-3 | Task 17 |
| メタデータパネルのデータ接続 | 刷新 spec §5-2 | Task 13〜15 |

---

## 本計画で決着させた 2 点

### 決着 1: GUI は刷新の 3 モード rail に寄せる。spec §GUI を書き換える

メタデータ編集 spec は「**右カラムだけ**を変換 / メタデータで切替」と書いているが、
刷新は navigation rail の 3 モード（変換 / 情報 / フレーム）で着地し、main に統合済み
（7719cd4）。**刷新側に寄せる。** 根拠:

- `docs/README.md` が刷新 spec を「現行仕様」、メタデータ編集 spec を「設計確定・実装前」と
  している。レイアウトの現行仕様は刷新側にある
- 刷新は本 spec を**入力として**設計されており（刷新 spec §5-2 は本 spec の 7 要素を
  そのまま部品化している）、器はすでに存在する。`MetadataPanel.svelte` /
  `metadataDraft.svelte.ts` / `Rating.svelte` / `editingPath` の分離まで実装済み
- 3 モード rail は「右カラムだけ切替」の**上位互換**である。左 2 カラム（フォルダーツリー・
  写真グリッド）が変換とメタデータで完全共通、という本 spec の要求は刷新でも成立している。
  違うのは切替の見せ方（rail の destination か、右カラム内のトグルか）だけ
- ラベルは「メタデータ」ではなく**「情報」**。rail 幅 80px に収めるための決着で、
  `e2e/shell.spec.ts` が毎回検査している（刷新 spec §8）

→ Task 18 で spec §GUI を「刷新 spec の該当節を参照する」形に置換する。**spec に同じ設計を
2 つ書かない**（食い違ったときにどちらが正か決められなくなる）。

### 決着 2: 既存 XMP パケットの保持を独立タスクにする

spec の「検証済みの事実と、未検証のリスク」は、実測が**パケットを削除して作り直す方式**で
行われたこと、したがって「既存 XMP パケットは保持する（必須要件）」が**未実装・未検証**で
あることを明記している。これを **Task 7** として独立させた。

- Task 6（既存パケットからの**読み取り**）とは別タスクにする。読めることと、
  他名前空間を 1 バイトも壊さずに**差し替えられる**ことは別の保証であり、
  レビューアが片方だけ落とせる必要がある
- 受け入れ条件は「4 プロパティ以外が 1 バイトも変わらないこと」。
  Lightroom 書き出し（`crs:` を持つ）をフィクスチャに含めて検査する
- **「削除して作り直す」実装は、いかなる理由があっても採用しない**（spec の必須条件）

---

## Global Constraints

すべてのタスクの要件に、暗黙にこの節が含まれる。

| 制約 | 内容 |
|---|---|
| ブランチ | `feature/metadata-editing`。**`main` で作業しない**（作成済み） |
| ピクセル不変 | JPEG をデコード・再エンコードしない。SOS 以降のバイト列が書き込み前後で完全一致すること。**努力目標ではなく必須条件**。満たせなくなったらクレート選定ごと設計を見直す |
| 既存 XMP 保持 | 本ツールが扱う 4 プロパティ（`xmp:Rating` / `MicrosoftPhoto:Rating` / `dc:title` / `dc:description`）**だけ**を差し替える。他名前空間の**プロパティと値は 1 つも変えない**。触る必要が無いタグはバイト列のまま写す。ただし本ツールの属性を落とした `rdf:Description` の start タグだけは、組み直しにより属性の区切りが単一スペースへ正規化される（値は不変。検査は Task 7 の `foreign_content` による意味論比較で行う） |
| `Some(0)` を書かない | `RatingPercent` は 0 が「未評価」を意味するため、★0 という状態は表現できない。未評価はタグ削除で表す。他アプリが書いた `xmp:Rating = 0` は読み取り時のみ受け入れる |
| 対象は JPEG のみ | RAW・PNG・WebP は対象外。`security` の検証で拡張子ごと弾く |
| 信頼境界 | webview から来たパスは必ず `gui/src/security.rs` を経由させる。webview にプラグイン権限を与えない（`gui/capabilities/default.json` は `core:default` のまま） |
| DTO 契約 | `gui-frontend/src/lib/dto.contract.ts` は**手で編集しない**。`gui/src/types.rs` の `mod contract` を直し、`UPDATE_DTO_CONTRACT=1 cargo test -p picture-tool-gui contract` で再生成する |
| core は `eprintln!` しない | 利用者に伝える事象は戻り値の `warnings` に積む（唯一の例外は Task 9 で明示） |
| 依存のバージョン固定 | `little_exif = "=0.6.23"` / `img-parts = "=0.4.0"` / `quick-xml = "0.37"`。前 2 者は 1.0 未満・更新頻度が緩やかなため `=` で固定する（spec の要求） |
| `quick-xml` は 1 版に揃える | `little_exif` 0.6.23 が `quick-xml` 0.37.5 に依存している。**0.42 を足すと同じクレートの 2 版が同時にリンクされる**（workspace `Cargo.toml` の S6-M19 の禁止事項）。`0.37` を指定して cargo に統合させること（`cargo tree` で 1 行になるのを確認済み） |
| Svelte 5 | runes（`$state` / `$derived` / `$effect`）。旧構文は使わない |
| 生の色を書かない | 余白・角丸・タイポも `gui-frontend/src/styles/` のトークン経由 |
| e2e のポート | 5174 が塞がっている環境では `E2E_PORT=5199 bun run e2e` |
| 検証コマンド | Rust: `cargo test --workspace` / `make lint`。フロント: `make typecheck` / `make test-frontend` / `cd gui-frontend && bun run e2e`。まとめて `make check`（e2e は含まれない） |

---

## File Structure

### 新規

| ファイル | 責務 |
|---|---|
| `core/src/metadata/mod.rs` | 公開 API（`EditableMetadata` / `MetadataRead` / `read_metadata` / `write_metadata`）、読み取りの優先順位と食い違い警告、★⇄パーセントの変換 |
| `core/src/metadata/exif.rs` | `little_exif` による EXIF 側の読み書き。UCS-2LE と `UserComment` の文字コード識別子 |
| `core/src/metadata/xmp.rs` | XMP パケットの取り出し・**プロパティ単位の差し替え**・新規生成・`img-parts` によるセグメント挿入 |
| `core/src/metadata/atomic.rs` | 検証付き原子的置換と `catch_unwind` |
| `core/tests/metadata_spike.rs` | `#[ignore]` の実測用。カメラ直出し JPEG での round-trip とフィクスチャ生成 |
| `core/tests/metadata_integration.rs` | 実カメラ由来フィクスチャでの統合テスト |
| `core/tests/fixtures/camera-direct.jpg` | カメラ直出しの EXIF（MakerNote 付き）を持つ小さい JPEG |
| `core/tests/fixtures/lightroom-xmp.jpg` | `crs:` 等の他名前空間を持つ XMP パケット付き JPEG |
| `gui-frontend/src/lib/panels/metadataValues.ts` | `EditableMetadata` ⇄ フォーム値の変換（純粋。`bun test` の対象） |
| `gui-frontend/src/lib/panels/grants.ts` | パスが承認済みルート配下かの判定（純粋。`bun test` の対象） |
| `gui-frontend/src/lib/panels/grants.svelte.ts` | 承認済みルートの保持（runes） |
| `gui-frontend/src/lib/UnsavedChangesDialog.svelte` | 未保存のまま離脱しようとしたときの webview 内ダイアログ |

### 変更

| ファイル | 変更内容 |
|---|---|
| `core/Cargo.toml` | 依存 3 本の追加 |
| `core/src/lib.rs` | `pub mod metadata;` の追加、`ExifInfo` に `PartialEq` を derive |
| `gui/src/security.rs` | `readable_jpeg` / `writable_image` の追加とモジュールコメントの更新 |
| `gui/src/state.rs` | `unsaved` / `close_prompt_open` の `AtomicBool` を追加 |
| `gui/src/commands.rs` | コマンド 5 本の追加 |
| `gui/src/types.rs` | `RatingEntry` の追加、`mod contract` にエントリ追加 |
| `gui/src/main.rs` | ハンドラ登録と `on_window_event` |
| `gui-frontend/src/lib/types.ts` / `api.ts` | 境界の写しと `invoke` ラッパー |
| `gui-frontend/src/lib/panels/metadataDraft.svelte.ts` | `load()` の実装、保存、警告の保持 |
| `gui-frontend/src/lib/panels/MetadataPanel.svelte` | 繋ぎ込み |
| `gui-frontend/src/App.svelte` | 未保存ガードの配線、承認状態、★の読み込み |
| `gui-frontend/src/lib/browser/PhotoGrid.svelte` | タイルへの★と未保存マーク |
| `gui-frontend/e2e/stub.ts` / `e2e/metadata.spec.ts` | 新コマンドのスタブと検査の書き換え |
| `docs/superpowers/specs/2026-08-18-metadata-editing-design.md` | §GUI・§実装順序・§未検証リスクの書き換え |
| `CLAUDE.md` / `docs/README.md` | 新 API・新コマンド・設計方針の更新 |

---
## Task 1: 依存の追加と、カメラ直出し JPEG での round-trip 実測

spec の「未検証（実装時に必ず潰すこと）」への回答。**ここが割れたらクレート選定ごと
設計を見直す**ので、いちばん最初に置く。

**Files:**
- Modify: `core/Cargo.toml`
- Create: `core/tests/metadata_spike.rs`

**Interfaces:**
- Consumes: なし
- Produces: `core/tests/metadata_spike.rs` の `scan_start(bytes: &[u8]) -> usize`（このファイル内限り。
  本実装の同等物は Task 9 で `core/src/metadata/atomic.rs` に作る）と、
  環境変数 `PICTURE_TOOL_SPIKE_JPEG` の約束（Task 2 も同じ変数を使う）

- [ ] **Step 1: 検証用のカメラ直出し JPEG を用意する**

**Lightroom 等を通していない、カメラが書いたままの JPEG** が要る。
spec の実測に使われた画像は `Software` が `Adobe Photoshop Lightroom Classic 15.2.1` で、
**MakerNote と GPS IFD を持っていなかった**。`little_exif` の issue #93
（既存 EXIF を読み込んで書き戻すと `failed to fill whole buffer` で失敗する）は
MakerNote が絡む可能性があり、再現条件を満たしていない。

利用者の写真フォルダーから 1 枚選び、絶対パスを控える。次で条件を機械的に確認する。

- [ ] **Step 2: 実測テストを書く**

`core/tests/metadata_spike.rs`:

```rust
//! カメラ直出し JPEG（MakerNote 付き）での round-trip 実測。
//!
//! spec 2026-08-18「未検証（実装時に必ず潰すこと）」への回答であり、
//! 将来クレートを上げたときの再検査にも使う。**通常のテスト実行では走らない**
//! （実カメラの画像はリポジトリに入れないため）。
//!
//! 実行:
//!   PICTURE_TOOL_SPIKE_JPEG=/path/to/DSC00123.JPG \
//!     cargo test -p picture-tool-core --test metadata_spike -- --ignored --nocapture

use little_exif::exif_tag::ExifTag;
use little_exif::filetype::FileExtension;
use little_exif::ifd::ExifTagGroup;
use little_exif::metadata::Metadata;

const MAKER_NOTE: u16 = 0x927c;
const GPS_IFD: u16 = 0x8825;
const XP_TITLE: u16 = 0x9c9b;

fn spike_source() -> std::path::PathBuf {
    let raw = std::env::var("PICTURE_TOOL_SPIKE_JPEG")
        .expect("PICTURE_TOOL_SPIKE_JPEG にカメラ直出し JPEG の絶対パスを渡すこと");
    std::path::PathBuf::from(raw)
}

/// SOS（`FF DA`）の開始位置。ここから EOI までが実際の画素データ。
///
/// **同じ計算が Task 9 の `core/src/metadata/atomic.rs` にも要る。** ここに写しを置くのは、
/// この実測が本実装より先に走るため。本実装ができたらこちらは触らない
/// （クレート更新時の再検査を、本実装から独立させておく価値がある）。
fn scan_start(bytes: &[u8]) -> usize {
    let mut i = 2; // SOI をとばす
    loop {
        assert!(i + 1 < bytes.len(), "SOS が見つからない");
        if bytes[i] != 0xFF {
            panic!("セグメント境界が壊れている: offset {}", i);
        }
        // 0xFF のパディング（fill byte）を読み飛ばす
        let mut m = i + 1;
        while bytes[m] == 0xFF {
            m += 1;
        }
        let marker = bytes[m];
        if marker == 0xDA {
            return i;
        }
        let len = u16::from_be_bytes([bytes[m + 1], bytes[m + 2]]) as usize;
        i = m + 1 + len;
    }
}

/// kamadak-exif で全フィールドを列挙する。**little_exif とは独立の実装**なので、
/// 「little_exif で書いて little_exif で読めた」の同語反復にならない。
fn exif_fields(bytes: &[u8]) -> Vec<(String, String)> {
    let mut cursor = std::io::Cursor::new(bytes);
    let reader = exif::Reader::new()
        .read_from_container(&mut cursor)
        .expect("EXIF が読めない");
    let mut fields: Vec<(String, String)> = reader
        .fields()
        .map(|f| {
            (
                format!("{:?}/{}", f.ifd_num, f.tag),
                f.display_value().to_string(),
            )
        })
        .collect();
    fields.sort();
    fields
}

#[test]
#[ignore = "実カメラの画像が要る。PICTURE_TOOL_SPIKE_JPEG で渡す"]
fn camera_direct_jpeg_survives_two_write_cycles() {
    let source = spike_source();
    let original = std::fs::read(&source).expect("読めない");

    // --- 前提条件: この画像が本当に「カメラ直出し」か ---
    let meta = Metadata::new_from_path(&source).expect("EXIF が読めない");
    let has_maker_note = meta.get_tag_by_hex(MAKER_NOTE, None).next().is_some();
    let has_gps = meta.get_tag_by_hex(GPS_IFD, Some(ExifTagGroup::GENERIC)).next().is_some()
        || meta.get_ifd(ExifTagGroup::GPS, 0).is_some();
    println!("MakerNote: {} / GPS: {}", has_maker_note, has_gps);
    assert!(
        has_maker_note,
        "MakerNote が無い。Lightroom 等を通した画像では issue #93 の再現条件を満たさない"
    );

    let baseline_fields = exif_fields(&original);
    let baseline_scan = &original[scan_start(&original)..];

    // --- 1 回目 ---
    let mut buffer = original.clone();
    let mut meta = Metadata::new_from_vec(&buffer, FileExtension::JPEG).expect("読めない");
    meta.set_tag(ExifTag::UnknownINT8U(
        vec![0x41, 0x00, 0x00, 0x00], // "A" を UCS-2LE + NUL 終端で
        XP_TITLE,
        ExifTagGroup::GENERIC,
    ));
    meta.write_to_vec(&mut buffer, FileExtension::JPEG)
        .expect("1 回目の書き込みが失敗した（issue #93 の疑い）");

    assert_eq!(
        &buffer[scan_start(&buffer)..],
        baseline_scan,
        "1 回目でピクセルが変わった"
    );

    // --- 2 回目（自分が書いたものを読み直して書く） ---
    let mut second = buffer.clone();
    let mut meta = Metadata::new_from_vec(&second, FileExtension::JPEG).expect("読めない");
    meta.set_tag(ExifTag::UnknownINT8U(
        vec![0x42, 0x00, 0x00, 0x00],
        XP_TITLE,
        ExifTagGroup::GENERIC,
    ));
    meta.write_to_vec(&mut second, FileExtension::JPEG)
        .expect("2 回目の書き込みが失敗した（issue #93 の疑い）");

    assert_eq!(
        &second[scan_start(&second)..],
        baseline_scan,
        "2 回目でピクセルが変わった"
    );

    // --- 既存タグが消えていないか（追加した XPTitle 以外に差分が無いこと） ---
    let after_fields = exif_fields(&second);
    let lost: Vec<&(String, String)> = baseline_fields
        .iter()
        .filter(|f| !after_fields.contains(f))
        .collect();
    assert!(lost.is_empty(), "消えた・変わったタグがある: {:#?}", lost);

    println!(
        "タグ数 {} → {}（増分は XPTitle のみのはず）",
        baseline_fields.len(),
        after_fields.len()
    );
}
```

- [ ] **Step 3: 依存を足す**

`core/Cargo.toml` の `[dependencies]` に追記する。

```toml
# メタデータ編集（spec 2026-08-18）。どちらも 1.0 未満で更新頻度が緩やかなため
# バージョンを固定する。API 破壊的変更とメンテナンス停滞のリスクを見込んでのこと。
little_exif = "=0.6.23"
img-parts = "=0.4.0"
# XMP パケット内をプロパティ単位で差し替えるためのパーサ。
# **0.37 系であることが重要**: little_exif 0.6.23 が quick-xml 0.37.5 に依存しており、
# 0.42 を足すと同じクレートの 2 版が同時にリンクされる（workspace Cargo.toml の S6-M19）。
quick-xml = "0.37"
```

- [ ] **Step 4: 依存が 1 版に揃っていることを確認する**

Run: `cargo tree -p picture-tool-core -i quick-xml`
Expected: `quick-xml v0.37.x` が **1 行だけ**。2 行出たらバージョン指定を直す。

- [ ] **Step 5: 実測を走らせる**

Run:
```bash
PICTURE_TOOL_SPIKE_JPEG=/path/to/DSC00123.JPG \
  cargo test -p picture-tool-core --test metadata_spike -- --ignored --nocapture
```
Expected: PASS。`MakerNote: true` が出ること。

**割れた場合はここで停止し、報告する。** 「クレート選定ごと設計を見直す」が spec の
決めた分岐であり、後続タスクを進めても無駄になる。出力（特に
`failed to fill whole buffer` の有無）をそのまま計画の実施メモに記録すること。

- [ ] **Step 6: 結果を実施メモに記録してコミット**

本計画末尾の「実施メモ」に、使った機種・MakerNote / GPS の有無・タグ数の増減を書く。

```bash
git add core/Cargo.toml core/tests/metadata_spike.rs docs/superpowers/plans/2026-08-26-metadata-editing.md
git commit -m "test(core): メタデータ編集: カメラ直出し JPEG で little_exif の round-trip を実測する"
```

---

## Task 2: 実カメラ由来のテストフィクスチャを作る

実カメラの JPEG（5〜25MB）はそのままリポジトリに入れられない（`.git` は現在 12MB）。
**EXIF の APP1 セグメントだけを小さい JPEG へ移植する。**
EXIF 内部のオフセットは APP1 の中の TIFF ヘッダ基準なので、セグメントを丸ごと移せば
MakerNote も IFD1 のサムネイルもそのまま生き残る。

**Files:**
- Modify: `core/tests/metadata_spike.rs`
- Create: `core/tests/fixtures/camera-direct.jpg`
- Create: `core/tests/fixtures/lightroom-xmp.jpg`

**Interfaces:**
- Consumes: Task 1 の `PICTURE_TOOL_SPIKE_JPEG`
- Produces: フィクスチャ 2 本。Task 10 の統合テストが
  `PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/camera-direct.jpg")` で参照する
  （`core/src/exif_frame/logo.rs:171` と同じ流儀）

- [ ] **Step 1: 生成テストを書く**

`core/tests/metadata_spike.rs` に追記:

```rust
use img_parts::jpeg::{markers, Jpeg};
use img_parts::Bytes;

fn fixture_path(name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// 64x80 の合成 JPEG。画素は何でもよい ── 守りたいのは移植する EXIF の中身。
fn tiny_jpeg() -> Vec<u8> {
    let img = image::RgbImage::from_fn(64, 80, |x, y| {
        image::Rgb([(x * 4) as u8, (y * 3) as u8, 128])
    });
    let mut buffer = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(img)
        .write_to(&mut buffer, image::ImageOutputFormat::Jpeg(92))
        .unwrap();
    buffer.into_inner()
}

#[test]
#[ignore = "フィクスチャの生成。PICTURE_TOOL_SPIKE_JPEG で実カメラの画像を渡す"]
fn generate_camera_direct_fixture() {
    let source = spike_source();
    let src = Jpeg::from_bytes(Bytes::from(std::fs::read(&source).unwrap())).unwrap();

    let exif_app1 = src
        .segments_by_marker(markers::APP1)
        .find(|s| s.contents().starts_with(b"Exif\0\0"))
        .expect("EXIF の APP1 が無い")
        .clone();

    let mut out = Jpeg::from_bytes(Bytes::from(tiny_jpeg())).unwrap();
    // APP0(JFIF) の直後に置く。**末尾に push すると EOI の後ろに行き、
    // 再パース時に消える**（spec「img-parts の落とし穴」）
    out.segments_mut().insert(1, exif_app1);

    let bytes = out.encoder().bytes();
    std::fs::write(fixture_path("camera-direct.jpg"), &bytes).unwrap();
    println!("camera-direct.jpg: {} bytes", bytes.len());

    // 移植できたことを、独立の実装（kamadak-exif）で確かめる
    let fields = exif_fields(&bytes);
    println!("フィールド数: {}", fields.len());
    for (name, value) in &fields {
        println!("  {} = {}", name, value);
    }
}
```

- [ ] **Step 2: 生成して、中身を目で確認する**

Run:
```bash
PICTURE_TOOL_SPIKE_JPEG=/path/to/DSC00123.JPG \
  cargo test -p picture-tool-core --test metadata_spike generate_camera_direct -- --ignored --nocapture
```
Expected: `camera-direct.jpg` が 40〜120KB 程度で生成され、MakerNote / Make / Model /
LensModel / DateTimeOriginal が列挙に出ること。

**あわせて「編集項目が 1 つも入っていない」ことを確認する。**
`ImageDescription` / `XPTitle` / `XPComment` / `UserComment` / `Rating` /
`RatingPercent` が列挙に出ないこと。出る場合は別の 1 枚で作り直す ── Task 8 の
`reading_a_file_without_any_editable_metadata_yields_none` がこの前提に立っている。
（空白詰めの `ImageDescription` を書くカメラがあるが、読み取り側は trim するので
値が空白だけなら「タグ無し」として扱われる。）

- [ ] **Step 3: 個人情報を確認し、コミットしてよいか利用者に確認する**

**これはリポジトリに永久に残る。** Step 2 の列挙から次を確認する:

- `GPS*` — 撮影地の座標。**入っていたら、そのままコミットしてよいか必ず利用者に確認する**
- `BodySerialNumber` / `LensSerialNumber` / `OwnerName` / `Artist` / `Copyright`

利用者が座標の混入を望まない場合は、GPS を持たない別の 1 枚で作り直す。
**GPS 付きでの検証は spec の要求だが、リポジトリに入れるかどうかは別の判断**であり、
検証だけなら Task 1 の spike（コミットしない）で足りる。

- [ ] **Step 4: XMP 付きフィクスチャを用意する**

Task 7（既存 XMP パケットの保持）に要る。**`crs:` 等の他名前空間を持つ**
Lightroom 書き出し JPEG から、同じ手口で APP1 を 2 つとも移植する。
`camera-direct.jpg` と違い、EXIF の APP1 と XMP の APP1 の**両方**を移す。

```rust
#[test]
#[ignore = "フィクスチャの生成。PICTURE_TOOL_LIGHTROOM_JPEG で Lightroom 書き出しを渡す"]
fn generate_lightroom_xmp_fixture() {
    let raw = std::env::var("PICTURE_TOOL_LIGHTROOM_JPEG")
        .expect("PICTURE_TOOL_LIGHTROOM_JPEG に Lightroom 書き出し JPEG のパスを渡すこと");
    let src = Jpeg::from_bytes(Bytes::from(std::fs::read(&raw).unwrap())).unwrap();

    let app1: Vec<_> = src
        .segments_by_marker(markers::APP1)
        .filter(|s| {
            s.contents().starts_with(b"Exif\0\0")
                || s.contents().starts_with(b"http://ns.adobe.com/xap/1.0/\0")
        })
        .cloned()
        .collect();
    assert_eq!(app1.len(), 2, "EXIF と XMP の APP1 が両方あること");

    let mut out = Jpeg::from_bytes(Bytes::from(tiny_jpeg())).unwrap();
    for (i, seg) in app1.into_iter().enumerate() {
        out.segments_mut().insert(1 + i, seg);
    }
    let bytes = out.encoder().bytes();
    std::fs::write(fixture_path("lightroom-xmp.jpg"), &bytes).unwrap();
    println!("lightroom-xmp.jpg: {} bytes", bytes.len());
}
```

Lightroom 書き出しが手元に無い場合は、**`crs:` を含む XMP パケットを手で書いて**
`tiny_jpeg()` に挿してもよい。守りたい性質は「本ツールが知らない名前空間が入っていること」
だけなので、実物である必要はない。その場合は spec の「Lightroom 書き出し画像を
フィクスチャに含める」から外れるので、**実施メモにその旨を書く**。

- [ ] **Step 5: コミット**

```bash
git add core/tests/metadata_spike.rs core/tests/fixtures/camera-direct.jpg core/tests/fixtures/lightroom-xmp.jpg
git commit -m "test(core): メタデータ編集: 実カメラ由来の EXIF を持つ小さいフィクスチャを用意する"
```

---
## Task 3: 型とレーティングの値変換

**Files:**
- Create: `core/src/metadata/mod.rs`
- Modify: `core/src/lib.rs`（`pub mod metadata;` と `ExifInfo` への `PartialEq`）

**Interfaces:**
- Consumes: なし
- Produces:
  - `picture_tool_core::metadata::EditableMetadata { title: Option<String>, comment: Option<String>, rating: Option<u8> }`
    （`Debug + Clone + Default + PartialEq + Eq + Serialize + Deserialize`）
  - `picture_tool_core::metadata::MetadataRead { metadata: EditableMetadata, warnings: Vec<String> }`
    （`Debug + Clone + Default + Serialize`）
  - `pub(crate) fn stars_to_percent(stars: u8) -> u16`
  - `pub(crate) fn percent_to_stars(percent: u16) -> Option<u8>`

- [ ] **Step 1: モジュールを登録し、失敗するテストを書く**

**先に `core/src/lib.rs` へ `pub mod metadata;` を足す。** 後回しにすると
`core/src/metadata/mod.rs` はコンパイル対象にならず、Step 2 の
`cargo test` が「0 tests / PASS」で通ってしまう ── red が偽物になる。

そのうえで `core/src/metadata/mod.rs` を新規作成し、末尾に:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// Microsoft 公式の変換表（spec §レーティングの値変換）。
    /// 1 が 1% なのは 20% ではない ── ここを等間隔にすると Windows 側の★が 1 つずれる
    #[test]
    fn stars_map_to_the_microsoft_percent_table() {
        assert_eq!(stars_to_percent(1), 1);
        assert_eq!(stars_to_percent(2), 25);
        assert_eq!(stars_to_percent(3), 50);
        assert_eq!(stars_to_percent(4), 75);
        assert_eq!(stars_to_percent(5), 99);
    }

    #[test]
    fn percent_maps_back_through_the_official_ranges() {
        assert_eq!(percent_to_stars(0), None, "0 は未評価であって★0 ではない");
        for (percent, stars) in [(1, 1), (12, 1), (13, 2), (37, 2), (38, 3), (62, 3), (63, 4), (87, 4), (88, 5), (99, 5)] {
            assert_eq!(percent_to_stars(percent), Some(stars), "{}%", percent);
        }
    }

    /// 他アプリが範囲外を書いていても落ちない
    #[test]
    fn out_of_range_percent_clamps_to_five_stars() {
        assert_eq!(percent_to_stars(100), Some(5));
        assert_eq!(percent_to_stars(65535), Some(5));
    }

    /// 往復して同じ★に戻ること。表を片方だけ直した事故を捕まえる
    #[test]
    fn stars_survive_a_round_trip_through_percent() {
        for stars in 1..=5u8 {
            assert_eq!(percent_to_stars(stars_to_percent(stars)), Some(stars));
        }
    }
}
```

- [ ] **Step 2: 落ちることを確認する**

Run: `cargo test -p picture-tool-core metadata::`
Expected: FAIL（`cannot find function stars_to_percent` / モジュール未登録）

- [ ] **Step 3: 実装する**

`core/src/metadata/mod.rs` の先頭に:

```rust
//! 人が編集するメタデータ（タイトル・コメント・レーティング）の読み書き。
//!
//! 既存の [`crate::read_exif_info`] が「**カメラが書いた撮影情報**を読む」のに対し、
//! こちらは「**人が書くもの**を読み書きする」。目的が違うのでモジュールを分けてある。
//!
//! 書くときは Windows 系（`XPTitle` / `XPComment` / `RatingPercent` /
//! `MicrosoftPhoto:Rating`）と MWG 系（`dc:title` / `dc:description` / `xmp:Rating`）の
//! **両方**へ同じ値を書く。両者は根拠タグが完全に分離しており、片方だけでは
//! もう一方のアプリから「未設定」に見えるため。詳細は
//! `docs/superpowers/specs/2026-08-18-metadata-editing-design.md`。

use serde::{Deserialize, Serialize};

/// 人が編集する対象のメタデータ。
///
/// `None` は「タグが無い」、`Some("")` は「空文字のタグがある」を意味し、**区別する**。
/// UI で欄を空にして保存した場合は `None`（＝タグを削除）として扱う。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditableMetadata {
    pub title: Option<String>,
    pub comment: Option<String>,
    /// `None` = 未評価。`Some(1..=5)`。
    /// **書き込み時に `Some(0)` は生成しない** ── `RatingPercent` では 0 が未評価を
    /// 意味するため、★0 という状態はパーセント系に表現できない。
    /// 他アプリが書いた `xmp:Rating = 0` は読み取り時のみ受け入れる
    pub rating: Option<u8>,
}

/// 読み取り結果。`warnings` には箇所ごとの値の食い違いが載る。
///
/// core は「どちらが新しいか」を判断できない（ファイル内にその情報が無い）。
/// 優先順位で 1 つ採ったうえで、**捨てた値があった事実だけを上へ渡す**。
#[derive(Debug, Clone, Default, Serialize)]
pub struct MetadataRead {
    pub metadata: EditableMetadata,
    pub warnings: Vec<String>,
}

/// ★ → パーセント（Microsoft 公式の変換表）
pub(crate) fn stars_to_percent(stars: u8) -> u16 {
    match stars {
        1 => 1,
        2 => 25,
        3 => 50,
        4 => 75,
        5 => 99,
        _ => 0,
    }
}

/// パーセント → ★。`None` は未評価
pub(crate) fn percent_to_stars(percent: u16) -> Option<u8> {
    match percent {
        0 => None,
        1..=12 => Some(1),
        13..=37 => Some(2),
        38..=62 => Some(3),
        63..=87 => Some(4),
        _ => Some(5),
    }
}
```

`core/src/lib.rs` の `ExifInfo` に `PartialEq` を足す
（Task 9 の「書き込み前後で既存 EXIF が壊れていないか」の比較に要る）:

```rust
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ExifInfo {
```

- [ ] **Step 4: 通ることを確認する**

Run: `cargo test -p picture-tool-core metadata::`
Expected: PASS（4 テスト）

- [ ] **Step 5: コミット**

```bash
git add core/src/metadata/mod.rs core/src/lib.rs
git commit -m "feat(core): メタデータ編集: 編集対象の型とレーティングの値変換を足す"
```

---

## Task 4: EXIF 側の読み取り

**Files:**
- Create: `core/src/metadata/exif.rs`
- Modify: `core/src/metadata/mod.rs`（`mod exif;`）

**Interfaces:**
- Consumes: Task 3 の `percent_to_stars`
- Produces:
  ```rust
  pub(super) struct ExifSide {
      pub title_xp: Option<String>,
      pub title_description: Option<String>,
      pub comment_xp: Option<String>,
      pub comment_user: Option<String>,
      pub rating_percent: Option<u16>,
  }
  pub(super) fn read(meta: &little_exif::metadata::Metadata) -> ExifSide;
  pub(super) fn from_ucs2le_nul(bytes: &[u8]) -> Option<String>;
  ```
  **`Metadata` を引数に取る**（パスやバイト列ではない）のは、`mod.rs` が
  読み取りと書き込みで同じ `Metadata` を使い回すため。ファイルを 2 回開かない。

- [ ] **Step 1: モジュールを登録し、失敗するテストを書く**

先に `core/src/metadata/mod.rs` へ `mod exif;` を足す（Task 3 と同じ理由。
足さないと Step 2 の red が偽物になる）。そのうえで
`core/src/metadata/exif.rs` の末尾に:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ucs2le_decodes_japanese_and_stops_at_the_nul_terminator() {
        // "港" = U+6E2F → 2F 6E（LE）、続いて NUL 終端
        let bytes = [0x2F, 0x6E, 0x00, 0x00];
        assert_eq!(from_ucs2le_nul(&bytes).as_deref(), Some("港"));
    }

    #[test]
    fn ucs2le_handles_a_missing_terminator() {
        let bytes = [0x2F, 0x6E];
        assert_eq!(from_ucs2le_nul(&bytes).as_deref(), Some("港"));
    }

    #[test]
    fn ucs2le_survives_surrogate_pairs() {
        // "𠮟" = U+20B9F → サロゲートペア D842 DF9F
        let bytes = [0x42, 0xD8, 0x9F, 0xDF, 0x00, 0x00];
        assert_eq!(from_ucs2le_nul(&bytes).as_deref(), Some("𠮟"));
    }

    /// **`UserComment` は `XPComment` と構造が違う。**
    /// 先頭 8 バイトが文字コード識別子で、本文はその後ろ
    #[test]
    fn user_comment_reads_the_unicode_identifier() {
        let mut bytes = b"UNICODE\0".to_vec();
        bytes.extend_from_slice(&[0x2F, 0x6E]);
        assert_eq!(
            decode_user_comment(&bytes, &Endian::Little).as_deref(),
            Some("港")
        );
    }

    #[test]
    fn user_comment_reads_the_ascii_identifier() {
        let mut bytes = b"ASCII\0\0\0".to_vec();
        bytes.extend_from_slice(b"hello");
        assert_eq!(
            decode_user_comment(&bytes, &Endian::Little).as_deref(),
            Some("hello")
        );
    }

    /// 識別子が無い / 未知のものは「読めなかった」にする。
    /// 8 バイトを本文と誤読すると先頭が化けた文字列を利用者に見せることになる
    #[test]
    fn user_comment_rejects_an_unknown_identifier() {
        let bytes = b"\0\0\0\0\0\0\0\0hello".to_vec();
        assert_eq!(decode_user_comment(&bytes, &Endian::Little), None);
    }

    /// **読み取りは前後の空白を落とす。**
    /// XMP 側は整形の改行を必ず含むので、EXIF だけ残すと同じ値が食い違いに見える。
    /// 空白詰めの ImageDescription を書くカメラもここで救われる
    #[test]
    fn reading_trims_surrounding_whitespace_and_nul_padding() {
        assert_eq!(from_ucs2le_nul(&[0x20, 0x00, 0x2F, 0x6E, 0x20, 0x00, 0x00, 0x00]).as_deref(), Some("港"));
        assert_eq!(from_ucs2le_nul(&[0x20, 0x00, 0x00, 0x00]), None, "空白だけならタグ無し扱い");
    }

    /// 前提条件つきの否定形。空の `Metadata` で「全部 None」になること
    #[test]
    fn an_empty_metadata_yields_no_values() {
        let side = read(&Metadata::new());
        assert!(side.title_xp.is_none());
        assert!(side.title_description.is_none());
        assert!(side.comment_xp.is_none());
        assert!(side.comment_user.is_none());
        assert!(side.rating_percent.is_none());
    }
}
```

- [ ] **Step 2: 落ちることを確認する**

Run: `cargo test -p picture-tool-core metadata::exif`
Expected: FAIL（未実装）

- [ ] **Step 3: 実装する**

`core/src/metadata/exif.rs` の先頭に:

```rust
//! EXIF 側（`little_exif`）の読み書き。
//!
//! **`XPTitle` / `XPComment` と `UserComment` は別物である。**
//! 前者は EXIF 上の型が BYTE 配列で、中身は UCS-2LE ＋ 2 バイトの NUL 終端。
//! 後者は `undef` 型だが構造が違い、**先頭 8 バイトが文字コード識別子**
//! （`UNICODE\0` / `ASCII\0\0\0` 等）で本文はその後ろに続く。混同すると
//! 先頭 8 バイトが化けた文字列を利用者に見せることになる。

use little_exif::endian::Endian;
use little_exif::exif_tag::ExifTag;
use little_exif::ifd::ExifTagGroup;
use little_exif::metadata::Metadata;

/// IFD0 に置くタグ。`little_exif` に専用の enum バリアントが無いので生の ID で扱う
pub(super) const XP_TITLE: u16 = 0x9c9b;
pub(super) const XP_COMMENT: u16 = 0x9c9c;
pub(super) const RATING_PERCENT: u16 = 0x4749;
pub(super) const IMAGE_DESCRIPTION: u16 = 0x010e;
/// ExifIFD に置かれる。グループが IFD0 ではないので削除時に注意
pub(super) const USER_COMMENT: u16 = 0x9286;

const ID_UNICODE: &[u8; 8] = b"UNICODE\0";
const ID_ASCII: &[u8; 8] = b"ASCII\0\0\0";

/// EXIF 側から読めた生の値。**優先順位の判断はここではしない**（`mod.rs` の責務）
#[derive(Debug, Default)]
pub(super) struct ExifSide {
    pub title_xp: Option<String>,
    pub title_description: Option<String>,
    pub comment_xp: Option<String>,
    pub comment_user: Option<String>,
    pub rating_percent: Option<u16>,
}

pub(super) fn read(meta: &Metadata) -> ExifSide {
    let endian = meta.get_endian();

    let xp = |hex: u16| -> Option<String> {
        let tag = meta.get_tag_by_hex(hex, Some(ExifTagGroup::GENERIC)).next()?;
        from_ucs2le_nul(&tag.value_as_u8_vec(&endian))
    };

    let title_description = meta
        .get_tag_by_hex(IMAGE_DESCRIPTION, Some(ExifTagGroup::GENERIC))
        .next()
        .and_then(|tag| match tag {
            ExifTag::ImageDescription(text) => Some(text.clone()),
            _ => None,
        })
        .and_then(|text| cleaned(&text));

    let comment_user = meta
        .get_tag_by_hex(USER_COMMENT, Some(ExifTagGroup::EXIF))
        .next()
        .and_then(|tag| match tag {
            ExifTag::UserComment(bytes) => decode_user_comment(bytes, &endian),
            _ => None,
        });

    let rating_percent = meta
        .get_tag_by_hex(RATING_PERCENT, Some(ExifTagGroup::GENERIC))
        .next()
        .and_then(|tag| match tag {
            ExifTag::UnknownINT16U(values, _, _) => values.first().copied(),
            _ => None,
        });

    ExifSide {
        title_xp: xp(XP_TITLE),
        title_description,
        comment_xp: xp(XP_COMMENT),
        comment_user,
        rating_percent,
    }
}

/// 読めた文字列を正規形に寄せる。
///
/// **前後の空白は落とす。** XMP は要素テキストが整形の改行・インデントを含みうるので
/// XMP 側は必ず trim することになり、EXIF 側だけ残すと同じ値が「食い違い」に見える。
/// 書き込み側も `EditableMetadata::normalized` で trim するので、
/// 保存 → 読み戻しの検証（Task 9）が一致する。
/// 空白詰めの `ImageDescription` を書くカメラも救われる。
fn cleaned(text: &str) -> Option<String> {
    let trimmed = text.trim_matches(|c: char| c == '\0' || c.is_whitespace());
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// UCS-2LE（実質 UTF-16LE）＋ NUL 終端を読む
pub(super) fn from_ucs2le_nul(bytes: &[u8]) -> Option<String> {
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    let end = units.iter().position(|&u| u == 0).unwrap_or(units.len());
    cleaned(&String::from_utf16(&units[..end]).ok()?)
}

/// `UserComment` の本文を取り出す。先頭 8 バイトの識別子で本文の解釈が変わる
pub(super) fn decode_user_comment(bytes: &[u8], endian: &Endian) -> Option<String> {
    if bytes.len() < 8 {
        return None;
    }
    let (id, body) = bytes.split_at(8);
    let text = if id == ID_UNICODE {
        let units: Vec<u16> = body
            .chunks_exact(2)
            .map(|pair| match endian {
                Endian::Little => u16::from_le_bytes([pair[0], pair[1]]),
                Endian::Big => u16::from_be_bytes([pair[0], pair[1]]),
            })
            .collect();
        String::from_utf16(&units).ok()?
    } else if id == ID_ASCII {
        String::from_utf8(body.to_vec()).ok()?
    } else {
        // 識別子が無い / 未知。読めなかったものとして扱う
        return None;
    };
    cleaned(&text)
}
```

- [ ] **Step 4: 通ることを確認する**

Run: `cargo test -p picture-tool-core metadata::exif`
Expected: PASS（9 テスト）

- [ ] **Step 5: コミット**

```bash
git add core/src/metadata/exif.rs core/src/metadata/mod.rs
git commit -m "feat(core): メタデータ編集: EXIF 側の読み取りと UCS-2LE のデコードを足す"
```

---

## Task 5: EXIF 側の書き込み

`ImageDescription` の条件分岐がこのタスクの本体。EXIF 仕様上 `ImageDescription` は
**ASCII 型**で日本語を入れると化ける。一方 ExifTool のソースには
`XPTitle is ignored by Windows Explorer if ImageDescription exists` という注記があり、
Microsoft 公式ドキュメント（XPTitle 優先と読める）と矛盾している。**どちらが正しいかは
Windows 実機がないため断定できない。** 両説から確実に導けるのは
「両者に矛盾する値を入れなければ、どちらが優先されても正しい値が出る」ことだけ。

**Files:**
- Modify: `core/src/metadata/exif.rs`

**Interfaces:**
- Consumes: Task 3 の `EditableMetadata` / `stars_to_percent`、Task 4 の定数と `ExifSide`
- Produces: `pub(super) fn apply(meta: &mut Metadata, values: &EditableMetadata);`
  `pub(super) fn to_ucs2le_nul(text: &str) -> Vec<u8>;`

- [ ] **Step 1: 失敗するテストを書く**

`core/src/metadata/exif.rs` の `mod tests` に追記:

```rust
use crate::metadata::EditableMetadata;

fn applied(values: EditableMetadata) -> ExifSide {
    let mut meta = Metadata::new();
    apply(&mut meta, &values);
    read(&meta)
}

#[test]
fn ucs2le_round_trips_japanese() {
    assert_eq!(to_ucs2le_nul("港"), vec![0x2F, 0x6E, 0x00, 0x00]);
    assert_eq!(from_ucs2le_nul(&to_ucs2le_nul("夕暮れの港")).as_deref(), Some("夕暮れの港"));
}

#[test]
fn a_title_lands_in_every_exif_slot_when_it_is_ascii() {
    let side = applied(EditableMetadata { title: Some("Harbour".into()), ..Default::default() });
    assert_eq!(side.title_xp.as_deref(), Some("Harbour"));
    assert_eq!(side.title_description.as_deref(), Some("Harbour"));
}

/// **日本語のタイトルでは `ImageDescription` を書かない。**
/// ASCII 型なので化ける。Windows がどちらを優先しても正しい値が出るよう、
/// 矛盾する値を残さないのが狙い
#[test]
fn a_japanese_title_skips_image_description() {
    let side = applied(EditableMetadata { title: Some("夕暮れの港".into()), ..Default::default() });
    assert_eq!(side.title_xp.as_deref(), Some("夕暮れの港"));
    assert_eq!(side.title_description, None);
}

/// 古い ASCII の `ImageDescription` が残ると、Windows でそちらが出る事故になる。
/// **上書きではなく削除**まで踏み込む
#[test]
fn a_japanese_title_deletes_an_existing_image_description() {
    let mut meta = Metadata::new();
    apply(&mut meta, &EditableMetadata { title: Some("Harbour".into()), ..Default::default() });
    // 前提条件: いったん書けていること。ここが空なら次の assert は自明に通る
    assert_eq!(read(&meta).title_description.as_deref(), Some("Harbour"));

    apply(&mut meta, &EditableMetadata { title: Some("夕暮れの港".into()), ..Default::default() });
    assert_eq!(read(&meta).title_description, None);
}

#[test]
fn a_comment_lands_in_both_exif_slots() {
    let side = applied(EditableMetadata { comment: Some("順光。もう少し寄れた".into()), ..Default::default() });
    assert_eq!(side.comment_xp.as_deref(), Some("順光。もう少し寄れた"));
    assert_eq!(side.comment_user.as_deref(), Some("順光。もう少し寄れた"));
}

/// **コメントは `ImageDescription` を一切読み書きしない**（タイトル用途と衝突するため）
#[test]
fn a_comment_never_touches_image_description() {
    let side = applied(EditableMetadata { comment: Some("hello".into()), ..Default::default() });
    assert_eq!(side.title_description, None);
}

#[test]
fn a_rating_is_written_as_the_official_percent() {
    let side = applied(EditableMetadata { rating: Some(3), ..Default::default() });
    assert_eq!(side.rating_percent, Some(50));
}

/// `Some(0)` は「未評価」であって★0 ではない。**書かない**
#[test]
fn zero_stars_removes_the_rating_instead_of_writing_zero() {
    let mut meta = Metadata::new();
    apply(&mut meta, &EditableMetadata { rating: Some(4), ..Default::default() });
    assert_eq!(read(&meta).rating_percent, Some(75)); // 前提条件

    apply(&mut meta, &EditableMetadata { rating: Some(0), ..Default::default() });
    assert_eq!(read(&meta).rating_percent, None);
}

/// 空欄で保存したらタグごと消す。空文字のタグが残ると、他アプリで
/// 「空のコメントがある」状態になる
#[test]
fn clearing_a_field_removes_the_tags() {
    let mut meta = Metadata::new();
    apply(&mut meta, &EditableMetadata {
        title: Some("Harbour".into()),
        comment: Some("hello".into()),
        rating: Some(5),
    });
    let before = read(&meta); // 前提条件
    assert!(before.title_xp.is_some() && before.comment_xp.is_some() && before.rating_percent.is_some());

    apply(&mut meta, &EditableMetadata::default());
    let after = read(&meta);
    assert!(after.title_xp.is_none());
    assert!(after.title_description.is_none());
    assert!(after.comment_xp.is_none());
    assert!(after.comment_user.is_none());
    assert!(after.rating_percent.is_none());
}
```

- [ ] **Step 2: 落ちることを確認する**

Run: `cargo test -p picture-tool-core metadata::exif`
Expected: FAIL（`cannot find function apply`）

- [ ] **Step 3: 実装する**

`core/src/metadata/exif.rs` に追記:

```rust
use crate::metadata::{stars_to_percent, EditableMetadata};

/// UCS-2LE（実質 UTF-16LE）＋ 2 バイトの NUL 終端に直す
pub(super) fn to_ucs2le_nul(text: &str) -> Vec<u8> {
    let mut bytes: Vec<u8> = text
        .encode_utf16()
        .flat_map(|unit| unit.to_le_bytes())
        .collect();
    bytes.extend_from_slice(&[0, 0]);
    bytes
}

/// `UserComment` の本文を組み立てる。**先頭 8 バイトは文字コード識別子**で、
/// 本文のバイト順は TIFF ヘッダの endian に従う
fn encode_user_comment(text: &str, endian: &Endian) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + text.len() * 2);
    out.extend_from_slice(ID_UNICODE);
    for unit in text.encode_utf16() {
        match endian {
            Endian::Little => out.extend_from_slice(&unit.to_le_bytes()),
            Endian::Big => out.extend_from_slice(&unit.to_be_bytes()),
        }
    }
    out
}

/// EXIF 側の 5 箇所を、`values` の内容で置き換える。
///
/// `None` の項目は**タグごと削除する**（空文字のタグを残さない）。
pub(super) fn apply(meta: &mut Metadata, values: &EditableMetadata) {
    let endian = meta.get_endian();

    // --- タイトル ---
    match &values.title {
        Some(title) if !title.is_empty() => {
            meta.set_tag(ExifTag::UnknownINT8U(
                to_ucs2le_nul(title),
                XP_TITLE,
                ExifTagGroup::GENERIC,
            ));
            if title.is_ascii() {
                meta.set_tag(ExifTag::ImageDescription(title.clone()));
            } else {
                // ASCII 型に日本語は入らない。古い ASCII 値が残ると
                // Windows でそちらが出るので消す（正しい値は XPTitle と dc:title にある）
                meta.remove_tag_by_hex_group(IMAGE_DESCRIPTION, ExifTagGroup::GENERIC);
            }
        }
        _ => {
            meta.remove_tag_by_hex_group(XP_TITLE, ExifTagGroup::GENERIC);
            meta.remove_tag_by_hex_group(IMAGE_DESCRIPTION, ExifTagGroup::GENERIC);
        }
    }

    // --- コメント（ImageDescription には一切触らない） ---
    match &values.comment {
        Some(comment) if !comment.is_empty() => {
            meta.set_tag(ExifTag::UnknownINT8U(
                to_ucs2le_nul(comment),
                XP_COMMENT,
                ExifTagGroup::GENERIC,
            ));
            meta.set_tag(ExifTag::UserComment(encode_user_comment(comment, &endian)));
        }
        _ => {
            meta.remove_tag_by_hex_group(XP_COMMENT, ExifTagGroup::GENERIC);
            meta.remove_tag_by_hex_group(USER_COMMENT, ExifTagGroup::EXIF);
        }
    }

    // --- レーティング ---
    match values.rating {
        Some(stars @ 1..=5) => {
            meta.set_tag(ExifTag::UnknownINT16U(
                vec![stars_to_percent(stars)],
                RATING_PERCENT,
                ExifTagGroup::GENERIC,
            ));
        }
        // Some(0) も None も「未評価」。0 を書くと Windows 側で
        // 「評価なし」と区別が付かなくなるうえ、タグだけが増える
        _ => {
            meta.remove_tag_by_hex_group(RATING_PERCENT, ExifTagGroup::GENERIC);
        }
    }
}
```

- [ ] **Step 4: 通ることを確認する**

Run: `cargo test -p picture-tool-core metadata::exif`
Expected: PASS（16 テスト）

- [ ] **Step 5: コミット**

**ここでは `make lint` を走らせない。** `-D warnings` の clippy は lib ターゲットを
`cfg(test)` 抜きで通すため、テストからしか呼ばれていない `exif::{read, apply}` /
`stars_to_percent` が `dead_code` で落ちる。実利用者（`read_metadata` /
`write_metadata`）が揃う **Task 9 Step 6** でまとめて掛ける。

```bash
git add core/src/metadata/exif.rs
git commit -m "feat(core): メタデータ編集: EXIF 側へ書き込む。日本語タイトルでは ImageDescription を消す"
```

---
## Task 6: XMP パケットの取り出しと読み取り

**Files:**
- Create: `core/src/metadata/xmp.rs`
- Modify: `core/src/metadata/mod.rs`（`mod xmp;`）

**Interfaces:**
- Consumes: なし（`img_parts::jpeg::Jpeg` と `quick_xml`）
- Produces:
  ```rust
  pub(super) const XMP_ID: &[u8];      // b"http://ns.adobe.com/xap/1.0/\0"
  pub(super) const NS_RDF: &str;
  pub(super) const NS_XMP: &str;
  pub(super) const NS_DC: &str;
  pub(super) const NS_MS_PHOTO: &str;
  pub(super) struct XmpSide {
      pub title: Option<String>,
      pub description: Option<String>,
      pub rating: Option<u8>,
      pub ms_rating_percent: Option<u16>,
  }
  pub(super) fn extract(jpeg: &img_parts::jpeg::Jpeg) -> Option<String>;
  pub(super) fn read(packet: &str) -> anyhow::Result<XmpSide>;
  ```

- [ ] **Step 1: モジュールを登録し、失敗するテストを書く**

先に `core/src/metadata/mod.rs` へ `mod xmp;` を足す（Task 3 と同じ理由）。
そのうえで `core/src/metadata/xmp.rs` の末尾に:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// 要素形式（Lightroom が `dc:title` / `dc:description` を書く形）
    const ELEMENT_FORM: &str = r#"<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about=""
    xmlns:dc="http://purl.org/dc/elements/1.1/"
    xmlns:xmp="http://ns.adobe.com/xap/1.0/"
    xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/"
    crs:Exposure2012="+0.35">
   <xmp:Rating>4</xmp:Rating>
   <dc:title><rdf:Alt><rdf:li xml:lang="x-default">夕暮れの港</rdf:li></rdf:Alt></dc:title>
   <dc:description><rdf:Alt><rdf:li xml:lang="x-default">順光</rdf:li></rdf:Alt></dc:description>
  </rdf:Description>
 </rdf:RDF>
</x:xmpmeta><?xpacket end="w"?>"#;

    /// 圧縮形式（単純値を属性で書く形）
    const ATTRIBUTE_FORM: &str = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about=""
    xmlns:xmp="http://ns.adobe.com/xap/1.0/"
    xmlns:MicrosoftPhoto="http://ns.microsoft.com/photo/1.0/"
    xmp:Rating="2" MicrosoftPhoto:Rating="25"/>
 </rdf:RDF>
</x:xmpmeta>"#;

    #[test]
    fn reads_properties_written_as_elements() {
        let side = read(ELEMENT_FORM).unwrap();
        assert_eq!(side.rating, Some(4));
        assert_eq!(side.title.as_deref(), Some("夕暮れの港"));
        assert_eq!(side.description.as_deref(), Some("順光"));
    }

    #[test]
    fn reads_properties_written_as_attributes() {
        let side = read(ATTRIBUTE_FORM).unwrap();
        assert_eq!(side.rating, Some(2));
        assert_eq!(side.ms_rating_percent, Some(25));
    }

    /// **接頭辞ではなく名前空間 URI で判定する。**
    /// `xmp:` `dc:` は慣習であって規約ではない
    #[test]
    fn resolves_by_namespace_uri_not_by_prefix() {
        let packet = r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about="" xmlns:zz="http://ns.adobe.com/xap/1.0/" zz:Rating="5"/>
 </rdf:RDF>"#;
        assert_eq!(read(packet).unwrap().rating, Some(5));
    }

    /// 他アプリが書いた 0（未評価）と -1（rejected）は★にしない
    #[test]
    fn zero_and_rejected_ratings_read_as_unrated() {
        for value in ["0", "-1"] {
            let packet = format!(
                r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about="" xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmp:Rating="{}"/>
 </rdf:RDF>"#,
                value
            );
            assert_eq!(read(&packet).unwrap().rating, None, "xmp:Rating={}", value);
        }
    }

    /// `xml:lang` の付いていない `rdf:li` しかない場合も拾う
    #[test]
    fn falls_back_to_the_first_alternative_without_x_default() {
        let packet = r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about="" xmlns:dc="http://purl.org/dc/elements/1.1/">
   <dc:title><rdf:Alt><rdf:li xml:lang="ja-JP">港</rdf:li></rdf:Alt></dc:title>
  </rdf:Description>
 </rdf:RDF>"#;
        assert_eq!(read(packet).unwrap().title.as_deref(), Some("港"));
    }

    /// 前提条件つきの否定形。XMP を持たない JPEG から取り出そうとしても落ちない
    #[test]
    fn extract_returns_none_when_there_is_no_xmp_segment() {
        use img_parts::jpeg::Jpeg;
        use img_parts::Bytes;
        let bytes = std::fs::read(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/camera-direct.jpg"),
        )
        .unwrap();
        let jpeg = Jpeg::from_bytes(Bytes::from(bytes)).unwrap();
        // 前提条件: EXIF の APP1 は在る（＝「APP1 が無いから None」ではない）
        assert!(jpeg
            .segments_by_marker(img_parts::jpeg::markers::APP1)
            .any(|s| s.contents().starts_with(b"Exif\0\0")));
        assert_eq!(extract(&jpeg), None);
    }
}
```

- [ ] **Step 2: 落ちることを確認する**

Run: `cargo test -p picture-tool-core metadata::xmp`
Expected: FAIL（未実装）

- [ ] **Step 3: 実装する**

`core/src/metadata/xmp.rs`:

```rust
//! XMP パケット（APP1 セグメント）の読み書き。
//!
//! **既存のパケットを削除して作り直してはならない。** Lightroom は現像設定を `crs:`
//! に、他のツールも独自の名前空間に大量のデータを書き込む。本ツールが扱う 4 プロパティ
//! だけを差し替え、他はすべて元のまま残す（spec の必須条件。ピクセル不変と同格）。
//!
//! 判定は**接頭辞ではなく名前空間 URI**で行う。`xmp:` `dc:` は慣習であって規約ではなく、
//! 書き手が別の接頭辞を使っていても同じプロパティである。

use anyhow::Result;
use img_parts::jpeg::{markers, Jpeg};
use quick_xml::events::{BytesStart, Event};
use quick_xml::name::{Namespace, ResolveResult};
use quick_xml::NsReader;

/// APP1 の XMP パケットに前置きされる識別子（末尾の NUL 込み）
pub(super) const XMP_ID: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";

pub(super) const NS_RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
pub(super) const NS_XMP: &str = "http://ns.adobe.com/xap/1.0/";
pub(super) const NS_DC: &str = "http://purl.org/dc/elements/1.1/";
/// **末尾のスラッシュに注意。** 落とすと Windows 側から読めない
pub(super) const NS_MS_PHOTO: &str = "http://ns.microsoft.com/photo/1.0/";

/// 本ツールが差し替える 4 プロパティ。(名前空間 URI, ローカル名)
pub(super) const OWNED: [(&str, &str); 4] = [
    (NS_XMP, "Rating"),
    (NS_MS_PHOTO, "Rating"),
    (NS_DC, "title"),
    (NS_DC, "description"),
];

/// XMP 側から読めた生の値。優先順位の判断はここではしない（`mod.rs` の責務）
#[derive(Debug, Default)]
pub(super) struct XmpSide {
    pub title: Option<String>,
    pub description: Option<String>,
    /// `xmp:Rating`（0〜5）。0 と -1（rejected）は `None` に落とす
    pub rating: Option<u8>,
    pub ms_rating_percent: Option<u16>,
}

/// JPEG から XMP パケットの XML を取り出す
pub(super) fn extract(jpeg: &Jpeg) -> Option<String> {
    jpeg.segments_by_marker(markers::APP1).find_map(|segment| {
        let xml = segment.contents().strip_prefix(XMP_ID)?;
        std::str::from_utf8(xml).ok().map(str::to_owned)
    })
}

fn is_owned_name(ns: &ResolveResult, local: &[u8]) -> bool {
    OWNED.iter().any(|(uri, name)| matches(ns, local, uri, name))
}

fn uri_of(ns: &ResolveResult) -> Option<&[u8]> {
    match ns {
        ResolveResult::Bound(Namespace(uri)) => Some(uri),
        _ => None,
    }
}

fn matches(ns: &ResolveResult, local: &[u8], uri: &str, name: &str) -> bool {
    uri_of(ns) == Some(uri.as_bytes()) && local == name.as_bytes()
}

/// `xmp:Rating` の文字列を★にする。
/// 0（未評価）と -1（rejected）は `None`。★0 はパーセント系に表現できないため
fn parse_stars(text: &str) -> Option<u8> {
    let value: f64 = text.trim().parse().ok()?;
    let rounded = value.round();
    (1.0..=5.0).contains(&rounded).then(|| rounded as u8)
}

pub(super) fn read(packet: &str) -> Result<XmpSide> {
    let mut reader = NsReader::from_str(packet);
    let mut side = XmpSide::default();

    loop {
        let (ns, event) = reader.read_resolved_event()?;
        match event {
            Event::Eof => break,
            Event::Empty(ref start) => {
                if matches(&ns, start.local_name().as_ref(), NS_RDF, "Description") {
                    read_compact_attributes(&reader, start, &mut side)?;
                }
            }
            Event::Start(ref start) => {
                let local = start.local_name().as_ref().to_vec();
                if matches(&ns, &local, NS_RDF, "Description") {
                    read_compact_attributes(&reader, start, &mut side)?;
                    continue;
                }
                // **本ツールの 4 プロパティ以外では中身を読み進めない。**
                // ここで無条件に read_first_text を呼ぶと、x:xmpmeta や rdf:RDF の
                // 中身まで丸ごと飲み込んで、以降が一切パースされなくなる
                if !is_owned_name(&ns, &local) {
                    continue;
                }
                // URI を写して ns の借用を終わらせてから &mut reader を使う
                let Some(uri) = uri_of(&ns).map(<[u8]>::to_vec) else {
                    continue;
                };
                let end = start.to_end().into_owned();
                let text = read_first_text(&mut reader, &end)?;
                let Some(text) = text else { continue };
                match (uri.as_slice(), local.as_slice()) {
                    (u, b"Rating") if u == NS_XMP.as_bytes() => side.rating = parse_stars(&text),
                    (u, b"Rating") if u == NS_MS_PHOTO.as_bytes() => {
                        side.ms_rating_percent = text.trim().parse().ok()
                    }
                    (u, b"title") if u == NS_DC.as_bytes() => side.title = Some(text),
                    (u, b"description") if u == NS_DC.as_bytes() => side.description = Some(text),
                    _ => {}
                }
            }
            _ => {}
        }
    }
    Ok(side)
}

/// 圧縮形式（`rdf:Description` の属性として書かれた単純値）を拾う
fn read_compact_attributes(
    reader: &NsReader<&[u8]>,
    start: &BytesStart<'_>,
    side: &mut XmpSide,
) -> Result<()> {
    for attribute in start.attributes() {
        let attribute = attribute?;
        let (ns, local) = reader.resolve_attribute(attribute.key);
        let Some(uri) = uri_of(&ns) else { continue };
        let value = attribute.unescape_value()?.into_owned();
        match (uri, local.as_ref()) {
            (u, b"Rating") if u == NS_XMP.as_bytes() => side.rating = parse_stars(&value),
            (u, b"Rating") if u == NS_MS_PHOTO.as_bytes() => {
                side.ms_rating_percent = value.trim().parse().ok()
            }
            (u, b"title") if u == NS_DC.as_bytes() => side.title = Some(value),
            (u, b"description") if u == NS_DC.as_bytes() => side.description = Some(value),
            _ => {}
        }
    }
    Ok(())
}

/// 要素の中から最初の非空テキストを取る。`rdf:Alt` があれば
/// `xml:lang="x-default"` の `rdf:li` を優先する
fn read_first_text(
    reader: &mut NsReader<&[u8]>,
    end: &quick_xml::events::BytesEnd<'_>,
) -> Result<Option<String>> {
    let mut preferred: Option<String> = None;
    let mut first: Option<String> = None;
    let mut in_default = false;

    loop {
        let (ns, event) = reader.read_resolved_event()?;
        match event {
            Event::Eof => break,
            Event::Start(ref start) if matches(&ns, start.local_name().as_ref(), NS_RDF, "li") => {
                in_default = start.attributes().flatten().any(|attribute| {
                    attribute.key.as_ref() == b"xml:lang" && attribute.value.as_ref() == b"x-default"
                });
            }
            Event::End(ref e) if e.name() == end.name() => break,
            Event::End(ref e) if matches(&ns, e.local_name().as_ref(), NS_RDF, "li") => {
                in_default = false;
            }
            Event::Text(text) => {
                let text = text.unescape()?.trim().to_string();
                if text.is_empty() {
                    continue;
                }
                if in_default && preferred.is_none() {
                    preferred = Some(text.clone());
                }
                if first.is_none() {
                    first = Some(text);
                }
            }
            _ => {}
        }
    }
    Ok(preferred.or(first))
}
```

> **借用エラーが出たらここを疑う。** `NsReader::read_resolved_event` が返す
> `ResolveResult` は**リーダー自身を借りている**。`ns` が生きているあいだは
> `&mut reader`（`read_first_text` / `read_to_end`）も `&reader`
> （`resolve_attribute`）も使えない。URI とローカル名を `Vec<u8>` に写して
> `ns` の借用を終わらせてから、リーダーを触ること。

- [ ] **Step 4: 通ることを確認する**

Run: `cargo test -p picture-tool-core metadata::xmp`
Expected: PASS（6 テスト）

- [ ] **Step 5: コミット**

```bash
git add core/src/metadata/xmp.rs core/src/metadata/mod.rs
git commit -m "feat(core): メタデータ編集: XMP パケットを名前空間 URI で読む"
```

---

## Task 7: XMP パケットのプロパティ単位の差し替え（既存名前空間の保持）

**spec が「ピクセル不変と同格の必須条件」と書いている要件。** spec の実測は
**パケットを削除して作り直す方式**で行われており、この要件は未実装・未検証のまま
残っている（spec §検証済みの事実と、未検証のリスク）。ここで独立して潰す。

**名前空間の宣言は、差し込むプロパティ要素自身に付ける。**
`rdf:Description` 側に足すと、同じ接頭辞が別の URI に束縛されていた場合に
兄弟のプロパティを壊しうる。要素に付ければ影響はその要素の中だけで閉じる。

**Files:**
- Modify: `core/src/metadata/xmp.rs`

**Interfaces:**
- Consumes: Task 3 の `EditableMetadata` / `stars_to_percent`、Task 6 の定数・`OWNED`・`matches` / `is_owned_name`
- Produces:
  ```rust
  pub(super) fn patch(packet: &str, values: &EditableMetadata) -> anyhow::Result<String>;
  pub(super) fn new_packet(values: &EditableMetadata) -> String;
  pub(super) fn install(jpeg: &mut img_parts::jpeg::Jpeg, packet: &str) -> anyhow::Result<()>;
  ```

- [ ] **Step 1: 失敗するテストを書く**

`core/src/metadata/xmp.rs` の `mod tests` に追記:

```rust
use crate::metadata::EditableMetadata;

/// 名前空間つきで、本ツールの 4 プロパティ**以外**をすべて列挙する。
/// 「1 バイトも変わらない」の検査対象を、読み取りとは別の経路で取り出す
fn foreign_content(packet: &str) -> Vec<String> {
    let mut reader = NsReader::from_str(packet);
    let mut out = Vec::new();
    loop {
        let (ns, event) = reader.read_resolved_event().unwrap();
        match event {
            Event::Eof => break,
            Event::Start(ref s) | Event::Empty(ref s) => {
                let uri = uri_of(&ns).map(|u| String::from_utf8_lossy(u).into_owned());
                let local = String::from_utf8_lossy(s.local_name().as_ref()).into_owned();
                let is_owned = OWNED.iter().any(|(o_uri, o_local)| {
                    uri.as_deref() == Some(*o_uri) && local == *o_local
                });
                if !is_owned {
                    out.push(format!("<{:?}:{}>", uri, local));
                }
                for attribute in s.attributes() {
                    let attribute = attribute.unwrap();
                    let (a_ns, a_local) = reader.resolve_attribute(attribute.key);
                    let a_uri = uri_of(&a_ns).map(|u| String::from_utf8_lossy(u).into_owned());
                    let a_local = String::from_utf8_lossy(a_local.as_ref()).into_owned();
                    let is_owned = OWNED.iter().any(|(o_uri, o_local)| {
                        a_uri.as_deref() == Some(*o_uri) && a_local == *o_local
                    });
                    if !is_owned {
                        out.push(format!(
                            "@{:?}:{}={}",
                            a_uri,
                            a_local,
                            attribute.unescape_value().unwrap()
                        ));
                    }
                }
            }
            _ => {}
        }
    }
    out.sort();
    out
}

#[test]
fn patching_leaves_every_other_namespace_untouched() {
    let before = foreign_content(ELEMENT_FORM);
    // 前提条件: 検査対象（crs:）が本当に入っていること。
    // ここが空だと「壊れていない」は自明に通ってしまう
    assert!(
        before.iter().any(|item| item.contains("camera-raw-settings")),
        "フィクスチャに他名前空間が無い: {:?}",
        before
    );

    let patched = patch(
        ELEMENT_FORM,
        &EditableMetadata {
            title: Some("新しい題".into()),
            comment: Some("新しい注".into()),
            rating: Some(2),
        },
    )
    .unwrap();

    assert_eq!(foreign_content(&patched), before);
}

/// 落とす属性が無い `rdf:Description` は、**タグごとバイト列で保たれる**こと。
/// 組み直すと属性の区切りが単一スペースに正規化される
#[test]
fn a_description_without_owned_attributes_is_copied_byte_for_byte() {
    let patched = patch(ELEMENT_FORM, &EditableMetadata { rating: Some(1), ..Default::default() }).unwrap();
    let original_tag = &ELEMENT_FORM[ELEMENT_FORM.find("<rdf:Description").unwrap()
        ..ELEMENT_FORM.find("crs:Exposure2012=\"+0.35\">").unwrap() + "crs:Exposure2012=\"+0.35\">".len()];
    assert!(patched.contains(original_tag), "改行込みの属性の並びが変わった:\n{}", patched);
}

#[test]
fn patching_replaces_the_four_owned_properties() {
    let patched = patch(
        ELEMENT_FORM,
        &EditableMetadata {
            title: Some("新しい題".into()),
            comment: Some("新しい注".into()),
            rating: Some(2),
        },
    )
    .unwrap();

    let side = read(&patched).unwrap();
    assert_eq!(side.title.as_deref(), Some("新しい題"));
    assert_eq!(side.description.as_deref(), Some("新しい注"));
    assert_eq!(side.rating, Some(2));
    assert_eq!(side.ms_rating_percent, Some(25));
}

/// 圧縮形式で書かれていた古い値が**属性として残らない**こと。
/// 要素だけ差し替えて属性を残すと、読み手によって新旧どちらも出うる
#[test]
fn patching_drops_owned_values_written_as_attributes() {
    let patched = patch(
        ATTRIBUTE_FORM,
        &EditableMetadata { rating: Some(5), ..Default::default() },
    )
    .unwrap();
    assert!(!patched.contains("xmp:Rating=\""), "属性形式の古い値が残っている: {}", patched);
    assert_eq!(read(&patched).unwrap().rating, Some(5));
}

/// 空欄で保存したらプロパティごと消える
#[test]
fn patching_with_empty_values_removes_the_properties() {
    let patched = patch(ELEMENT_FORM, &EditableMetadata::default()).unwrap();
    let side = read(&patched).unwrap();
    assert_eq!(side.title, None);
    assert_eq!(side.description, None);
    assert_eq!(side.rating, None);
    // 他名前空間は残っている
    assert!(patched.contains("camera-raw-settings"));
}

#[test]
fn a_new_packet_is_readable_by_our_own_reader() {
    let packet = new_packet(&EditableMetadata {
        title: Some("港".into()),
        comment: Some("順光".into()),
        rating: Some(3),
    });
    let side = read(&packet).unwrap();
    assert_eq!(side.title.as_deref(), Some("港"));
    assert_eq!(side.description.as_deref(), Some("順光"));
    assert_eq!(side.rating, Some(3));
    assert_eq!(side.ms_rating_percent, Some(50));
}

/// XML の特殊文字を含むテキストが往復すること
#[test]
fn special_characters_survive_the_round_trip() {
    let text = r#"a < b & c > d "e" 'f'"#;
    let packet = new_packet(&EditableMetadata { title: Some(text.into()), ..Default::default() });
    assert_eq!(read(&packet).unwrap().title.as_deref(), Some(text));
}

/// **push すると EOI の後ろに置かれ、再パース時に消える**（spec の落とし穴）
#[test]
fn install_puts_the_packet_before_the_scan() {
    use img_parts::jpeg::Jpeg;
    use img_parts::Bytes;
    let bytes = std::fs::read(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/camera-direct.jpg"),
    )
    .unwrap();
    let mut jpeg = Jpeg::from_bytes(Bytes::from(bytes)).unwrap();
    // 前提条件: いまは XMP を持っていない
    assert_eq!(extract(&jpeg), None);

    let packet = new_packet(&EditableMetadata { rating: Some(1), ..Default::default() });
    install(&mut jpeg, &packet).unwrap();

    // 書き出して読み直しても残っていること（＝ SOS より前に入った）
    let encoded = jpeg.encoder().bytes();
    let reparsed = Jpeg::from_bytes(encoded).unwrap();
    assert_eq!(read(&extract(&reparsed).unwrap()).unwrap().rating, Some(1));
}

/// 2 回書いてもセグメントが増えない
#[test]
fn install_replaces_the_existing_packet_instead_of_adding_one() {
    use img_parts::jpeg::{markers, Jpeg};
    use img_parts::Bytes;
    let bytes = std::fs::read(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/lightroom-xmp.jpg"),
    )
    .unwrap();
    let mut jpeg = Jpeg::from_bytes(Bytes::from(bytes)).unwrap();
    let before = jpeg.segments_by_marker(markers::APP1).count();
    // 前提条件: すでに XMP を持っている
    assert!(extract(&jpeg).is_some());

    for stars in [1u8, 5] {
        let packet = patch(
            &extract(&jpeg).unwrap(),
            &EditableMetadata { rating: Some(stars), ..Default::default() },
        )
        .unwrap();
        install(&mut jpeg, &packet).unwrap();
    }

    assert_eq!(jpeg.segments_by_marker(markers::APP1).count(), before);
    assert_eq!(read(&extract(&jpeg).unwrap()).unwrap().rating, Some(5));
}
```

- [ ] **Step 2: 落ちることを確認する**

Run: `cargo test -p picture-tool-core metadata::xmp`
Expected: FAIL（`cannot find function patch`）

- [ ] **Step 3: 実装する**

`core/src/metadata/xmp.rs` に追記:

```rust
use crate::metadata::{stars_to_percent, EditableMetadata};
use anyhow::{anyhow, bail};
use img_parts::jpeg::JpegSegment;
use img_parts::Bytes;
use quick_xml::events::BytesText;
use quick_xml::Writer;

/// APP1 セグメントの内容の上限。長さフィールドは 2 バイトで自身を含む
const MAX_APP1_CONTENTS: usize = 65_533;

/// 既存パケットのうち、本ツールが扱う 4 プロパティ**だけ**を差し替える。
///
/// 他名前空間のデータには一切触らない。`rdf:Description` が 1 つも無い（＝
/// 差し込み先が無い）壊れたパケットのときだけ、新規パケットに置き換える。
pub(super) fn patch(packet: &str, values: &EditableMetadata) -> Result<String> {
    let mut reader = NsReader::from_str(packet);
    let mut writer = Writer::new(Vec::new());
    let mut injected = false;

    loop {
        let (ns, event) = reader.read_resolved_event()?;
        match event {
            Event::Eof => break,

            Event::Start(start) => {
                if is_owned_name(&ns, start.local_name().as_ref()) {
                    // 中身ごと捨てる（あとで作り直す）
                    let end = start.to_end().into_owned();
                    reader.read_to_end(end.name())?;
                    continue;
                }
                if matches(&ns, start.local_name().as_ref(), NS_RDF, "Description") {
                    // **落とす属性が無いなら元の start タグをそのまま流す。**
                    // 組み直すと quick-xml の push_attribute が区切りを単一スペースに
                    // 揃えるため、Lightroom が複数行に分けて書いた crs: の属性群は
                    // 値は保たれても整形が変わる。触らなくてよいものは触らない
                    match strip_owned_attributes(&reader, &start)? {
                        Some(cleaned) => writer.write_event(Event::Start(cleaned))?,
                        None => writer.write_event(Event::Start(start.clone()))?,
                    }
                    if !injected {
                        write_properties(&mut writer, values)?;
                        injected = true;
                    }
                    continue;
                }
                writer.write_event(Event::Start(start))?;
            }

            Event::Empty(start) => {
                if is_owned_name(&ns, start.local_name().as_ref()) {
                    continue;
                }
                if matches(&ns, start.local_name().as_ref(), NS_RDF, "Description") {
                    let tag = strip_owned_attributes(&reader, &start)?.unwrap_or_else(|| start.clone());
                    if injected {
                        writer.write_event(Event::Empty(tag))?;
                    } else {
                        // 空要素のままでは子を持てないので Start + 子 + End に開く
                        let end = tag.to_end().into_owned();
                        writer.write_event(Event::Start(tag))?;
                        write_properties(&mut writer, values)?;
                        writer.write_event(Event::End(end))?;
                        injected = true;
                    }
                    continue;
                }
                writer.write_event(Event::Empty(start))?;
            }

            other => writer.write_event(other)?,
        }
    }

    if !injected {
        // 差し込み先が無い＝ rdf:Description を持たない壊れたパケット。
        // ここだけは作り直す（他名前空間のデータもそもそも入らない形なので失うものが無い）
        return Ok(new_packet(values));
    }
    Ok(String::from_utf8(writer.into_inner())?)
}

/// `rdf:Description` の属性から、本ツールの 4 プロパティだけを落とす。
/// 要素形式だけ差し替えて属性形式を残すと、読み手によって新旧どちらも出うる。
///
/// **落とすものが無ければ `None` を返す。** 呼び出し側は元の start タグをそのまま
/// 書き戻す ── 組み直すと属性の区切りが単一スペースへ正規化されるため、
/// 触る必要が無いときは触らない。
fn strip_owned_attributes(
    reader: &NsReader<&[u8]>,
    start: &BytesStart<'_>,
) -> Result<Option<BytesStart<'static>>> {
    let mut kept = Vec::new();
    let mut dropped = false;
    for attribute in start.attributes() {
        let attribute = attribute?;
        let (ns, local) = reader.resolve_attribute(attribute.key);
        if is_owned_name(&ns, local.as_ref()) {
            dropped = true;
        } else {
            kept.push(attribute);
        }
    }
    if !dropped {
        return Ok(None);
    }
    let name = String::from_utf8(start.name().as_ref().to_vec())?;
    let mut cleaned = BytesStart::new(name);
    for attribute in kept {
        cleaned.push_attribute(attribute);
    }
    Ok(Some(cleaned))
}

fn write_properties<W: std::io::Write>(
    writer: &mut Writer<W>,
    values: &EditableMetadata,
) -> Result<()> {
    let xml = properties_xml(values);
    if xml.is_empty() {
        return Ok(());
    }
    // 組み立て済みの断片をそのまま流す。中の利用者テキストは properties_xml が
    // escape 済みなので、ここで再エスケープさせない
    writer.write_event(Event::Text(BytesText::from_escaped(xml)))?;
    Ok(())
}

fn properties_xml(values: &EditableMetadata) -> String {
    let mut out = String::new();
    if let Some(stars @ 1..=5) = values.rating {
        out.push_str(&format!(
            r#"<xmp:Rating xmlns:xmp="{NS_XMP}">{stars}</xmp:Rating>"#
        ));
        out.push_str(&format!(
            r#"<MicrosoftPhoto:Rating xmlns:MicrosoftPhoto="{NS_MS_PHOTO}">{}</MicrosoftPhoto:Rating>"#,
            stars_to_percent(stars)
        ));
    }
    if let Some(title) = values.title.as_deref().filter(|t| !t.is_empty()) {
        out.push_str(&lang_alt("dc:title", title));
    }
    if let Some(comment) = values.comment.as_deref().filter(|c| !c.is_empty()) {
        out.push_str(&lang_alt("dc:description", comment));
    }
    out
}

/// `dc:title` / `dc:description` は Lang Alt（`rdf:Alt` + `rdf:li`）。
///
/// **名前空間の宣言を要素自身に付ける。** `rdf:Description` 側に足すと、
/// 同じ接頭辞が別の URI に束縛されていた場合に兄弟のプロパティを壊しうる
fn lang_alt(name: &str, text: &str) -> String {
    let escaped = quick_xml::escape::escape(text);
    format!(
        r#"<{name} xmlns:dc="{NS_DC}" xmlns:rdf="{NS_RDF}"><rdf:Alt><rdf:li xml:lang="x-default">{escaped}</rdf:li></rdf:Alt></{name}>"#
    )
}

/// パケットが存在しない場合にだけ使う。**既存パケットの置き換えには使わない**
pub(super) fn new_packet(values: &EditableMetadata) -> String {
    format!(
        "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\
<x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\
<rdf:RDF xmlns:rdf=\"{NS_RDF}\">\
<rdf:Description rdf:about=\"\">{}</rdf:Description>\
</rdf:RDF></x:xmpmeta><?xpacket end=\"w\"?>",
        properties_xml(values)
    )
}

/// XMP パケットを APP1 として据え付ける。既存の XMP セグメントは置き換える
pub(super) fn install(jpeg: &mut Jpeg, packet: &str) -> Result<()> {
    let mut contents = Vec::with_capacity(XMP_ID.len() + packet.len());
    contents.extend_from_slice(XMP_ID);
    contents.extend_from_slice(packet.as_bytes());
    if contents.len() > MAX_APP1_CONTENTS {
        bail!(
            "XMP が APP1 の上限（{} バイト）を超えました。既存のデータを削らずには収まりません",
            MAX_APP1_CONTENTS
        );
    }

    // EXIF の APP1 は残す。落とすのは XMP の APP1 だけ
    jpeg.segments_mut()
        .retain(|segment| {
            !(segment.marker() == markers::APP1 && segment.contents().starts_with(XMP_ID))
        });

    // **push ではなく insert。** push すると EOI の後ろに置かれ、再パース時に消える。
    // 置き場所は Exif の APP1 の直後 ── XMP 仕様が推奨する位置であり、
    // 読み手を選ばない。Exif が無ければ SOS の直前に落とす（規格上はどちらも合法）
    let scan = jpeg
        .segments()
        .iter()
        .position(|segment| segment.marker() == markers::SOS)
        .ok_or_else(|| anyhow!("SOS セグメントが見つかりません"))?;
    let at = jpeg
        .segments()
        .iter()
        .position(|segment| {
            segment.marker() == markers::APP1 && segment.contents().starts_with(b"Exif\0\0")
        })
        .map_or(scan, |exif| exif + 1);
    jpeg.segments_mut().insert(
        at,
        JpegSegment::new_with_contents(markers::APP1, Bytes::from(contents)),
    );
    Ok(())
}
```

- [ ] **Step 4: 通ることを確認する**

Run: `cargo test -p picture-tool-core metadata::xmp`
Expected: PASS（14 テスト）

**`patching_leaves_every_other_namespace_untouched` が落ちたら、
そこで止めて設計を報告する。** これは spec が「いかなる理由があっても
削除して作り直す実装は採用しない」と書いた要件そのものであり、
迂回してはならない。

- [ ] **Step 5: コミット**

Task 5 と同じ理由でここでも `make lint` は走らせない
（`xmp::{patch, new_packet, install}` の実利用者は Task 9 で揃う）。

```bash
git add core/src/metadata/xmp.rs
git commit -m "feat(core): メタデータ編集: XMP を4プロパティだけ差し替え、他名前空間を保持する"
```

---
## Task 8: 優先順位のマージと食い違い警告（`read_metadata`）

書くときは全箇所に同じ値を書くので迷わない。**問題は読むとき**である。
Windows で編集すれば `XPComment` だけが、Lightroom で編集すれば `dc:description` だけが
新しくなり、**どちらが新しいかを知る手段はファイル内に存在しない。**
黙って片方を捨てるのが最もまずいので、優先順位を固定したうえで**捨てた値の存在を
警告として上へ返す**。

**Files:**
- Modify: `core/src/metadata/mod.rs`

**Interfaces:**
- Consumes: Task 4 の `exif::read` / `ExifSide`、Task 6 の `xmp::extract` / `xmp::read` / `XmpSide`
- Produces: `pub fn read_metadata(path: &std::path::Path) -> anyhow::Result<MetadataRead>;`

- [ ] **Step 1: 失敗するテストを書く**

`core/src/metadata/mod.rs` の `mod tests` に追記:

```rust
use std::path::PathBuf;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

/// テンポラリに複製して返す。元のフィクスチャは書き換えない
fn scratch(name: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(name);
    std::fs::copy(fixture(name), &path).unwrap();
    (dir, path)
}

#[test]
fn reading_a_file_without_any_editable_metadata_yields_none() {
    let (_dir, path) = scratch("camera-direct.jpg");
    let read = read_metadata(&path).unwrap();
    assert_eq!(read.metadata, EditableMetadata::default());
    assert!(read.warnings.is_empty());
}

```

`core/Cargo.toml` の `[dev-dependencies]` に `tempfile` は既にある。

- [ ] **Step 2: 落ちることを確認する**

Run: `cargo test -p picture-tool-core metadata::tests`
Expected: FAIL（`cannot find function read_metadata`）

- [ ] **Step 3: 実装する**

`core/src/metadata/mod.rs` に追記:

```rust
use anyhow::{Context, Result};
use img_parts::jpeg::Jpeg;
use img_parts::Bytes;
use little_exif::filetype::FileExtension;
use little_exif::metadata::Metadata;
use std::path::Path;

/// 1 箇所ぶんの候補。`&'static str` は利用者に見せる場所の名前
type Candidate = (&'static str, Option<String>);

/// 優先順位の高い順に並んだ候補から 1 つ採る。
/// **採らなかった側に別の値があれば警告に積む**（core は判断せず事実だけを上へ渡す）
fn pick(label: &str, candidates: Vec<Candidate>, warnings: &mut Vec<String>) -> Option<String> {
    let present: Vec<Candidate> = candidates.into_iter().filter(|(_, v)| v.is_some()).collect();
    let (chosen_where, chosen) = present.first()?.clone();

    let others: Vec<String> = present
        .iter()
        .skip(1)
        .filter(|(_, value)| *value != chosen)
        .map(|(place, value)| format!("{} = 「{}」", place, value.as_deref().unwrap_or("")))
        .collect();

    if !others.is_empty() {
        warnings.push(format!(
            "{}に、他のアプリが書いた別の値があります（{} を採用。ほかに {}）。保存すると揃います",
            label,
            chosen_where,
            others.join(" / ")
        ));
    }
    chosen
}

/// 画像から、人が編集する 3 項目を読む。
///
/// 箇所ごとに値が食い違っていた場合は優先順位で 1 つ採り、
/// 捨てた値の存在を `warnings` に積む。
pub fn read_metadata(path: &Path) -> Result<MetadataRead> {
    let raw = std::fs::read(path)
        .with_context(|| format!("ファイルを読めません: {}", path.display()))?;

    let mut warnings = Vec::new();

    // Exif が無い画像でも読み取り自体は成立させる（全項目 None になるだけ）。
    // ただし**解析に失敗したことは黙らない** ── そのまま保存すると Exif を
    // 作り直すことになるので、利用者に伝わっている必要がある
    // （書き込み側の `build` は同じ状況で Err にする）
    let exif_meta = match Metadata::new_from_vec(&raw, FileExtension::JPEG) {
        Ok(meta) => meta,
        Err(error) => {
            warnings.push(format!("Exif を解析できませんでした: {}", error));
            Metadata::new()
        }
    };
    let exif_side = exif::read(&exif_meta);

    let jpeg = Jpeg::from_bytes(Bytes::from(raw))
        .with_context(|| format!("JPEG として読めません: {}", path.display()))?;
    let xmp_side = match xmp::extract(&jpeg) {
        Some(packet) => xmp::read(&packet)?,
        None => xmp::XmpSide::default(),
    };

    let title = pick(
        "タイトル",
        vec![
            ("XMP dc:title", xmp_side.title.clone()),
            ("Exif XPTitle", exif_side.title_xp.clone()),
            ("Exif ImageDescription", exif_side.title_description.clone()),
        ],
        &mut warnings,
    );

    let comment = pick(
        "コメント",
        vec![
            ("XMP dc:description", xmp_side.description.clone()),
            ("Exif UserComment", exif_side.comment_user.clone()),
            ("Exif XPComment", exif_side.comment_xp.clone()),
        ],
        &mut warnings,
    );

    // ★は型が違うので、いったん文字列に寄せてから同じ仕組みに通す
    let rating = pick(
        "レーティング",
        vec![
            ("XMP xmp:Rating", xmp_side.rating.map(|s| s.to_string())),
            (
                "Exif RatingPercent",
                exif_side.rating_percent.and_then(percent_to_stars).map(|s| s.to_string()),
            ),
            (
                "XMP MicrosoftPhoto:Rating",
                xmp_side.ms_rating_percent.and_then(percent_to_stars).map(|s| s.to_string()),
            ),
        ],
        &mut warnings,
    )
    .and_then(|s| s.parse::<u8>().ok());

    Ok(MetadataRead {
        metadata: EditableMetadata { title, comment, rating },
        warnings,
    })
}
```

- [ ] **Step 4: 通ることを確認する**

Run: `cargo test -p picture-tool-core metadata::tests`
Expected: PASS（1 テスト。食い違いの検査は `write_metadata` が要るので Task 9 に置く）

- [ ] **Step 5: コミット**

```bash
git add core/src/metadata/mod.rs
git commit -m "feat(core): メタデータ編集: 読み取りの優先順位を固定し、食い違いを警告に積む"
```

---

## Task 9: 検証付き原子的置換と panic の封じ込め（`write_metadata`）

元写真が対象である以上、ここは厚くする。**どの段階で失敗しても元写真は無傷**であること。

**Files:**
- Create: `core/src/metadata/atomic.rs`
- Modify: `core/src/metadata/mod.rs`

**Interfaces:**
- Consumes: Task 5 の `exif::apply`、Task 7 の `xmp::patch` / `xmp::new_packet` / `xmp::install`、
  Task 8 の `read_metadata`
- Produces:
  ```rust
  pub fn write_metadata(path: &std::path::Path, values: &EditableMetadata) -> anyhow::Result<()>;
  impl EditableMetadata { pub fn normalized(&self) -> EditableMetadata; }
  pub(super) fn scan_offset(bytes: &[u8]) -> anyhow::Result<usize>;   // atomic.rs
  ```

- [ ] **Step 1: 失敗するテストを書く**

`core/src/metadata/atomic.rs` の末尾に:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// SOI(2) + APP0 長さ 4 + SOS
    #[test]
    fn scan_offset_finds_the_start_of_the_scan() {
        let bytes = [
            0xFF, 0xD8, // SOI
            0xFF, 0xE0, 0x00, 0x04, 0x11, 0x22, // APP0（長さ 4 = 自身 2 + 中身 2）
            0xFF, 0xDA, 0x00, 0x02, // SOS
            0x99, 0x99,
        ];
        assert_eq!(scan_offset(&bytes).unwrap(), 8);
    }

    /// 0xFF のパディング（fill byte）が挟まっても迷子にならない
    #[test]
    fn scan_offset_skips_fill_bytes() {
        let bytes = [
            0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0x11, 0x22, 0xFF, 0xFF, 0xFF, 0xDA, 0x00, 0x02,
        ];
        assert_eq!(scan_offset(&bytes).unwrap(), 8);
    }

    #[test]
    fn scan_offset_reports_a_truncated_file_instead_of_panicking() {
        assert!(scan_offset(&[0xFF, 0xD8, 0xFF, 0xE0]).is_err());
    }
}
```

`core/src/metadata/mod.rs` の `mod tests` に追記:

```rust
/// **末尾改行つきのコメントが保存できること。**
/// 読み取り側が trim するので、書き込み側も trim しないと読み戻し検証が必ず落ちる
#[test]
fn a_comment_with_a_trailing_newline_survives_a_save() {
    let (_dir, path) = scratch("camera-direct.jpg");
    write_metadata(&path, &EditableMetadata {
        comment: Some("順光。もう少し寄れた\n".into()),
        ..Default::default()
    })
    .unwrap();
    let read = read_metadata(&path).unwrap();
    assert_eq!(read.metadata.comment.as_deref(), Some("順光。もう少し寄れた"));
    assert!(read.warnings.is_empty(), "trim の非対称で食い違い警告が出てはいけない: {:?}", read.warnings);
}

#[test]
fn normalizing_trims_and_folds_empty_strings_and_zero_stars_into_none() {
    let normalized = EditableMetadata {
        title: Some("   ".into()),
        comment: Some(" 残る\n".into()),
        rating: Some(0),
    }
    .normalized();
    assert_eq!(normalized.title, None, "空白だけの欄はタグごと消す");
    assert_eq!(normalized.comment.as_deref(), Some("残る"));
    assert_eq!(normalized.rating, None);
}

/// 優先順位は XMP が先頭。業界標準であり MWG も digiKam も Lightroom もそこを見る
#[test]
fn xmp_wins_over_exif_and_the_discarded_value_becomes_a_warning() {
    let (_dir, path) = scratch("camera-direct.jpg");

    // EXIF だけに古い値を入れる
    write_metadata(&path, &EditableMetadata {
        title: Some("old".into()),
        comment: Some("old comment".into()),
        rating: Some(2),
    }).unwrap();
    // 前提条件: いま食い違いは無い
    assert!(read_metadata(&path).unwrap().warnings.is_empty());

    // XMP 側だけを別の値に書き換える（他アプリが編集した状況を作る）
    overwrite_xmp_only(&path, &EditableMetadata {
        title: Some("new".into()),
        comment: Some("new comment".into()),
        rating: Some(5),
    });

    let read = read_metadata(&path).unwrap();
    assert_eq!(read.metadata.title.as_deref(), Some("new"));
    assert_eq!(read.metadata.comment.as_deref(), Some("new comment"));
    assert_eq!(read.metadata.rating, Some(5));
    assert_eq!(read.warnings.len(), 3, "3 項目それぞれに警告が出る: {:?}", read.warnings);
    assert!(read.warnings.iter().any(|w| w.contains("old")), "捨てた値を文面に含める: {:?}", read.warnings);
}

/// 値が一致していれば警告は出ない。**保存すれば全箇所が揃うので警告は自然に消える**
#[test]
fn matching_values_produce_no_warning() {
    let (_dir, path) = scratch("camera-direct.jpg");
    write_metadata(&path, &EditableMetadata {
        title: Some("同じ".into()),
        comment: Some("同じ".into()),
        rating: Some(3),
    }).unwrap();
    assert!(read_metadata(&path).unwrap().warnings.is_empty());
}

/// XMP 側だけを差し替えるテスト用ヘルパー。
/// `write_metadata` は両方に書くので、食い違いは作れない
fn overwrite_xmp_only(path: &std::path::Path, values: &EditableMetadata) {
    use img_parts::jpeg::Jpeg;
    use img_parts::Bytes;
    let bytes = std::fs::read(path).unwrap();
    let mut jpeg = Jpeg::from_bytes(Bytes::from(bytes)).unwrap();
    let packet = match xmp::extract(&jpeg) {
        Some(existing) => xmp::patch(&existing, values).unwrap(),
        None => xmp::new_packet(values),
    };
    xmp::install(&mut jpeg, &packet).unwrap();
    std::fs::write(path, jpeg.encoder().bytes()).unwrap();
}

/// **ピクセル不変は設計の必須条件。** 回帰したら即座に落ちること
#[test]
fn writing_metadata_does_not_touch_a_single_pixel_byte() {
    let (_dir, path) = scratch("camera-direct.jpg");
    let before = std::fs::read(&path).unwrap();
    let before_scan = before[atomic::scan_offset(&before).unwrap()..].to_vec();

    write_metadata(&path, &EditableMetadata {
        title: Some("夕暮れの港".into()),
        comment: Some("順光。もう少し寄れた".into()),
        rating: Some(4),
    }).unwrap();

    let after = std::fs::read(&path).unwrap();
    assert_eq!(after[atomic::scan_offset(&after).unwrap()..], before_scan[..]);
}

#[test]
fn writing_metadata_round_trips_japanese() {
    let (_dir, path) = scratch("camera-direct.jpg");
    let values = EditableMetadata {
        title: Some("夕暮れの港".into()),
        comment: Some("順光。もう少し寄れた".into()),
        rating: Some(4),
    };
    write_metadata(&path, &values).unwrap();
    assert_eq!(read_metadata(&path).unwrap().metadata, values);
}

/// 一時ファイルが残らないこと。残ると次回の書き込みで衝突しうる
#[test]
fn writing_metadata_leaves_no_temporary_file_behind() {
    let (dir, path) = scratch("camera-direct.jpg");
    write_metadata(&path, &EditableMetadata { rating: Some(1), ..Default::default() }).unwrap();
    let leftovers: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains("picture-tool.tmp"))
        .collect();
    assert!(leftovers.is_empty(), "残骸: {:?}", leftovers);
}

/// 書き込めない対象では**元ファイルを 1 バイトも変えずに** Err で戻ること
#[test]
fn a_non_jpeg_fails_without_modifying_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("broken.jpg");
    std::fs::write(&path, b"not a jpeg at all").unwrap();

    assert!(write_metadata(&path, &EditableMetadata { rating: Some(3), ..Default::default() }).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"not a jpeg at all");
}
```

- [ ] **Step 2: 落ちることを確認する**

Run: `cargo test -p picture-tool-core metadata::`
Expected: FAIL（`cannot find function write_metadata` / `scan_offset`）

- [ ] **Step 3: `atomic.rs` を実装する**

```rust
//! 書き込みの安全性。**どの段階で失敗しても元写真は無傷**であること。
//!
//! 1. 同一ディレクトリに一時ファイルとして書く（`rename` が原子的であるために
//!    同じファイルシステム上にいる必要がある）
//! 2. **読み戻して検証する** ── 書いた値が正しく読めるか、既存 Exif が壊れていないか、
//!    画素が 1 バイトも変わっていないか
//! 3. 検証を通ったときだけ `rename` で置き換える
//!
//! 手順 2 を挟むのは、書いた結果が壊れていた場合に元ファイルを置き換えないため。
//! 失敗しても残るのは一時ファイルだけで、それも呼び出し前に片付ける。

use super::EditableMetadata;
use anyhow::{anyhow, bail, Context, Result};
use std::path::Path;

/// SOS（`FF DA`）の開始位置。ここから EOI までが実際の画素データ。
///
/// 本ツールは APP セグメントしか触らないので、ここから後ろは**必ず**不変のはずである。
/// それを機械的に確かめるための位置。
pub(super) fn scan_offset(bytes: &[u8]) -> Result<usize> {
    let mut cursor = 2; // SOI をとばす
    loop {
        if cursor + 1 >= bytes.len() {
            bail!("SOS が見つかりません（JPEG が途中で切れている可能性があります）");
        }
        if bytes[cursor] != 0xFF {
            bail!("JPEG のセグメント境界が壊れています（offset {}）", cursor);
        }
        // 0xFF のパディング（fill byte）を読み飛ばす
        let mut marker_at = cursor + 1;
        while marker_at < bytes.len() && bytes[marker_at] == 0xFF {
            marker_at += 1;
        }
        if marker_at + 2 >= bytes.len() {
            bail!("SOS が見つかりません（JPEG が途中で切れている可能性があります）");
        }
        if bytes[marker_at] == 0xDA {
            return Ok(cursor);
        }
        let length = u16::from_be_bytes([bytes[marker_at + 1], bytes[marker_at + 2]]) as usize;
        cursor = marker_at + 1 + length;
    }
}

/// 検証付きの原子的置換
pub(super) fn replace_verified(
    path: &Path,
    original: &[u8],
    patched: Vec<u8>,
    expected: &EditableMetadata,
) -> Result<()> {
    // --- ピクセル不変（設計の必須条件） ---
    if original[scan_offset(original)?..] != patched[scan_offset(&patched)?..] {
        bail!("画素データが変化したため書き込みを中止しました（元ファイルは変更していません）");
    }

    let dir = path
        .parent()
        .ok_or_else(|| anyhow!("親フォルダーがありません: {}", path.display()))?;
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| anyhow!("ファイル名を解釈できません: {}", path.display()))?;
    let temp = dir.join(format!(".{}.{}.picture-tool.tmp", name, std::process::id()));

    let prepared = (|| -> Result<()> {
        std::fs::write(&temp, &patched)
            .with_context(|| format!("一時ファイルを書けません: {}", temp.display()))?;
        // 元の権限を引き継ぐ。落とすと「作った人しか読めない」写真になる
        if let Ok(meta) = std::fs::metadata(path) {
            let _ = std::fs::set_permissions(&temp, meta.permissions());
        }
        verify(&temp, path, expected)
    })();

    if let Err(error) = prepared {
        let _ = std::fs::remove_file(&temp);
        return Err(error);
    }

    // 同一ディレクトリなので rename は原子的
    std::fs::rename(&temp, path).with_context(|| {
        format!("置き換えに失敗しました: {} -> {}", temp.display(), path.display())
    })?;
    Ok(())
}

/// 書いた結果を読み戻して確かめる
fn verify(temp: &Path, original: &Path, expected: &EditableMetadata) -> Result<()> {
    let written = super::read_metadata(temp)?.metadata;
    if written != *expected {
        bail!("書き込んだ値を読み戻せませんでした（期待 {:?} / 実際 {:?}）", expected, written);
    }

    // 撮影情報が壊れていないこと。**独立の実装（kamadak-exif）で見る**ので、
    // 「little_exif で書いて little_exif で読めた」の同語反復にならない
    let before = crate::read_exif_info(original)?;
    let after = crate::read_exif_info(temp)?;
    if before != after {
        bail!("既存の Exif が壊れました（期待 {:?} / 実際 {:?}）", before, after);
    }
    Ok(())
}
```

- [ ] **Step 4: `mod.rs` に `write_metadata` を実装する**

```rust
mod atomic;

impl EditableMetadata {
    /// 保存の前に「前後の空白」「空文字」「★0」を落とす。
    ///
    /// **trim が要るのは、XMP の要素テキストが整形の改行・インデントを含むため。**
    /// 読み取り側は必ず trim することになる（`exif::cleaned` と XMP の
    /// `read_first_text`）ので、書き込み側も trim しておかないと
    /// 「コメント欄の末尾で改行して保存 → 読み戻しが一致せず必ず失敗する」になる。
    /// コメント欄は複数行入力なので、末尾改行は例外ではなく常用入力である。
    ///
    /// 空文字と★0 を `None` に寄せるのは、どちらも書き込みがタグ削除として扱い、
    /// 読み戻すと `None` が返るため。
    pub fn normalized(&self) -> EditableMetadata {
        fn tidy(text: &Option<String>) -> Option<String> {
            let trimmed = text.as_deref().unwrap_or("").trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        }
        EditableMetadata {
            title: tidy(&self.title),
            comment: tidy(&self.comment),
            rating: self.rating.filter(|stars| (1..=5).contains(stars)),
        }
    }
}

/// 画像のメタデータセグメントだけを差し替える。**画素は再エンコードしない。**
///
/// `little_exif` は `unwrap()` が多数残っており、壊れた Exif で panic しうる既知の
/// issue（#77）がある。core が panic するとアプリごと落ちるため `catch_unwind` で包む。
///
/// **既定の panic hook が stderr にメッセージを出す点だけは「core は `eprintln!` しない」
/// から外れる。** hook を差し替えるとプロセス全体（rayon のワーカーを含む）に効いてしまい、
/// 本物のバグの手掛かりを消す。panic は routine な事象ではないので、そちらを避けた。
pub fn write_metadata(path: &Path, values: &EditableMetadata) -> Result<()> {
    let values = values.normalized();
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| write_inner(path, &values)))
        .map_err(|_| {
            anyhow::anyhow!("メタデータの書き込みが異常終了しました（元ファイルは変更していません）")
        })?
}

fn write_inner(path: &Path, values: &EditableMetadata) -> Result<()> {
    let original = std::fs::read(path)
        .with_context(|| format!("ファイルを読めません: {}", path.display()))?;
    let patched = build(&original, values)?;
    atomic::replace_verified(path, &original, patched, values)
}

/// メタデータを差し替えたバイト列を作る。**ファイルには触らない。**
///
/// XMP パケットは **`little_exif` を通す前の原本から**取り出す。順序を逆にすると、
/// EXIF の書き込みが XMP セグメントに何をするかに依存してしまう。
fn build(original: &[u8], values: &EditableMetadata) -> Result<Vec<u8>> {
    let jpeg = Jpeg::from_bytes(Bytes::from(original.to_vec()))
        .context("JPEG として読めません")?;
    let packet = match xmp::extract(&jpeg) {
        Some(existing) => xmp::patch(&existing, values)?,
        None => xmp::new_packet(values),
    };

    // **既存 Exif の解析に失敗したら、そこで止める。**
    // 空の Metadata から書き直すと little_exif は既存の Exif APP1 を丸ごと落とし、
    // MakerNote も GPS もサムネイルも消えた「本ツールの 3 タグだけの Exif」になる。
    // しかも `read_exif_info` は解析失敗時も `Ok(ExifInfo::default())` を返すので、
    // 前後とも default で一致し、Task 9 の検証を素通りする ── これは spec が
    // 「クレート選定ごと見直す」と決めた issue #93 の症状そのものの経路である。
    let mut meta = match Metadata::new_from_vec(&original.to_vec(), FileExtension::JPEG) {
        Ok(meta) => meta,
        Err(error) if has_exif_segment(&jpeg) => {
            return Err(anyhow::anyhow!(
                "既存の Exif を解析できないため書き込みを中止しました（元ファイルは変更していません）: {}",
                error
            ))
        }
        // Exif をそもそも持たない画像。新しく作ってよい
        Err(_) => Metadata::new(),
    };
    exif::apply(&mut meta, values);
    let mut buffer = original.to_vec();
    meta.write_to_vec(&mut buffer, FileExtension::JPEG)
        .context("Exif を書き込めません")?;

    let mut written = Jpeg::from_bytes(Bytes::from(buffer)).context("書き込み後の JPEG を読めません")?;
    xmp::install(&mut written, &packet)?;
    Ok(written.encoder().bytes().to_vec())
}

/// Exif の APP1 を持っているか。「Exif が無い」と「Exif を解析できない」を分ける
fn has_exif_segment(jpeg: &Jpeg) -> bool {
    jpeg.segments_by_marker(img_parts::jpeg::markers::APP1)
        .any(|segment| segment.contents().starts_with(b"Exif\0\0"))
}
```

- [ ] **Step 5: 通ることを確認する**

Run: `cargo test -p picture-tool-core metadata::`
Expected: PASS（Task 8 で保留にした 2 本を含め、全部）

- [ ] **Step 6: lint とコミット**

Run: `make lint`

```bash
git add core/src/metadata/atomic.rs core/src/metadata/mod.rs
git commit -m "feat(core): メタデータ編集: 検証付き原子的置換で書き込み、panic を Err に変える"
```

---

## Task 10: 実カメラ由来フィクスチャでの統合テスト

単体テストは合成データでも通ってしまう。**spec のテスト方針が要求する項目**を、
実カメラの EXIF と実物の XMP パケットに対して 1 本にまとめる。

**Files:**
- Create: `core/tests/metadata_integration.rs`

**Interfaces:**
- Consumes: Task 2 のフィクスチャ、`picture_tool_core::metadata::{read_metadata, write_metadata, EditableMetadata}`
- Produces: なし（検査のみ）

- [ ] **Step 1: テストを書く**

```rust
//! メタデータ編集の統合テスト。
//!
//! 合成画像だけでは spec の「未検証リスク」を踏み抜く。**実カメラ由来の EXIF**
//! （MakerNote 付き）と、**他名前空間を含む実物の XMP パケット**に対して検査する。

use picture_tool_core::metadata::{read_metadata, write_metadata, EditableMetadata};
use std::path::{Path, PathBuf};

fn scratch(name: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(name);
    std::fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name),
        &path,
    )
    .unwrap();
    (dir, path)
}

/// kamadak-exif で全フィールドを列挙する。little_exif とは独立の実装
fn exif_fields(path: &Path) -> Vec<(String, String)> {
    let file = std::fs::File::open(path).unwrap();
    let mut reader = std::io::BufReader::new(file);
    let exif = exif::Reader::new().read_from_container(&mut reader).unwrap();
    let mut fields: Vec<(String, String)> = exif
        .fields()
        .map(|f| (format!("{:?}/{}", f.ifd_num, f.tag), f.display_value().to_string()))
        .collect();
    fields.sort();
    fields
}

/// **既存タグの保存** — 追加した項目以外のタグ集合に差分が無いこと
#[test]
fn writing_metadata_keeps_every_existing_exif_tag() {
    let (_dir, path) = scratch("camera-direct.jpg");
    let before = exif_fields(&path);
    // 前提条件: 実カメラ由来である（MakerNote を持つ）
    assert!(
        before.iter().any(|(name, _)| name.contains("MakerNote")),
        "フィクスチャに MakerNote が無い。Task 2 を見直すこと"
    );

    write_metadata(&path, &EditableMetadata {
        title: Some("夕暮れの港".into()),
        comment: Some("順光".into()),
        rating: Some(4),
    })
    .unwrap();

    let after = exif_fields(&path);
    let lost: Vec<&(String, String)> = before.iter().filter(|f| !after.contains(f)).collect();
    assert!(lost.is_empty(), "消えた・変わったタグ: {:#?}", lost);
}

/// **再編集** — 2 回書いても壊れない（`little_exif` issue #93 の再現条件）
#[test]
fn writing_twice_keeps_the_file_readable() {
    let (_dir, path) = scratch("camera-direct.jpg");
    for stars in [2u8, 5] {
        write_metadata(&path, &EditableMetadata {
            title: Some(format!("{} 回目", stars)),
            rating: Some(stars),
            ..Default::default()
        })
        .unwrap();
    }
    let read = read_metadata(&path).unwrap();
    assert_eq!(read.metadata.rating, Some(5));
    assert_eq!(read.metadata.title.as_deref(), Some("5 回目"));
}

/// XMP パケットの XML だけを取り出す。`xmp::extract` は private なので自前で切る
fn xmp_packet(path: &Path) -> String {
    const ID: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";
    let bytes = std::fs::read(path).unwrap();
    let start = bytes
        .windows(ID.len())
        .position(|window| window == ID)
        .expect("XMP パケットが無い")
        + ID.len();
    let tail = &bytes[start..];
    let end = tail
        .windows(13)
        .position(|window| window == b"<?xpacket end")
        .unwrap_or(tail.len());
    String::from_utf8(tail[..end].to_vec()).expect("XMP は UTF-8")
}

/// `名前空間接頭辞:属性名="値"` を 1 つ丸ごと切り出す。
/// **固定長で切らない** ── `from_utf8_lossy` した任意位置は char 境界を割って panic する
fn first_attribute(packet: &str, prefix: &str) -> String {
    let at = packet.find(prefix).expect("接頭辞が見つからない");
    let rest = &packet[at..];
    let open = rest.find('"').expect("属性値の開き引用符が無い");
    let close = rest[open + 1..].find('"').expect("属性値の閉じ引用符が無い");
    rest[..open + 1 + close + 1].to_string()
}

/// **既存 XMP の保持** — 本ツールの 4 プロパティ以外の値が 1 つも変わらないこと
#[test]
fn writing_metadata_preserves_foreign_xmp_namespaces() {
    let (_dir, path) = scratch("lightroom-xmp.jpg");
    let before = xmp_packet(&path);

    // 前提条件: 他名前空間（crs:）が本当に入っていること。
    // ここが空だと「壊れていない」は自明に通ってしまう
    assert!(
        before.contains("camera-raw-settings"),
        "フィクスチャの XMP に crs: が無い。Task 2 の Step 4 を見直すこと"
    );
    let sample = first_attribute(&before, "crs:");

    write_metadata(&path, &EditableMetadata {
        title: Some("差し替えた題".into()),
        rating: Some(1),
        ..Default::default()
    })
    .unwrap();

    let after = xmp_packet(&path);
    assert!(after.contains(&sample), "crs: の値が失われた: {:?}", sample);
    assert!(after.contains("camera-raw-settings"), "crs: の名前空間宣言が消えた");
    assert_eq!(read_metadata(&path).unwrap().metadata.rating, Some(1));
}

/// **原子性** — 書き込みに失敗しても元ファイルが無傷であること
#[test]
fn a_failed_write_leaves_the_original_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("truncated.jpg");
    // SOS まで届かずに終わる JPEG。`Jpeg::from_bytes` が Truncated で落ちる
    // （`scan_offset` まで到達しない）ので、一時ファイルすら作られない
    std::fs::write(&path, [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0x11, 0x22]).unwrap();
    let before = std::fs::read(&path).unwrap();

    assert!(write_metadata(&path, &EditableMetadata { rating: Some(3), ..Default::default() }).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
}
```

- [ ] **Step 2: 走らせる**

Run: `cargo test -p picture-tool-core --test metadata_integration`
Expected: PASS（4 テスト）

- [ ] **Step 3: 全体を通す**

Run: `cargo test --workspace && make lint`
Expected: PASS

- [ ] **Step 4: コミット**

```bash
git add core/tests/metadata_integration.rs
git commit -m "test(core): メタデータ編集: 実カメラ由来の画像で既存タグ・XMP・原子性を検査する"
```

---
## Task 11: `security::writable_image`

**Tauri コマンドがこのアプリの唯一の信頼境界。** capabilities/ACL は自作コマンドに
一切効かないので、webview から来たパスはここを通す。

**spec からの逸脱を 1 つ入れる**: spec は条件 1 を「`readable_image` と同じ強さ」と
書いているが、**JPEG 限定に強める**。メタデータ編集の対象は JPEG だけで（spec §スコープ）、
PNG / WebP は core の `metadata` が扱わない。塞げる分は塞ぐ。

**Files:**
- Modify: `gui/src/security.rs`

**Interfaces:**
- Consumes: 既存の `readable_image` / `WritableRoots`
- Produces:
  ```rust
  pub fn readable_jpeg(raw: &str) -> Result<PathBuf, String>;
  pub fn writable_image(roots: &WritableRoots, raw: &str) -> Result<PathBuf, String>;
  ```

- [ ] **Step 1: 失敗するテストを書く**

`gui/src/security.rs` の `mod tests` に追記:

```rust
#[test]
fn writable_image_rejects_everything_before_the_user_approves_a_folder() {
    let dir = TempDir::new().unwrap();
    let photo = dir.path().join("a.jpg");
    write_real_jpeg(&photo);
    let roots = WritableRoots::default();
    assert!(writable_image(&roots, &as_str(&photo)).is_err());
}

#[test]
fn writable_image_accepts_an_existing_jpeg_under_an_approved_folder() {
    let dir = TempDir::new().unwrap();
    let photo = dir.path().join("a.jpg");
    write_real_jpeg(&photo);
    let roots = WritableRoots::default();
    roots.grant(fs::canonicalize(dir.path()).unwrap());
    assert!(writable_image(&roots, &as_str(&photo)).is_ok());
}

/// **新規ファイル作成は一切許可しない。** メタデータ編集が新しいファイルを
/// 作る理由が無いので、塞げる分は塞ぐ
#[test]
fn writable_image_rejects_a_path_that_does_not_exist_yet() {
    let dir = TempDir::new().unwrap();
    let roots = WritableRoots::default();
    roots.grant(fs::canonicalize(dir.path()).unwrap());
    assert!(writable_image(&roots, &as_str(&dir.path().join("new.jpg"))).is_err());
}

/// 承認したのは 1 フォルダーだけ。名前が前方一致する兄弟は別物
#[test]
fn writable_image_rejects_a_sibling_whose_name_merely_starts_with_an_approved_root() {
    let base = TempDir::new().unwrap();
    let approved = base.path().join("photos");
    let sibling = base.path().join("photos-old");
    fs::create_dir(&approved).unwrap();
    fs::create_dir(&sibling).unwrap();
    let photo = sibling.join("a.jpg");
    write_real_jpeg(&photo);

    let roots = WritableRoots::default();
    roots.grant(fs::canonicalize(&approved).unwrap());
    assert!(writable_image(&roots, &as_str(&photo)).is_err());
}

/// JPEG 以外は、承認済みフォルダーの中にあっても拒否する
#[test]
fn writable_image_rejects_formats_the_metadata_module_cannot_handle() {
    let dir = TempDir::new().unwrap();
    let png = dir.path().join("a.png");
    image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(4, 4, image::Rgb([1, 2, 3])))
        .save(&png)
        .unwrap();
    let roots = WritableRoots::default();
    roots.grant(fs::canonicalize(dir.path()).unwrap());
    // 前提条件: readable_image としては通る（＝「画像ではないから落ちた」ではない）
    assert!(readable_image(&as_str(&png)).is_ok());
    assert!(writable_image(&roots, &as_str(&png)).is_err());
}

/// リンク解決後の実体で判定する。承認フォルダーの中に置いた `.jpg` という名前の
/// リンクで、外のファイルを書き換えられては境界の意味が無い
#[cfg(unix)]
#[test]
fn writable_image_rejects_a_jpeg_named_symlink_pointing_outside() {
    let base = TempDir::new().unwrap();
    let approved = base.path().join("photos");
    fs::create_dir(&approved).unwrap();
    let outside = base.path().join("secret.jpg");
    write_real_jpeg(&outside);
    let link = approved.join("innocent.jpg");
    std::os::unix::fs::symlink(&outside, &link).unwrap();

    let roots = WritableRoots::default();
    roots.grant(fs::canonicalize(&approved).unwrap());
    assert!(writable_image(&roots, &as_str(&link)).is_err());
}
```

- [ ] **Step 2: 落ちることを確認する**

Run: `cargo test -p picture-tool-gui security`
Expected: FAIL（`cannot find function writable_image`）

- [ ] **Step 3: 実装する**

`gui/src/security.rs` の `writable_dir` の隣に:

```rust
/// メタデータを読み書きしてよい JPEG として検証し、実体のパスを返す
///
/// `readable_image` より**強い**。メタデータ編集の対象は JPEG だけで
/// （spec §スコープ）、PNG / WebP は core の `metadata` が扱わない。
pub fn readable_jpeg(raw: &str) -> Result<PathBuf, String> {
    let resolved = readable_image(raw)?;
    let is_jpeg = resolved
        .extension()
        .and_then(OsStr::to_str)
        .is_some_and(|ext| ext.eq_ignore_ascii_case("jpg") || ext.eq_ignore_ascii_case("jpeg"));
    if !is_jpeg {
        return Err(format!("Metadata editing supports JPEG only: {}", raw));
    }
    Ok(resolved)
}

/// 書き込んでよい画像として検証し、実体のパスを返す
///
/// 条件は 3 つ:
/// 1. 実体が JPEG であること
/// 2. 承認済みルート配下であること
/// 3. **既に存在するファイルであること**（`readable_image` が `is_file` を見る）
///
/// **新規ファイル作成は一切許可しない。** メタデータ編集が新しいファイルを作る理由が
/// 無いので、塞げる分は塞ぐ。`writable_dir` が未作成パスを許すのと対照的なのは、
/// あちらが「出力先フォルダーを作る」用途だから。
pub fn writable_image(roots: &WritableRoots, raw: &str) -> Result<PathBuf, String> {
    let resolved = readable_jpeg(raw)?;
    if !roots.allows(&resolved) {
        return Err(format!(
            "This folder has not been approved for metadata editing: {}",
            raw
        ));
    }
    Ok(resolved)
}
```

- [ ] **Step 4: モジュールコメントの表を更新する**

`gui/src/security.rs` の冒頭の表に 1 行足し、削除の段落の後ろに追記する。

```
//! | 書き込み（変換の出力） | ネイティブダイアログで許可したルート配下 |
//! | 書き込み（メタデータ編集） | 利用者がフォルダー単位で承認したルート配下の、既存の JPEG のみ |
```

```
//! メタデータ編集の書き込み先も「許可ルート配下」に縛れない点は削除と同じだが、
//! 保存のたびにダイアログを出すのは使い物にならない。そこで**フォルダー単位で 1 度だけ**
//! OS ネイティブの確認を挟み（`commands::grant_metadata_editing`）、承認されたルートを
//! `WritableRoots` に足す。乗っ取られた webview にできる最悪のことは
//! 「利用者が自分で承認したフォルダーのメタデータを壊す」まで。承認していない
//! フォルダーには一切書けず、承認は次回起動時に消える。
```

- [ ] **Step 5: 通ることを確認してコミット**

Run: `cargo test -p picture-tool-gui security && make lint`

```bash
git add gui/src/security.rs
git commit -m "feat(gui): メタデータ編集: 書き込んでよい JPEG の検証を境界に足す"
```

---

## Task 12: Tauri コマンド 3 本と DTO 契約

**Files:**
- Modify: `gui/src/commands.rs`, `gui/src/types.rs`, `gui/src/main.rs`
- Regenerate: `gui-frontend/src/lib/dto.contract.ts`（**手で編集しない**）

**Interfaces:**
- Consumes: Task 8 / 9 の `core::metadata::{read_metadata, write_metadata, EditableMetadata, MetadataRead}`、
  Task 11 の `security::{readable_jpeg, writable_image}`
- Produces（webview から見える形）:
  | コマンド | 引数 | 戻り値 |
  |---|---|---|
  | `read_image_metadata` | `path: String` | `MetadataRead` |
  | `write_image_metadata` | `path: String`, `metadata: EditableMetadata` | `MetadataRead`（保存直後の実際の中身） |
  | `grant_metadata_editing` | `folder: String` | `Option<String>`（承認されたルートの**実体パス**。拒否なら `null`） |

- [ ] **Step 1: コマンドを実装する**

`gui/src/commands.rs` に追記:

```rust
/// 編集対象の現在値と、箇所ごとの値の食い違いを返す
#[tauri::command]
pub async fn read_image_metadata(path: String) -> Result<core::metadata::MetadataRead, String> {
    let file = security::readable_jpeg(&path)?;
    tokio::task::spawn_blocking(move || {
        core::metadata::read_metadata(&file).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 検証付き原子的置換で書き込み、**保存直後の実際の中身を返す**
///
/// 戻り値を保存済みの値としてそのまま使えるので、フロント側が
/// 「送った値」と「ファイルの中身」を取り違えることが無い。
/// 食い違いの警告も、書き込みで揃った結果が返る。
#[tauri::command]
pub async fn write_image_metadata(
    state: tauri::State<'_, ProcessingState>,
    path: String,
    metadata: core::metadata::EditableMetadata,
) -> Result<core::metadata::MetadataRead, String> {
    let file = security::writable_image(&state.writable_roots, &path)?;
    tokio::task::spawn_blocking(move || {
        core::metadata::write_metadata(&file, &metadata).map_err(|e| e.to_string())?;
        core::metadata::read_metadata(&file).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// フォルダー単位でメタデータの書き込みを承認させる
///
/// **確認を OS ネイティブのダイアログで出すことが本質。** webview 内の確認は
/// 乗っ取られた状態では素通りする（`confirm_delete_originals` と同型）。
/// `folder` は webview から来るが、承認されるのは `existing_dir` が解決した実体パスで、
/// その実体パスをダイアログの文面に出すので、利用者は「見覚えのない場所」に気づける。
/// 戻り値も**解決後の実体パス**にして、フロントの表示と境界の判定を一致させる。
#[tauri::command]
pub async fn grant_metadata_editing(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, ProcessingState>,
    folder: String,
) -> Result<Option<String>, String> {
    let dir = security::existing_dir(&folder)?;

    let mut builder = app_handle
        .dialog()
        .message(format!(
            "このフォルダーの写真に、タイトル・コメント・評価を書き込めるようにします。\n\n{}\n\n書き換えるのはメタデータだけで、写真そのもの（画素）は変わりません。\n許可はアプリを終了すると消えます。",
            dir.display()
        ))
        .title("このフォルダーへの書き込みを許可しますか？")
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom(
            "許可する".to_string(),
            "許可しない".to_string(),
        ));

    if let Some(window) = app_handle.get_webview_window("main") {
        builder = builder.parent(&window);
    }

    if !builder.blocking_show() {
        return Ok(None);
    }
    // **返すのは解決後の実体パス。** 呼び出し側が送った生の文字列を保持すると、
    // シンボリックリンク越しなどで境界とフロントの表示がずれ、
    // 「承認済みと出ているのに保存が弾かれる」になる
    let granted = dir.to_string_lossy().to_string();
    state.writable_roots.grant(dir);
    Ok(Some(granted))
}
```

- [ ] **Step 2: ハンドラを登録する**

`gui/src/main.rs` の `generate_handler!` に 3 行足す:

```rust
            commands::read_image_metadata,
            commands::write_image_metadata,
            commands::grant_metadata_editing,
```

- [ ] **Step 3: 契約にエントリを足す**

`gui/src/types.rs` の `mod contract`:

- `use picture_tool_core::{...}` に `metadata::{EditableMetadata, MetadataRead}` を足す
- `contract()` の `entries` に追記:

```rust
            entry(
                "editableMetadataFilled",
                "EditableMetadata",
                "3 項目とも入っている",
                EditableMetadata {
                    title: Some("夕暮れの港".to_string()),
                    comment: Some("順光。もう少し寄れた".to_string()),
                    rating: Some(4),
                },
            ),
            entry(
                "editableMetadataEmpty",
                "EditableMetadata",
                "タグが 1 つも無い（null と空文字は別物）",
                EditableMetadata::default(),
            ),
            entry(
                "metadataRead",
                "MetadataRead",
                "他のアプリが書いた別の値を検出した場合",
                MetadataRead {
                    metadata: EditableMetadata {
                        title: Some("夕暮れの港".to_string()),
                        comment: None,
                        rating: Some(4),
                    },
                    warnings: vec![
                        "コメントに、他のアプリが書いた別の値があります（XMP dc:description を採用。ほかに Exif XPComment = 「古い注」）。保存すると揃います".to_string(),
                    ],
                },
            ),
```

- `render(&entries, &[...])` の import 一覧に `"EditableMetadata"` と `"MetadataRead"` を
  **アルファベット順の位置に**足す

- [ ] **Step 4: 契約が食い違うことを確認する**

Run: `cargo test -p picture-tool-gui contract`
Expected: FAIL（「生成物と食い違っている」）。**これが正しい挙動**

- [ ] **Step 5: 生成し直す**

Run: `UPDATE_DTO_CONTRACT=1 cargo test -p picture-tool-gui contract`

続けて `cargo test -p picture-tool-gui contract` が PASS になること。
`gui-frontend/src/lib/dto.contract.ts` に 3 つの `export const` が増える。
**このファイルを手で触らないこと。**

- [ ] **Step 6: 型検査が落ちることを確認する**

Run: `make typecheck`
Expected: FAIL（`types.ts` に `EditableMetadata` / `MetadataRead` が無い）。
**これが契約の効き目** ── Rust を変えると TS 側が必ず赤くなる。

- [ ] **Step 7: `types.ts` に境界の写しを足す**

赤のままコミットしない。**契約の red→green は Step 4〜6 で見届けたので、
ここで緑に戻してからコミットする**（部分適用や切り戻しの起点にならない
コミットを残さない）。

`gui-frontend/src/lib/types.ts`:

```ts
/**
 * 人が編集するメタデータ。
 *
 * `null` は「タグが無い」、`""` は「空文字のタグがある」を意味し、Rust 側は**区別する**。
 * 空欄で保存したら `null` を送る（＝タグを削除する）。
 */
export interface EditableMetadata {
  title: string | null;
  comment: string | null;
  /** 1〜5。null は未評価。**0 は存在しない**（RatingPercent に表現できないため） */
  rating: number | null;
}

/** 読み取り結果。warnings には箇所ごとの値の食い違いが載る */
export interface MetadataRead {
  metadata: EditableMetadata;
  warnings: string[];
}
```

Run: `make typecheck`
Expected: PASS

- [ ] **Step 8: コミット**

```bash
git add gui/src/commands.rs gui/src/main.rs gui/src/types.rs gui-frontend/src/lib/dto.contract.ts gui-frontend/src/lib/types.ts
git commit -m "feat(gui): メタデータ編集: 読み書きと書き込み承認の Tauri コマンドを足す"
```

---

## Task 13: フロントの純粋ロジックと境界の写し

`bun test` の対象は runes を含まない純粋ロジックだけ。**変換と判定をここに寄せて
テストできる形にし、runes のストアは薄く保つ。**

**Files:**
- Create: `gui-frontend/src/lib/panels/metadataValues.ts` + `.test.ts`
- Create: `gui-frontend/src/lib/panels/grants.ts` + `.test.ts`
- Modify: `gui-frontend/src/lib/api.ts`
- Modify: `gui-frontend/src/lib/panels/metadataDraft.svelte.ts`

**Interfaces:**
- Consumes: Task 12 の 3 コマンドと `dto.contract.ts`
- Produces:
  ```ts
  // metadataValues.ts
  export interface MetadataValues { title: string; comment: string; rating: number }
  export function emptyValues(): MetadataValues
  export function toValues(metadata: EditableMetadata): MetadataValues
  export function toMetadata(values: MetadataValues): EditableMetadata
  export function sameValues(a: MetadataValues, b: MetadataValues): boolean
  // grants.ts
  export function isUnder(root: string, path: string): boolean
  export function anyAllows(roots: readonly string[], path: string): boolean
  export function parentFolder(path: string): string
  // api.ts
  readImageMetadata(path): Promise<MetadataRead>
  writeImageMetadata(path, metadata): Promise<MetadataRead>
  grantMetadataEditing(folder): Promise<boolean>
  ```

- [ ] **Step 1: 失敗するテストを書く**

`gui-frontend/src/lib/panels/metadataValues.test.ts`:

```ts
import { describe, expect, test } from "bun:test";
import { emptyValues, sameValues, toMetadata, toValues } from "./metadataValues";

describe("フォーム値 ⇄ EditableMetadata", () => {
  test("null は空欄・未評価として見せる", () => {
    expect(toValues({ title: null, comment: null, rating: null })).toEqual(emptyValues());
  });

  // null（タグが無い）と ""（空文字のタグがある）は Rust 側で別物。
  // 空欄で保存したら「タグを消す」＝ null を送る
  test("空欄は null にして送る", () => {
    expect(toMetadata({ title: "", comment: "", rating: 0 })).toEqual({
      title: null,
      comment: null,
      rating: null,
    });
  });

  test("入力された値はそのまま送る", () => {
    expect(toMetadata({ title: "港", comment: "順光", rating: 4 })).toEqual({
      title: "港",
      comment: "順光",
      rating: 4,
    });
  });

  // ★0 は「未評価」であって 0 という評価ではない（RatingPercent に表現できない）
  test("★0 は null になる", () => {
    expect(toMetadata({ title: "港", comment: "", rating: 0 }).rating).toBeNull();
  });

  test("往復しても値が変わらない", () => {
    const metadata = { title: "港", comment: "順光", rating: 3 };
    expect(toMetadata(toValues(metadata))).toEqual(metadata);
  });

  test("sameValues は 3 項目すべてを見る", () => {
    const base = { title: "a", comment: "b", rating: 1 };
    expect(sameValues(base, { ...base })).toBe(true);
    expect(sameValues(base, { ...base, title: "z" })).toBe(false);
    expect(sameValues(base, { ...base, comment: "z" })).toBe(false);
    expect(sameValues(base, { ...base, rating: 5 })).toBe(false);
  });
});
```

`gui-frontend/src/lib/panels/grants.test.ts`:

```ts
import { describe, expect, test } from "bun:test";
import { anyAllows, isUnder, parentFolder } from "./grants";

describe("承認済みフォルダーの判定（表示用）", () => {
  test("配下のファイルは許可される", () => {
    expect(isUnder("/home/u/photos", "/home/u/photos/a.jpg")).toBe(true);
    expect(isUnder("/home/u/photos", "/home/u/photos/2026/a.jpg")).toBe(true);
  });

  // 名前が前方一致するだけの兄弟は別のフォルダー。
  // ここを落とすと「承認済み」と表示して、保存でいきなり Rust に弾かれる
  test("名前が前方一致する兄弟は許可されない", () => {
    expect(isUnder("/home/u/photos", "/home/u/photos-old/a.jpg")).toBe(false);
  });

  test("末尾のスラッシュの有無で結果が変わらない", () => {
    expect(isUnder("/home/u/photos/", "/home/u/photos/a.jpg")).toBe(true);
  });

  test("Windows の区切り文字でも判定できる", () => {
    expect(isUnder("C:\\Users\\u\\photos", "C:\\Users\\u\\photos\\a.jpg")).toBe(true);
  });

  test("anyAllows はどれか 1 つに当たれば true", () => {
    expect(anyAllows(["/a", "/b"], "/b/x.jpg")).toBe(true);
    expect(anyAllows(["/a", "/b"], "/c/x.jpg")).toBe(false);
    expect(anyAllows([], "/a/x.jpg")).toBe(false);
  });

  test("parentFolder は両方の区切り文字を扱う", () => {
    expect(parentFolder("/home/u/photos/a.jpg")).toBe("/home/u/photos");
    expect(parentFolder("C:\\Users\\u\\photos\\a.jpg")).toBe("C:\\Users\\u\\photos");
  });
});
```

- [ ] **Step 2: 落ちることを確認する**

Run: `make test-frontend`
Expected: FAIL（モジュールが無い）

- [ ] **Step 3: 実装する**

`gui-frontend/src/lib/api.ts` に追記（`EditableMetadata` / `MetadataRead` は
Task 12 Step 7 で `types.ts` に足してある）:

```ts
export async function readImageMetadata(path: string): Promise<MetadataRead> {
  return invoke("read_image_metadata", { path });
}

/** 保存し、**保存直後の実際の中身**を返す。戻り値をそのまま「保存済みの値」に使える */
export async function writeImageMetadata(
  path: string,
  metadata: EditableMetadata
): Promise<MetadataRead> {
  return invoke("write_image_metadata", { path, metadata });
}

/**
 * フォルダー単位で書き込みを承認させる。承認されたら true。
 *
 * 確認ダイアログは Rust 側が OS ネイティブで出す。webview 内の確認は
 * 乗っ取られた状態では素通りするため、不可逆な操作の根拠にならない（S6-H8）。
 */
export async function grantMetadataEditing(folder: string): Promise<string | null> {
  return invoke("grant_metadata_editing", { folder });
}
```

`import type { ... }` に `EditableMetadata` と `MetadataRead` を足す。

`gui-frontend/src/lib/panels/metadataValues.ts`:

```ts
import type { EditableMetadata } from "../types";

/** 入力欄が扱う形。空欄は "" で、未評価は 0 で表す */
export interface MetadataValues {
  title: string;
  comment: string;
  /** 0〜5。0 は未評価 */
  rating: number;
}

export function emptyValues(): MetadataValues {
  return { title: "", comment: "", rating: 0 };
}

export function toValues(metadata: EditableMetadata): MetadataValues {
  return {
    title: metadata.title ?? "",
    comment: metadata.comment ?? "",
    rating: metadata.rating ?? 0,
  };
}

/**
 * 保存のために境界の形へ戻す。
 *
 * 空欄は `null`（タグを削除）にする。`""` のまま送ると「空文字のタグがある」状態になり、
 * 他アプリで「空のコメントがある」と見える。
 */
export function toMetadata(values: MetadataValues): EditableMetadata {
  return {
    title: values.title === "" ? null : values.title,
    comment: values.comment === "" ? null : values.comment,
    rating: values.rating >= 1 && values.rating <= 5 ? values.rating : null,
  };
}

export function sameValues(a: MetadataValues, b: MetadataValues): boolean {
  return a.title === b.title && a.comment === b.comment && a.rating === b.rating;
}
```

`gui-frontend/src/lib/panels/grants.ts`:

```ts
/**
 * 承認済みフォルダーの判定。**表示を決めるためだけのもの。**
 *
 * 本当の境界は Rust 側（`gui/src/security.rs` の `writable_image`）にあり、
 * ここが誤って true を返しても保存は弾かれる。逆にここが厳しすぎると
 * 「承認したのに保存ボタンが押せない」になるので、緩さより一致を優先する。
 *
 * Windows の大文字小文字非区別までは見ていない。Rust 側が `canonicalize` した
 * 実体パスで判定するので、食い違った場合は保存時のエラーで気づける。
 */
function normalize(path: string): string {
  return path.replace(/\\/g, "/").replace(/\/+$/, "");
}

export function isUnder(root: string, path: string): boolean {
  const r = normalize(root);
  const p = normalize(path);
  return p === r || p.startsWith(`${r}/`);
}

export function anyAllows(roots: readonly string[], path: string): boolean {
  return roots.some((root) => isUnder(root, path));
}

/** ファイルパスから親フォルダーを取る。区切り文字は元の綴りのまま返す */
export function parentFolder(path: string): string {
  const cut = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  return cut <= 0 ? path : path.slice(0, cut);
}
```

`gui-frontend/src/lib/panels/metadataDraft.svelte.ts` から `MetadataValues` と
`empty()` を削り、`metadataValues.ts` から import するように直す（型の定義を 1 箇所にする）。

- [ ] **Step 4: 通ることを確認する**

Run: `make test-frontend && make typecheck`
Expected: PASS

- [ ] **Step 5: コミット**

```bash
git add gui-frontend/src/lib/api.ts gui-frontend/src/lib/panels/metadataValues.ts gui-frontend/src/lib/panels/metadataValues.test.ts gui-frontend/src/lib/panels/grants.ts gui-frontend/src/lib/panels/grants.test.ts gui-frontend/src/lib/panels/metadataDraft.svelte.ts
git commit -m "feat(gui): メタデータ編集: 境界の写しと、フォーム値・承認判定の純粋ロジックを足す"
```

---
## Task 14: メタデータパネルの繋ぎ込み

刷新が作った器（`MetadataPanel.svelte` / `metadataDraft.svelte.ts`）に載せる。
**作り直さない。**

**Files:**
- Modify: `gui-frontend/src/lib/panels/metadataDraft.svelte.ts`
- Modify: `gui-frontend/src/lib/panels/metadataValues.ts` + `.test.ts`（`isEditableImage`）
- Create: `gui-frontend/src/lib/panels/grants.svelte.ts`
- Modify: `gui-frontend/src/lib/panels/MetadataPanel.svelte`
- Modify: `gui-frontend/src/App.svelte`
- Modify: `gui-frontend/e2e/stub.ts`, `gui-frontend/e2e/metadata.spec.ts`

**Interfaces:**
- Consumes: Task 13 の `metadataValues` / `grants` / `api`
- Produces:
  ```ts
  // metadataDraft.svelte.ts（既存の形を保ったまま中身を実装する）
  createMetadataDraft(): {
    readonly path: string | null
    readonly values: MetadataValues
    readonly warnings: string[]
    readonly loading: boolean
    readonly saving: boolean
    readonly isDirty: boolean
    load(next: string | null): Promise<void>
    save(): Promise<void>
    discard(): void
  }
  // grants.svelte.ts
  createGrantStore(): { readonly roots: string[]; allows(path): boolean; request(folder): Promise<boolean> }
  ```

- [ ] **Step 1: `isEditableImage` の失敗するテストを書く**

`gui-frontend/src/lib/panels/metadataValues.test.ts` に追記:

```ts
import { isEditableImage } from "./metadataValues";

describe("編集できるファイルの判定", () => {
  // Rust 側（security.rs の readable_jpeg）と同じ条件。
  // ここが緩いと「編集できる顔をして保存で弾かれる」になる
  test("JPEG だけを編集できる", () => {
    for (const name of ["a.jpg", "a.JPG", "a.jpeg", "a.JPEG"]) {
      expect(isEditableImage(name)).toBe(true);
    }
    for (const name of ["a.png", "a.webp", "a.jpg.png", "a"]) {
      expect(isEditableImage(name)).toBe(false);
    }
  });
});
```

Run: `make test-frontend` → FAIL

- [ ] **Step 2: `isEditableImage` を実装する**

`gui-frontend/src/lib/panels/metadataValues.ts` に追記:

```ts
/** メタデータを編集できるのは JPEG だけ（Rust 側 `security.rs` の `readable_jpeg` と同条件） */
export function isEditableImage(name: string): boolean {
  return /\.jpe?g$/i.test(name);
}
```

Run: `make test-frontend` → PASS

- [ ] **Step 3: 下書きストアを実装する**

`gui-frontend/src/lib/panels/metadataDraft.svelte.ts` を差し替える:

```ts
import { readImageMetadata, writeImageMetadata } from "../api";
import {
  emptyValues,
  sameValues,
  toMetadata,
  toValues,
  type MetadataValues,
} from "./metadataValues";

/**
 * メタデータの下書き。
 *
 * 離脱経路（写真の切替 / rail でのモード移動 / フォルダー変更 / ウィンドウを閉じる）は
 * すべて `isDirty` を通す（刷新 spec §3-4）。判定をここ 1 箇所に置いてあるので、
 * 呼び出し側は分岐を増やさずに済む。
 */
export function createMetadataDraft() {
  let path = $state<string | null>(null);
  let values = $state<MetadataValues>(emptyValues());
  let saved = $state<MetadataValues>(emptyValues());
  let warnings = $state<string[]>([]);
  let loading = $state(false);
  let saving = $state(false);

  /** 写真を連打したときに、古い応答が新しい下書きを上書きしないよう守る */
  let token = 0;

  return {
    get path() {
      return path;
    },
    get values() {
      return values;
    },
    /** 箇所ごとの値の食い違い。保存すると全箇所が揃うので自然に消える */
    get warnings() {
      return warnings;
    },
    get loading() {
      return loading;
    },
    get saving() {
      return saving;
    },
    get isDirty(): boolean {
      return !sameValues(values, saved);
    },

    async load(next: string | null) {
      path = next;
      const current = ++token;
      warnings = [];
      saved = emptyValues();
      values = emptyValues();
      if (next === null) {
        loading = false;
        return;
      }
      loading = true;
      try {
        const read = await readImageMetadata(next);
        if (current !== token) return;
        saved = toValues(read.metadata);
        values = toValues(read.metadata);
        warnings = read.warnings;
      } finally {
        if (current === token) loading = false;
      }
    },

    /**
     * 保存する。**戻り値は「ファイルの実際の中身」**なので、それを保存済みの値に採る。
     * 送った値をそのまま saved にすると、Rust 側の正規化（空文字→タグ削除など）と
     * 食い違って未保存表示が消えなくなる。
     */
    async save() {
      if (path === null || saving) return;
      const target = path;
      const sending = toMetadata($state.snapshot(values));
      saving = true;
      try {
        const read = await writeImageMetadata(target, sending);
        // 保存中に別の写真へ移っていたら、いまの下書きを上書きしない
        if (target !== path) return;
        saved = toValues(read.metadata);
        values = toValues(read.metadata);
        warnings = read.warnings;
      } finally {
        saving = false;
      }
    },

    discard() {
      values = { ...saved };
    },
  };
}
```

- [ ] **Step 4: 承認ストアを作る**

`gui-frontend/src/lib/panels/grants.svelte.ts`:

```ts
import { grantMetadataEditing } from "../api";
import { anyAllows } from "./grants";

/**
 * 書き込みを承認済みのフォルダー。
 *
 * **保持は表示のためだけ。** 実際の許可は Rust 側の `WritableRoots` にあり、
 * アプリを終了すると消える。だからここも永続化しない ── 永続化すると
 * 「承認済みと出ているのに保存できない」状態が起動直後に必ず生まれる。
 */
export function createGrantStore() {
  let roots = $state<string[]>([]);

  return {
    get roots() {
      return roots;
    },
    allows(path: string): boolean {
      return anyAllows(roots, path);
    },
    async request(folder: string): Promise<boolean> {
      // 保持するのは Rust が解決した実体パス。送った生の文字列を保持すると
      // 境界の判定とずれて「承認済み表示なのに保存が弾かれる」になる
      const granted = await grantMetadataEditing(folder);
      if (granted !== null && !roots.includes(granted)) roots = [...roots, granted];
      return granted !== null;
    },
  };
}
```

- [ ] **Step 5: パネルを繋ぐ**

`gui-frontend/src/lib/panels/MetadataPanel.svelte`:

`Props` に足す:

```ts
    /** このフォルダーへの書き込みが承認済みか */
    canWrite: boolean;
    /** JPEG 以外は編集できない（Rust 側が弾く） */
    editable: boolean;
    /** 「保存して次の写真へ」で送る先があるか */
    hasNext: boolean;
    onGrant: () => void;
    onSave: () => void;
    onSaveAndNext: () => void;
```

食い違い警告の表示領域（既存の `.mismatch`）を埋める:

```svelte
        <div class="mismatch" aria-live="polite">
          {#each draft.warnings as warning (warning)}
            <p class="warning">{warning}</p>
          {/each}
        </div>
```

```css
  .warning {
    margin: var(--space-2) 0 0;
    font: var(--md-sys-typescale-body-sm);
    color: var(--md-sys-color-on-surface-variant);
  }
```

入力欄に `disabled={!editable || draft.loading}` を渡す（`TextField` / `Rating` の
既存プロパティ）。`editable` が false のときは「書き込みの許可」カードの上に出す:

```svelte
      {#if !editable}
        <Card level={1} title="編集できない形式">
          <p class="empty">タイトル・コメント・評価を書き込めるのは JPEG だけです。</p>
        </Card>
      {/if}
```

「書き込みの許可」カードを繋ぐ:

```svelte
      <Card level={1} title="書き込みの許可">
        {#if canWrite}
          <p class="empty">このフォルダーへ書き込めます（アプリを終了すると解除されます）。</p>
        {:else}
          <p class="empty">このフォルダーへの書き込みはまだ許可されていません。</p>
          <div class="sub">
            <Button variant="outlined" onclick={onGrant}>このフォルダーへの書き込みを許可...</Button>
          </div>
        {/if}
      </Card>
```

最下部の 2 ボタン:

```svelte
  <div class="action">
    <Button
      variant="filled"
      full
      disabled={!canSave || !hasNext}
      onclick={onSaveAndNext}>保存して次の写真へ</Button>
    <Button variant="outlined" full disabled={!canSave} onclick={onSave}>保存</Button>
  </div>
```

```ts
  // 保存できるのは「JPEG で、承認済みで、変更があって、保存中でない」とき
  let canSave = $derived(editable && canWrite && draft.isDirty && !draft.saving);
```

- [ ] **Step 6: `App.svelte` を繋ぐ**

```ts
  import { untrack } from "svelte";
  import { createGrantStore } from "./lib/panels/grants.svelte";
  import { isEditableImage } from "./lib/panels/metadataValues";
  import { parentFolder } from "./lib/panels/grants";

  const grants = createGrantStore();

  let editingFolder = $derived(editingPath === null ? "" : parentFolder(editingPath));
  let canWriteEditing = $derived(editingPath !== null && grants.allows(editingPath));
  let editingIsJpeg = $derived(editingImage !== null && isEditableImage(editingImage.name));
  let nextImagePath = $derived.by(() => {
    if (editingPath === null) return null;
    const index = images.findIndex((img) => img.path === editingPath);
    return index >= 0 && index + 1 < images.length ? images[index + 1].path : null;
  });
```

編集対象が変わったら読み込む。**既存の
`$effect(() => { metadata.load(editingPath); })`（`App.svelte:72-75`）を
書き換える**（足すと `load` が二重に走る）:

```ts
  $effect(() => {
    // 非 JPEG では読みに行かない。read_image_metadata は readable_jpeg で
    // PNG / WebP を弾くので、無条件に呼ぶと情報モードで PNG をクリックするたびに
    // エラートーストが出る（グリッドは PNG / WebP も並べる）
    const target = editingPath !== null && editingIsJpeg ? editingPath : null;
    // `untrack` で囲む ── `load` はストアの $state を書くので、
    // 囲まないと自分の書き込みで再実行されうる
    untrack(() => {
      metadata.load(target).catch((e) => {
        toast.error(`メタデータを読めませんでした: ${describeError(e)}`);
      });
    });
  });
```

ハンドラー:

```ts
  async function handleGrant() {
    if (editingFolder === "") return;
    try {
      const approved = await grants.request(editingFolder);
      if (!approved) toast.warning("書き込みは許可されませんでした。");
    } catch (e) {
      toast.error(`許可を求められませんでした: ${describeError(e)}`);
    }
  }

  async function handleSaveMetadata(): Promise<boolean> {
    try {
      await metadata.save();
      toast.success("保存しました。");
      return true;
    } catch (e) {
      toast.error(`保存できませんでした: ${describeError(e)}`);
      return false;
    }
  }

  async function handleSaveAndNext() {
    const next = nextImagePath;
    if (!(await handleSaveMetadata())) return;
    editingPath = next;
    // **グリッドの選択も動かす。** 単一選択モードの `aria-selected` と太枠は
    // `focusedPath` で決まる（`PhotoGrid.svelte` の `isSelected`）ので、
    // ここを落とすと「パネルは次の写真・グリッドの枠は前の写真」になる
    if (next !== null) focusedPath = next;
  }
```

`MetadataPanel` の呼び出しを差し替える:

```svelte
      <MetadataPanel
        image={editingImage}
        draft={metadata}
        canWrite={canWriteEditing}
        editable={editingIsJpeg}
        hasNext={nextImagePath !== null}
        onGrant={handleGrant}
        onSave={handleSaveMetadata}
        onSaveAndNext={handleSaveAndNext}
        thumbnailFor={thumbnails.get}
        onRequestThumbnail={thumbnails.request}
      />
```

右パネルを畳んでいる間の主導線（`collapsedPrimaryAction`）も繋ぐ:

```svelte
    <Button
      variant="filled"
      disabled={!canWriteEditing || !editingIsJpeg || !metadata.isDirty || metadata.saving}
      onclick={handleSaveMetadata}>保存</Button>
```

- [ ] **Step 7: スタブに 3 コマンドを足す**

`gui-frontend/e2e/stub.ts` の `TEMPLATE` に追記（**形は契約から取る**）:

```ts
  metadata: dto.metadataRead,
  emptyMetadata: dto.editableMetadataEmpty,
```

`StubOptions` に追記:

```ts
  /** read_image_metadata が返す食い違いの警告 */
  metadataWarnings?: string[];
  /** grant_metadata_editing が「許可しない」を返す */
  denyGrant?: boolean;
```

`handlers` に追記（**パスごとに保存して、保存 → 読み直しが繋がるようにする**）:

```ts
      read_image_metadata: ({ path }: { path: string }) => ({
        metadata: stored.get(path) ?? template.emptyMetadata,
        warnings: stored.has(path) ? [] : metadataWarnings,
      }),
      write_image_metadata: ({ path, metadata }: { path: string; metadata: unknown }) => {
        if (!granted) throw "This folder has not been approved for metadata editing";
        stored.set(path, metadata);
        return { metadata, warnings: [] };
      },
      grant_metadata_editing: ({ folder }: { folder: string }) => {
        if (denyGrant) return null;
        granted = true;
        return folder;
      },
```

`const stored = new Map<string, unknown>();` と `let granted = false;` を
`addInitScript` の中（`presets` の隣）に置く。

**`addInitScript` に渡す引数オブジェクトにも `metadataWarnings` と `denyGrant` を
足すのを忘れないこと。** `installTauriStub` は options を分解して明示的な
オブジェクトで渡しており、`StubOptions` に型を足しただけでは
ページの中に値が届かない（既定値も同じ場所で埋める）。

- [ ] **Step 8: e2e を書き換える**

`gui-frontend/e2e/metadata.spec.ts` の「保存ボタンは disabled（データ接続は次工程）」を
削除し、次に差し替える:

```ts
  test("承認前は保存できない（spec §衝突2）", async ({ page }) => {
    await page.getByLabel("タイトル").fill("夕暮れの港");
    // 前提条件: 未保存の変更はある（＝「変更が無いから disabled」ではない）
    await expect(
      page.locator('[data-region="right"]').getByText("未保存の変更があります")
    ).toBeVisible();
    await expect(page.getByRole("button", { name: "保存", exact: true })).toBeDisabled();
  });

  test("承認すると保存でき、値がファイルに残る", async ({ page }) => {
    await page.getByRole("button", { name: "このフォルダーへの書き込みを許可..." }).click();
    await expect(page.getByText("このフォルダーへ書き込めます", { exact: false })).toBeVisible();

    await page.getByLabel("タイトル").fill("夕暮れの港");
    await page.getByRole("button", { name: "保存", exact: true }).click();

    const panel = page.locator('[data-region="right"]');
    await expect(panel.getByText("未保存の変更があります")).toBeHidden();

    // 別の写真へ移って戻ると、保存した値が読み直される
    await page.getByRole("option").nth(1).click();
    await expect(page.getByLabel("タイトル")).toHaveValue("");
    await page.getByRole("option").nth(0).click();
    await expect(page.getByLabel("タイトル")).toHaveValue("夕暮れの港");
  });

  test("食い違い警告が編集欄の脇に出る（spec §食い違いは警告として返す）", async ({
    page,
  }) => {
    // beforeEach とは別条件なのでこのテスト内で貼り直す
    await installTauriStub(page, {
      imageCount: 4,
      metadataWarnings: ["コメントに、他のアプリが書いた別の値があります"],
    });
    await page.goto("/");
    await page.getByRole("button", { name: "photos", exact: false }).first().click();
    await page.getByRole("option").first().click();
    await page
      .getByRole("navigation", { name: "モード" })
      .getByRole("button", { name: "情報" })
      .click();

    await expect(
      page.locator('[data-region="right"]').getByText("他のアプリが書いた別の値", { exact: false })
    ).toBeVisible();
  });

  test("「保存して次の写真へ」は編集対象を 1 つ進める", async ({ page }) => {
    await page.getByRole("button", { name: "このフォルダーへの書き込みを許可..." }).click();
    await page.getByLabel("タイトル").fill("一枚目");
    await page.getByRole("button", { name: "保存して次の写真へ" }).click();
    await expect(
      page.locator('[data-region="right"]').getByText("photo-0001.jpg")
    ).toBeVisible();
  });
```

- [ ] **Step 9: 走らせる**

Run: `make typecheck && make test-frontend && cd gui-frontend && bun run e2e metadata`
（5174 が塞がっていれば `E2E_PORT=5199 bun run e2e metadata`）
Expected: PASS

- [ ] **Step 10: コミット**

```bash
git add gui-frontend/src gui-frontend/e2e
git commit -m "feat(gui): メタデータ編集: パネルを読み書きと書き込み承認に繋ぐ"
```

---

## Task 15: 未保存ガード（アプリ内の離脱経路）

刷新 spec §3-4 の表のうち、**ウィンドウを閉じる以外の 3 経路**をここで塞ぐ。
`isDirty` の判定は Task 14 のストアにあるので、呼び出し側は 1 つの関数に通すだけ。

**変換モードでのクリック（選択トグル）ではガードを出さない。** あれは `editingPath` を
動かさないため（刷新 spec §3-2）。

**Files:**
- Create: `gui-frontend/src/lib/UnsavedChangesDialog.svelte`
- Modify: `gui-frontend/src/App.svelte`
- Modify: `gui-frontend/e2e/metadata.spec.ts`

**Interfaces:**
- Consumes: Task 14 の `metadata.isDirty` / `save` / `discard`
- Produces: `App.svelte` 内の `guarded(action: () => void): void`

- [ ] **Step 1: ダイアログを作る**

`gui-frontend/src/lib/UnsavedChangesDialog.svelte`:

```svelte
<script lang="ts">
  import Button from "./ui/Button.svelte";
  import Dialog from "./ui/Dialog.svelte";

  interface Props {
    /** 編集中の写真のファイル名 */
    name: string;
    onCancel: () => void;
    onDiscard: () => void;
    onSave: () => void;
    /** 承認前など、保存できない状態では「保存して移動」を出さない */
    canSave: boolean;
  }

  let { name, onCancel, onDiscard, onSave, canSave }: Props = $props();
</script>

<!-- 破棄が選べるので alertdialog。初期フォーカスは破棄でない側に置く -->
<Dialog title="保存していない変更があります" danger initialFocus="footer button" onClose={onCancel}>
  <p>{name} の変更はまだ書き込まれていません。</p>
  {#snippet actions()}
    <Button variant="text" onclick={onCancel}>キャンセル</Button>
    <Button variant="text" danger onclick={onDiscard}>破棄して移動</Button>
    {#if canSave}
      <Button variant="filled" onclick={onSave}>保存して移動</Button>
    {/if}
  {/snippet}
</Dialog>
```

- [ ] **Step 2: `App.svelte` に離脱経路を通す**

```ts
  // 未保存のまま離れようとしたときに保留する操作。
  // 3 経路（写真の切替 / モードの移動 / フォルダーの変更）をここへ集める
  let pendingLeave = $state<(() => void) | null>(null);

  function guarded(action: () => void) {
    if (metadata.isDirty) {
      pendingLeave = action;
      return;
    }
    action();
  }

  function resumeLeave() {
    const action = pendingLeave;
    pendingLeave = null;
    action?.();
  }

  function handleDiscardAndLeave() {
    metadata.discard();
    resumeLeave();
  }

  async function handleSaveAndLeave() {
    if (await handleSaveMetadata()) resumeLeave();
    else pendingLeave = null;
  }
```

3 経路をくぐらせる:

```ts
  function handleFocus(image: ImageEntry) {
    if (mode === "metadata") {
      // **`focusedPath` の代入もガードの内側に入れる。** 外に出すと、
      // ダイアログでキャンセルしてもグリッドの選択だけが移動する
      guarded(() => {
        focusedPath = image.path;
        editingPath = image.path;
      });
      return;
    }
    // 変換モードのクリックは focusedPath と selectedPaths にしか触らない（刷新 spec §3-2）
    focusedPath = image.path;
  }

  function handleModeChange(next: AppMode) {
    // メタデータモードから出るときだけガードする。入るときは失うものが無い
    if (mode === "metadata" && next !== "metadata") {
      guarded(() => applyModeChange(next));
      return;
    }
    applyModeChange(next);
  }

  function applyModeChange(next: AppMode) {
    mode = next;
    if (next === "metadata" && editingPath === null) editingPath = focusedPath;
    if (next === "frame" && frame.draft === null) frame.select(presets.selectedName);
  }

  async function handleSelectFolder(path: string) {
    guarded(() => void loadFolder(path));
  }
```
（既存の `handleSelectFolder` の中身を `loadFolder` に切り出し、
フォルダーを変えたら `editingPath = null` も足す。）

描画:

```svelte
{#if pendingLeave !== null}
  <UnsavedChangesDialog
    name={editingImage?.name ?? ""}
    canSave={canWriteEditing && editingIsJpeg}
    onCancel={() => (pendingLeave = null)}
    onDiscard={handleDiscardAndLeave}
    onSave={handleSaveAndLeave}
  />
{/if}
```

- [ ] **Step 3: e2e を足す**

`gui-frontend/e2e/metadata.spec.ts` に追記:

```ts
  test("未保存のまま別の写真へ移ろうとすると止められる（刷新 spec §3-4）", async ({ page }) => {
    await page.getByLabel("タイトル").fill("書きかけ");
    await page.getByRole("option").nth(1).click();

    await expect(page.getByRole("alertdialog", { name: "保存していない変更があります" })).toBeVisible();
    // 前提条件: まだ移動していない
    await expect(page.locator('[data-region="right"]').getByText("photo-0000.jpg")).toBeVisible();

    await page.getByRole("button", { name: "破棄して移動" }).click();
    await expect(page.locator('[data-region="right"]').getByText("photo-0001.jpg")).toBeVisible();
    await expect(page.getByLabel("タイトル")).toHaveValue("");
  });

  test("キャンセルすると編集内容が残る", async ({ page }) => {
    await page.getByLabel("タイトル").fill("書きかけ");
    await page.getByRole("option").nth(1).click();
    await page.getByRole("button", { name: "キャンセル" }).click();

    await expect(page.getByLabel("タイトル")).toHaveValue("書きかけ");
    await expect(page.locator('[data-region="right"]').getByText("photo-0000.jpg")).toBeVisible();
  });

  test("rail で別モードへ移るときもガードが出る", async ({ page }) => {
    await page.getByLabel("タイトル").fill("書きかけ");
    await page
      .getByRole("navigation", { name: "モード" })
      .getByRole("button", { name: "変換" })
      .click();
    await expect(page.getByRole("alertdialog", { name: "保存していない変更があります" })).toBeVisible();
  });

  /**
   * **変換モードのクリックではガードを出さない。**
   * 出すと、メタデータで編集途中のまま変換モードで写真をチェックするたびに
   * ダイアログが出る（刷新 spec §3-2）
   */
  test("変換モードでの選択トグルではガードが出ない", async ({ page }) => {
    await page.getByLabel("タイトル").fill("書きかけ");
    const rail = page.getByRole("navigation", { name: "モード" });
    await rail.getByRole("button", { name: "変換" }).click();
    await page.getByRole("button", { name: "破棄して移動" }).click(); // ここは出てよい

    // 変換モードに入った（タイトル欄はもう無い）
    await expect(page.getByLabel("タイトル")).toHaveCount(0);
    await expect(page.getByRole("alertdialog")).toBeHidden();
    await page.getByRole("option").nth(3).click();
    await expect(page.getByRole("alertdialog")).toBeHidden();
  });
```

> **Playwright の自動リトライに注意。** 「ダイアログが出ない」の検査は、
> 欠陥をわざと戻して**落ちることを実測してから**採用すること
> （短命な状態は自動リトライで素通りする）。

- [ ] **Step 4: 走らせてコミット**

Run: `make typecheck && cd gui-frontend && bun run e2e metadata`

```bash
git add gui-frontend/src gui-frontend/e2e
git commit -m "feat(gui): メタデータ編集: 未保存のまま離れる3経路にガードを入れる"
```

---
## Task 16: 終了時の未保存ガード（`set_unsaved_state` + `on_window_event`）

刷新 spec §3-4 が**本工程へ明示的に持ち越した**もの。

> フロントは dirty 状態が変わるたびに `set_unsaved_state(bool)` で Rust に通知する。
> `on_window_event` が `CloseRequested` を捕まえ、未保存があれば
> **常に `api.prevent_close()` を呼んでから、別スレッドで** OS ネイティブのダイアログを出し、
> 承認された場合にのみ `window.destroy()` を呼ぶ。

**`on_window_event` の中で `blocking_show()` を直接呼んではならない。**
既存の `confirm_delete_originals` は `async` コマンドの中＝ tokio のワーカースレッド上で
走るので成立している。`on_window_event` はメインスレッドで走るため、同じ書き方をすると
イベントループを塞いでダイアログが出ないまま固まる。

**Files:**
- Modify: `gui/src/state.rs`, `gui/src/commands.rs`, `gui/src/main.rs`
- Modify: `gui-frontend/src/lib/api.ts`, `gui-frontend/src/App.svelte`
- Modify: `gui-frontend/e2e/stub.ts`, `gui-frontend/e2e/metadata.spec.ts`

**Interfaces:**
- Consumes: Task 14 の `metadata.isDirty`
- Produces: `set_unsaved_state(unsaved: bool)` コマンド、`api.setUnsavedState(unsaved)`

- [ ] **Step 1: 状態を足す**

`gui/src/state.rs`:

```rust
pub struct ProcessingState {
    pub cancel_flag: Arc<AtomicBool>,
    pub thumbnail_cache: Mutex<LruCache<String, String>>,
    pub writable_roots: WritableRoots,
    /// メタデータの下書きに未保存の変更があるか。
    /// フロントが `set_unsaved_state` で更新し、終了時の確認だけがこれを読む
    pub unsaved: AtomicBool,
    /// 終了確認のダイアログを表示中か。
    /// **表示中にもう一度 `CloseRequested` が来うる**ので抑止に使う（刷新 spec §3-4）
    pub close_prompt_open: AtomicBool,
}
```

`new()` に `unsaved: AtomicBool::new(false)` と `close_prompt_open: AtomicBool::new(false)` を足す。

- [ ] **Step 2: コマンドを足す**

`gui/src/commands.rs`:

```rust
/// 未保存の有無を Rust 側へ通知する
///
/// フロントで `onCloseRequested` を使わないのは、`core:default` に
/// `window:allow-destroy` が含まれず、webview に追加権限が要るため。
/// 「webview にプラグイン権限を与えない」という既存の前提（`security.rs`）に反する。
#[tauri::command]
pub fn set_unsaved_state(state: tauri::State<'_, ProcessingState>, unsaved: bool) {
    state.unsaved.store(unsaved, Ordering::SeqCst);
}
```

`main.rs` の `generate_handler!` に `commands::set_unsaved_state,` を足す。

- [ ] **Step 3: `on_window_event` を足す**

`gui/src/main.rs`:

```rust
use std::sync::atomic::Ordering;
use tauri::Manager;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
```

`tauri::Builder::default()` の鎖に:

```rust
        // 終了だけは webview に任せない。不可逆な操作は OS ネイティブで確認する
        // （元ファイル削除と同じ扱い）。刷新 spec §3-4
        .on_window_event(|window, event| {
            let tauri::WindowEvent::CloseRequested { api, .. } = event else {
                return;
            };
            let state = window.state::<ProcessingState>();
            if !state.unsaved.load(Ordering::SeqCst) {
                return; // そのまま閉じてよい
            }

            // 未保存があるときは**常に**止めてから確認する
            api.prevent_close();

            // ダイアログ表示中に再度 CloseRequested が来ても二重に出さない
            if state.close_prompt_open.swap(true, Ordering::SeqCst) {
                return;
            }

            // **メインスレッドで blocking_show() を呼ばない。**
            // イベントループを塞いでダイアログが出ないまま固まる
            let handle = window.app_handle().clone();
            std::thread::spawn(move || {
                let mut builder = handle
                    .dialog()
                    .message("保存していない変更があります。\n閉じると失われます。")
                    .title("保存せずに終了しますか？")
                    .kind(MessageDialogKind::Warning)
                    .buttons(MessageDialogButtons::OkCancelCustom(
                        "保存せずに終了".to_string(),
                        "戻る".to_string(),
                    ));
                let main = handle.get_webview_window("main");
                if let Some(ref w) = main {
                    builder = builder.parent(w);
                }
                let approved = builder.blocking_show();

                let state = handle.state::<ProcessingState>();
                state.close_prompt_open.store(false, Ordering::SeqCst);
                if approved {
                    // 次の CloseRequested で再び聞かれないようにしてから閉じる
                    state.unsaved.store(false, Ordering::SeqCst);
                    if let Some(w) = main {
                        let _ = w.destroy();
                    }
                }
            });
        })
```

- [ ] **Step 4: フロントから通知する**

`gui-frontend/src/lib/api.ts`:

```ts
/**
 * 未保存の有無を Rust に伝える。終了時の確認は Rust 側が OS ネイティブで出す。
 *
 * webview で `onCloseRequested` を使うには `window:allow-destroy` が要り、
 * capabilities を `core:default` に留める前提から外れる（S6-H8 / 刷新 spec §3-4）。
 */
export async function setUnsavedState(unsaved: boolean): Promise<void> {
  return invoke("set_unsaved_state", { unsaved });
}
```

`gui-frontend/src/App.svelte`:

```ts
  // 未保存の有無が変わるたびに Rust へ伝える。Tauri の外（vite dev）では
  // invoke が必ず reject するので、握りつぶす
  $effect(() => {
    const dirty = metadata.isDirty;
    untrack(() => {
      void setUnsavedState(dirty).catch(() => {});
    });
  });
```

- [ ] **Step 5: スタブに呼び出しの記録を足す**

`gui-frontend/e2e/stub.ts` の invoke シムに、検査用の記録を足す:

```ts
    // どのコマンドが何で呼ばれたかを検査できるようにする。
    // 「Rust へ通知した」型の配線は、画面に出ないので他に確かめようが無い
    const calls: { cmd: string; args: unknown }[] = [];
    (window as unknown as Record<string, unknown>).__stubCalls = calls;
```

`invoke(cmd, args)` の入口で `calls.push({ cmd, args })`。
`handlers` に `set_unsaved_state: () => null,` を足す。

- [ ] **Step 6: e2e を足す**

```ts
  test("未保存の有無が Rust へ伝わる（刷新 spec §3-4）", async ({ page }) => {
    const lastUnsaved = () =>
      page.evaluate(() => {
        const calls = (window as any).__stubCalls as { cmd: string; args: any }[];
        const hits = calls.filter((c) => c.cmd === "set_unsaved_state");
        return hits.length === 0 ? null : hits[hits.length - 1].args.unsaved;
      });

    // 前提条件: 触る前は false（＝「常に true」ではない）
    await expect.poll(lastUnsaved).toBe(false);

    await page.getByLabel("タイトル").fill("書きかけ");
    await expect.poll(lastUnsaved).toBe(true);

    await page.getByRole("button", { name: "このフォルダーへの書き込みを許可..." }).click();
    await page.getByRole("button", { name: "保存", exact: true }).click();
    await expect.poll(lastUnsaved).toBe(false);
  });
```

- [ ] **Step 7: 実機で確かめる**

**e2e では検査できない部分がある。** `blocking_show()` をメインスレッドで
呼んだ場合の症状は「ダイアログが出ないままアプリが固まる」で、
`vite dev` には `on_window_event` 自体が存在しない。

Run: `make dev`
手順: 情報モードでタイトルを編集 → ウィンドウを閉じる
Expected: OS ネイティブのダイアログが出て、アプリが固まらない。
「戻る」で閉じない。「保存せずに終了」で閉じる。**閉じるボタンを連打しても
ダイアログが 2 つ出ない**（多重発火の抑止）。

結果を実施メモに記録すること。

- [ ] **Step 8: コミット**

```bash
git add gui/src gui-frontend/src gui-frontend/e2e
git commit -m "feat(gui): メタデータ編集: 未保存のまま終了しようとしたら OS のダイアログで確認する"
```

---

## Task 17: 写真グリッドの★と未保存マーク

刷新 spec §4-3 が**本工程へ持ち越した**もの。「そのとき、3,000 枚分のメタデータ
読み取りコストを実測し、重ければ可視範囲のみの遅延読み込みに切り替える」。

**実測してから決める。** ただし 3,000 枚を先読みする案は最初から採らない
（サムネイルと同じく可視範囲で足りる）。実測は「可視範囲 1 画面ぶんが
体感できる遅れなく出るか」を確かめるために行う。

**Files:**
- Modify: `core/tests/metadata_spike.rs`（計測）
- Modify: `gui/src/types.rs`, `gui/src/commands.rs`, `gui/src/main.rs`
- Create: `gui-frontend/src/lib/browser/ratings.svelte.ts`
- Modify: `gui-frontend/src/lib/browser/PhotoGrid.svelte`, `gui-frontend/src/App.svelte`
- Modify: `gui-frontend/e2e/stub.ts`, `gui-frontend/e2e/metadata.spec.ts`

**Interfaces:**
- Consumes: Task 8 の `read_metadata`、Task 11 の `readable_jpeg`
- Produces:
  ```rust
  pub struct RatingEntry { pub path: String, pub rating: Option<u8> }
  #[tauri::command] read_image_ratings(paths: Vec<String>) -> Result<Vec<RatingEntry>, String>
  ```
  ```ts
  createRatingStore(): { get(path): number | null | undefined; request(paths: string[]): void; invalidate(path: string): void; forget(): void }
  ```

- [ ] **Step 1: 読み取りコストを実測する**

`core/tests/metadata_spike.rs` に追記:

```rust
#[test]
#[ignore = "実測。PICTURE_TOOL_SPIKE_FOLDER に写真フォルダーを渡す"]
fn measure_read_metadata_cost() {
    let folder = std::env::var("PICTURE_TOOL_SPIKE_FOLDER")
        .expect("PICTURE_TOOL_SPIKE_FOLDER に写真フォルダーのパスを渡すこと");
    let files: Vec<_> = std::fs::read_dir(&folder)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("jpg") || e.eq_ignore_ascii_case("jpeg"))
        })
        .take(60)
        .collect();
    assert!(!files.is_empty(), "JPEG が 1 枚も無い: {}", folder);

    let started = std::time::Instant::now();
    for file in &files {
        let _ = picture_tool_core::metadata::read_metadata(file);
    }
    let elapsed = started.elapsed();
    println!(
        "{} 枚 / {:?} → 1 枚あたり {:?}",
        files.len(),
        elapsed,
        elapsed / files.len() as u32
    );
}
```

Run:
```bash
PICTURE_TOOL_SPIKE_FOLDER=/path/to/photos \
  cargo test -p picture-tool-core --test metadata_spike measure_read_metadata -- --ignored --nocapture
```

**測った値を実施メモに書く。** 目安: 1 画面ぶん（約 30 枚）が 300ms 以内に揃うこと。
超える場合は Step 2 の `read_image_ratings` を rayon で並列化し、それでも足りなければ
実施メモに数字とともに記録して、★表示自体を見送る判断を報告する。

- [ ] **Step 2: バッチ読み取りコマンドを足す**

`gui/src/types.rs`:

```rust
/// 写真グリッドに★を出すための最小の応答。
/// `read_image_metadata` を 1 枚ずつ呼ぶと可視範囲だけで数十回の IPC になる
#[derive(Debug, Clone, Serialize)]
pub struct RatingEntry {
    pub path: String,
    pub rating: Option<u8>,
}
```

`mod contract` に:

```rust
            entry(
                "ratingEntry",
                "RatingEntry",
                "グリッドの★1 件（読めなかった写真は rating: null）",
                RatingEntry {
                    path: "/home/user/photos/photo.jpg".to_string(),
                    rating: Some(3),
                },
            ),
```
と、import 一覧に `"RatingEntry"` を足す。

`gui/src/commands.rs`:

```rust
/// 可視範囲ぶんの★をまとめて読む
///
/// 読めなかった写真（JPEG 以外・壊れている・消えた）は `rating: None` で返す。
/// **1 枚の失敗で全件を落とさない** ── グリッドの飾りであって、
/// 出なくても操作は成立する。
#[tauri::command]
pub async fn read_image_ratings(paths: Vec<String>) -> Result<Vec<RatingEntry>, String> {
    /// 1 回の要求で読む上限。可視範囲は多くても 40 枚程度で、
    /// 乗っ取られた webview がライブラリ全体を投げてきても走査量が跳ねない
    const MAX_BATCH: usize = 64;

    let paths: Vec<String> = paths.into_iter().take(MAX_BATCH).collect();
    tokio::task::spawn_blocking(move || {
        use rayon::prelude::*;
        let entries = paths
            .par_iter()
            .map(|raw| {
                let rating = security::readable_jpeg(raw)
                    .ok()
                    .and_then(|file| core::metadata::read_metadata(&file).ok())
                    .and_then(|read| read.metadata.rating);
                RatingEntry { path: raw.clone(), rating }
            })
            .collect();
        Ok(entries)
    })
    .await
    .map_err(|e| e.to_string())?
}
```

**`rayon` は workspace で固定する。** ルートの `Cargo.toml` の
`[workspace.dependencies]` に `rayon = "1.10"` を移し、`core/Cargo.toml` と
`gui/Cargo.toml` の双方を `rayon.workspace = true` にする。gui 側に版を直書きすると
「複数クレートが使う依存は workspace で固定する」（S6-M19）に自分で反することになる。
`main.rs` の `generate_handler!` に `commands::read_image_ratings,` を足す。

契約を再生成する:
Run: `UPDATE_DTO_CONTRACT=1 cargo test -p picture-tool-gui contract`

- [ ] **Step 3: フロントのストアを作る**

`gui-frontend/src/lib/types.ts`:

```ts
export interface RatingEntry {
  path: string;
  /** 1〜5。null は未評価、または読めなかった */
  rating: number | null;
}
```

`gui-frontend/src/lib/api.ts`:

```ts
export async function readImageRatings(paths: string[]): Promise<RatingEntry[]> {
  return invoke("read_image_ratings", { paths });
}
```

`gui-frontend/src/lib/browser/ratings.svelte.ts`:

```ts
import { SvelteMap } from "svelte/reactivity";
import { readImageRatings } from "../api";

/**
 * 写真グリッドに出す★の保持。
 *
 * **可視範囲だけ読む。** 3,000 枚を先読みすると、1 枚ごとにファイル全体を読んで
 * Exif と XMP を解析することになる（サムネイルと同じ理由で可視範囲に閉じる）。
 * 1 回の要求は 1 度の IPC にまとめる ── 40 回の往復にすると、
 * スクロールのたびに IPC が詰まる。
 */
/** 1 回の要求で送る枚数。Rust 側の `MAX_BATCH` と揃える */
const CHUNK = 64;

export function createRatingStore() {
  const ratings = new SvelteMap<string, number | null>();
  let inFlight = new Set<string>();

  return {
    /** 未取得なら undefined。読んだ結果が「未評価」なら null */
    get(path: string): number | null | undefined {
      return ratings.get(path);
    },

    request(paths: string[]) {
      const missing = paths.filter((p) => !ratings.has(p) && !inFlight.has(p));
      if (missing.length === 0) return;
      // **Rust 側の上限（64）で分割してから送る。**
      // 超過分をサーバーに切らせると、返ってこなかったパスは ratings に入らないまま
      // inFlight から消え、可視範囲が動くたびに同じ要求を繰り返して永遠に埋まらない
      for (let at = 0; at < missing.length; at += CHUNK) {
        const chunk = missing.slice(at, at + CHUNK);
        chunk.forEach((p) => inFlight.add(p));
        readImageRatings(chunk)
          .then((entries) => {
            for (const entry of entries) ratings.set(entry.path, entry.rating);
          })
          .catch(() => {
            // ★はグリッドの飾り。読めなくても操作は成立するので通知しない
          })
          .finally(() => {
            chunk.forEach((p) => inFlight.delete(p));
          });
      }
    },

    /** 保存した 1 枚だけ取り直す */
    invalidate(path: string) {
      ratings.delete(path);
    },

    /** フォルダーを変えたら捨てる */
    forget() {
      ratings.clear();
      inFlight = new Set();
    },
  };
}
```

- [ ] **Step 4: グリッドに出す**

`PhotoGrid.svelte` の `Props` に足す:

```ts
    /** タイルに出す★。undefined は未取得 */
    ratingFor: (path: string) => number | null | undefined;
    /** 未保存の変更があるメタデータの編集対象 */
    unsavedPath: string | null;
```

**`{@const}` はブロック（`{#each}` / `{#if}` など）の直接の子でなければならない。**
`<div class="tile">` の直下に書くと `const_tag_invalid_placement` でコンパイルが落ちる。
既存の `{@const index = ...}` / `{@const thumb = ...}` と同じ位置、
`{#each visible as image, offset (image.path)}` の直下に足す:

```svelte
      {#each visible as image, offset (image.path)}
        {@const index = range.startIndex + offset}
        {@const thumb = thumbnailFor(image.path, metrics.thumbnailSize)}
        {@const stars = ratingFor(image.path)}
```

そのうえで、タイルの中（既存のファイル名の並びに合わせて）:

```svelte
          {#if unsavedPath === image.path}
            <span class="unsaved-mark" title="未保存の変更があります" aria-hidden="true">●</span>
          {/if}
          {#if stars}
            <span class="stars" aria-hidden="true">{"★".repeat(stars)}</span>
          {/if}
```

★と未保存マークは `aria-hidden`。タイルの `aria-label` に足すと、
読み上げが「photo-0001.jpg ★★★」になって選択操作の邪魔になる。
値そのものは右パネルの `Rating`（`role="slider"`）が読み上げる。

色は必ずトークンから取る:

```css
  .stars {
    color: var(--md-sys-color-primary);
    font: var(--md-sys-typescale-body-sm);
  }

  .unsaved-mark {
    color: var(--md-sys-color-error);
  }
```

`App.svelte`:

```ts
  const ratings = createRatingStore();

  // 可視範囲を App にも持たせる。**モードの切替では可視範囲が変わらない**ので、
  // onVisibleRangeChange だけを起点にすると、変換モードから情報モードへ入って
  // スクロールするまで★が 1 つも出ない（PhotoGrid の $effect は range 依存）
  let visibleRange = $state({ start: 0, end: -1 });

  function handleVisibleRangeChange(start: number, end: number) {
    thumbnails.setVisibleRange(start, end);
    visibleRange = { start, end };
  }

  // 可視範囲・モード・写真一覧のどれが動いても、情報モードなら★を取りに行く
  $effect(() => {
    if (mode !== "metadata") return;
    const paths = images.slice(visibleRange.start, visibleRange.end + 1).map((img) => img.path);
    untrack(() => ratings.request(paths));
  });
```
`loadFolder` の中で `ratings.forget()`、`handleSaveMetadata` の成功時に
`ratings.invalidate(metadata.path!)` を呼ぶ。

`PhotoGrid` に `ratingFor={ratings.get}` と
`unsavedPath={metadata.isDirty ? editingPath : null}` を渡す。

- [ ] **Step 5: スタブと e2e**

`stub.ts` の `handlers` に:

```ts
      read_image_ratings: ({ paths }: { paths: string[] }) =>
        paths.map((path) => ({
          path,
          rating: (stored.get(path) as { rating: number | null } | undefined)?.rating ?? null,
        })),
```

`metadata.spec.ts` に:

```ts
  test("保存した★がグリッドのタイルにも出る（刷新 spec §4-3）", async ({ page }) => {
    const tile = page.getByRole("option").first();
    // 前提条件: いまは★が出ていない
    await expect(tile.getByText("★", { exact: false })).toBeHidden();

    await page.getByRole("button", { name: "このフォルダーへの書き込みを許可..." }).click();
    const rating = page.getByRole("slider", { name: "レーティング" });
    await rating.focus();
    await rating.press("ArrowRight");
    await rating.press("ArrowRight");
    await rating.press("ArrowRight");
    await page.getByRole("button", { name: "保存", exact: true }).click();

    await expect(tile.getByText("★★★")).toBeVisible();
  });

  test("未保存の写真にタイル上の印が出る", async ({ page }) => {
    const tile = page.getByRole("option").first();
    await expect(tile.getByTitle("未保存の変更があります")).toBeHidden(); // 前提条件
    await page.getByLabel("タイトル").fill("書きかけ");
    await expect(tile.getByTitle("未保存の変更があります")).toBeVisible();
  });
```

- [ ] **Step 6: 走らせてコミット**

Run: `make check && cd gui-frontend && bun run e2e metadata`

```bash
git add core gui gui-frontend
git commit -m "feat(gui): メタデータ編集: 可視範囲だけ読んでグリッドに★と未保存の印を出す"
```

---

## Task 18: spec と CLAUDE.md の書き換え

**Files:**
- Modify: `docs/superpowers/specs/2026-08-18-metadata-editing-design.md`
- Modify: `docs/README.md`
- Modify: `CLAUDE.md`

- [ ] **Step 1: spec の §GUI を差し替える**

「## GUI」の「### 構造」から「### 保存の体験」までを、次で置き換える。
**同じ設計を 2 つの spec に書かない**（食い違ったときにどちらが正か決められなくなる）。

```markdown
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
```

- [ ] **Step 2: spec の `security.rs` の節を直す**

「### security.rs に追加する検証」の条件 1 を、実装に合わせて直す。

```markdown
1. 実体が **JPEG** の拡張子を持つこと（`readable_image` より強い。メタデータ編集の
   対象は JPEG だけで、PNG / WebP は core の `metadata` が扱わない）
```

追加する関数は `writable_image` と、その内側で使う `readable_jpeg`（読み取り側の
コマンドも同じ条件で通す）の 2 本である旨を書き足す。

- [ ] **Step 3: spec の「実装順序」と「未検証のリスク」を現状に合わせる**

- 「実装順序」の 1 と 2 に **完了印**を付け、3 が本計画であることと計画への
  リンクを書く
- 「検証済みの事実と、未検証のリスク」の**冒頭に結論を追記する**。Task 1 の実測結果
  （カメラ直出し JPEG での round-trip）と、Task 7 で既存 XMP 保持を満たしたこと。
  **数字と機種名を書く** ── 「通った」だけでは次に読む人が再検査できない

- [ ] **Step 4: `docs/README.md` を直す**

- メタデータ編集 spec を「設計確定・実装前」から「現行仕様」の表へ移す
- 「直近の実装計画」の先頭に本計画を足す

- [ ] **Step 5: `CLAUDE.md` を直す**

- 「Core ライブラリ API」の表に行を足す:

```markdown
| メタデータ編集 | `metadata::read_metadata`（→ `MetadataRead { metadata, warnings }`）, `metadata::write_metadata`, `metadata::EditableMetadata` |
```

- 「設計方針」の 1 行目を書き換える（**spec §衝突1 の要求**）:

```markdown
- 変換処理は元の画像ファイルを上書きしない（`--delete-originals` で明示的に削除）。
  メタデータ編集だけは元ファイルの**メタデータセグメントのみを差し替える**
  （画素は再エンコードせずバイト単位で不変。読み戻し検証つきの原子的置換）
```

- 「GUI」の節のモード表の「情報」行に、メタデータ編集ができることを書き足す
- 「バックエンド（`gui/src/`）の前提」に、書き込み承認の 1 行を足す:

```markdown
- メタデータ編集の書き込み先は、**フォルダー単位で 1 度だけ** OS ネイティブの確認を
  経て `WritableRoots` に足す（`grant_metadata_editing`）。承認はアプリ終了で消える
```

- 「ユーザーデータの置き場所」の下に、メタデータの書き込み先が
  **利用者の写真そのもの**であることを 1 行足す

- [ ] **Step 6: 全部通してコミット**

Run: `make check && cd gui-frontend && bun run e2e`
Expected: PASS

```bash
git add docs CLAUDE.md
git commit -m "docs: メタデータ編集: spec の GUI 節を刷新に寄せ、実装済みの前提を反映する"
```

- [ ] **Step 7: ブランチを畳む**

`superpowers:finishing-a-development-branch` に従って `main` へ統合する。

---

## 実施メモ

> 実行者が埋める。**「なぜその形なのか」はここに残る**（spec には結果だけが載る）。

### Task 1 の実測（必須）

| 項目 | 結果 |
|---|---|
| 使った画像（機種 / サイズ） | |
| MakerNote / GPS の有無 | |
| 1 回目の書き込み | |
| 2 回目の書き込み（issue #93） | |
| ピクセル不変 | |
| 消えたタグ | |

### Task 17 の実測（必須）

| 項目 | 結果 |
|---|---|
| `read_metadata` 1 枚あたり | |
| 可視範囲 30 枚ぶん | |
| 採った方針 | |

### Task 16 の実機確認（必須）

| 項目 | 結果 |
|---|---|
| 終了時にダイアログが出るか | |
| アプリが固まらないか | |
| 閉じるボタン連打で 2 つ出ないか | |

### spec と食い違った点

（実装して初めて分かったことを書く。spec 側も直したなら、どちらをどう直したか）

---

## 未決（ユーザー回答待ち。決まるまで着手しない）

| # | 何が分からないか | どちらに倒すと何が変わるか |
|---|---|---|
| U1 | Task 1 / 2 が要るカメラ直出し JPEG（MakerNote 付き）と Lightroom 書き出し JPEG が手元にあるか | **無いまま進める**と、spec の「割れたらクレート選定ごと見直す」分岐が判定不能のまま Task 3 以降へ進む。**待つ**なら Task 1 が着手のブロッカーになる |
| U2 | `docs/README.md` の「現行仕様」表に、レイアウトを刷新 spec へ委譲したメタデータ編集 spec を並べてよいか | **並べる**と表は 1 つで済むが、読者がレイアウトの出典を取り違えうる。**注記付きで分ける**と索引が複雑になる |
| U3 | 「保存して次の写真へ」でグリッドのスクロールを追従させるか | **追従させない**と、可視範囲の外の写真を編集し続ける状態になり得る。**追従させる**なら `PhotoGrid` にスクロール指示の窓口を足すことになる（刷新 spec §4-3 に記述なし） |
| U4 | 既存 XMP 保持の受け入れ条件を「バイト単位」と「値の単位」のどちらで書くか | spec は「1 バイトも変わらないこと」と書いているが、所有属性を落とした `rdf:Description` の start タグだけは組み直しで整形が変わる（値は不変）。**バイト単位を貫く**なら XML の書き戻しをやめてバイト列の切り貼りで実装することになり、Task 7 の複雑さが跳ねる。**値の単位に緩める**なら spec の当該行も直す |

U4 は本計画の Global Constraints と Task 7 に「値の単位」で暫定的に書いてあるが、
**spec 本文（§既存 XMP パケットは保持する / §テスト方針）はまだ直していない。**
U4 の回答が出るまで Task 18 で spec を書き換えないこと。

---

## レビュー反映（2026-09-01）

`docs/tmp/review.md` の指摘を 1 件ずつコードに当てて確認し、**全件採用**した。
主な変更点と、なぜそれが問題だったか:

| ID | 反映先 | 何が壊れていたか |
|---|---|---|
| C1 | Task 4 / 9 | XMP は読み取りで `trim()` するのに EXIF はしない。書き込み側も trim しないので、**末尾改行つきのコメントは保存が必ず失敗**し、食い違い警告も誤発火する。読み取りを全経路 trim に揃え、`normalized()` でも trim する |
| S1 | Task 5 / 7 | `make lint` が `-D warnings` の `dead_code` で落ちる（実利用者がテストだけの段階）。lint は Task 9 / 10 に寄せた |
| S2 | Task 17 | `{@const}` はブロックの直接の子でないとコンパイルできない。`{#each}` 直下へ移した |
| S3 | Task 8 / 9 | 既存 Exif の解析失敗を `unwrap_or_else` で握りつぶすと、**MakerNote ごと消えた Exif に置き換わる**。しかも `read_exif_info` が失敗時に `default` を返すため検証を素通りする（issue #93 の症状そのもの）。書き込みは Err、読み取りは warning にした |
| S4 | Task 7 / 10 | `push_attribute` は属性の区切りを単一スペースに正規化するので、`rdf:Description` を組み直すと整形が変わる。**落とす属性が無ければ元のタグをそのまま流す**ようにし、Global Constraints の言い方も実装に合わせた。統合テストの「40 バイト固定長で切る」は char 境界を割って panic するので、属性 1 つを丸ごと切り出す形に直した |
| S5 | Task 3 / 4 / 6 | モジュール宣言を Step 3 でしていたため、Step 2 の「落ちることを確認する」が **0 tests / PASS** で通っていた（red が偽物）。宣言を Step 1 へ |
| S6 | Task 12 / 13 | `types.ts` を赤のままコミットする手順だった。契約の red→green は同一タスク内で見届けてから緑にしてコミットする |
| S7 | Task 14 / 15 | 単一選択の枠は `focusedPath` で決まる。「保存して次へ」が `editingPath` しか進めず、ガードのキャンセル時も `focusedPath` だけ動いていた |
| S8 | Task 17 | ★の取得起点が可視範囲の変化だけで、**モードを切り替えただけでは出ない**。加えて 64 枚超をサーバー側で切ると、返らなかったぶんが永久に埋まらない。分割を送信側に置いた |
| S9 | Task 14 | 情報モードで PNG をクリックするたびにエラートーストが出る（グリッドは PNG / WebP も並べる）。非 JPEG では読みに行かない |
| S10 | Task 2 | フィクスチャに編集項目が入っていないことの確認手順が無かった |
| S11 | Task 17 | `gui/Cargo.toml` への `rayon` 直書きは、本計画自身が引く S6-M19 に反する。workspace で固定する |
| Nit ×6 | 各所 | 誤った根拠のコメント、検査していない e2e の行、`$effect` を「置き換える」の明示、XMP を Exif APP1 の隣に置く、Exif 解析失敗の warning、`grant_metadata_editing` が解決後のルートを返す |

---

## Self-Review（計画作成時に実施済み）

**spec の網羅**: §スコープ / §衝突 1・2 / §書き込むタグの仕様 / §エンコーディング /
§レーティングの値変換 / §`ImageDescription` の扱い / §既存 XMP パケットは保持する /
§読み取りの優先順位 / §食い違いは警告 / §Core API / §依存クレート / §`img-parts` の落とし穴 /
§書き込みの安全性 / §panic の封じ込め / §GUI / §追加する Tauri コマンド /
§`security.rs` に追加する検証 / §未検証のリスク / §テスト方針 ── すべて担当タスクあり。

**刷新 spec §8 の持ち越し 3 件**（`set_unsaved_state` / ★のグリッド表示 /
パネルのデータ接続）も Task 13〜17 で引き受けている。

**計画作成時に確認した事実**（推測ではない）:

- `little_exif` 0.6.23 の API: `Metadata::{new, new_from_vec, new_from_path, get_endian,
  get_ifd, get_tag_by_hex, set_tag, remove_tag_by_hex_group, write_to_vec, write_to_file}`、
  `ExifTag::{ImageDescription(String), UserComment(Vec<u8>), UnknownINT8U, UnknownINT16U}`、
  `ExifTagGroup::{GENERIC, EXIF, INTEROP, GPS}`、`Endian::{Little, Big}`
- `img-parts` 0.4.0: `Jpeg::{from_bytes, segments, segments_mut, segments_by_marker, encoder}`、
  `JpegSegment::{new_with_contents, marker, contents}`（`Clone` あり）、
  `markers::{APP1, SOS}`、`ImageEncoder::bytes`。**SOS はセグメント列に含まれる**ので
  `position(SOS)` の位置に `insert` できる
- `little_exif` 0.6.23 は `quick-xml` 0.37.5 に依存している（`cargo tree` で確認）。
  `quick-xml = "0.37"` を指定すれば 1 版に統合される
- `JpegSegment` に `entropy()` の公開アクセサは**無い**。ピクセル不変の検査は
  生バイト列の SOS 位置以降を比較する（Task 9 の `scan_offset`）
- `core/tests/fixtures/` の参照は `env!("CARGO_MANIFEST_DIR")` 経由
  （`core/src/exif_frame/logo.rs:171` と同じ）
- 刷新済みの `MetadataPanel.svelte` / `metadataDraft.svelte.ts` / `Rating.svelte` /
  `editingPath` の分離 / `e2e/metadata.spec.ts` は**既に存在する**。作り直さない
