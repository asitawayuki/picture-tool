# メタデータ書き込みの検証強化 実装計画

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** メタデータの保存で、編集対象 5 タグ以外の Exif・IFD1 サムネイル・Exif/XMP 以外のセグメントが消えた／変わったときに、元写真を置き換えずに保存を失敗させ、その理由を GUI の利用者まで届ける。

**Architecture:** 比較処理を新モジュール `core/src/metadata/preserve.rs` に置く。入力は書き込み前後の **SOS より前のバイト列**だけで、ファイルには触らない。`atomic::verify` の 3 番目の検査（`read_exif_info` の 9 項目比較）を `preserve::check` の呼び出しに置き換え、MakerNote の位置の変化だけは警告（`Ok(Vec<String>)`）として `write_metadata` の戻り値まで上げる。GUI の境界は保存失敗を `{e:#}` で文字列化し、警告を保存後の `MetadataRead.warnings` に足す。

**Tech Stack:** Rust（`picture-tool-core` / `picture-tool-gui`）、kamadak-exif 0.6.1（crate 名 `exif`）、little_exif =0.6.23、img-parts =0.4.0、anyhow

**Spec:** [`docs/superpowers/specs/2026-09-29-metadata-write-verification-design.md`](../specs/2026-09-29-metadata-write-verification-design.md)
（前提 spec: [`2026-08-18-metadata-editing-design.md`](../specs/2026-08-18-metadata-editing-design.md)。書き込むタグ・信頼境界・原子的置換はそちらが正）

## Global Constraints

- **検証は `make check`（lint / cargo test / typecheck / bun test）。** 各タスクの終わりで exit 0 にする。
  lint は `cargo clippy --workspace --all-targets -- -D warnings`（**未使用コードも失敗になる**）
- **`core` は `eprintln!` しない。** MakerNote の警告は `write_metadata` の戻り値で返し、出すかどうかは境界が決める
- **Exif は kamadak-exif の厳格読み（`continue_on_error` は既定の `false`）で読み、読めなければ拒否する。**
  `continue_on_error(true)` の部分結果で比べない（利用者判断 2026-09-30）
- **MakerNote の位置の変化は拒否せず、警告を返す**（利用者判断 2026-09-30）。バイト列の変化・消失は拒否
- **やらないこと:** `little_exif` の置き換え、Exif の自前書き込み、APP0 の配置の是正、XMP 内部の保全強化、
  MakerNote 内部オフセットの整合検査
- **`core/tests/metadata_spike.rs` は触らない。** `docs/tmp/review.md` は読まない（spec に反映済み）
- **`read_exif_info` は検証に使わない**（読めないと `ExifInfo::default()` を返し、前後とも空で一致して素通りする）
- **`MetadataRead` の形は変えない**（Rust ⇄ TS の DTO 契約・`dto.contract.ts` は変わらない）
- **否定形の検査（「通す」「警告が出ない」）には前提条件の assert を添える。** 各拒否テストでも、
  対象が書き込み前に実在することを先に assert する（memory: negative-assertion-tests-need-a-precondition）
- **テストを書く・直す前に `test-integrity` スキルを起動する**（グローバル指示）
- **この環境の `cargo test` は出力が要約され `--nocapture` の println が見えない。** 出力が要るときは
  テストバイナリを直接実行してスクラッチパッドのファイルへリダイレクトする（カレントは `core/`）
- **コミットは `commit-message` スキルに従う。** `<対応の概要>` は直近 commit から逐語継承して
  **`メタデータ書き込みの検証強化`**。footer は `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>`

## Review Focus

spec が含意するが、spec の「テスト」表の行だけでは押さえきれず、利用者が最初に踏みうるもの（多い順）。
各行のテストは右端のタスクに入れてある。

1. **カメラ撮って出しの MakerNote 付き JPEG への保存** — 再実測の 3 枚はどれも MakerNote を持たない。
   `little_exif` が MakerNote のバイト列を変えると全保存が拒否される。期待: 通って警告が 1 件出る
   → Task 5 `a_moved_maker_note_passes_with_a_warning`（実際の `build` を通す）／Task 7 `a_save_warning_reaches_the_returned_warnings`
2. **本ツール自身が書いた写真への 2 回目以降の保存**（自分の書いたタグの変更・削除・日本語タイトルへの切り替えで
   `ImageDescription` が消える）。期待: 通る → Task 2 `edited_tags_may_appear_change_and_disappear`
3. **Exif を持たない写真（本ツールの変換出力など）への初回保存。** 期待: 通る
   → Task 2 `a_head_without_exif_passes`（単体）／既存 `gui/src/commands.rs` の `list_reads_the_rating_from_the_file`（`image` クレートで作った Exif なし JPEG への実書き込み）
4. **拒否されたとき、元写真が 1 バイトも変わらず一時ファイルも残らない。** 期待: 無傷
   → Task 6 `replace_verified_refuses_a_buffer_that_lost_the_existing_exif`（`assert_untouched`）／Task 7 `a_refused_save_reports_the_inner_reason`
5. **サムネイルが後ろへずれただけの正常な保存**（IFD0 が伸びると必ず起きる）。期待: 通る
   → Task 3 `a_moved_thumbnail_with_the_same_bytes_passes`

---

## spec を実装者の目で一読した結果

設計の再検討はしていない。実装に落とすときに決める必要があったことと、試作で確かめたことだけを記す。
**計画の全コードは作成時に作業ツリーで試作し、`cargo clippy -D warnings` とワークスペース全テスト（326 件）の
通過を確認してから元に戻した**（再実測は Task 1 の仕事として残してある）。

| # | spec の記述 | 計画での扱い |
|---|---|---|
| A-1 | 読めない場合の表は「書き込み後の Exif を読めない」を拒否とするが、**書き込み後に Exif APP1 自体が無い**場合を書いていない | 書き込み後のフィールド集合を空とみなし、検査 1 の文面（失われたキーの列挙）で拒否する。文面の方が利用者に正確 |
| A-2 | `atomic.rs` の既存テストの buffer が検査 1／「検証できない」のどちらで拒否されるかは実装時に決める | **試作で実測: 空の `Metadata` で書き直した Exif は kamadak-exif の厳格読みで `Truncated IFD count` になる。** 「検証できない」経路。前提条件の assert でこの経路を固定する（Task 6） |
| A-3 | 書き込み後のバイト列は「フィクスチャのコピーから対象を消す・変えることで作る」。フィクスチャ追加の要否は計画で決める | **フィクスチャファイルは追加しない。** TIFF 本体をその場で書き換える（タグ番号の付け替え・値の 1 バイト変更・型コードの変更）。オフセットがずれないので、変えたもの以外は前後で同じになる。MakerNote は 2 枚とも持たないので、テスト内で `little_exif` で `exif-only.jpg` に足す |
| A-4 | GUI の境界テスト「内側の文面（例「既存の Exif が保存で失われるか変わるため」）が含まれる」 | 実書き込み経路で `preserve` の拒否を起こす入力は作れない（`little_exif` はテスト入力を壊さない）。**内側の文面には `build` の「既存の Exif を解析できない」を使う。** 修正の対象は文字列化（`{e:#}`）であって文面の種類に依らない |
| A-5 | `write_metadata` の戻り値を `Result<Vec<String>>` に変えると、約 25 か所の呼び出しに波及する（引き継ぎ時の見積もり） | **試作で確認: GUI 境界の 1 か所以外は変更不要。** 他はすべて `.unwrap()` / `.expect()` / `.unwrap_err()` / `?` で受けており、`Vec<String>` は `#[must_use]` でないので警告も出ない。意味の変更が要るのは `gui/src/commands.rs` の `write_image_metadata` だけ |
| A-6 | セグメント列の走査が壊れた入力を受けたときの扱いは書かれていない | `Err` で拒否する（文面は `atomic::scan_offset` と同じ調子）。`scan_offset` を通った入力なので実際には長さ 2 未満のセグメントでしか起きない |
| A-7 | GUI 境界テストで MakerNote 付き JPEG を作る手段 | `gui` の dev-dependency に `little_exif = "=0.6.23"` を足す（core と同じ版で依存木に既にあり、ビルドは増えない） |

---

## ファイル構成

### 新規

| ファイル | 責務 |
|---|---|
| `core/src/metadata/preserve.rs` | 書き込み前後の SOS より前を比べる。セグメント分割・kamadak-exif での Exif 読み・値の比較規則・検査 1〜3・MakerNote の警告。ファイルには触らない |

### 変更

| ファイル | 変更内容 |
|---|---|
| `core/src/metadata/mod.rs` | `mod preserve;`。`write_metadata` / `write_inner` の戻り値を `Result<Vec<String>>` に。`build` 内のコメント更新。警告が返ることのテスト |
| `core/src/metadata/atomic.rs` | `verify` の 3 番目の検査を `preserve::check` に置き換え、`verify` / `replace_verified` の戻り値を `Result<Vec<String>>` に。モジュールコメント更新。既存テスト 1 本の書き換え |
| `core/src/lib.rs` | crate ドキュメントの「呼び出し側が守ること」に `write_metadata` の警告を足す |
| `gui/src/commands.rs` | `write_image_metadata` の本体を `save_and_read_back` に出し、`{e:#}` 化と警告の合流。テスト 2 本 |
| `gui/Cargo.toml` | dev-dependency `little_exif = "=0.6.23"` |
| `docs/superpowers/specs/2026-08-18-metadata-editing-design.md` | 「この保証の実際の強度」の節を強化後の実態に書き換える |
| `docs/superpowers/specs/2026-09-29-metadata-write-verification-design.md` | 「実測」の比較方法の注記に、Task 1 の再実測の結果を追記する |
| `docs/README.md` | 本計画の行（計画作成時に追加済み）の内容を実施後に更新 |

---

## Task 1: 値の比較規則を実装し、3 枚を再実測する

spec の実測は `Value` の `Debug` 表現で比べたもので、spec の比較規則そのものでの確認ではない（レビュー Q3）。
**この再実測が以降の全タスクの前提**（「現行の `little_exif` はこの 3 枚で編集対象以外を変えない」＝誤拒否しない根拠）。
そのために、再実測に要る部品（セグメント分割・Exif の読み方・値の比較規則）をここで実装する。

**Files:**
- Create: `core/src/metadata/preserve.rs`
- Modify: `core/src/metadata/mod.rs:12-15`（`mod` 宣言）

**Interfaces:**
- Consumes: `super::atomic::scan_offset(&[u8]) -> Result<usize>`（テストの `head_of` のみ）、`super::build(&[u8], &EditableMetadata) -> Result<Vec<u8>>`（再実測のみ。`mod.rs` の private 関数だが子モジュールから見える）
- Produces（`preserve.rs` 内、以降のタスクが使う）:
  - `const APP0: u8 = 0xE0; const APP1: u8 = 0xE1; const EXIF_ID: &[u8] = b"Exif\0\0";`
  - `#[derive(Debug, PartialEq)] struct Segment<'a> { marker: u8, contents: &'a [u8] }`
  - `fn segments(head: &[u8]) -> Result<Vec<Segment<'_>>>`
  - `fn first_app1(segments: &[Segment], prefix: &[u8]) -> Option<usize>`
  - `fn read_exif(segments: &[Segment]) -> Result<Option<Exif>>`（読めなければ `Err("既存の Exif を検証できないため中止しました")`）
  - `fn same_value(a: &Value, b: &Value) -> bool`
  - テスト用: `fn fixture(name: &str) -> Vec<u8>`、`fn head_of(bytes: &[u8]) -> Vec<u8>`、`fn head_with(segments: &[(u8, &[u8])]) -> Vec<u8>`

- [ ] **Step 1: `test-integrity` スキルを起動する**

- [ ] **Step 2: モジュールを宣言する**

`core/src/metadata/mod.rs` の `mod header;` の次の行に足す。`allow` は Task 6 で `atomic::verify` から呼ぶまでの仮置き
（lint が `-D warnings` なので、未使用のままだと `make check` が落ちる）:

```rust
mod header;
// Task 6 で atomic::verify から呼ぶまでの仮置き。Task 6 で外す
#[allow(dead_code)]
mod preserve;
mod xmp;
```

- [ ] **Step 3: 失敗するテストを書く**

`core/src/metadata/preserve.rs` を作り、まずモジュールコメント・`use`・型と関数のシグネチャだけを `todo!()` で置き、テストを書く。
最終形（Step 5）と同じテストをここで書く:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // --- 入力の組み立て ------------------------------------------------------

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures")
                .join(name),
        )
        .unwrap()
    }

    /// SOS より前だけを切り出す（`check` への入力の形）
    fn head_of(bytes: &[u8]) -> Vec<u8> {
        bytes[..super::super::atomic::scan_offset(bytes).unwrap()].to_vec()
    }

    /// SOI ＋ 与えたセグメント列（SOS より前の形）
    fn head_with(segments: &[(u8, &[u8])]) -> Vec<u8> {
        let mut head = vec![0xFF, 0xD8];
        for (marker, contents) in segments {
            head.extend([0xFF, *marker]);
            head.extend(((contents.len() + 2) as u16).to_be_bytes());
            head.extend(*contents);
        }
        head
    }

    // --- セグメントの走査 ----------------------------------------------------

    #[test]
    fn segments_splits_a_head_into_markers_and_contents() {
        let head = head_with(&[(APP0, b"JFIF\0abc"), (0xDB, b"\x00\x01")]);
        assert_eq!(
            segments(&head).unwrap(),
            vec![
                Segment { marker: APP0, contents: b"JFIF\0abc" },
                Segment { marker: 0xDB, contents: b"\x00\x01" },
            ]
        );
    }

    /// 実データの SOS より前が、exiv2 で見た構造どおりに分かれる
    /// （APP0・Exif APP1・XMP APP1・SOF0・DQT×2・DHT×4）
    #[test]
    fn segments_reads_the_fixture_structure() {
        let head = head_of(&fixture("lightroom-xmp.jpg"));
        let markers: Vec<u8> = segments(&head).unwrap().iter().map(|s| s.marker).collect();
        assert_eq!(
            markers,
            vec![APP0, APP1, APP1, 0xC0, 0xDB, 0xDB, 0xC4, 0xC4, 0xC4, 0xC4]
        );
    }

    /// 壊れた入力で panic せず Err を返す
    #[test]
    fn segments_never_panics_on_malformed_input() {
        let cases: Vec<Vec<u8>> = vec![
            vec![0xFF, 0xD8, 0xFF],
            vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00],
            // 長さが末尾を飛び越える
            vec![0xFF, 0xD8, 0xFF, 0xE0, 0xFF, 0xFF, 0x11],
            // 長さが 2 未満
            vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x01, 0x11],
            // 境界が 0xFF で始まらない
            vec![0xFF, 0xD8, 0x00, 0x00, 0x00],
        ];
        for head in cases {
            assert!(segments(&head).is_err(), "壊れた入力を Ok にした: {:?}", head);
        }
    }

    // --- Exif の読み方 -------------------------------------------------------

    #[test]
    fn read_exif_reads_the_first_exif_app1_of_the_fixture() {
        let head = head_of(&fixture("exif-only.jpg"));
        let exif = read_exif(&segments(&head).unwrap())
            .unwrap()
            .expect("Exif が無い");
        assert!(exif.get_field(Tag::Make, In::PRIMARY).is_some());
    }

    #[test]
    fn read_exif_returns_none_without_an_exif_app1() {
        let head = head_with(&[(APP0, b"JFIF\0abc")]);
        assert!(read_exif(&segments(&head).unwrap()).unwrap().is_none());
    }

    /// Exif APP1 はあるが TIFF として読めない → 「検証できない」
    #[test]
    fn read_exif_refuses_an_exif_app1_that_kamadak_cannot_read() {
        let head = head_with(&[(APP1, b"Exif\0\0\xAA\xAA\xAA\xAA\xAA\xAA\xAA\xAA")]);
        // `Exif` は Debug を持たないので unwrap_err は使えない
        let Err(error) = read_exif(&segments(&head).unwrap()) else {
            panic!("読めない Exif を読めたことにした");
        };
        assert!(
            error.to_string().contains("既存の Exif を検証できない"),
            "{error:#}"
        );
    }

    // --- 値の比較規則 --------------------------------------------------------

    #[test]
    fn same_value_ignores_the_position_of_undefined_and_unknown() {
        assert!(same_value(
            &Value::Undefined(b"0232".to_vec(), 100),
            &Value::Undefined(b"0232".to_vec(), 250)
        ));
        assert!(same_value(&Value::Unknown(99, 4, 10), &Value::Unknown(99, 4, 30)));
    }

    #[test]
    fn same_value_detects_changed_bytes_and_changed_unknown_shape() {
        assert!(!same_value(
            &Value::Undefined(b"0232".to_vec(), 100),
            &Value::Undefined(b"0231".to_vec(), 100)
        ));
        assert!(!same_value(&Value::Unknown(99, 4, 10), &Value::Unknown(99, 5, 10)));
        assert!(!same_value(&Value::Unknown(99, 4, 10), &Value::Unknown(98, 4, 10)));
    }

    /// 同じ数値でも型が違えば「変わった」
    #[test]
    fn same_value_treats_a_type_change_as_a_change() {
        assert!(!same_value(&Value::Short(vec![2]), &Value::Long(vec![2])));
    }

    /// NaN は自分自身と一致させる（`==` で比べると誤拒否になる）。+0.0 と -0.0 は別物
    #[test]
    fn same_value_compares_floats_by_bits() {
        assert!(same_value(&Value::Float(vec![f32::NAN]), &Value::Float(vec![f32::NAN])));
        assert!(same_value(&Value::Double(vec![f64::NAN]), &Value::Double(vec![f64::NAN])));
        assert!(!same_value(&Value::Float(vec![0.0]), &Value::Float(vec![-0.0])));
    }

    /// 1/2 と 2/4 は別物（約分しない）
    #[test]
    fn same_value_does_not_reduce_rationals() {
        let r = |num, denom| exif::Rational { num, denom };
        assert!(same_value(&Value::Rational(vec![r(1, 2)]), &Value::Rational(vec![r(1, 2)])));
        assert!(!same_value(&Value::Rational(vec![r(1, 2)]), &Value::Rational(vec![r(2, 4)])));
        let s = |num, denom| exif::SRational { num, denom };
        assert!(!same_value(
            &Value::SRational(vec![s(-1, 2)]),
            &Value::SRational(vec![s(-2, 4)])
        ));
    }

    #[test]
    fn same_value_compares_element_sequences() {
        assert!(same_value(
            &Value::Ascii(vec![b"SONY".to_vec()]),
            &Value::Ascii(vec![b"SONY".to_vec()])
        ));
        assert!(!same_value(
            &Value::Ascii(vec![b"SONY".to_vec()]),
            &Value::Ascii(vec![b"Sony".to_vec()])
        ));
        assert!(!same_value(&Value::Short(vec![1, 2]), &Value::Short(vec![1])));
    }
}
```

- [ ] **Step 4: テストが失敗することを確かめる**

Run: `cargo test -p picture-tool-core --lib metadata::preserve`
Expected: FAIL（`todo!()` による panic。`segments_*`・`read_exif_*`・`same_value_*` の全 12 件）

- [ ] **Step 5: 実装する**

`core/src/metadata/preserve.rs` の `#[cfg(test)]` より上を次の内容にする:

```rust
//! 書き込みで**既存のメタデータが失われていないか**を確かめる
//! （spec `docs/superpowers/specs/2026-09-29-metadata-write-verification-design.md`）。
//!
//! 入力は書き込み前後の **SOS より前のバイト列**だけで、ファイルには触らない。
//! 画素データの一致は `atomic::replace_verified` の冒頭が受け持つ。
//!
//! | 検査 | 守るもの |
//! |---|---|
//! | 1 | Exif の全フィールド（編集対象 5 タグと、位置に依存するだけの値を除く） |
//! | 2 | IFD1 サムネイル本体のバイト列 |
//! | 3 | Exif・XMP 以外のセグメント（ICC・IPTC・拡張 XMP・JFIF・量子化表など）と相対順序 |
//!
//! **Exif は kamadak-exif の厳格読み（`continue_on_error` は既定の `false`）で読む。**
//! 書き込み側の `little_exif` とは独立した実装で見るので、「little_exif で書いて
//! little_exif で読めた」の同語反復にならない。読めなければ比べる基準が無いので拒否する。

use anyhow::{Result, anyhow, bail};
use exif::{Exif, Value};

const APP0: u8 = 0xE0;
const APP1: u8 = 0xE1;
const EXIF_ID: &[u8] = b"Exif\0\0";

/// SOS より前の 1 セグメント。`contents` は長さフィールドの後ろ（識別子を含む）
#[derive(Debug, PartialEq)]
struct Segment<'a> {
    marker: u8,
    contents: &'a [u8],
}

/// `head`（SOI から SOS の直前まで）をセグメントに分ける。
///
/// 走査の規則は `atomic::scan_offset` と同じ（0xFF の fill byte を読み飛ばす）。
/// 入力は webview 由来のパスから読んだバイト列なので、添字は長さで押さえてから触る。
fn segments(head: &[u8]) -> Result<Vec<Segment<'_>>> {
    let mut found = Vec::new();
    let mut cursor = 2; // SOI をとばす
    while cursor < head.len() {
        if head[cursor] != 0xFF {
            bail!("JPEG のセグメント境界が壊れています（offset {}）", cursor);
        }
        let mut marker_at = cursor + 1;
        while marker_at < head.len() && head[marker_at] == 0xFF {
            marker_at += 1;
        }
        if marker_at + 2 >= head.len() {
            bail!("JPEG のセグメントが途中で切れています（offset {}）", cursor);
        }
        let length = u16::from_be_bytes([head[marker_at + 1], head[marker_at + 2]]) as usize;
        let end = marker_at + 1 + length;
        if length < 2 || end > head.len() {
            bail!("JPEG のセグメント長が壊れています（offset {}）", cursor);
        }
        found.push(Segment {
            marker: head[marker_at],
            contents: &head[marker_at + 3..end],
        });
        cursor = end;
    }
    Ok(found)
}

/// `prefix` で始まる**最初の** APP1 の位置。
/// `has_exif_segment`・`xmp::extract`・kamadak-exif が見るのも最初の 1 つだけ
fn first_app1(segments: &[Segment], prefix: &[u8]) -> Option<usize> {
    segments
        .iter()
        .position(|s| s.marker == APP1 && s.contents.starts_with(prefix))
}

/// 最初の Exif APP1 を kamadak-exif の厳格読みで読む。Exif APP1 が無ければ `None`。
///
/// `read_from_container` は使わない ── 有無の判定が `has_exif_segment` と 2 系統に
/// 分かれ、食い違えば検査 1・2 が空振りで通るため（spec「読めない場合の扱い」）
fn read_exif(segments: &[Segment]) -> Result<Option<Exif>> {
    let Some(at) = first_app1(segments, EXIF_ID) else {
        return Ok(None);
    };
    let tiff = segments[at].contents[EXIF_ID.len()..].to_vec();
    exif::Reader::new()
        .read_raw(tiff)
        .map(Some)
        .map_err(|_| anyhow!("既存の Exif を検証できないため中止しました"))
}

/// kamadak-exif 0.6 の `Value` は `PartialEq` を持たないので、spec の規則で比べる。
///
/// - バリアントが違えば不一致（SHORT → LONG の書き戻しも「変わった」）
/// - `Undefined` の第 2 要素と `Unknown` の第 3 要素は **Exif 内の位置**なので無視する
/// - 浮動小数はビット列で比べる（`NaN` が自分自身と不一致になる誤拒否を避ける）
/// - 有理数は約分せず分子・分母の組で比べる（`Rational` も `PartialEq` を持たない）
fn same_value(a: &Value, b: &Value) -> bool {
    use Value::*;
    match (a, b) {
        (Byte(x), Byte(y)) => x == y,
        (Ascii(x), Ascii(y)) => x == y,
        (Short(x), Short(y)) => x == y,
        (Long(x), Long(y)) => x == y,
        (SByte(x), SByte(y)) => x == y,
        (SShort(x), SShort(y)) => x == y,
        (SLong(x), SLong(y)) => x == y,
        (Rational(x), Rational(y)) => {
            x.len() == y.len()
                && x.iter().zip(y).all(|(p, q)| (p.num, p.denom) == (q.num, q.denom))
        }
        (SRational(x), SRational(y)) => {
            x.len() == y.len()
                && x.iter().zip(y).all(|(p, q)| (p.num, p.denom) == (q.num, q.denom))
        }
        (Float(x), Float(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| p.to_bits() == q.to_bits())
        }
        (Double(x), Double(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| p.to_bits() == q.to_bits())
        }
        (Undefined(x, _), Undefined(y, _)) => x == y,
        // 未知の型コードは中身を返さないので、中身の変化は検知できない（spec の残る制約）
        (Unknown(tx, cx, _), Unknown(ty, cy, _)) => (tx, cx) == (ty, cy),
        _ => false,
    }
}
```

テストモジュールでは `Tag` と `In` を使うので、`mod tests` の先頭に `use exif::{In, Tag};` を足す
（本体側の `use` は Task 2 で `Tag`・`In` を使い始めるときに広げ、そのときテスト側の `use` は消す）。

- [ ] **Step 6: テストが通ることを確かめる**

Run: `cargo test -p picture-tool-core --lib metadata::preserve`
Expected: PASS（12 件）

- [ ] **Step 7: 再実測用の一時テストを足す（コミットしない）**

`mod tests` の末尾に足す。**このテストは Step 10 で消す**:

```rust
    /// Task 1 の再実測（**コミットしない**）。`PICTURE_TOOL_REMEASURE` に `:` 区切りでパスを渡す
    #[test]
    #[ignore]
    fn remeasure() {
        use crate::metadata::EditableMetadata;
        let values = EditableMetadata {
            title: Some("再実測".into()),
            comment: Some("再実測のコメント".into()),
            rating: Some(4),
        };
        let paths = std::env::var("PICTURE_TOOL_REMEASURE").expect("PICTURE_TOOL_REMEASURE が無い");
        for path in paths.split(':') {
            let original = std::fs::read(path).unwrap();
            let written = super::super::build(&original, &values).unwrap();
            let (before_head, after_head) = (head_of(&original), head_of(&written));
            let before_segments = segments(&before_head).unwrap();
            let after_segments = segments(&after_head).unwrap();
            let before = read_exif(&before_segments).unwrap().expect("書き込み前に Exif が無い");
            let after = read_exif(&after_segments).unwrap().expect("書き込み後に Exif が無い");
            println!("== {path}");
            println!(
                "フィールド数: 前 {} / 後 {}",
                before.fields().count(),
                after.fields().count()
            );
            // 除外キーを当てずに、比較規則だけで「書き込み前 ⊆ 書き込み後」を見る
            for field in before.fields() {
                let kept = after.fields().any(|g| {
                    g.ifd_num == field.ifd_num
                        && g.tag == field.tag
                        && same_value(&g.value, &field.value)
                });
                if !kept {
                    let now: Vec<String> = after
                        .fields()
                        .filter(|g| g.ifd_num == field.ifd_num && g.tag == field.tag)
                        .map(|g| format!("{:?}", g.value))
                        .collect();
                    println!(
                        "消えた/変わった: {}/{} 前 {:?} 後 {:?}",
                        field.ifd_num, field.tag, field.value, now
                    );
                }
            }
            for field in after.fields() {
                if !before
                    .fields()
                    .any(|f| f.ifd_num == field.ifd_num && f.tag == field.tag)
                {
                    println!("増えた: {}/{}", field.ifd_num, field.tag);
                }
            }
            let markers = |list: &[Segment]| {
                list.iter()
                    .map(|s| format!("{:02X}:{}", s.marker, s.contents.len()))
                    .collect::<Vec<_>>()
                    .join(" ")
            };
            println!("セグメント 前: {}", markers(&before_segments));
            println!("セグメント 後: {}", markers(&after_segments));
        }
    }
```

- [ ] **Step 8: 3 枚を再実測する**

`cargo test` の出力は要約されて println が見えないので、テストバイナリを直接実行する（カレントは `core/`）:

```bash
cargo test -p picture-tool-core --lib --no-run
BIN=$(ls -t target/debug/deps/picture_tool_core-* | grep -v '\.d$' | head -1)
SCRATCH=<スクラッチパッドのディレクトリ>
(cd core && PICTURE_TOOL_REMEASURE="$PWD/tests/fixtures/exif-only.jpg:$PWD/tests/fixtures/lightroom-xmp.jpg:$PWD/../docs/tmp/20260328-DSC05024.jpg" \
  "../$BIN" metadata::preserve::tests::remeasure --ignored --nocapture > "$SCRATCH/remeasure.txt" 2>&1)
cat "$SCRATCH/remeasure.txt"
```

Expected（spec「実測」の表を比較規則で読み直したもの。3 枚とも同じ形）:
- `消えた/変わった:` は **`1/JPEGInterchangeFormat` の 1 行だけ**（値が後ろへずれる）
- `ExifVersion`・`FileSource`・`SceneType` は**出ない**（`Undefined` の位置を無視する規則が効いている）
- `増えた:` は `0/Tag(Tiff, 18249)`（RatingPercent）・`0/Tag(Tiff, 40091)`（XPTitle）・`0/Tag(Tiff, 40092)`（XPComment）・`0/UserComment` の 4 行
  （タイトルが日本語なので `ImageDescription` は書かれない）
- セグメントは Exif（`E1`）・XMP（`E1`）以外の並びと長さが前後で同じ（実写真は `ED`・`E2`・`DB`・`DD`・`EE` 等を含む）

- [ ] **Step 9: 結果で分岐する（ゲート）**

- **Expected どおり** → Step 10 へ。
- **それ以外が 1 行でも出た**（例: 型の変わったフィールド、消えたフィールド、他セグメントの長さの変化）→
  **ここで止めて利用者に報告する。** 比較規則や除外キーを自分の判断で広げない（spec の設計判断に戻る話）。
  出力ファイルをそのまま添える。

- [ ] **Step 10: 再実測の結果を記録し、一時テストを消す**

1. `remeasure` テストを削除する（`git diff` で `preserve.rs` に残っていないことを確かめる）
2. 本計画末尾の「実施メモ」の「Task 1: 再実測」に、実行日・3 枚それぞれの `消えた/変わった:` と `増えた:` の行・セグメント列を貼る
3. spec の「実測」の節の注記（「比較方法の注記（レビュー Q3）」の引用ブロック）の末尾に 1 文足す:
   `> **再実測（2026-MM-DD、実装計画 Task 1）**: 比較規則で読み直しても結果は同じだった（`JPEGInterchangeFormat` だけが変わり、`Undefined` の 3 項目は一致）。詳細は計画の実施メモ。`
   （日付は実行日。結果が違えば Step 9 で止まっているので、この文面にはならない）

- [ ] **Step 11: `make check` を通してコミットする**

```bash
cargo fmt --all
make check
git add core/src/metadata/preserve.rs core/src/metadata/mod.rs docs/superpowers/specs/2026-09-29-metadata-write-verification-design.md docs/superpowers/plans/2026-09-30-metadata-write-verification.md
git commit   # commit-message スキル: feat(core): メタデータ書き込みの検証強化: 値の比較規則を実装し 3 枚を再実測する
```

---

## Task 2: 検査 1（Exif 全フィールドの保全）と `check` の骨格

**Files:**
- Modify: `core/src/metadata/preserve.rs`

**Interfaces:**
- Consumes: Task 1 の `segments` / `first_app1` / `read_exif` / `same_value` / `EXIF_ID` / `APP0` / `APP1`、テスト用 `fixture` / `head_of` / `head_with`、`super::build`
- Produces:
  - `pub(super) fn check(original_head: &[u8], written_head: &[u8]) -> Result<Vec<String>>`（この時点では常に `Ok(Vec::new())` か `Err`）
  - `fn excluded(field: &Field) -> bool`、`fn lost_fields(before: &Exif, after: Option<&Exif>) -> Vec<(In, Tag)>`、
    `fn key_label(key: (In, Tag)) -> String`、`fn lost_message(lost: &[(In, Tag)]) -> String`
  - テスト用: `u16_le` / `u32_le` / `ifd0` / `ifd1` / `entry` / `rename_tag` / `edit_tiff` / `exif_of` / `refusal` / `built` / `all_three` / `edited_value` / `EDITED_TAGS`

- [ ] **Step 1: `test-integrity` スキルを起動する**

- [ ] **Step 2: 失敗するテストを書く**

本体側の `use` を `use exif::{Context, Exif, Field, In, Tag, Value};` に広げ、Task 1 でテスト側に足した `use exif::{In, Tag};` を消す。
`check` / `excluded` / `lost_fields` / `key_label` / `lost_message` を `todo!()` で置き、`mod tests` の末尾に足す:

```rust
    // --- TIFF 本体の書き換え（検査 1・2 の入力づくり） -----------------------
    //
    // 書き込み後のバイト列は、フィクスチャのコピーの TIFF 本体をその場で書き換えて作る
    // （spec「テスト」）。オフセットをずらさないので、変えたもの以外は前後で同じになる。
    // フィクスチャは 2 枚ともリトルエンディアン（`II`）。

    fn u16_le(tiff: &[u8], at: usize) -> u16 {
        u16::from_le_bytes([tiff[at], tiff[at + 1]])
    }

    fn u32_le(tiff: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(tiff[at..at + 4].try_into().unwrap())
    }

    fn ifd0(tiff: &[u8]) -> usize {
        assert_eq!(&tiff[..2], b"II", "前提条件: リトルエンディアンの TIFF");
        u32_le(tiff, 4) as usize
    }

    /// IFD0 の直後の「次の IFD」が IFD1（サムネイル側）
    fn ifd1(tiff: &[u8]) -> usize {
        let ifd0 = ifd0(tiff);
        let count = u16_le(tiff, ifd0) as usize;
        u32_le(tiff, ifd0 + 2 + 12 * count) as usize
    }

    /// IFD 内で `tag` を持つエントリの位置（TIFF 先頭基準）
    fn entry(tiff: &[u8], ifd: usize, tag: u16) -> usize {
        let count = u16_le(tiff, ifd) as usize;
        (0..count)
            .map(|i| ifd + 2 + 12 * i)
            .find(|&at| u16_le(tiff, at) == tag)
            .unwrap_or_else(|| panic!("前提条件: タグ 0x{tag:04x} が IFD（{ifd}）に無い"))
    }

    /// エントリのタグ番号を、どのタグとも衝突しない番号へ付け替える。
    /// kamadak-exif から見ると「元のタグが消え、未知のタグが増えた」になる
    fn rename_tag(tiff: &mut [u8], ifd: usize, tag: u16, to: u16) {
        let at = entry(tiff, ifd, tag);
        tiff[at..at + 2].copy_from_slice(&to.to_le_bytes());
    }

    /// head の最初の Exif APP1 の TIFF 本体だけを `edit` で書き換えた head を返す
    fn edit_tiff(head: &[u8], edit: impl FnOnce(&mut Vec<u8>)) -> Vec<u8> {
        let segments = segments(head).unwrap();
        let exif_at = first_app1(&segments, EXIF_ID).expect("前提条件: Exif APP1 が無い");
        let mut tiff = segments[exif_at].contents[EXIF_ID.len()..].to_vec();
        edit(&mut tiff);
        let exif_contents = [EXIF_ID, &tiff[..]].concat();
        let rebuilt: Vec<(u8, &[u8])> = segments
            .iter()
            .enumerate()
            .map(|(at, s)| {
                if at == exif_at {
                    (s.marker, &exif_contents[..])
                } else {
                    (s.marker, s.contents)
                }
            })
            .collect();
        head_with(&rebuilt)
    }

    fn exif_of(head: &[u8]) -> Exif {
        read_exif(&segments(head).unwrap())
            .unwrap()
            .expect("Exif が無い")
    }

    fn refusal(before: &[u8], after: &[u8]) -> String {
        match check(before, after) {
            Ok(warnings) => panic!("拒否されなかった（警告 {warnings:?}）"),
            Err(error) => error.to_string(),
        }
    }

    // --- 検査 1: Exif 全フィールドの保全 --------------------------------------

    /// 同じ head を比べれば通る。以降の拒否テストが「何でも拒否する」実装で
    /// 緑にならないための対照
    #[test]
    fn identical_heads_pass_without_warnings() {
        for name in ["exif-only.jpg", "lightroom-xmp.jpg"] {
            let head = head_of(&fixture(name));
            assert_eq!(check(&head, &head).unwrap(), Vec::<String>::new(), "{name}");
        }
    }

    #[test]
    fn a_lost_tag_is_refused_by_name() {
        let before = head_of(&fixture("exif-only.jpg"));
        assert!(
            exif_of(&before).get_field(Tag::Make, In::PRIMARY).is_some(),
            "前提条件: Make がある"
        );
        let after = edit_tiff(&before, |tiff| {
            let ifd0 = ifd0(tiff);
            rename_tag(tiff, ifd0, 0x010f, 0xfff0);
        });
        assert!(exif_of(&after).get_field(Tag::Make, In::PRIMARY).is_none());

        assert_eq!(
            refusal(&before, &after),
            "既存の Exif が保存で失われるか変わるため中止しました: Make"
        );
    }

    #[test]
    fn a_changed_value_is_refused_by_name() {
        let before = head_of(&fixture("exif-only.jpg"));
        let after = edit_tiff(&before, |tiff| {
            // Make は 5 バイト（"SONY\0"）なので値は外に置かれ、エントリはその位置を持つ
            let at = u32_le(tiff, entry(tiff, ifd0(tiff), 0x010f) + 8) as usize;
            assert_eq!(&tiff[at..at + 4], b"SONY", "前提条件: Make の値の位置");
            tiff[at] = b'X';
        });

        assert_eq!(
            refusal(&before, &after),
            "既存の Exif が保存で失われるか変わるため中止しました: Make"
        );
    }

    /// 同じ数値 2 のまま SHORT → LONG。リトルエンディアンの埋め込み値は
    /// どちらも `02 00 00 00` なので、型コードだけが変わる
    #[test]
    fn a_type_change_with_the_same_number_is_refused() {
        let before = head_of(&fixture("exif-only.jpg"));
        let after = edit_tiff(&before, |tiff| {
            let at = entry(tiff, ifd0(tiff), 0x0128) + 2;
            assert_eq!(u16_le(tiff, at), 3, "前提条件: ResolutionUnit は SHORT");
            tiff[at..at + 2].copy_from_slice(&4u16.to_le_bytes());
        });
        let unit = |head: &[u8]| {
            exif_of(head)
                .get_field(Tag::ResolutionUnit, In::PRIMARY)
                .unwrap()
                .value
                .clone()
        };
        assert!(matches!(unit(&before), Value::Short(ref v) if v == &[2]));
        assert!(matches!(unit(&after), Value::Long(ref v) if v == &[2]));

        assert_eq!(
            refusal(&before, &after),
            "既存の Exif が保存で失われるか変わるため中止しました: ResolutionUnit"
        );
    }

    /// 3 件まで名前を並べ、残りは件数だけ。並びは (IFD, タグ番号) 順
    #[test]
    fn more_than_three_lost_tags_are_summarized() {
        let before = head_of(&fixture("exif-only.jpg"));
        let after = edit_tiff(&before, |tiff| {
            let ifd0 = ifd0(tiff);
            // Copyright(0x8298)・Artist(0x013b)・Software(0x0131)・Model(0x0110)・Make(0x010f)
            // （並べる順を番号順と逆にして、文面の並びが実装の整列によることを確かめる）
            for (tag, to) in [
                (0x8298, 0xfff4),
                (0x013b, 0xfff3),
                (0x0131, 0xfff2),
                (0x0110, 0xfff1),
                (0x010f, 0xfff0),
            ] {
                rename_tag(tiff, ifd0, tag, to);
            }
        });

        assert_eq!(
            refusal(&before, &after),
            "既存の Exif が保存で失われるか変わるため中止しました: Make, Model, Software ほか 2 件"
        );
    }

    /// kamadak-exif が名前を知らないタグは 16 進、IFD1 のタグには「（サムネイル）」
    #[test]
    fn keys_are_labeled_by_name_hex_and_thumbnail_suffix() {
        assert_eq!(key_label((In::PRIMARY, Tag::Make)), "Make");
        assert_eq!(key_label((In::PRIMARY, Tag(Context::Tiff, 0x9c9b))), "0x9c9b");
        assert_eq!(
            key_label((In::THUMBNAIL, Tag::XResolution)),
            "XResolution（サムネイル）"
        );
    }

    /// Exif APP1 ごと消えれば、書き込み前の全フィールドが失われたことになる（計画 A-1）
    #[test]
    fn losing_the_whole_exif_app1_is_refused() {
        let before = head_of(&fixture("exif-only.jpg"));
        let without_exif: Vec<(u8, &[u8])> = segments(&before)
            .unwrap()
            .iter()
            .filter(|s| !(s.marker == APP1 && s.contents.starts_with(EXIF_ID)))
            .map(|s| (s.marker, s.contents))
            .collect();
        let after = head_with(&without_exif);

        assert!(
            refusal(&before, &after)
                .starts_with("既存の Exif が保存で失われるか変わるため中止しました: Make, Model"),
        );
    }

    /// 1 エントリだけ値の位置が範囲外 → 厳格読みでは Err → 拒否
    #[test]
    fn an_exif_with_one_broken_entry_is_refused_as_unverifiable() {
        let head = edit_tiff(&head_of(&fixture("exif-only.jpg")), |tiff| {
            let at = entry(tiff, ifd0(tiff), 0x010f) + 8;
            tiff[at..at + 4].copy_from_slice(&0xFFFF_FF00u32.to_le_bytes());
        });
        // 前提条件: 壊れているのは 1 エントリだけ（部分読みなら残りは読める）
        let list = segments(&head).unwrap();
        let tiff = list[first_app1(&list, EXIF_ID).unwrap()].contents[EXIF_ID.len()..].to_vec();
        match exif::Reader::new().continue_on_error(true).read_raw(tiff) {
            Err(exif::Error::PartialResult(partial)) => {
                let (exif, errors) = partial.into_inner();
                assert_eq!(errors.len(), 1, "前提条件: 壊れたのは 1 エントリ");
                assert!(exif.get_field(Tag::Model, In::PRIMARY).is_some());
            }
            _ => panic!("前提条件: 部分結果にならなかった"),
        }

        assert_eq!(
            refusal(&head, &head),
            "既存の Exif を検証できないため中止しました"
        );
    }

    #[test]
    fn an_unreadable_written_exif_is_refused_as_unverifiable() {
        let before = head_of(&fixture("exif-only.jpg"));
        let after = edit_tiff(&before, |tiff| tiff[..8].copy_from_slice(&[0xAA; 8]));
        assert_eq!(
            refusal(&before, &after),
            "既存の Exif を検証できないため中止しました"
        );
    }

    #[test]
    fn a_head_without_exif_passes() {
        let head = head_with(&[(APP0, b"JFIF\0abc")]);
        assert_eq!(check(&head, &head).unwrap(), Vec::<String>::new());
    }

    // --- 実際の書き込み（build）を通した入力 ----------------------------------

    use crate::metadata::EditableMetadata;

    fn built(original: &[u8], values: &EditableMetadata) -> Vec<u8> {
        super::super::build(original, values).unwrap()
    }

    /// タイトルは ASCII にする。`exif::apply` は ASCII のタイトルだけを
    /// ImageDescription にも書くので、日本語だと 5 タグのうち 1 つが現れない
    fn all_three() -> EditableMetadata {
        EditableMetadata {
            title: Some("Harbor at dusk".into()),
            comment: Some("順光".into()),
            rating: Some(4),
        }
    }

    fn edited_value(head: &[u8], tag: Tag) -> Option<Value> {
        exif_of(head)
            .get_field(tag, In::PRIMARY)
            .map(|f| f.value.clone())
    }

    /// 編集対象の 5 タグ。除外リスト（本体の `excluded`）とは**独立に**書く ──
    /// 同じ定数を参照すると、context の取り違えが両側で一致して見逃される
    const EDITED_TAGS: [Tag; 5] = [
        Tag::ImageDescription,
        Tag(Context::Tiff, 0x9c9b),
        Tag(Context::Tiff, 0x9c9c),
        Tag(Context::Tiff, 0x4749),
        Tag::UserComment,
    ];

    /// 編集対象の 5 タグが増えても、変わっても、消えても通す（Review Focus 2）
    #[test]
    fn edited_tags_may_appear_change_and_disappear() {
        let plain = fixture("exif-only.jpg");
        let first = built(&plain, &all_three());
        let second = built(
            &first,
            &EditableMetadata {
                title: Some("Another title".into()),
                comment: Some("別のコメント".into()),
                rating: Some(2),
            },
        );
        let cleared = built(&first, &EditableMetadata::default());
        // 日本語のタイトルに変えると ImageDescription だけが消える
        let japanese = built(
            &first,
            &EditableMetadata {
                title: Some("夕暮れの港".into()),
                ..all_three()
            },
        );
        for tag in EDITED_TAGS {
            // 前提条件: 5 タグとも 1 回目の書き込みで現れ、2 回目で値が変わり、空欄で消える
            // （＝除外が効いていなければ拒否される入力になっている）
            let a = edited_value(&head_of(&first), tag)
                .unwrap_or_else(|| panic!("前提条件: {tag:?} が書かれていない"));
            let b = edited_value(&head_of(&second), tag).unwrap();
            assert!(!same_value(&a, &b), "前提条件: {tag:?} の値が変わっていない");
            assert!(
                edited_value(&head_of(&cleared), tag).is_none(),
                "前提条件: {tag:?} が消えていない"
            );
        }
        assert!(
            edited_value(&head_of(&japanese), Tag::ImageDescription).is_none(),
            "前提条件: 日本語タイトルで ImageDescription が消えていない"
        );

        for (before, after) in [
            (&plain, &first),
            (&first, &second),
            (&first, &cleared),
            (&first, &japanese),
        ] {
            assert_eq!(
                check(&head_of(before), &head_of(after)).unwrap(),
                Vec::<String>::new()
            );
        }
    }
```

- [ ] **Step 3: テストが失敗することを確かめる**

Run: `cargo test -p picture-tool-core --lib metadata::preserve`
Expected: FAIL（新しい 11 件が `todo!()` で panic。Task 1 の 12 件は PASS のまま）

- [ ] **Step 4: 実装する**

`same_value` の後ろ（`#[cfg(test)]` の前）に足す:

```rust
/// 比較から外すフィールドか（spec 検査 1「比較から除外するキー」）。
///
/// 編集対象の 5 タグ（値の正しさは `atomic::verify` の読み戻し検査が受け持つ）と、
/// サムネイルへのオフセット（サムネイル本体は検査 2 が受け持つ）。
/// **context を取り違えると除外が効かず、全保存が拒否される。**
/// kamadak-exif には XPTitle 等の名前付き定数が無いので番号で組み立てる
fn excluded(field: &Field) -> bool {
    const EDITED: [Tag; 5] = [
        Tag::ImageDescription,
        Tag(Context::Tiff, 0x9c9b), // XPTitle
        Tag(Context::Tiff, 0x9c9c), // XPComment
        Tag(Context::Tiff, 0x4749), // RatingPercent
        Tag::UserComment,
    ];
    match field.ifd_num {
        In::PRIMARY => EDITED.contains(&field.tag),
        In::THUMBNAIL => field.tag == Tag::JPEGInterchangeFormat,
        _ => false,
    }
}

/// 書き込み前にあったのに、書き込み後に**同じ値で**残っていないフィールドのキー。
///
/// 多重集合の包含「書き込み前 ⊆ 書き込み後」を見る。書き込み後にだけあるものは許す
/// （編集対象を初めて書けば増える）。書き込み後に Exif が無ければ空集合とみなす。
/// 返すキーは `(IFD 番号, タグ番号)` 順で重複なし
fn lost_fields(before: &Exif, after: Option<&Exif>) -> Vec<(In, Tag)> {
    let mut remaining: Vec<&Field> = after
        .map(|exif| exif.fields().filter(|f| !excluded(f)).collect())
        .unwrap_or_default();
    let mut lost = Vec::new();
    for field in before.fields().filter(|f| !excluded(f)) {
        let kept = remaining.iter().position(|candidate| {
            candidate.ifd_num == field.ifd_num
                && candidate.tag == field.tag
                && same_value(&candidate.value, &field.value)
        });
        match kept {
            Some(at) => {
                remaining.swap_remove(at);
            }
            None => lost.push((field.ifd_num, field.tag)),
        }
    }
    lost.sort_by_key(|(ifd, tag)| (ifd.index(), tag.number(), tag.context()));
    lost.dedup();
    lost
}

/// 利用者に見せるキーの表記。kamadak-exif の `Tag` の `Display` は名前を知らないタグで
/// `Tag(Tiff, 18249)` に落ちるので、そのときは 16 進の番号にする
fn key_label((ifd, tag): (In, Tag)) -> String {
    let name = if tag.description().is_some() {
        tag.to_string()
    } else {
        format!("0x{:04x}", tag.number())
    };
    if ifd == In::THUMBNAIL {
        format!("{name}（サムネイル）")
    } else {
        name
    }
}

/// 検査 1 の拒否の文面。最大 3 件を並べ、残りは件数だけ
fn lost_message(lost: &[(In, Tag)]) -> String {
    let shown: Vec<String> = lost.iter().take(3).map(|key| key_label(*key)).collect();
    let mut message = format!(
        "既存の Exif が保存で失われるか変わるため中止しました: {}",
        shown.join(", ")
    );
    if lost.len() > 3 {
        message.push_str(&format!(" ほか {} 件", lost.len() - 3));
    }
    message
}

/// 書き込み前後の SOS より前を比べ、既存のメタデータが失われていれば `Err`。
///
/// `Ok` の中身は保存を止めない警告。
/// 拒否の文面は `write_metadata` の文脈「メタデータを保存できませんでした…」の
/// **後ろに `: ` で連結されて**利用者に届くので、単独で意味が通るように書く
pub(super) fn check(original_head: &[u8], written_head: &[u8]) -> Result<Vec<String>> {
    let before_segments = segments(original_head)?;
    let after_segments = segments(written_head)?;
    let warnings = Vec::new();

    if let Some(before) = read_exif(&before_segments)? {
        let after = read_exif(&after_segments)?;

        // 検査 1: Exif 全フィールドの保全
        let lost = lost_fields(&before, after.as_ref());
        if !lost.is_empty() {
            bail!(lost_message(&lost));
        }
    }

    Ok(warnings)
}
```

- [ ] **Step 5: テストが通ることを確かめる**

Run: `cargo test -p picture-tool-core --lib metadata::preserve`
Expected: PASS（23 件）

- [ ] **Step 6: `make check` を通してコミットする**

```bash
cargo fmt --all
make check
git add core/src/metadata/preserve.rs
git commit   # feat(core): メタデータ書き込みの検証強化: Exif 全フィールドの保全を検査する
```

---

## Task 3: 検査 2（IFD1 サムネイル本体のバイト列一致）

**Files:**
- Modify: `core/src/metadata/preserve.rs`

**Interfaces:**
- Consumes: Task 2 の `check` / `read_exif`、テスト用 `edit_tiff` / `entry` / `ifd1` / `rename_tag` / `exif_of` / `refusal`
- Produces: `fn thumbnail(exif: &Exif) -> Option<&[u8]>`

- [ ] **Step 1: `test-integrity` スキルを起動する**

- [ ] **Step 2: 失敗するテストを書く**

`thumbnail` を `todo!()` で置き、`mod tests` の末尾に足す:

```rust
    // --- 検査 2: サムネイル ---------------------------------------------------

    #[test]
    fn a_changed_thumbnail_is_refused() {
        let before = head_of(&fixture("exif-only.jpg"));
        let original = exif_of(&before);
        let thumb = thumbnail(&original)
            .expect("前提条件: サムネイルがある")
            .to_vec();
        let offset = original
            .get_field(Tag::JPEGInterchangeFormat, In::THUMBNAIL)
            .unwrap()
            .value
            .get_uint(0)
            .unwrap() as usize;
        let after = edit_tiff(&before, |tiff| {
            assert_eq!(&tiff[offset..offset + thumb.len()], &thumb[..]);
            tiff[offset + thumb.len() / 2] ^= 0xFF;
        });

        assert_eq!(
            refusal(&before, &after),
            "Exif のサムネイルが保存で変わるため中止しました"
        );
    }

    /// `JPEGInterchangeFormat` は検査 1 の除外キーなので、ここを消しても検査 1 は通る。
    /// 検査 2 が捕まえる
    #[test]
    fn a_lost_thumbnail_pointer_is_refused() {
        let before = head_of(&fixture("exif-only.jpg"));
        assert!(thumbnail(&exif_of(&before)).is_some(), "前提条件: サムネイルがある");
        let after = edit_tiff(&before, |tiff| {
            let ifd1 = ifd1(tiff);
            rename_tag(tiff, ifd1, 0x0201, 0xfff0);
        });
        assert!(
            thumbnail(&exif_of(&after)).is_none(),
            "前提条件: サムネイルが引けない"
        );

        assert_eq!(
            refusal(&before, &after),
            "Exif のサムネイルが保存で変わるため中止しました"
        );
    }

    /// サムネイルが後ろへ動いても、本体が同じなら通す（IFD0 が伸びたときの正常な変化。
    /// Review Focus 5）
    #[test]
    fn a_moved_thumbnail_with_the_same_bytes_passes() {
        let before = head_of(&fixture("exif-only.jpg"));
        let thumb = thumbnail(&exif_of(&before)).unwrap().to_vec();
        let after = edit_tiff(&before, |tiff| {
            let moved_to = tiff.len() as u32;
            tiff.extend(&thumb);
            let at = entry(tiff, ifd1(tiff), 0x0201) + 8;
            tiff[at..at + 4].copy_from_slice(&moved_to.to_le_bytes());
        });
        let pointer = |head: &[u8]| {
            exif_of(head)
                .get_field(Tag::JPEGInterchangeFormat, In::THUMBNAIL)
                .unwrap()
                .value
                .get_uint(0)
        };
        assert_ne!(pointer(&before), pointer(&after), "前提条件: オフセットが変わっている");

        assert_eq!(check(&before, &after).unwrap(), Vec::<String>::new());
    }

    /// 範囲外・溢れるサムネイルで panic しない。元写真のサムネイルが壊れていれば比べない
    #[test]
    fn a_broken_thumbnail_range_neither_panics_nor_is_compared() {
        let head = head_of(&fixture("exif-only.jpg"));
        for (offset, length) in [(u32::MAX, 16), (16, u32::MAX), (0xFFFF_0000, 0x1_0000)] {
            let broken = edit_tiff(&head, |tiff| {
                let ifd1 = ifd1(tiff);
                let at = entry(tiff, ifd1, 0x0201) + 8;
                tiff[at..at + 4].copy_from_slice(&offset.to_le_bytes());
                let at = entry(tiff, ifd1, 0x0202) + 8;
                tiff[at..at + 4].copy_from_slice(&length.to_le_bytes());
            });
            assert!(
                thumbnail(&exif_of(&broken)).is_none(),
                "{offset:#x}+{length:#x}"
            );
            assert_eq!(check(&broken, &broken).unwrap(), Vec::<String>::new());
        }
    }
```

- [ ] **Step 3: テストが失敗することを確かめる**

Run: `cargo test -p picture-tool-core --lib metadata::preserve`
Expected: FAIL（新しい 4 件が `todo!()` で panic）

- [ ] **Step 4: 実装する**

`lost_message` の後ろに足す:

```rust
/// IFD1 サムネイル本体。オフセットは TIFF 先頭（`Exif\0\0` の直後）基準で、
/// 検査 1 で読んだのと同じ Exif APP1 の中身から切り出す。
///
/// `JPEGInterchangeFormat` と `JPEGInterchangeFormatLength` の**両方**があり、
/// 範囲が溢れず TIFF 本体に収まるときだけ `Some`。範囲外で panic しない
/// （panic は `catch_unwind` で捕まるが、文面が「異常終了しました」に化ける）
fn thumbnail(exif: &Exif) -> Option<&[u8]> {
    let uint = |tag| exif.get_field(tag, In::THUMBNAIL)?.value.get_uint(0);
    let offset = uint(Tag::JPEGInterchangeFormat)?;
    let length = uint(Tag::JPEGInterchangeFormatLength)?;
    let end = offset.checked_add(length)?;
    exif.buf().get(offset as usize..end as usize)
}
```

`check` の検査 1 の `if` の後ろ（`read_exif` の `if let` ブロックの中）に足す:

```rust
        // 書き込み前に 1 つでも比べるフィールドがあれば、検査 1 を通った時点で after は
        // 必ず Some。フィールドが 1 つも無い Exif から Exif が消えた場合だけ None で残る
        if let Some(after) = &after {
            // 検査 2: サムネイル本体（元写真のサムネイルが壊れているか無ければ比べない）
            if let Some(original_thumbnail) = thumbnail(&before) {
                if thumbnail(after) != Some(original_thumbnail) {
                    bail!("Exif のサムネイルが保存で変わるため中止しました");
                }
            }
        }
```

- [ ] **Step 5: テストが通ることを確かめる**

Run: `cargo test -p picture-tool-core --lib metadata::preserve`
Expected: PASS（27 件）

- [ ] **Step 6: `make check` を通してコミットする**

```bash
cargo fmt --all
make check
git add core/src/metadata/preserve.rs
git commit   # feat(core): メタデータ書き込みの検証強化: サムネイル本体の一致を検査する
```

---

## Task 4: 検査 3（Exif・XMP 以外のセグメントの一致）

**Files:**
- Modify: `core/src/metadata/preserve.rs`

**Interfaces:**
- Consumes: Task 1 の `segments` / `first_app1` / `EXIF_ID`、`super::xmp::XMP_ID`（`pub(super) const XMP_ID: &[u8] = b"http://ns.adobe.com/xap/1.0/\0"`）、テスト用 `head_of` / `head_with` / `refusal`
- Produces: `const APP2: u8 = 0xE2; const APP13: u8 = 0xED;`、`fn others<'a>(segments: &'a [Segment<'a>]) -> Vec<&'a Segment<'a>>`、`fn segment_name(segment: &Segment) -> String`

- [ ] **Step 1: `test-integrity` スキルを起動する**

- [ ] **Step 2: 失敗するテストを書く**

`APP2` / `APP13` の定数を足し、`others` / `segment_name` を `todo!()` で置き、`mod tests` の末尾に足す:

```rust
    // --- 検査 3: Exif・XMP 以外のセグメント -----------------------------------

    const ICC: &[u8] = b"ICC_PROFILE\0\x01\x01profile-bytes";

    /// フィクスチャの APP0 の直後に `extra` を差し込んだ head
    /// （フィクスチャは 2 枚とも ICC を持たないので、テスト内で足す。spec「テスト」）
    fn with_segment_after_app0(name: &str, extra: (u8, &[u8])) -> Vec<u8> {
        let head = head_of(&fixture(name));
        let mut list: Vec<(u8, &[u8])> = segments(&head)
            .unwrap()
            .iter()
            .map(|s| (s.marker, s.contents))
            .collect();
        assert_eq!(list[0].0, APP0, "前提条件: 先頭は APP0");
        list.insert(1, extra);
        head_with(&list)
    }

    #[test]
    fn a_lost_icc_profile_is_refused_by_name() {
        let before = with_segment_after_app0("exif-only.jpg", (APP2, ICC));
        let after = head_of(&fixture("exif-only.jpg"));
        assert!(
            segments(&before).unwrap().iter().any(|s| s.marker == APP2),
            "前提条件: 書き込み前に ICC がある"
        );

        assert_eq!(
            refusal(&before, &after),
            "Exif・XMP 以外のメタデータ（ICC プロファイル）が保存で変わるため中止しました"
        );
    }

    #[test]
    fn a_changed_icc_profile_is_refused_by_name() {
        let before = with_segment_after_app0("exif-only.jpg", (APP2, ICC));
        let after = with_segment_after_app0(
            "exif-only.jpg",
            (APP2, b"ICC_PROFILE\0\x01\x01PROFILE-bytes"),
        );

        assert_eq!(
            refusal(&before, &after),
            "Exif・XMP 以外のメタデータ（ICC プロファイル）が保存で変わるため中止しました"
        );
    }

    /// little_exif / `xmp::install` は前置きが一致する APP1 を**すべて**消すので、
    /// 2 つ目以降は検査 3 が捕まえる
    #[test]
    fn a_lost_second_exif_or_xmp_app1_is_refused() {
        let head = head_of(&fixture("lightroom-xmp.jpg"));
        let list = segments(&head).unwrap();
        let exif = list[first_app1(&list, EXIF_ID).unwrap()].contents;
        let xmp = list[first_app1(&list, super::super::xmp::XMP_ID).unwrap()].contents;

        for (second, name) in [(exif, "2 つ目の Exif"), (xmp, "2 つ目の XMP")] {
            let mut doubled: Vec<(u8, &[u8])> =
                list.iter().map(|s| (s.marker, s.contents)).collect();
            // APP0・Exif・XMP の後ろに 2 つ目を置く
            doubled.insert(3, (APP1, second));
            let before = head_with(&doubled);

            assert_eq!(
                refusal(&before, &head),
                format!("Exif・XMP 以外のメタデータ（{name}）が保存で変わるため中止しました")
            );
        }
    }

    /// Exif APP1 と XMP APP1 が先頭へ動くのは既存の挙動（spec「実測」）。相対順序で比べる
    #[test]
    fn moving_exif_and_xmp_to_the_front_passes() {
        let before = with_segment_after_app0("lightroom-xmp.jpg", (APP2, ICC));
        let list = segments(&before).unwrap();
        let (moved, rest): (Vec<_>, Vec<_>) = list.iter().partition(|s| s.marker == APP1);
        assert_eq!(moved.len(), 2, "前提条件: Exif と XMP の 2 つ");
        let after = head_with(
            &moved
                .iter()
                .chain(rest.iter())
                .map(|s| (s.marker, s.contents))
                .collect::<Vec<_>>(),
        );
        assert_ne!(before, after, "前提条件: 並びが変わっている");

        assert_eq!(check(&before, &after).unwrap(), Vec::<String>::new());
    }

    /// Exif・XMP 以外どうしの順序が入れ替われば拒否。書き込み前の列の側を名指す
    #[test]
    fn swapping_other_segments_is_refused() {
        let before = with_segment_after_app0("exif-only.jpg", (APP2, ICC));
        let mut list: Vec<(u8, &[u8])> = segments(&before)
            .unwrap()
            .iter()
            .map(|s| (s.marker, s.contents))
            .collect();
        list.swap(0, 1); // APP0 と APP2
        let after = head_with(&list);

        assert_eq!(
            refusal(&before, &after),
            "Exif・XMP 以外のメタデータ（JFIF）が保存で変わるため中止しました"
        );
    }

    #[test]
    fn segments_are_named_by_the_spec_table() {
        let name = |marker, contents: &[u8]| segment_name(&Segment { marker, contents });
        assert_eq!(name(APP2, ICC), "ICC プロファイル");
        assert_eq!(name(APP13, b"Photoshop 3.0\08BIM"), "IPTC（Photoshop）");
        assert_eq!(name(APP1, b"http://ns.adobe.com/xmp/extension/\0abc"), "拡張 XMP");
        assert_eq!(name(APP1, b"Exif\0\0II*\0"), "2 つ目の Exif");
        assert_eq!(name(APP1, b"http://ns.adobe.com/xap/1.0/\0<x"), "2 つ目の XMP");
        assert_eq!(name(APP0, b"JFIF\0\x01"), "JFIF");
        assert_eq!(name(0xEE, b"Adobe"), "APP14");
        assert_eq!(name(0xDB, b"\x00"), "画像の符号化情報");
    }
```

- [ ] **Step 3: テストが失敗することを確かめる**

Run: `cargo test -p picture-tool-core --lib metadata::preserve`
Expected: FAIL（`segments_are_named_by_the_spec_table` が `todo!()` で panic、ICC・2 つ目・入れ替えの 4 件が「拒否されなかった」で panic。`moving_exif_and_xmp_to_the_front_passes` は検査 3 が無いので PASS のまま）

- [ ] **Step 4: 実装する**

`thumbnail` の後ろに足す:

```rust
/// 検査 3 で比べる列: 最初の Exif APP1 と最初の XMP APP1 を除いたもの。
///
/// 除外を「最初の 1 つ」に限るのは、検査 1・2 と kamadak-exif・`has_exif_segment`・
/// `xmp::extract` が見るのが最初の 1 つだけだから。2 つ目以降の消失はここで検知する
fn others<'a>(segments: &'a [Segment<'a>]) -> Vec<&'a Segment<'a>> {
    let exif = first_app1(segments, EXIF_ID);
    let xmp = first_app1(segments, super::xmp::XMP_ID);
    segments
        .iter()
        .enumerate()
        .filter(|(at, _)| Some(*at) != exif && Some(*at) != xmp)
        .map(|(_, segment)| segment)
        .collect()
}

/// 検査 3 の文面に出すセグメントの名前（spec「エラー文面」の対応表）
fn segment_name(segment: &Segment) -> String {
    let starts = |prefix: &[u8]| segment.contents.starts_with(prefix);
    match segment.marker {
        APP2 if starts(b"ICC_PROFILE\0") => "ICC プロファイル".into(),
        APP13 if starts(b"Photoshop 3.0\0") => "IPTC（Photoshop）".into(),
        APP1 if starts(b"http://ns.adobe.com/xmp/extension/\0") => "拡張 XMP".into(),
        APP1 if starts(EXIF_ID) => "2 つ目の Exif".into(),
        APP1 if starts(super::xmp::XMP_ID) => "2 つ目の XMP".into(),
        APP0 if starts(b"JFIF\0") => "JFIF".into(),
        marker @ 0xE0..=0xEF => format!("APP{}", marker - 0xE0),
        _ => "画像の符号化情報".into(),
    }
}
```

`check` の `Ok(warnings)` の直前（`read_exif` の `if let` ブロックの外）に足す:

```rust
    // 検査 3: Exif・XMP 以外のセグメント（相対順序を含む）
    let before_others = others(&before_segments);
    let after_others = others(&after_segments);
    let length = before_others.len().max(after_others.len());
    if let Some(at) = (0..length).find(|&at| before_others.get(at) != after_others.get(at)) {
        // 書き込み前の列の側を名指す。先に尽きていたら書き込み後の列の側
        if let Some(segment) = before_others.get(at).or(after_others.get(at)) {
            bail!(
                "Exif・XMP 以外のメタデータ（{}）が保存で変わるため中止しました",
                segment_name(segment)
            );
        }
    }
```

- [ ] **Step 5: テストが通ることを確かめる**

Run: `cargo test -p picture-tool-core --lib metadata::preserve`
Expected: PASS（33 件。Task 2 の `edited_tags_may_appear_change_and_disappear` も PASS ＝ 実際の `build` の出力で検査 3 が誤拒否しない）

- [ ] **Step 6: `make check` を通してコミットする**

```bash
cargo fmt --all
make check
git add core/src/metadata/preserve.rs
git commit   # feat(core): メタデータ書き込みの検証強化: Exif・XMP 以外のセグメントの一致を検査する
```

---

## Task 5: MakerNote の位置の変化を警告にする

**Files:**
- Modify: `core/src/metadata/preserve.rs`

**Interfaces:**
- Consumes: Task 2 の `check` / `built` / `all_three` / `edited_value` / `edit_tiff` / `refusal`
- Produces:
  - `pub(super) const MAKER_NOTE_MOVED: &str`（spec の文面そのもの）
  - `fn maker_note_moved(before: &Exif, after: &Exif) -> bool`
  - `check` が `Ok(vec![MAKER_NOTE_MOVED.to_string()])` を返しうるようになる

- [ ] **Step 1: `test-integrity` スキルを起動する**

- [ ] **Step 2: 失敗するテストを書く**

Step 4 の定数 `MAKER_NOTE_MOVED` を先に足し、`maker_note_moved` を `todo!()` で置き、`mod tests` の末尾に足す:

```rust
    // --- MakerNote の位置の変化（警告） ---------------------------------------

    use little_exif::exif_tag::ExifTag;
    use little_exif::filetype::FileExtension;
    use little_exif::metadata::Metadata;

    const MAKER_NOTE: &[u8] = b"SONY DSC \0\0\0maker-note-body-0123456789";

    /// exif-only.jpg に MakerNote を足したもの（全体のバイト列）。
    /// フィクスチャは 2 枚とも MakerNote を持たない（計画 A-3）
    fn with_maker_note() -> Vec<u8> {
        let mut bytes = fixture("exif-only.jpg");
        let mut meta = Metadata::new_from_vec(&bytes, FileExtension::JPEG).unwrap();
        meta.set_tag(ExifTag::MakerNote(MAKER_NOTE.to_vec()));
        meta.write_to_vec(&mut bytes, FileExtension::JPEG).unwrap();
        bytes
    }

    fn maker_note(head: &[u8]) -> (Vec<u8>, u32) {
        match edited_value(head, Tag::MakerNote) {
            Some(Value::Undefined(bytes, at)) => (bytes, at),
            other => panic!("前提条件: MakerNote が無い（{other:?}）"),
        }
    }

    /// 実際の書き込み（`build`）で IFD0 が伸び、MakerNote が後ろへずれる。
    /// 通したうえで警告を返す（Review Focus 1）
    #[test]
    fn a_moved_maker_note_passes_with_a_warning() {
        let original = with_maker_note();
        let written = built(&original, &all_three());
        let (before_bytes, before_at) = maker_note(&head_of(&original));
        let (after_bytes, after_at) = maker_note(&head_of(&written));
        assert_eq!(before_bytes, MAKER_NOTE, "前提条件: 足した MakerNote が読める");
        assert_eq!(after_bytes, before_bytes, "前提条件: バイト列は保たれている");
        assert_ne!(after_at, before_at, "前提条件: 位置が変わっている");

        assert_eq!(
            check(&head_of(&original), &head_of(&written)).unwrap(),
            vec![MAKER_NOTE_MOVED.to_string()]
        );
    }

    #[test]
    fn an_unmoved_maker_note_passes_without_a_warning() {
        let head = head_of(&with_maker_note());
        maker_note(&head); // 前提条件: MakerNote がある
        assert_eq!(check(&head, &head).unwrap(), Vec::<String>::new());
    }

    /// MakerNote の中身が変われば警告ではなく検査 1 の拒否
    #[test]
    fn a_changed_maker_note_is_refused() {
        let before = head_of(&with_maker_note());
        let (_, at) = maker_note(&before);
        let after = edit_tiff(&before, |tiff| tiff[at as usize] ^= 0xFF);

        assert_eq!(
            refusal(&before, &after),
            "既存の Exif が保存で失われるか変わるため中止しました: MakerNote"
        );
    }
```

（MakerNote を持たない写真で警告が出ないことは Task 2 の `identical_heads_pass_without_warnings` と
`edited_tags_may_appear_change_and_disappear` が `Vec::new()` との一致で押さえている）

- [ ] **Step 3: テストが失敗することを確かめる**

Run: `cargo test -p picture-tool-core --lib metadata::preserve`
Expected: FAIL（`a_moved_maker_note_passes_with_a_warning` が「`[]` ≠ `[MAKER_NOTE_MOVED]`」、または `todo!()` で panic）。
**前提条件の assert（位置が変わっている）で落ちたら、ここで止めて報告する** ── `little_exif` が MakerNote の位置を
保つなら警告の経路はこの入力では動かず、テストの作り方から見直しが要る

- [ ] **Step 4: 実装する**

定数（Step 2 で足したもの。`EXIF_ID` の後ろに置く）:

```rust
/// MakerNote の位置だけが変わったときに返す警告（spec「MakerNote の位置の変化」）
pub(super) const MAKER_NOTE_MOVED: &str = "保存しました。ただしカメラ固有の情報（MakerNote）の位置が変わったため、他のソフトで一部の撮影情報が読めなくなる可能性があります";
```

`thumbnail` の後ろ:

```rust
/// 前後に MakerNote が 1 つずつあり、バイト列が同じで位置だけが違うか。
///
/// 拒否しないのは、IFD0 が伸びれば後方のデータは一律にずれ、拒否にすると MakerNote 付きの
/// 写真がほぼすべて保存できなくなるから。警告にするのは、メーカーによっては MakerNote 内部の
/// オフセットが TIFF 先頭基準で、他のソフトで読めなくなりうるから（中身の整合は判定しない）
fn maker_note_moved(before: &Exif, after: &Exif) -> bool {
    fn only(exif: &Exif) -> Option<&Value> {
        let mut found = exif
            .fields()
            .filter(|f| f.ifd_num == In::PRIMARY && f.tag == Tag::MakerNote);
        match (found.next(), found.next()) {
            (Some(field), None) => Some(&field.value),
            _ => None,
        }
    }
    matches!(
        (only(before), only(after)),
        (Some(Value::Undefined(a, at)), Some(Value::Undefined(b, bt))) if a == b && at != bt
    )
}
```

`check` の `let warnings = Vec::new();` を `let mut warnings = Vec::new();` にし、検査 2 の `if let` の後ろ
（`if let Some(after) = &after {` ブロックの中）に足す:

```rust
            if maker_note_moved(&before, after) {
                warnings.push(MAKER_NOTE_MOVED.to_string());
            }
```

`check` の doc コメントの「`Ok` の中身は保存を止めない警告。」を「`Ok` の中身は保存を止めない警告（MakerNote の位置の変化）。」にする。

- [ ] **Step 5: テストが通ることを確かめる**

Run: `cargo test -p picture-tool-core --lib metadata::preserve`
Expected: PASS（36 件）

- [ ] **Step 6: `make check` を通してコミットする**

```bash
cargo fmt --all
make check
git add core/src/metadata/preserve.rs
git commit   # feat(core): メタデータ書き込みの検証強化: MakerNote の位置の変化を警告として返す
```

---

## Task 6: `atomic::verify` に繋ぎ、警告を `write_metadata` まで上げる

**Files:**
- Modify: `core/src/metadata/mod.rs`（`mod preserve` の `allow` 削除、`write_metadata` / `write_inner`、`build` 内コメント、テスト 1 本）
- Modify: `core/src/metadata/atomic.rs`（モジュールコメント、`replace_verified`、`verify`、テスト 1 本の書き換え）
- Modify: `core/src/lib.rs:21-25`（crate ドキュメント）

**Interfaces:**
- Consumes: `preserve::check(&[u8], &[u8]) -> Result<Vec<String>>`、`preserve::MAKER_NOTE_MOVED`
- Produces:
  - `pub fn write_metadata(path: &Path, values: &EditableMetadata) -> Result<Vec<String>>`（公開 API。`Ok` の中身は警告）
  - `pub(super) fn replace_verified(path, original: &[u8], patched: Vec<u8>, expected) -> Result<Vec<String>>`
  - `fn verify(temp: &Path, original: &[u8], expected: &EditableMetadata) -> Result<Vec<String>>`（第 2 引数が `&Path` から `&[u8]` に変わる）

- [ ] **Step 1: `test-integrity` スキルを起動する**

- [ ] **Step 2: 失敗するテストを書く（警告が `write_metadata` から返る）**

`core/src/metadata/mod.rs` の `mod tests` の末尾に足す:

```rust
    /// **保存を止めない警告は `write_metadata` の戻り値で返る**（core は `eprintln!` しない）。
    /// MakerNote を持たない写真では空であることを対照に置く
    #[test]
    fn a_moved_maker_note_is_returned_as_a_warning() {
        let (_dir, plain) = scratch("exif-only.jpg");
        let values = EditableMetadata {
            title: Some("Harbor at dusk".into()),
            ..Default::default()
        };
        assert_eq!(
            write_metadata(&plain, &values).unwrap(),
            Vec::<String>::new(),
            "MakerNote が無ければ警告も無い"
        );

        let (_dir, path) = scratch("exif-only.jpg");
        let mut bytes = std::fs::read(&path).unwrap();
        let mut meta = Metadata::new_from_vec(&bytes, FileExtension::JPEG).unwrap();
        meta.set_tag(little_exif::exif_tag::ExifTag::MakerNote(
            b"SONY DSC \0\0\0maker-note-body".to_vec(),
        ));
        meta.write_to_vec(&mut bytes, FileExtension::JPEG).unwrap();
        std::fs::write(&path, &bytes).unwrap();

        assert_eq!(
            write_metadata(&path, &values).unwrap(),
            vec![preserve::MAKER_NOTE_MOVED.to_string()]
        );
        assert_eq!(read_metadata(&path).unwrap().metadata, values, "保存はされている");
    }
```

Run: `cargo test -p picture-tool-core --lib metadata::tests::a_moved_maker_note_is_returned_as_a_warning`
Expected: コンパイルエラー（`write_metadata` が `Result<()>` を返すので `Vec<String>` と比べられない）

- [ ] **Step 3: `atomic.rs` を繋ぎ替える**

モジュールコメントの手順 2:

```rust
//! 2. **読み戻して検証する** ── 書いた値が正しく読めるか、EXIF 側と XMP 側が食い違って
//!    いないか、編集対象以外のメタデータ（Exif・サムネイル・他のセグメント）が失われて
//!    いないか（`preserve`）、画素が 1 バイトも変わっていないか
```

`replace_verified`:
- doc を `/// 検証付きの原子的置換。`Ok` の中身は保存を止めない警告（`preserve::check` 由来）` にする
- 戻り値を `-> Result<Vec<String>>` にする
- `verify(&temp.0, path, expected)?;` を `let warnings = verify(&temp.0, original, expected)?;` にする
- 末尾の `Ok(())` を `Ok(warnings)` にする

`verify` の全体を次にする（前半 2 つの検査は現行のまま。3 番目の `read_exif_info` の比較を置き換える）:

```rust
/// 書いた結果を読み戻して確かめる。`Ok` の中身は保存を止めない警告
fn verify(temp: &Path, original: &[u8], expected: &EditableMetadata) -> Result<Vec<String>> {
    let read = super::read_metadata(temp)?;
    if read.metadata != *expected {
        bail!(
            "書き込んだ値を読み戻せませんでした（期待 {:?} / 実際 {:?}）",
            expected,
            read.metadata
        );
    }

    // **`write_metadata` は EXIF 側と XMP 側へ同じ値を書く契約なので、自分が書いた
    // 直後のファイルで食い違い警告が出ることは定義上あってはならない。** EXIF 側は
    // little_exif、XMP 側は自前の文字列組み立てという独立した2経路で書いているため、
    // 上の `read.metadata != *expected` は `pick`（食い違えば XMP を優先）が期待値を
    // 返す限り通ってしまい、片側だけの書き込み失敗や古い値の残存を見逃す。
    // ここで拾う。EXIF 実装と XMP 実装の相互照合になるので、同一パーサで自分の
    // 出力を読む同語反復ではない。
    if !read.warnings.is_empty() {
        bail!(
            "書き込み直後の読み戻しで食い違い警告が出ました（EXIF 側と XMP 側が揃っていません）: {:?}",
            read.warnings
        );
    }

    // 編集対象以外のメタデータが失われていないこと（`preserve` のモジュールコメント）。
    // **メモリ上の `patched` ではなく、実際に rename される一時ファイルを読み戻して見る**
    let written = std::fs::read(temp)
        .with_context(|| format!("一時ファイルを読み戻せません: {}", temp.display()))?;
    super::preserve::check(
        &original[..scan_offset(original)?],
        &written[..scan_offset(&written)?],
    )
}
```

- [ ] **Step 4: `mod.rs` を繋ぎ替える**

- `mod preserve;` の上の 2 行（コメントと `#[allow(dead_code)]`）を消す
- `write_metadata` と `write_inner` の戻り値を `Result<Vec<String>>` にする（本体は変えない。`catch_unwind` の型は推論で追従する）
- `write_metadata` の doc コメントの末尾に段落を足す:

```rust
///
/// **`Ok` の中身は保存を止めない警告**（MakerNote の位置の変化。`preserve` 参照）。
/// 出すか捨てるかは呼び出し側（境界）が決める。
```

- `build` 内のコメント「**既存 Exif の解析に失敗したら、そこで止める。**」の段落を次に置き換える:

```rust
    // **既存 Exif の解析に失敗したら、そこで止める。**
    // 空の Metadata から書き直すと little_exif は既存の Exif APP1 を丸ごと落とし、
    // MakerNote も GPS もサムネイルも消えた「本ツールの 3 タグだけの Exif」になる。
    // `atomic::verify` の `preserve::check` もこの結果を拒否する（書き込み前の全フィールドが
    // 失われるか、書き直した Exif を kamadak-exif が読めない）が、ここで止めた方が
    // 理由（「解析できない」）が正確に伝わる。**二重にあるのは意図的**:
    // ここは little_exif が読めない場合、`preserve` は kamadak-exif で比べた結果を受け持つ。
```

- [ ] **Step 5: `atomic.rs` の既存テストを書き換える**

`replace_verified_refuses_a_buffer_that_lost_the_existing_exif` は現行の 3 番目の検査の文面（「既存の Exif が壊れました」）を
assert している。試作での実測（計画 A-2）: この buffer は kamadak-exif の厳格読みで `Truncated IFD count` になり、
**「検証できない」の経路**で拒否される。前提条件の assert でこの経路を固定してから、その文面を assert する。

doc コメントの 1 段落目を次にする:

```rust
    /// **既存 Exif が消えたら置き換えない。**
    /// little_exif が既存の Exif を落とす事故（spec の issue #93）は、
    /// 「本ツールの 3 タグは読み戻せる」ので値の一致だけでは捕まらない。
    /// 独立実装（kamadak-exif）で見て初めて見える（`preserve`。この buffer は
    /// kamadak-exif が読めないので「検証できない」として拒否される）。
```

2 段落目の末尾「この gate（kamadak-exif との突き合わせ）を単独で動かせなくなる（実測済み）」を
「この gate を単独で動かせなくなる（実測済み）」にする。

「前提条件: 画素は動いていない」の `assert_eq!` の後ろに足す:

```rust
        // 前提条件: どちらの経路で拒否されるかを固定する。little_exif が空の Metadata で
        // 書き直した Exif は kamadak-exif の厳格読みでは読めない（実測: "Truncated IFD
        // count"）ので、検査 1 ではなく「検証できない」の経路で拒否される
        assert!(
            exif::Reader::new()
                .read_from_container(&mut std::io::Cursor::new(&buffer))
                .is_err(),
            "前提条件が崩れている: kamadak-exif が読めた（検査 1 の経路になる）"
        );
```

最後の assert を次にする:

```rust
        assert!(
            error
                .to_string()
                .contains("既存の Exif を検証できないため中止しました"),
            "{error:#}"
        );
```

- [ ] **Step 6: crate ドキュメントを更新する**

`core/src/lib.rs` の「呼び出し側が守ること」の `eprintln!` の項を次にする:

```rust
//! - **このクレートは `eprintln!` しない。** 利用者に伝えるべき事象は
//!   [`ProcessResult::warnings`] や `ExifFrameOutput::warnings` /
//!   `ExifFramePreview::warnings`、[`metadata::write_metadata`] の戻り値に積んで
//!   呼び出し元へ返す。出すか捨てるかは境界（CLI は stderr、GUI は
//!   `gui/src/commands.rs`）が決める。
```

- [ ] **Step 7: テストが通ることを確かめる**

Run: `cargo test -p picture-tool-core`
Expected: PASS（全件。`core/tests/metadata_integration.rs` の実書き込みテスト群が**誤拒否しない**ことの確認を兼ねる。
`a_file_whose_exif_cannot_be_parsed_is_refused_without_being_modified` も PASS のまま）

`write_metadata(` の他の呼び出し（`mod.rs` のテスト 13 か所・`metadata_integration.rs` 9 か所・`rating_scan_spike.rs`・
`gui/src/commands.rs` の 2 か所）は `.unwrap()` / `.expect()` / `.unwrap_err()` / `?` で受けているので変更不要（計画 A-5）。
`cargo build --workspace --all-targets` が警告なしで通ることで確かめる。

- [ ] **Step 8: `make check` を通してコミットする**

```bash
cargo fmt --all
make check
git add core/src/metadata/mod.rs core/src/metadata/atomic.rs core/src/lib.rs
git commit   # feat(core): メタデータ書き込みの検証強化: 置き換え前の検証を既存メタデータ全体の保全に置き換える
```

---

## Task 7: GUI の境界で拒否理由と警告を利用者へ届ける

**Files:**
- Modify: `gui/src/commands.rs:753-772`（`write_image_metadata`）、`mod tests` の末尾
- Modify: `gui/Cargo.toml`（`[dev-dependencies]`）

**Interfaces:**
- Consumes: `core::metadata::write_metadata(&Path, &EditableMetadata) -> anyhow::Result<Vec<String>>`、`core::metadata::read_metadata_fast`
- Produces: `fn save_and_read_back(file: &Path, metadata: &core::metadata::EditableMetadata) -> Result<core::metadata::MetadataRead, String>`

- [ ] **Step 1: `test-integrity` スキルを起動する**

- [ ] **Step 2: dev-dependency を足す**

`gui/Cargo.toml` の `[dev-dependencies]` の `image.workspace = true` の後ろ:

```toml
# 保存の警告（MakerNote の位置の変化）が境界を通って届くかのテストで、MakerNote 付きの
# JPEG を作るためだけに使う。core と同じ版に固定する（依存木に既にあり、ビルドは増えない）
little_exif = "=0.6.23"
```

- [ ] **Step 3: 失敗するテストを書く**

`save_and_read_back` を次の形で先に置く（**現行の挙動をそのまま移しただけ**。テストが落ちることを見るため）:

```rust
fn save_and_read_back(
    file: &Path,
    metadata: &core::metadata::EditableMetadata,
) -> Result<core::metadata::MetadataRead, String> {
    core::metadata::write_metadata(file, metadata).map_err(|e| e.to_string())?;
    core::metadata::read_metadata_fast(file).map_err(|e| e.to_string())
}
```

`gui/src/commands.rs` の `mod tests` の末尾に足す:

```rust
    /// **保存の拒否理由が境界を越えて届く。** `to_string()` では最上位の文脈
    /// （「メタデータを保存できませんでした…」）しか残らず、内側の理由が落ちる。
    /// 内側の文面には `build` の「解析できない」を使う（実書き込み経路で `preserve` の
    /// 拒否を起こす入力は作れない。計画 A-4）
    #[test]
    fn a_refused_save_reports_the_inner_reason() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("broken-exif.jpg");
        write_real_jpeg(&path);
        // SOI の直後に、TIFF として読めない Exif APP1 を差し込む
        let mut bytes = fs::read(&path).unwrap();
        let contents = b"Exif\0\0\xAA\xAA\xAA\xAA\xAA\xAA\xAA\xAA";
        let mut app1 = vec![0xFF, 0xE1];
        app1.extend(((contents.len() + 2) as u16).to_be_bytes());
        app1.extend(contents);
        bytes.splice(2..2, app1);
        fs::write(&path, &bytes).unwrap();

        let error = save_and_read_back(
            &path,
            &core::metadata::EditableMetadata {
                rating: Some(3),
                ..Default::default()
            },
        )
        .unwrap_err();

        assert!(error.contains("メタデータを保存できませんでした"), "{error}");
        assert!(error.contains("既存の Exif を解析できない"), "{error}");
        assert_eq!(fs::read(&path).unwrap(), bytes, "元ファイルが変わった");
    }

    /// **保存が返した警告は、保存後に返す `warnings` に載る。**
    /// フロントは保存後の `warnings` だけを表示するので、ここで落とすと利用者に届かない
    #[test]
    fn a_save_warning_reaches_the_returned_warnings() {
        use little_exif::exif_tag::ExifTag;
        use little_exif::filetype::FileExtension;
        use little_exif::metadata::Metadata;

        let dir = TempDir::new().unwrap();
        let path = dir.path().join("maker-note.jpg");
        write_real_jpeg(&path);
        let mut bytes = fs::read(&path).unwrap();
        let mut meta = Metadata::new();
        meta.set_tag(ExifTag::MakerNote(b"SONY DSC \0\0\0maker-note-body".to_vec()));
        meta.write_to_vec(&mut bytes, FileExtension::JPEG).unwrap();
        fs::write(&path, &bytes).unwrap();

        let read = save_and_read_back(
            &path,
            &core::metadata::EditableMetadata {
                title: Some("Harbor at dusk".into()),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(read.metadata.title.as_deref(), Some("Harbor at dusk"));
        assert!(
            read.warnings.iter().any(|w| w.contains("MakerNote")),
            "{:?}",
            read.warnings
        );
    }
```

- [ ] **Step 4: テストが失敗することを確かめる**

Run: `cargo test -p picture-tool-gui save`
Expected: FAIL（2 件。1 件目は「既存の Exif を解析できない」が含まれない、2 件目は `warnings` に MakerNote が無い）

- [ ] **Step 5: 実装する**

`write_image_metadata` の本体と `save_and_read_back` を次にする:

```rust
#[tauri::command]
pub async fn write_image_metadata(
    state: tauri::State<'_, ProcessingState>,
    path: String,
    metadata: core::metadata::EditableMetadata,
) -> Result<core::metadata::MetadataRead, String> {
    let file = metadata_target(&state, &path)?;
    tokio::task::spawn_blocking(move || save_and_read_back(&file, &metadata))
        .await
        .map_err(|e| e.to_string())?
}

/// `write_image_metadata` の本体。コマンド本体（`tauri::State` を取る）は単体テストから
/// 呼べないので、ここへ出してある（`metadata_target` と同じ理由）。
///
/// **保存失敗は文脈のチェーン全体（`{e:#}`）で文字列化する。** `to_string()` だと
/// 最上位の「メタデータを保存できませんでした…」しか届かず、なぜ止めたか
/// （「既存の Exif が保存で失われるか変わるため」等）が利用者に見えない。
/// 保存が返した警告（MakerNote の位置の変化）は、読み戻した結果の `warnings` に足す ──
/// フロントは保存後の `warnings` をそのまま表示している（`metadataDraft.svelte.ts`）
fn save_and_read_back(
    file: &Path,
    metadata: &core::metadata::EditableMetadata,
) -> Result<core::metadata::MetadataRead, String> {
    let saved = core::metadata::write_metadata(file, metadata).map_err(|e| format!("{e:#}"))?;
    // **読み戻しも入口を通す。** ここだけ経路が違うと、★の書き戻しの
    // 「値の出所」（絞り込み spec §5）がグリッドの★と別経路になる
    let mut read = core::metadata::read_metadata_fast(file).map_err(|e| e.to_string())?;
    read.warnings.extend(saved);
    Ok(read)
}
```

（`write_image_metadata` の既存の doc コメント「検証付き原子的置換で書き込み、**保存直後の実際の中身を返す**…」は残す）

- [ ] **Step 6: テストが通ることを確かめる**

Run: `cargo test -p picture-tool-gui`
Expected: PASS（全件。DTO 契約テスト `contract` も PASS ＝ `MetadataRead` の形は変わっていない）

- [ ] **Step 7: `make check` を通してコミットする**

```bash
cargo fmt --all
make check
git add gui/src/commands.rs gui/Cargo.toml Cargo.lock
git commit   # fix(gui): メタデータ書き込みの検証強化: 保存の拒否理由と警告を利用者へ届ける
```

---

## Task 8: 検査が効いていることの実測・実写真・ドキュメント

**Files:**
- Modify: `docs/superpowers/specs/2026-08-18-metadata-editing-design.md:433-445`（「この保証の実際の強度」）
- Modify: `docs/README.md`（本計画の行）
- Modify: 本計画の「実施メモ」

- [ ] **Step 1: 検査を 1 つずつ無効化し、対応するテストが落ちることを実測する（コミットしない）**

spec「テスト」の「検査が効いていることの確認」。各変異は `git stash` で戻せるよう 1 つずつ入れ、
`cargo test -p picture-tool-core --lib metadata::` と `cargo test -p picture-tool-gui save` を走らせる。

| # | 変異（`preserve.rs` / `atomic.rs`） | 落ちるべきテスト（少なくとも） |
|---|---|---|
| M1 | 検査 1: `if !lost.is_empty()` → `if false` | `a_lost_tag_is_refused_by_name`・`a_changed_value_is_refused_by_name`・`a_type_change_with_the_same_number_is_refused`・`more_than_three_lost_tags_are_summarized`・`losing_the_whole_exif_app1_is_refused`・`a_changed_maker_note_is_refused` |
| M2 | 検査 2: `if thumbnail(after) != Some(original_thumbnail)` → `if false` | `a_changed_thumbnail_is_refused`・`a_lost_thumbnail_pointer_is_refused` |
| M3 | 検査 3: `find(...)` の結果を `None::<usize>` に置き換える | `a_lost_icc_profile_is_refused_by_name`・`a_changed_icc_profile_is_refused_by_name`・`a_lost_second_exif_or_xmp_app1_is_refused`・`swapping_other_segments_is_refused` |
| M4 | 除外: `excluded` が常に `false` を返す | `edited_tags_may_appear_change_and_disappear`・`a_moved_thumbnail_with_the_same_bytes_passes`・既存の実書き込みテスト多数 |
| M5 | 除外の context 取り違え: `Tag::UserComment` → `Tag(Context::Tiff, 0x9286)` | `edited_tags_may_appear_change_and_disappear` |
| M6 | 警告: `warnings.push(...)` を消す | `a_moved_maker_note_passes_with_a_warning`・`a_moved_maker_note_is_returned_as_a_warning`・`a_save_warning_reaches_the_returned_warnings` |
| M7 | 配線: `verify` の `super::preserve::check(...)` を `Ok(Vec::new())` に | `replace_verified_refuses_a_buffer_that_lost_the_existing_exif`・`a_moved_maker_note_is_returned_as_a_warning`・`a_save_warning_reaches_the_returned_warnings` |
| M8 | 厳格読みをやめる: `read_exif` を `exif::Reader::new().continue_on_error(true).read_raw(tiff)` にし、`Err(exif::Error::PartialResult(p))` を `Ok(Some(p.into_inner().0))` として扱う | `an_exif_with_one_broken_entry_is_refused_as_unverifiable` |

**1 本も落ちない変異があったら、それはテストの欠陥として扱う**（到達不能の説明で済ませず、落ちる形に直す。
memory: negative-assertion-tests-need-a-precondition）。結果は「実施メモ」に表で残す。

- [ ] **Step 2: 実写真で保存が通ることを確かめる（コミットしない）**

`docs/tmp/20260328-DSC05024.jpg`（Lightroom 書き出し、APP13・ICC 付き。git 管理外）の**コピー**へ実際に保存を通す。
一時テストを `core/tests/metadata_integration.rs` の末尾に足し、実行後に消す:

```rust
#[test]
#[ignore]
fn real_photo_saves_without_refusal() {
    let source = std::env::var("PICTURE_TOOL_REAL_JPEG").expect("PICTURE_TOOL_REAL_JPEG が無い");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("real.jpg");
    std::fs::copy(&source, &path).unwrap();
    let values = EditableMetadata {
        title: Some("実写真".into()),
        comment: Some("検証".into()),
        rating: Some(5),
    };
    let warnings = write_metadata(&path, &values).unwrap();
    println!("warnings: {warnings:?}");
    assert_eq!(read_metadata(&path).unwrap().metadata, values);
    // 2 回目の保存（自分が書いたタグの変更）も通る
    write_metadata(&path, &EditableMetadata::default()).unwrap();
}
```

```bash
cargo test -p picture-tool-core --test metadata_integration --no-run
SCRATCH=<スクラッチパッドのディレクトリ>
BIN=$(ls -t target/debug/deps/metadata_integration-* | grep -v '\.d$' | head -1)
(cd core && PICTURE_TOOL_REAL_JPEG="$PWD/../docs/tmp/20260328-DSC05024.jpg" "../$BIN" real_photo_saves_without_refusal --ignored --nocapture > "$SCRATCH/real-photo.txt" 2>&1)
cat "$SCRATCH/real-photo.txt"
```

Expected: `test result: ok. 1 passed`、`warnings: []`（Lightroom 書き出しは MakerNote を持たない）。
拒否されたら**止めて報告する**（文面をそのまま添える）。確認後、一時テストを削除し `git diff` が空であることを確かめる。

- [ ] **Step 3: メタデータ編集 spec の「この保証の実際の強度」を書き換える**

`docs/superpowers/specs/2026-08-18-metadata-editing-design.md` の「**この保証の実際の強度**（ブランチ最終レビュー I-3 の記録）」
の段落（「守られるのは」から「ここでは実態の記録に留める。」まで）を次に置き換える。
直後の「MakerNote 付きの JPEG が手に入ったら…」の段落は残す:

```markdown
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
```

- [ ] **Step 4: 索引と実施メモを更新する**

1. `docs/README.md` の「直近の実装計画」の本計画の行を、実施後の内容（再実測の結果・変異の実測・実写真の確認を持つこと）に合わせて書き換える
2. 本計画末尾の「実施メモ」に、Step 1 の変異表の結果、Step 2 の出力、spec と食い違った点（あれば）を書く

- [ ] **Step 5: 最終確認とコミット**

```bash
make check
git status --short   # 一時テスト・再実測テストが残っていないこと
git add docs/superpowers/specs/2026-08-18-metadata-editing-design.md docs/README.md docs/superpowers/plans/2026-09-30-metadata-write-verification.md
git commit   # docs: メタデータ書き込みの検証強化: 保証の実際の強度を強化後の実態に書き換える
```

GUI の見た目は変えていない（`MetadataRead` の形も、フロントの表示経路も同じ）ので e2e は必須ではない。
拒否の文面が長くなる（パスと理由を含む）ことの見た目を確かめたい場合だけ、memory
「verify-gui-error-paths-with-vite-dev」の手順で確認する。

---

## セルフレビュー（spec との突き合わせ）

| spec の要求 | タスク |
|---|---|
| 検査 1（除外キー・向き・多重集合・値の比較規則） | Task 1（比較規則）・Task 2 |
| 検査 2（TIFF 先頭基準・両方ある・`checked_add`・panic しない） | Task 3 |
| 検査 3（最初の Exif／XMP を除く・相対順序・名指しの規則と対応表） | Task 4 |
| MakerNote の位置の変化（警告・文面・経路・バイト列の変化は拒否） | Task 5・6・7 |
| 読めない場合の扱い（`has_exif_segment` と同じ規則・`read_raw`・厳格読み・4 行の表） | Task 1・2（A-1 の決定を含む） |
| エラー文面（検査 1〜3・読めない・利用者に届く形） | Task 2・3・4・7 |
| 置き場所（`preserve.rs`・SOS より前・一時ファイルの読み戻し・`read_exif_info` の削除） | Task 1・6 |
| テスト表の 14 行 | Task 2（1〜4・8・9・13・14 行目）・Task 3（5・10）・Task 4（6・7）・Task 5（11・12） |
| 誤拒否しない（既存結合テスト） | Task 6 Step 7 |
| 既存単体テストの書き換え | Task 6 Step 5 |
| GUI の境界（`{e:#}`・警告の合流） | Task 7 |
| 検査が効いていることの確認 | Task 8 Step 1 |
| 実写真 | Task 8 Step 2 |
| 再実測（レビュー Q3） | Task 1 |
| メタデータ編集 spec への反映 | Task 8 Step 3 |
| コメントの追従（`atomic.rs` 冒頭・`build` 内） | Task 6 Step 3・4 |

### spec から意図的に外したこと

- **テスト用フィクスチャファイルを足さない**（A-3）。spec は要否を計画に委ねていた
- **GUI 境界テストの内側の文面を `build` の拒否にした**（A-4）。spec の文面は「例」

---

## 実施メモ

### Task 1: 再実測

実行日: 2026-09-30。`build` でタイトル「再実測」・コメント・★4 を書き、除外キーを当てずに比較規則だけで「書き込み前 ⊆ 書き込み後」を見た（`remeasure` 一時テスト。コミットしていない）。

- 3 枚とも Expected どおり。`消えた/変わった:` は `JPEGInterchangeFormat`（IFD1）の 1 行だけ、`ExifVersion`・`FileSource`・`SceneType` は出ない、`増えた:` は編集対象の 4 行
- 値は spec の実測（1030 → 1180）と違い 1030 → 1144。書いた文字列が spec 実測時と違うため IFD0 の伸びが違うだけで、変化の種類は同じ
- kamadak-exif の `In` の `Display` は `0`/`1` ではなく `primary`/`thumbnail`
- セグメントは E1（Exif・XMP）以外の並びと長さが前後で同じ

```text
== core/tests/fixtures/exif-only.jpg
フィールド数: 前 55 / 後 59
消えた/変わった: thumbnail/JPEGInterchangeFormat 前 Long([1030]) 後 ["Long([1144])"]
増えた: primary/Tag(Tiff, 18249)
増えた: primary/Tag(Tiff, 40091)
増えた: primary/Tag(Tiff, 40092)
増えた: primary/UserComment
セグメント 前: E0:14 E1:13444 C0:15 DB:65 DB:65 C4:29 C4:179 C4:29 C4:179
セグメント 後: E1:13558 E1:847 E0:14 C0:15 DB:65 DB:65 C4:29 C4:179 C4:29 C4:179
== core/tests/fixtures/lightroom-xmp.jpg
フィールド数: 前 55 / 後 59
消えた/変わった: thumbnail/JPEGInterchangeFormat 前 Long([1030]) 後 ["Long([1144])"]
増えた: primary/Tag(Tiff, 18249)
増えた: primary/Tag(Tiff, 40091)
増えた: primary/Tag(Tiff, 40092)
増えた: primary/UserComment
セグメント 前: E0:14 E1:13444 E1:13076 C0:15 DB:65 DB:65 C4:29 C4:179 C4:29 C4:179
セグメント 後: E1:13558 E1:13651 E0:14 C0:15 DB:65 DB:65 C4:29 C4:179 C4:29 C4:179
== docs/tmp/20260328-DSC05024.jpg
フィールド数: 前 55 / 後 59
消えた/変わった: thumbnail/JPEGInterchangeFormat 前 Long([1030]) 後 ["Long([1144])"]
増えた: primary/Tag(Tiff, 18249)
増えた: primary/Tag(Tiff, 40091)
増えた: primary/Tag(Tiff, 40092)
増えた: primary/UserComment
セグメント 前: E1:11589 ED:10768 E2:3158 E1:15673 DB:130 DD:2 EE:12 C0:15 C4:185
セグメント 後: E1:11706 E1:16248 ED:10768 E2:3158 DB:130 DD:2 EE:12 C0:15 C4:185
```

### Task 8: 変異の実測

実行日: 2026-09-30。変異を 1 つずつ入れ、`cargo test -p picture-tool-core --lib metadata::` と
`cargo test -p picture-tool-gui save` を走らせた（コミットしていない）。**8 変異とも少なくとも計画の列挙どおりのテストが落ち、生き残りは無い。**

| # | 変異 | 落ちたテスト |
|---|---|---|
| M1 | 検査 1 を `if false` | `a_lost_tag_is_refused_by_name`・`a_changed_value_is_refused_by_name`・`a_type_change_with_the_same_number_is_refused`・`more_than_three_lost_tags_are_summarized`・`losing_the_whole_exif_app1_is_refused`・`a_changed_maker_note_is_refused`（6） |
| M2 | 検査 2 を `&& false` | `a_changed_thumbnail_is_refused`・`a_lost_thumbnail_pointer_is_refused`（2） |
| M3 | 検査 3 の `find` を `None::<usize>` | `a_lost_icc_profile_is_refused_by_name`・`a_changed_icc_profile_is_refused_by_name`・`a_lost_second_exif_or_xmp_app1_is_refused`・`swapping_other_segments_is_refused`（4） |
| M4 | `excluded` が常に `false` | `preserve` 4 件（`edited_tags_may_appear_change_and_disappear`・`a_moved_thumbnail_with_the_same_bytes_passes`・`a_moved_maker_note_passes_with_a_warning`・`a_lost_thumbnail_pointer_is_refused` ── 最後のものは検査 1 が先に別の文面で拒否するため）＋ `metadata::tests` の実書き込みテスト 12 件（16） |
| M5 | 除外の `Tag::UserComment` を `Tag(Context::Tiff, 0x9286)` | `edited_tags_may_appear_change_and_disappear`（1） |
| M6 | `warnings.push(...)` を削除 | `a_moved_maker_note_passes_with_a_warning`・`a_moved_maker_note_is_returned_as_a_warning`・`a_save_warning_reaches_the_returned_warnings`（3） |
| M7 | `verify` の `preserve::check` を `Ok(Vec::new())` | `replace_verified_refuses_a_buffer_that_lost_the_existing_exif`・`a_moved_maker_note_is_returned_as_a_warning`・`a_save_warning_reaches_the_returned_warnings`（3） |
| M8 | `read_exif` を部分結果で読む | `an_exif_with_one_broken_entry_is_refused_as_unverifiable`・`replace_verified_refuses_a_buffer_that_lost_the_existing_exif`（2。後者は部分結果で読めてしまい「検証できない」ではなく検査 1 の文面になるため） |

### Task 8: 実写真

実行日: 2026-09-30。`docs/tmp/20260328-DSC05024.jpg`（Lightroom Classic 書き出し、APP13・ICC 付き）の
コピーへ、タイトル「実写真」・コメント・★5 を保存し、続けて空欄で 2 回目の保存をした（一時テストはコミットしていない）。

```text
running 1 test
warnings: []
test real_photo_saves_without_refusal ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 8 filtered out; finished in 0.52s
```

拒否されず、警告も無い（Lightroom 書き出しは MakerNote を持たない）。

### spec と食い違った点

- spec の実測の値の変化は 1030 → 1180 だが、再実測では 1030 → 1144（書いた文字列の違いで IFD0 の伸びが違うだけ。Task 1 の実施メモ）
- 計画のコードのうち 1 か所（Task 3 の `check` 内の入れ子の `if`）は clippy の `collapsible_if` に掛かったので let-chain に畳んだ。挙動は同じ
