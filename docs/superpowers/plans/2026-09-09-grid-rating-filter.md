# 写真グリッドの★レーティング絞り込み（Phase 2）実装計画

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** フォルダーを開いた時点でフォルダー全件の★が揃い、名前付き 7 択の `<select>` で写真グリッドを★で絞り込めるようにする。

**Architecture:** ★の出所を `list_images` 1 本に寄せる。JPEG の先頭（SOI〜SOS）だけを読む軽量経路を core に置き、`read_metadata_fast` という唯一の入口の裏に隠す（読めなければ全体読みへフォールバック）。`list_images` は同時に `par_iter` 化し、★を畳んでも現行より速くする。絞り込みの意味論はフロントの純関数（`gridFilter.ts`）に足すだけで、順序を決める場所は 1 箇所のまま変えない。可視範囲起点の★取得（`read_image_ratings` コマンド・`ratings.svelte.ts` ストア）は丸ごと消える。

**Tech Stack:** Rust（`picture-tool-core` / `picture-tool-gui`、rayon、`img-parts` / `little_exif` / `quick-xml`）、Tauri v2、Svelte 5 runes、Vite、bun test、Playwright

**Spec:** [`docs/superpowers/specs/2026-09-08-grid-filtering-design.md`](../specs/2026-09-08-grid-filtering-design.md)
（レビュー記録は `docs/tmp/review.md` / `docs/tmp/review-2.md`。gitignore 対象だが spec が参照しているので消さない）

## Global Constraints

- **検証は `make check`（lint / cargo test / typecheck / bun test）。** 変更したら必ず exit 0 にする。
  e2e は別で `cd gui-frontend && bun run e2e`（ポート 5174 が塞がっていれば `E2E_PORT=5199 bun run e2e`）。
- **`gui-frontend/src/lib/dto.contract.ts` は自動生成物。手で編集しない。**
  再生成は `UPDATE_DTO_CONTRACT=1 cargo test -p picture-tool-gui contract`。
  生成元は `gui/src/types.rs` の `mod contract`。
- **webview から来たパスは `gui/src/security.rs` の検証関数を通す。** 本計画は
  **新しいコマンドを 1 つも足さない**（`list_images(path)` は既に `security::existing_dir` を通る）。
- **`core` は `eprintln!` しない。** 伝えるべき事象は `warnings` に積む。
  本計画では `list_images` が `warnings` を**捨てる**（spec §1 の決定）。
- **Svelte 5 の runes 構文（`$state` / `$derived` / `$effect`）を使う。** 旧構文は使わない。
- **生の色を書いてよいのは `gui-frontend/src/styles/` の 2 ファイルだけ。** 余白・角丸もトークン経由。
  本計画で使う間隔トークンは `--space-2` = 8px、`--space-3` = 12px。
- **一覧の順序を決めるのは `gui-frontend/src/lib/browser/gridFilter.ts` の 1 箇所だけ。**
  `App.svelte` の `visibleImages` が唯一の出所。★の軸を足しても、この構造は変えない。
- **絞り込みは表示だけを変える。** 選択は解除せず、変換対象からも減らさない。
- **e2e のロール参照は容器へスコープする。** 本計画は `<select>` の `<option>` を 7 個増やすので、
  スコープしていない `getByRole("option")` は一斉に壊れる（既存はすべてスコープ済みか
  name 指定済みであることを確認済み。新しく書く検査も必ず `gridOf(page)` に閉じる）。
- **否定形の検査（「出ない」「隠れる」「重ならない」）には前提条件の assert を添える。**
  検査対象がそもそも発生していないだけで緑になるのを防ぐ。
- **コミットは `commit-message` スキルに従う**（`<type>(<scope>): <対応の概要>: <description>`）。
  本計画の `<対応の概要>` は直近 commit から逐語継承して **`写真グリッドの絞り込みと並び替え`**。
  footer は `Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>` のみ
  （**`Claude-Session:` trailer は付けない** ── スキルの明示規定）。

---

## ファイル構成

### 新規

| ファイル | 責務 |
|---|---|
| `core/src/metadata/header.rs` | JPEG の先頭を辿り、**APP1 だけを写した最小の JPEG バイト列**を組み立てる。ファイル I/O とセグメント走査だけを持ち、解析はしない |

### 変更

| ファイル | 変更内容 |
|---|---|
| `core/src/metadata/mod.rs` | `mod header;` / `read_metadata_header` / `read_metadata_fast` を足す |
| `gui/src/commands.rs` | `list_images` を `collect_image_entries` ＋ `sort_entries` に割って `par_iter` 化し★を畳む。`read_image_metadata` と `write_image_metadata` の読み戻しを `read_metadata_fast` へ。`read_image_ratings` を削除 |
| `gui/src/types.rs` | `ImageEntry.rating` を足す。`RatingEntry` を削除。`mod contract` の該当 entry を更新 |
| `gui/src/main.rs` | `commands::read_image_ratings` の登録を削除 |
| `gui/src/security.rs` | モジュールコメントから `read_image_ratings` の節を削除 |
| `gui-frontend/src/lib/types.ts` | `ImageEntry.rating` を足す。`RatingEntry` を削除 |
| `gui-frontend/src/lib/api.ts` | `readImageRatings` を削除 |
| `gui-frontend/src/lib/browser/gridFilter.ts` | `RatingFilter` / `RATING_PRESETS` / `filterByRating` を足し、`arrangeImages` の引数を増やす。モジュールコメントの「★は別の設計になる」を書き換える |
| `gui-frontend/src/lib/browser/GridFilterBar.svelte` | ★の `<select>` を足す |
| `gui-frontend/src/lib/browser/GridHeader.svelte` | `.status` / `.filters-area` / `.tools` の 3 グループに分け、`grid-template-areas` ＋ コンテナクエリで段を切る |
| `gui-frontend/src/lib/browser/PhotoGrid.svelte` | `ratingFor` prop を廃し `image.rating` を直に使う。`.photo-grid` に `container-type: inline-size` |
| `gui-frontend/src/App.svelte` | ★の絞り込み状態を持つ。可視範囲起点の★取得 `$effect` と `visibleRange` を削除。保存後に `ImageEntry.rating` をその場で書き換える。スクロールリセットの依存に★を足す |
| `gui-frontend/src/lib/browser/gridFilter.test.ts` | ★の検査を足す。`arrangeImages` の呼び出しを直す |
| `gui-frontend/e2e/stub.ts` | `list_images` に★を持たせる。`read_image_ratings` ハンドラを削除 |
| `gui-frontend/e2e/gridFilter.spec.ts` | `tileNames()` を★込みでも壊れない形にする。★の e2e とレイアウト検査を足す |
| `gui-frontend/e2e/metadata.spec.ts` | 裁定 X1 のテストを書き換える。`:404` の前提条件コメントを実態に合わせる |
| `core/tests/rating_scan_spike.rs` | フォールバック率を数える `#[ignore]` テストを足す（未検証 3 の消し込み） |
| `docs/README.md` | 本計画を索引に載せる |
| `docs/superpowers/specs/2026-09-08-grid-filtering-design.md` | 「状態」を Phase 2 実装済みに更新 |

### 削除

| ファイル | 理由 |
|---|---|
| `gui-frontend/src/lib/browser/ratings.svelte.ts` | ★の出所が `list_images` になり、保持ストア（`CHUNK` / `inFlight` / 台帳）ごと不要になる |

---

## Task 1: core の軽量読み取り（`header.rs`）

JPEG の先頭から SOS までを辿り、**APP1 だけを写した最小の JPEG**（SOI ＋ APP1 群 ＋ SOS ヘッダ ＋ EOI）を組み立てる。解析はしない ── ここはバイト走査だけの層。

下敷きは spike の `header_jpeg`（`core/tests/rating_scan_spike.rs:45`）だが、**そのまま移植しない**:
spike は SOI 検査で非 JPEG を弾く形で、拡張子による分岐は Task 2 の入口に置く。

**Files:**
- Create: `core/src/metadata/header.rs`
- Modify: `core/src/metadata/mod.rs:12-14`（`mod header;` を足す）

**Interfaces:**
- Consumes: なし
- Produces: `pub(super) fn header_bytes(path: &std::path::Path) -> std::io::Result<Vec<u8>>`
  ── Task 2 の `read_metadata_header` が呼ぶ

- [ ] **Step 1: 失敗するテストを書く**

`core/src/metadata/header.rs` を新規作成し、**テストだけ**を書く（`scan` はまだ無い）。
テストは `Cursor` にバイト列を組み立てて `scan` に渡す ── 実ファイルの fixture を増やさない。
何が入っていて何が入っていないかがテストの中で読めることが要件（spec テスト方針 R-14）。

```rust
//! JPEG の先頭（SOI〜SOS）だけを読み、**APP1 だけを写した最小の JPEG** を組み立てる。
//!
//! Exif も XMP も SOS（画素データの開始）より前の APP1 セグメントにある。
//! ★1 つを知るためにファイル全体（5MB）を読む必要は無い ── 実測で
//! 1,000 枚あたり 1,118ms → 68ms、ピーク RSS 201MB → 7.4MB
//! （`core/tests/rating_scan_spike.rs` / spec の「実測」節）。
//!
//! **ここは解析をしない。** 組み立てたバイト列は
//! [`super::read_metadata_bytes`] がそのまま食える形にしてある ──
//! 解析の実装を変えずに「読む量」だけを変えるのが設計の核心で、
//! 実測でも解析コストは差に現れなかった（この機能は I/O 律速である）。

use std::fs::File;
use std::io::{BufReader, Read, Seek};
use std::path::Path;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// 長さ付きセグメント 1 つ分のバイト列。`len` は自分自身の 2 バイトを含む
    fn segment(marker: u8, payload: &[u8]) -> Vec<u8> {
        let len = u16::try_from(payload.len() + 2).unwrap();
        let mut out = vec![0xFF, marker];
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(payload);
        out
    }

    fn run(bytes: Vec<u8>) -> std::io::Result<Vec<u8>> {
        scan(&mut BufReader::new(Cursor::new(bytes)))
    }

    #[test]
    fn app1_だけを写し_画素は一切読まない() {
        let app1 = segment(0xE1, b"Exif\0\0dummy-exif-payload");
        let mut input = vec![0xFF, 0xD8];
        input.extend_from_slice(&segment(0xE0, b"JFIF\0捨てられる")); // APP0
        input.extend_from_slice(&app1);
        input.extend_from_slice(&segment(0xDB, b"quantization")); // DQT
        input.extend_from_slice(&segment(0xDA, b"sos-header")); // SOS
        input.extend_from_slice(&[0x12; 4096]); // 画素データ
        input.extend_from_slice(&[0xFF, 0xD9]); // EOI

        let mut expected = vec![0xFF, 0xD8];
        expected.extend_from_slice(&app1);
        expected.extend_from_slice(&segment(0xDA, b"sos-header"));
        expected.extend_from_slice(&[0xFF, 0xD9]);

        assert_eq!(run(input).unwrap(), expected);
    }

    #[test]
    fn app1_を持たない_jpeg_でも最小の_jpeg_が返る() {
        let mut input = vec![0xFF, 0xD8];
        input.extend_from_slice(&segment(0xE0, b"JFIF\0"));
        input.extend_from_slice(&segment(0xDA, b"sos"));
        input.extend_from_slice(&[0x34; 512]);
        input.extend_from_slice(&[0xFF, 0xD9]);

        let mut expected = vec![0xFF, 0xD8];
        expected.extend_from_slice(&segment(0xDA, b"sos"));
        expected.extend_from_slice(&[0xFF, 0xD9]);

        assert_eq!(run(input).unwrap(), expected);
    }

    #[test]
    fn sos_の前に_eoi_が来ても_eoi_で閉じて返す() {
        // Exif も画素も持たない退化した JPEG。ここで Err にすると
        // Task 2 のフォールバックが無駄に全体読みへ落ちる
        let input = vec![0xFF, 0xD8, 0xFF, 0xD9];
        assert_eq!(run(input).unwrap(), vec![0xFF, 0xD8, 0xFF, 0xD9]);
    }

    #[test]
    fn マーカー前の_0xff_パディングを読み飛ばす() {
        let app1 = segment(0xE1, b"Exif\0\0x");
        let mut input = vec![0xFF, 0xD8, 0xFF, 0xFF, 0xFF]; // fill byte が 2 つ
        input.extend_from_slice(&app1[1..]); // 0xFF は上で出したので marker から
        input.extend_from_slice(&segment(0xDA, b"s"));
        input.extend_from_slice(&[0xFF, 0xD9]);

        let mut expected = vec![0xFF, 0xD8];
        expected.extend_from_slice(&app1);
        expected.extend_from_slice(&segment(0xDA, b"s"));
        expected.extend_from_slice(&[0xFF, 0xD9]);

        assert_eq!(run(input).unwrap(), expected);
    }

    #[test]
    fn soi_で始まらなければ_err() {
        // PNG のヘッダ。Task 2 の入口が拡張子で弾くので通常は到達しないが、
        // 拡張子を偽った .jpg はここへ来る
        let input = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        assert!(run(input).is_err());
    }

    #[test]
    fn セグメント境界が_0xff_で始まらなければ_err() {
        let input = vec![0xFF, 0xD8, 0x00, 0xE1, 0x00, 0x04, 0x00, 0x00];
        assert!(run(input).is_err());
    }

    #[test]
    fn セグメント長が_2_未満なら_err() {
        // len = 1。payload = -1 バイトになるので、素直に引き算すると panic する
        let input = vec![0xFF, 0xD8, 0xFF, 0xE1, 0x00, 0x01];
        assert!(run(input).is_err());
    }

    #[test]
    fn 途中で切れたファイルは_err_になり_panic_しない() {
        // APP1 の長さは 1000 バイトだと言っているが、実体は 4 バイトしかない
        let input = vec![0xFF, 0xD8, 0xFF, 0xE1, 0x03, 0xE8, 1, 2, 3, 4];
        assert!(run(input).is_err());
    }
}
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cargo test -p picture-tool-core --lib metadata::header`
Expected: コンパイルエラー（`cannot find function 'scan' in this scope`）。
`core/src/metadata/mod.rs:14` の直後に `mod header;` を足していない場合は
「file not found for module」で先に落ちるので、先に足しておくこと。

- [ ] **Step 3: 最小の実装を書く**

`core/src/metadata/header.rs` のモジュールコメントと `use` の直後（`mod tests` の前）に足す。

```rust
fn broken(reason: &'static str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, reason)
}

/// 先頭だけを読んだ最小の JPEG を返す。
pub(super) fn header_bytes(path: &Path) -> std::io::Result<Vec<u8>> {
    scan(&mut BufReader::with_capacity(64 * 1024, File::open(path)?))
}

fn scan<R: Read + Seek>(file: &mut BufReader<R>) -> std::io::Result<Vec<u8>> {
    let mut out = Vec::with_capacity(64 * 1024);

    let mut soi = [0u8; 2];
    file.read_exact(&mut soi)?;
    if soi != [0xFF, 0xD8] {
        return Err(broken("SOI ではない"));
    }
    out.extend_from_slice(&soi);

    let mut byte = [0u8; 1];
    loop {
        file.read_exact(&mut byte)?;
        if byte[0] != 0xFF {
            return Err(broken("セグメント境界が 0xFF で始まっていない"));
        }
        // 0xFF のパディング（fill byte）を読み飛ばす
        let mut marker = 0xFF;
        while marker == 0xFF {
            file.read_exact(&mut byte)?;
            marker = byte[0];
        }
        match marker {
            // 長さを持たないマーカー
            0xD9 => {
                // APP1 も SOS も無いまま EOI へ着いた。**Err にしない** ──
                // 画素を持たない退化した JPEG でも「全項目 None」で成立する
                out.extend_from_slice(&[0xFF, 0xD9]);
                break;
            }
            0x01 | 0xD0..=0xD7 => continue,
            _ => {}
        }

        let mut len_bytes = [0u8; 2];
        file.read_exact(&mut len_bytes)?;
        let len = u16::from_be_bytes(len_bytes) as usize;
        if len < 2 {
            return Err(broken("セグメント長が短すぎる"));
        }
        let payload = len - 2;

        if marker == 0xDA {
            // SOS。ヘッダだけ写し、画素は 1 バイトも読まずに EOI で閉じる
            let mut body = vec![0u8; payload];
            file.read_exact(&mut body)?;
            out.extend_from_slice(&[0xFF, 0xDA]);
            out.extend_from_slice(&len_bytes);
            out.extend_from_slice(&body);
            out.extend_from_slice(&[0xFF, 0xD9]);
            break;
        }

        if marker == 0xE1 {
            // APP1 = Exif と XMP。**これだけが要る**
            let mut body = vec![0u8; payload];
            file.read_exact(&mut body)?;
            out.extend_from_slice(&[0xFF, marker]);
            out.extend_from_slice(&len_bytes);
            out.extend_from_slice(&body);
        } else {
            // **`Seek::seek` ではなく `seek_relative`。** 前者は BufReader の
            // バッファを無条件に捨てるので、飛ばした先を読むために同じ 64KB を
            // 読み直すことになる（spike の実測で 2 倍遅くなった原因。
            // spec の「計測の罠」3）
            file.seek_relative(payload as i64)?;
        }
    }
    Ok(out)
}
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cargo test -p picture-tool-core --lib metadata::header`
Expected: 8 tests passed

- [ ] **Step 5: コミット**

```bash
git add core/src/metadata/header.rs core/src/metadata/mod.rs
git commit -m "$(cat <<'EOF'
feat(core): 写真グリッドの絞り込みと並び替え: JPEG の先頭だけを写す軽量読み取りを足す

SOI から SOS までを辿り、APP1 だけを写した最小の JPEG を組み立てる。
Exif も XMP も SOS より前にあるので、★1 つのためにファイル全体（5MB）を
読む必要は無い ── 実測で 1,000 枚あたり 1,118ms → 68ms、
ピーク RSS 201MB → 7.4MB（spec の「実測」節）。

解析は一切しない。組み立てたバイト列は既存の read_metadata_bytes が
そのまま食える形にしてある。この機能は I/O 律速で、解析側を触る意味は
実測上まったく無かった。

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: 読み取りの唯一の入口 `read_metadata_fast`

Task 1 のバイト走査を `read_metadata_bytes` に繋ぎ、**★を読むところが全部通る 1 つの入口**にする。
段は 3 つ（spec §2）:

| 段 | 動き |
|---|---|
| 1 | 拡張子が JPEG でなければ `Ok`（全項目 `None`）で返す |
| 2 | `read_metadata_header` で先頭だけ読む。`Ok` ならそれを返す |
| 3 | `Err` なら `read_metadata`（全体読み）へ**フォールバック**する |

フォールバックは寛容さを落とさないために要る。`read_metadata_header` は手書きの
セグメント走査なので歪んだ JPEG で `Err` を返すが、いまの `read_metadata` は
`img_parts` / `little_exif` にバイト列ごと渡すので多少歪んでいても読める。
1 本化を素直にやると**いままで情報パネルで開けていた写真がエラートーストになる**。

**あわせて既存の 2 つの呼び出し元も切り替える**（spec §2「呼び出し元は 3 つ」）。
3 つ目（`list_images`）は Task 3。`write_image_metadata` の読み戻しを外してはならない ──
それが★の書き戻しの「値の出所」であり、ここだけ経路が違うと
「保存した直後だけ正しい★／フォルダーを開き直すと違う★」になる。

**Files:**
- Modify: `core/src/metadata/mod.rs`（`read_metadata` の直後、`:162` あたりに追加）
- Modify: `gui/src/commands.rs:686`（`read_image_metadata`）、`gui/src/commands.rs:728`（`write_image_metadata` の読み戻し）
- Test: `core/src/metadata/mod.rs` の `mod tests`

**Interfaces:**
- Consumes: `header::header_bytes(&Path) -> std::io::Result<Vec<u8>>`（Task 1）
- Produces:
  - `pub fn read_metadata_header(path: &Path) -> anyhow::Result<MetadataRead>`
  - `pub fn read_metadata_fast(path: &Path) -> anyhow::Result<MetadataRead>`
  ── Task 3 の `list_images` が `read_metadata_fast` を呼ぶ

- [ ] **Step 1: 失敗するテストを書く**

`core/src/metadata/mod.rs` の `mod tests` の末尾に足す。

```rust
    /// **JPEG については `read_metadata_fast` と `read_metadata` の結果は常に一致する。**
    ///
    /// 先頭経路が成功すれば同じ `read_metadata_bytes` に同じ APP1 を渡しており、
    /// 失敗すれば `read_metadata` そのものを呼ぶので、構成上そうなる。
    #[test]
    fn fast_and_full_reads_agree_on_jpeg() {
        let (_dir, path) = scratch("lightroom-xmp.jpg");
        write_metadata(
            &path,
            &EditableMetadata {
                title: Some("夕暮れの港".to_string()),
                comment: Some("テスト".to_string()),
                rating: Some(4),
            },
        )
        .unwrap();

        let full = read_metadata(&path).unwrap();
        // 前提条件: 比較対象が default でないこと。両方 default なら
        // `read_metadata_fast` が中身を読まずに Ok(None) を返しても一致してしまう
        assert_eq!(full.metadata.rating, Some(4), "前提条件が崩れている");
        assert_eq!(full.metadata.title.as_deref(), Some("夕暮れの港"));

        assert_eq!(read_metadata_fast(&path).unwrap().metadata, full.metadata);
    }

    /// 既存 fixture 2 つでも一致する（片方は Exif のみ、片方は XMP 付き）
    #[test]
    fn fast_and_full_reads_agree_on_the_fixtures() {
        for name in ["exif-only.jpg", "lightroom-xmp.jpg"] {
            let path = fixture(name);
            assert_eq!(
                read_metadata_fast(&path).unwrap().metadata,
                read_metadata(&path).unwrap().metadata,
                "{name} で食い違った"
            );
        }
    }

    /// **フォールバックが効くこと。**
    ///
    /// 前提条件として `read_metadata_header` 単体が `Err` になることを先に assert する ──
    /// 入力が実は歪んでいなければ、フォールバックを消しても緑のままになる。
    #[test]
    fn a_jpeg_the_header_scan_cannot_parse_falls_back_to_the_full_read() {
        let (_dir, path) = scratch("lightroom-xmp.jpg");
        // **読める中身を先に入れる。** 空の写真で比べると、フォールバックを消して
        // Err を握り潰す実装でも「両方 default」で一致してしまう
        write_metadata(
            &path,
            &EditableMetadata {
                rating: Some(4),
                ..Default::default()
            },
        )
        .unwrap();

        // SOI の直後にゴミを 2 バイト差し込み、セグメント境界を壊す。
        // `img_parts` は APP1 に辿り着けるが、手書きの走査は 0xFF を期待して落ちる
        // （この非対称は実測で確認済み。計画の Task 2 の実験ログ参照）
        let mut raw = std::fs::read(&path).unwrap();
        raw.splice(2..2, [0x00, 0x00]);
        std::fs::write(&path, &raw).unwrap();

        // 前提条件 1: 先頭だけの走査は失敗する
        assert!(
            read_metadata_header(&path).is_err(),
            "前提条件が崩れている: 壊したはずの JPEG を先頭走査が読めてしまった"
        );
        // 前提条件 2: 全体読みは成功し、★も生きている（＝フォールバック先が生きている）
        let full = read_metadata(&path).unwrap();
        assert_eq!(full.metadata.rating, Some(4), "前提条件が崩れている");

        assert_eq!(read_metadata_fast(&path).unwrap().metadata, full.metadata);
    }

    /// **非 JPEG は意図的に非対称にする。**
    ///
    /// `list_images` は PNG / WebP も並べる。★の読み取りは一覧表示の副作用なので、
    /// PNG が混ざっているだけで一覧が失敗したり警告が出たりしてはいけない。
    #[test]
    fn non_jpeg_is_deliberately_asymmetric() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("photo.png");
        std::fs::write(&path, [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]).unwrap();

        // 全体読みは「JPEG として読めません」で Err（現行のまま）
        assert!(read_metadata(&path).is_err());
        // 入口は Ok かつ全項目 None
        let read = read_metadata_fast(&path).unwrap();
        assert_eq!(read.metadata, EditableMetadata::default());
        assert!(read.warnings.is_empty());
    }

    /// **APP1 を持たない JPEG は、フォールバックへ落ちずに `Ok` かつ全項目 `None`。**
    ///
    /// これを `read_metadata_fast` で見てはならない ── 先頭走査が組み立てた
    /// バイト列を `read_metadata_bytes` が受け取れなくても、フォールバックが
    /// 全体読みで拾ってしまい**テストは緑のまま通る**。落ちていないことを
    /// 確かめたいので `read_metadata_header` 単体を見る。
    /// （本ツールが変換して出力した JPEG がこの形。`_processed` フォルダーを
    /// 開くと全写真がここを通る）
    #[test]
    fn a_jpeg_without_app1_reads_as_none_without_falling_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("no-app1.jpg");
        // SOI ＋ SOS ヘッダ ＋ 画素 ＋ EOI。APP1 も APP0 も持たない
        let mut raw = vec![0xFF, 0xD8, 0xFF, 0xDA, 0x00, 0x0C];
        raw.extend_from_slice(b"sos-header");
        raw.extend_from_slice(&[0x11; 256]);
        raw.extend_from_slice(&[0xFF, 0xD9]);
        std::fs::write(&path, &raw).unwrap();

        let read = read_metadata_header(&path).unwrap();
        assert_eq!(read.metadata, EditableMetadata::default());
        // **警告も出さない。** 「Exif が無い」と「Exif が壊れている」は別である
        assert!(read.warnings.is_empty(), "警告が出た: {:?}", read.warnings);
    }

    /// 拡張子が `.jpg` なのに中身が JPEG ですらないファイル。
    /// **先頭も全体も読めないので `Err`。panic はしない**
    #[test]
    fn a_file_that_is_not_a_jpeg_at_all_errors_without_panicking() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("liar.jpg");
        std::fs::write(&path, b"this is not a jpeg at all").unwrap();

        assert!(read_metadata_header(&path).is_err()); // 前提条件
        assert!(read_metadata_fast(&path).is_err());
    }
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cargo test -p picture-tool-core --lib metadata::tests`
Expected: コンパイルエラー（`cannot find function 'read_metadata_fast'` / `'read_metadata_header'`）

- [ ] **Step 3: 最小の実装を書く**

`core/src/metadata/mod.rs` の `read_metadata_bytes`（`:159-162`）の直後に足す。
`mod header;` は Task 1 で追加済み。

```rust
/// 拡張子だけで JPEG かどうかを見る。**中身は見ない** ──
/// ここは「読みに行くかどうか」の分岐であって、正しさの判定ではない
/// （中身の検査は `header_bytes` の SOI 検査と `read_metadata` がやる）。
/// 条件は `gui/src/security.rs` の `readable_jpeg` と同じ。
fn has_jpeg_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("jpg") || ext.eq_ignore_ascii_case("jpeg"))
}

/// JPEG の先頭（SOI〜SOS）だけから、人が編集する 3 項目を読む。
///
/// **解析の実装は一切変えない。** `read_metadata` と同じ [`read_metadata_bytes`] に、
/// APP1 だけを写したバイト列を渡す。Exif と XMP の優先順位・警告・trim 規則は
/// `docs/superpowers/specs/2026-08-18-metadata-editing-design.md` のまま。
///
/// 手書きのセグメント走査なので、歪んだ JPEG では `Err` になる。
/// **直接呼ばず [`read_metadata_fast`] を使うこと**（フォールバックがそこにある）。
pub fn read_metadata_header(path: &Path) -> Result<MetadataRead> {
    let raw = header::header_bytes(path)
        .with_context(|| format!("JPEG の先頭を読めません: {}", path.display()))?;
    read_metadata_bytes(raw, path)
}

/// **読み取りの唯一の入口。★を読むところは全部これを使う。**
///
/// 呼び出し元は 3 つ ── `list_images`（グリッドの★）、`read_image_metadata`
/// （情報パネル）、`write_image_metadata` の読み戻し。**1 つでも外すと、
/// 「保存した直後だけ正しい★／フォルダーを開き直すと違う★」という、
/// 再現条件が「保存を挟んだかどうか」になる差が出る。**
///
/// | 段 | 動き |
/// |---|---|
/// | 1 | 拡張子が JPEG でなければ `Ok`（全項目 `None`） |
/// | 2 | [`read_metadata_header`] で先頭だけ読む。`Ok` ならそれを返す |
/// | 3 | `Err` なら [`read_metadata`]（全体読み）へフォールバック |
///
/// **段 3 が要る理由**: 手書きの走査は「境界が 0xFF で始まっていない」等で `Err` を
/// 返すが、`read_metadata` は `img_parts` / `little_exif` にバイト列ごと渡すので
/// 多少歪んでいても APP1 に辿り着けば読める。フォールバックが無いと、
/// **いままで情報パネルで開けていた写真がエラートーストになる**（編集して保存する
/// 導線ごと塞がる）。落ちるのは歪んだファイルだけなので、性能は現行以上に悪くならない。
///
/// **段 1 は意図的に `read_metadata` と非対称**（`read_metadata` は非 JPEG で `Err`）。
/// ★の読み取りは一覧表示の副作用なので、PNG が混ざっているだけで一覧が失敗したり
/// 警告が出たりしてはいけない。段 1 に到達するのは `list_images` からの呼び出しだけで、
/// 情報パネル側は `security::readable_jpeg` が先に非 JPEG を弾く。
pub fn read_metadata_fast(path: &Path) -> Result<MetadataRead> {
    if !has_jpeg_extension(path) {
        return Ok(MetadataRead::default());
    }
    match read_metadata_header(path) {
        Ok(read) => Ok(read),
        Err(_) => read_metadata(path),
    }
}
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cargo test -p picture-tool-core --lib metadata::tests`
Expected: 既存 ＋ 新規 6 件がすべて PASS

- [ ] **Step 5: `read_metadata_bytes` の doc を実態に合わせる**

`core/src/metadata/mod.rs:155-159` はいま `/// **spike 用（暫定）**` ＋ `#[doc(hidden)]` である。
Task 2 でこれは `read_metadata` と `read_metadata_header` の**共通の下地**になるので、
「暫定」の表記を落とす。**`#[doc(hidden)]` は残す**（バイト列を直に渡す API は
呼び出し側が JPEG の構造を保証する必要があり、外向けの入口は
`read_metadata` / `read_metadata_fast` の 2 つだけにしたい）。

```rust
/// 読み込み済みのバイト列から同じ 3 項目を読む。`path` はエラーメッセージのためだけに使う。
///
/// **`read_metadata`（全体読み）と `read_metadata_header`（先頭だけ読む）の共通の下地。**
/// 解析の実装がここ 1 箇所にあるので、「読む量」を変えても Exif と XMP の
/// 優先順位・警告・trim 規則は変わらない。
///
/// バイト列が JPEG として妥当であることは呼び出し側の責任なので、
/// 外向けの入口にはしない（`read_metadata` か `read_metadata_fast` を使う）。
#[doc(hidden)]
pub fn read_metadata_bytes(raw: Vec<u8>, path: &Path) -> Result<MetadataRead> {
```

- [ ] **Step 6: 既存の呼び出し元 2 つを入口に通す**

`gui/src/commands.rs:686`（`read_image_metadata` の中）:

```rust
        core::metadata::read_metadata_fast(&file).map_err(|e| e.to_string())
```

`gui/src/commands.rs:728`（`write_image_metadata` の読み戻し）:

```rust
        core::metadata::write_metadata(&file, &metadata).map_err(|e| e.to_string())?;
        // **読み戻しも入口を通す。** ここだけ経路が違うと、★の書き戻しの
        // 「値の出所」（絞り込み spec §5）がグリッドの★と別経路になる
        core::metadata::read_metadata_fast(&file).map_err(|e| e.to_string())
```

- [ ] **Step 7: 全体の検証**

Run: `make check`
Expected: exit 0

- [ ] **Step 8: コミット**

```bash
git add core/src/metadata/mod.rs gui/src/commands.rs
git commit -m "$(cat <<'EOF'
feat(core): 写真グリッドの絞り込みと並び替え: 読み取りの入口を read_metadata_fast に 1 本化する

拡張子で非 JPEG を弾き、先頭だけ読み、失敗したら全体読みへ落ちる。
★を読むところは全部ここを通す ── 情報パネルも、保存の読み戻しも。
1 つでも外すと「保存した直後だけ正しい★／開き直すと違う★」という、
再現条件が「保存を挟んだかどうか」になる差が出る。

フォールバックは寛容さを落とさないために要る。手書きのセグメント走査は
歪んだ JPEG で Err を返すが、img_parts / little_exif は APP1 に辿り着けば
読める。無いと、いままで情報パネルで開けていた写真がエラートーストになる。

非 JPEG は意図的に非対称（read_metadata は Err、read_metadata_fast は
Ok かつ全項目 None）。★の読み取りは一覧表示の副作用なので、PNG が
混ざっているだけで一覧が失敗してはいけない。

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: `list_images` に★を畳み、`par_iter` にし、同値順を決定的にする

★の出所を `list_images` 1 本にする。**並列化とセットで入れることがこの設計の条件**
（spec §1）── 逐次のままだと 1,000 枚で 576 → 650ms の純粋な悪化になり、
`list_images` が返るまでグリッドは空なので、フォルダーを開いた瞬間の停止時間がそのまま伸びる。
並列化すれば 169ms で**現行（576ms）より速くなる**。

**同値の順序も直す。** `entries.sort_by_key(|a| a.name.to_lowercase())` は安定ソートなので、
同じ小文字名を持つ `A.jpg` と `a.jpg` の相対順は `read_dir` が返すファイルシステム順のまま。
**これは今日すでに実行ごとに変わりうる既存の欠陥**で、並列化が持ち込むものではない。

`list_images` は `warnings` を捨てる（spec §1 の決定）── 3,000 枚ぶんの警告は
利用者に見せる形が無く、同じ写真を情報モードで開けば `read_image_metadata` が
同じ警告を 1 枚分だけ返す。**情報が失われるのではなく、出す場所が 1 箇所に寄る。**

**Files:**
- Modify: `gui/src/types.rs:12-25`（`ImageEntry` に `rating`）、`gui/src/types.rs:241-257`（contract の `imageEntry`）
- Modify: `gui/src/commands.rs:89-146`（`list_images`）
- Modify: `gui-frontend/src/lib/types.ts:15-28`（`ImageEntry` に `rating`）
- Test: `gui/src/commands.rs` の `mod tests`

**Interfaces:**
- Consumes: `core::metadata::read_metadata_fast(&Path) -> anyhow::Result<MetadataRead>`（Task 2）
- Produces:
  - Rust: `ImageEntry { name, path, width, height, size_bytes, modified_ms, created_ms, rating: Option<u8> }`
  - Rust: `fn collect_image_entries(dir: &Path) -> Result<Vec<ImageEntry>, String>` / `fn sort_entries(entries: &mut [ImageEntry])`
  - TS: `ImageEntry.rating: number | null`（1〜5。null は未評価または読めなかった）

- [ ] **Step 1: 失敗するテストを書く**

`gui/src/commands.rs` の `mod tests` の末尾に足す。`write_real_jpeg`（`:839`）は既存のヘルパ。

**なぜ `sort_entries` を単体で検査するのか**: 「`read_dir` の順が実行ごとに変わること」に
依存した検査は、たまたま順序が安定していると緑のまま通る。入力の並びを 2 通り
こちらで作って**同じ出力になること**を見れば、現行実装（安定ソート）では確実に落ちる。

```rust
    fn entry_named(name: &str) -> ImageEntry {
        ImageEntry {
            name: name.to_string(),
            path: format!("/photos/{name}"),
            width: 4000,
            height: 3000,
            size_bytes: 4_500_000,
            modified_ms: Some(1_755_000_000_000),
            created_ms: None,
            rating: None,
        }
    }

    /// **同じ小文字名を持つ 2 枚の相対順が、入力の並びに依存しない。**
    ///
    /// 現行の `sort_by_key(|a| a.name.to_lowercase())` は安定ソートなので、
    /// `A.jpg` と `a.jpg` の順は `read_dir` が返したファイルシステム順のままになる。
    /// `par_iter` にしても収集順は保たれる（`IndexedParallelIterator`）ので、
    /// これは並列化が持ち込む問題ではなく、**ついでに直す既存の欠陥**である。
    #[test]
    fn entries_with_the_same_lowercased_name_are_ordered_deterministically() {
        let mut forward = vec![entry_named("A.jpg"), entry_named("a.jpg")];
        let mut backward = vec![entry_named("a.jpg"), entry_named("A.jpg")];
        // 前提条件: 入力は実際に別の並びである（同じものを 2 回並べていない）
        assert_ne!(forward[0].name, backward[0].name);

        sort_entries(&mut forward);
        sort_entries(&mut backward);

        let names = |v: &[ImageEntry]| v.iter().map(|e| e.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(&forward), names(&backward));
        // 小文字化した名前で同値なら、元の名前の昇順で決める
        assert_eq!(names(&forward), vec!["A.jpg".to_string(), "a.jpg".to_string()]);
    }

    /// 小文字化した比較が第 1 キーであること（`Z.jpg` が `a.jpg` より前に来ない）
    #[test]
    fn entries_are_ordered_case_insensitively_first() {
        let mut entries = vec![entry_named("Z.jpg"), entry_named("a.jpg")];
        sort_entries(&mut entries);
        assert_eq!(entries[0].name, "a.jpg");
    }

    /// ★がファイルから読めて `ImageEntry` に載る
    #[test]
    fn list_reads_the_rating_from_the_file() {
        let dir = TempDir::new().unwrap();
        let rated = dir.path().join("rated.jpg");
        write_real_jpeg(&rated);
        core::metadata::write_metadata(
            &rated,
            &core::metadata::EditableMetadata {
                rating: Some(4),
                ..Default::default()
            },
        )
        .unwrap();
        let plain = dir.path().join("plain.jpg");
        write_real_jpeg(&plain);

        let entries = collect_image_entries(dir.path()).unwrap();

        assert_eq!(entries.len(), 2);
        let rating_of = |name: &str| {
            entries
                .iter()
                .find(|e| e.name == name)
                .unwrap_or_else(|| panic!("{name} が一覧に無い"))
                .rating
        };
        assert_eq!(rating_of("rated.jpg"), Some(4));
        assert_eq!(rating_of("plain.jpg"), None);
    }

    /// **★の読み取り失敗で一覧ごと失敗しない。**
    ///
    /// グリッドは PNG / WebP も並べるし、壊れた JPEG も混ざる。
    /// ★は一覧表示の副作用であって、一覧そのものの前提ではない
    #[test]
    fn list_survives_files_whose_rating_cannot_be_read() {
        let dir = TempDir::new().unwrap();
        write_real_jpeg(&dir.path().join("ok.jpg"));
        fs::write(dir.path().join("broken.jpg"), b"not a jpeg at all").unwrap();
        fs::write(
            dir.path().join("shot.png"),
            [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A],
        )
        .unwrap();

        let entries = collect_image_entries(dir.path()).unwrap();

        // 前提条件: 3 枚とも一覧に載っている（壊れた分が消えていない）
        assert_eq!(entries.len(), 3);
        for entry in &entries {
            if entry.name != "ok.jpg" {
                assert_eq!(entry.rating, None, "{} で★が付いた", entry.name);
            }
        }
    }
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cargo test -p picture-tool-gui --lib commands`
Expected: コンパイルエラー（`cannot find function 'sort_entries'` / `'collect_image_entries'`、
`ImageEntry` に `rating` フィールドが無い）

- [ ] **Step 3: `ImageEntry` に `rating` を足す**

`gui/src/types.rs:12-25` の `ImageEntry` の末尾に:

```rust
    /// ★（1〜5）。`None` は未評価、または読めなかった。
    ///
    /// **`list_images` が唯一の出所。** 以前は可視範囲ぶんを別コマンド
    /// （`read_image_ratings`）で取りに行っていたが、先頭だけ読む軽量経路
    /// （`core::metadata::read_metadata_fast`）ができたので一覧に畳んだ。
    /// 絞り込みに使うには**フォルダー全件**が揃っている必要があり、
    /// 可視範囲を起点にする形では原理的に足りない
    pub rating: Option<u8>,
```

`gui/src/types.rs` の contract entry（`:241-257`）の `ImageEntry` リテラルにも
`rating: Some(3),` を足す。**★あり側を契約に残す**（`created_ms: None` が
「取れない環境の綴りを契約に残す」ためであるのと同じ理由で、
`Option<u8>` が `null` ではなく数値になる綴りをここで固定する）。

- [ ] **Step 4: `list_images` を割って並列化する。ただしソートは現行のまま**

**同値順の検査が「現行の欠陥を捕まえた」ことを目で見るため、ソートだけ直さない。**
spec §1 は「検査は並列化の前に落ちることを確認できる」と書いており、
完成形をいきなり書くと `entries_with_the_same_lowercased_name_are_ordered_deterministically`
が空振りしていても分からない。

`gui/src/commands.rs:89-146` を差し替える。

```rust
/// 一覧の順序。**この 1 段比較は Step 6 で直す**（現行 `:141` からそのまま移した形）。
fn sort_entries(entries: &mut [ImageEntry]) {
    entries.sort_by_key(|a| a.name.to_lowercase());
}

/// フォルダー直下の画像を `ImageEntry` にする。**再帰しない。**
///
/// **`par_iter` であることが設計の一部。** 1 枚あたり `image_dimensions_oriented`
/// ＋ `fs::metadata` ＋ ★の読み取りで open が 3 回になる。逐次のままだと
/// 1,000 枚で 576 → 650ms の悪化になり、`list_images` が返るまでグリッドは空なので
/// フォルダーを開いた瞬間の停止時間がそのまま伸びる。並列なら 169ms で現行より速い
/// （実測は絞り込み spec の「実測」節）。
///
/// **順序は崩れない。** `&Vec<T>` の `par_iter()` は `IndexedParallelIterator` で、
/// `map` は indexed 性を保ち、`Vec` への `collect` は index どおりに詰める。
///
/// コマンド本体（`#[tauri::command]` かつ `async`）は単体テストから素直に呼べないので、
/// 中身をここへ出してテスト可能にしている（`metadata_target` と同じ形）。
fn collect_image_entries(dir: &Path) -> Result<Vec<ImageEntry>, String> {
    use rayon::prelude::*;

    let read_dir = fs::read_dir(dir).map_err(|e| e.to_string())?;
    let direct_files: Vec<PathBuf> = read_dir
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && core::is_supported_image(p))
        .collect();

    let mut entries: Vec<ImageEntry> = direct_files
        .par_iter()
        .map(|file_path| {
            let name = file_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let path_str = file_path.to_string_lossy().to_string();

            // 生ピクセルの縦横は Orientation 5-8 で入れ替わるため、表示値も揃える。
            let (width, height) = core::image_dimensions_oriented(file_path).unwrap_or_default();
            // 1 度の stat から大きさも日時も取る。並び順のために stat をもう 1 回
            // 回すと、3,000 枚のフォルダーで走査量が倍になる
            let meta = fs::metadata(file_path).ok();
            let size_bytes = meta.as_ref().map(|m| m.len()).unwrap_or(0);
            let modified_ms = meta
                .as_ref()
                .and_then(|m| m.modified().ok())
                .and_then(epoch_ms);
            let created_ms = meta
                .as_ref()
                .and_then(|m| m.created().ok())
                .and_then(epoch_ms);

            // **★は読めなくても一覧を落とさない。** グリッドの飾りであって、
            // 一覧そのものの前提ではない（PNG も壊れた JPEG も並ぶ）。
            // **`warnings` はここで捨てる** ── 3,000 枚ぶんの警告は利用者に
            // 見せる形が無い。同じ写真を情報モードで開けば `read_image_metadata`
            // が同じ警告を 1 枚分だけ返すので、情報は失われず出す場所が寄るだけ
            let rating = core::metadata::read_metadata_fast(file_path)
                .ok()
                .and_then(|read| read.metadata.rating);

            ImageEntry {
                name,
                path: path_str,
                width,
                height,
                size_bytes,
                modified_ms,
                created_ms,
                rating,
            }
        })
        .collect();

    sort_entries(&mut entries);
    Ok(entries)
}

#[tauri::command]
pub async fn list_images(path: String) -> Result<Vec<ImageEntry>, String> {
    let dir = security::existing_dir(&path)?;
    tokio::task::spawn_blocking(move || collect_image_entries(&dir))
        .await
        .map_err(|e| e.to_string())?
}
```

- [ ] **Step 5: 同値順の検査だけが落ちることを確認する**

Run: `cargo test -p picture-tool-gui --lib commands`
Expected: **`entries_with_the_same_lowercased_name_are_ordered_deterministically` だけが FAIL。**
残り 3 件（`entries_are_ordered_case_insensitively_first` /
`list_reads_the_rating_from_the_file` / `list_survives_files_whose_rating_cannot_be_read`）は PASS。

落ちるのは `sort_by_key(|a| a.name.to_lowercase())` が安定ソートで、
`[A.jpg, a.jpg]` と `[a.jpg, A.jpg]` が入力の順のまま出てくるためである。
**この赤を目で見てから Step 6 へ進む。**

- [ ] **Step 6: ソートを 2 段比較に直す**

```rust
/// 一覧の順序。**小文字化した名前 → 元の名前**の 2 段で比較する。
///
/// 1 段だけだと安定ソート任せになり、同じ小文字名を持つ `A.jpg` と `a.jpg` の
/// 相対順が `read_dir` の返すファイルシステム順のままになる（＝実行ごとに変わりうる）。
/// フロントの `sortImages` が同値を名前の昇順で解決しているのと同じ規則に揃える。
fn sort_entries(entries: &mut [ImageEntry]) {
    entries.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.name.cmp(&b.name))
    });
}
```

Run: `cargo test -p picture-tool-gui --lib commands`
Expected: 既存 ＋ 新規 4 件がすべて PASS

- [ ] **Step 7: DTO 契約を再生成し、TS 側の型を合わせる**

```bash
UPDATE_DTO_CONTRACT=1 cargo test -p picture-tool-gui contract
```

`gui-frontend/src/lib/types.ts` の `ImageEntry` の末尾に足す（**手で写す側。
`dto.contract.ts` は自動生成物なので触らない**）:

```ts
  /**
   * ★（1〜5）。null は未評価、または読めなかった。
   *
   * **`list_images` が唯一の出所。** 絞り込みにはフォルダー全件が揃っている
   * 必要があり、可視範囲を起点にする形では原理的に足りない
   */
  rating: number | null;
```

`git diff gui-frontend/src/lib/dto.contract.ts` で `rating` が生えたことを目で確認する
（**再生成を忘れると Rust ⇄ TS の食い違いが型検査を素通りする**。spec 未検証 6）。

- [ ] **Step 8: 全体の検証**

Run: `make check`
Expected: exit 0。TypeScript が `ImageEntry` を組み立てている箇所
（`gui-frontend/src/lib/browser/gridFilter.test.ts:20` のヘルパ）は `rating` 必須になるので、
`rating: null` を既定として足す。

- [ ] **Step 9: コミット**

```bash
git add gui/src/types.rs gui/src/commands.rs gui-frontend/src/lib/types.ts gui-frontend/src/lib/dto.contract.ts gui-frontend/src/lib/browser/gridFilter.test.ts
git commit -m "$(cat <<'EOF'
feat(gui): 写真グリッドの絞り込みと並び替え: list_images に★を畳み、並列化する

★の出所を list_images 1 本にする。絞り込みにはフォルダー全件の★が要り、
可視範囲を起点にする形では原理的に足りない。

並列化はセットで入れる。逐次のままだと 1,000 枚で 576 → 650ms の
純粋な悪化になり、list_images が返るまでグリッドは空なのでフォルダーを
開いた瞬間の停止時間がそのまま伸びる。par_iter なら 169ms で現行より速い。

同値の順序も直した。sort_by_key(name.to_lowercase()) は安定ソートなので、
A.jpg と a.jpg の相対順が read_dir のファイルシステム順のままだった
（＝今日すでに実行ごとに変わりうる）。小文字名 → 元の名前の 2 段にする。
並列化が持ち込む問題ではなく、ついでに直す既存の欠陥である。

warnings はここで捨てる。3,000 枚ぶんの警告は見せる形が無く、同じ写真を
情報モードで開けば 1 枚分が返る。出す場所が 1 箇所に寄るだけで情報は消えない。

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: ★の出所を `ImageEntry` へ移し、`read_image_ratings` 経路を消す

Task 3 で★は `list_images` に乗った。ここで**古い経路を丸ごと落とす**（spec §1 の
「消えるもの / 直すもの」）。信頼境界は増えず**減る** ── `MAX_BATCH` は「パスの配列を
受ける」ことに由来する防御で、ディレクトリ 1 本を受ける `list_images` には元々不要。

**保存後の反映もここで入れ替える。** ストアの `invalidate` / `request` が無くなるので、
`images` の該当 `ImageEntry.rating` を**その場で書き換える**。`list_images` は呼び直さない
── スクロール位置と選択が飛ぶ。**値は保存の応答から採る**（送った下書きの値ではない）。
Rust 側の正規化（★の範囲外 → `None`）と食い違うと、グリッドの★がディスクに無い値になる。

**★は情報モードに入らなくても常時出るようになる**（裁定 X1 の反転。spec §1）。
これを許容する ── ★は「どれを変換するか」を選ぶための情報でもあり、変換モードで
見えない方が不自然である。並列化により変換モードだけの利用者も現行より速くなる。

**Files:**
- Delete: `gui-frontend/src/lib/browser/ratings.svelte.ts`
- Modify: `gui-frontend/src/lib/browser/PhotoGrid.svelte:41,70,341` / `gui-frontend/src/App.svelte` / `gui-frontend/src/lib/api.ts:147-149` / `gui-frontend/src/lib/types.ts`
- Modify: `gui/src/commands.rs:788-826`（`read_image_ratings` 削除）/ `gui/src/types.rs`（`RatingEntry` と contract entry 削除）/ `gui/src/main.rs:128` / `gui/src/security.rs:48-53`
- Test: `gui-frontend/e2e/stub.ts` / `gui-frontend/e2e/metadata.spec.ts` / `gui-frontend/e2e/gridFilter.spec.ts`

**Interfaces:**
- Consumes: `ImageEntry.rating: number | null`（Task 3）
- Produces: `PhotoGrid` から `ratingFor` prop が消える。`App.svelte` の `images` が★の唯一の保持先

- [ ] **Step 1: スタブの★を `list_images` へ移す（先にテスト側を実態に合わせる）**

`gui-frontend/e2e/stub.ts` の `POOL_SIZE` の定義の近く（`indexOfPath` の直後）に足す:

```ts
    /**
     * フォルダーを開いた時点で付いている★。**`list_images` が返す。**
     *
     * ★あり・なしを混ぜ、**名前順とも更新日順とも一致しない並び**にする ──
     * 「名前で並べたまま★の条件だけを見ている」実装がすり抜けられないため。
     * **index 0 は必ず未評価**にしてある（`metadata.spec.ts` の
     * 「保存した★がグリッドのタイルにも出る」が「まだ★が付いていない」を
     * 前提条件に取っているので、そこを崩さない）。
     */
    const RATING_PATTERN: (number | null)[] = [
      null, 5, 3, null, 1, 4, null, 2, 5, null, 3, null,
    ];

    /** `seedRatings` の指定が勝ち、無ければ既定パターン */
    function ratingOf(index: number): number | null {
      return seedRatings[index] ?? RATING_PATTERN[index % RATING_PATTERN.length];
    }
```

`imagesFor` を差し替える（`seedRatings` を `stored` へ書き込むループは消える）:

```ts
    /** フォルダーごとに別のパスを返す。同じパスを返すと、フォルダーを変えても
     *  キャッシュが当たってしまい「取り直している」ことを検査できない */
    function imagesFor(folder: string) {
      const dir = folder.replace(/\/+$/, "") || "/photos";
      return Array.from({ length: imageCount }, (_, i) => {
        const path = `${dir}/${i}.jpg`;
        const saved = stored.get(path) as { rating: number | null } | undefined;
        return {
          ...template.image,
          name: `photo-${String(i).padStart(4, "0")}.jpg`,
          path,
          // 更新日は**隣り合う 2 枚を入れ替えた順**に振る（0↔1, 2↔3, ...）。
          // 名前順・その逆順のどちらとも一致しない並びなので、
          // 「名前で並べたまま向きだけ変えている」実装が更新日順のテストを
          // すり抜けられない。
          // 作成日は取れない環境（Linux の一部 FS）を再現して null のまま
          modified_ms: ((i ^ 1) + 1) * 60_000,
          // 保存済みがあればそれが勝つ（保存 → フォルダーを開き直す経路の再現）
          rating: saved ? saved.rating : ratingOf(i),
        };
      });
    }
```

`read_image_metadata` ハンドラを、一覧の★と食い違わないようにする:

```ts
      // 形は契約（template.metadata = dto.metadataRead）から取り、値だけ上書きする。
      // { metadata, warnings } と手で組むと境界の写しが3つ目に増える。
      // **未保存の写真でも★は返す** ── そうしないと、タイルには★が出ているのに
      // パネルは 0 という、実機では起きない食い違いをスタブが作ってしまう
      read_image_metadata: ({ path }: { path: string }) => ({
        ...template.metadata,
        metadata:
          stored.get(path) ??
          { ...template.emptyMetadata, rating: ratingOf(indexOfPath(path)) },
        warnings: stored.has(path) ? [] : metadataWarnings,
      }),
```

**`read_image_ratings` ハンドラ（`:320-324`）はここでは消さない。** フロントは Step 4 まで
`readImageRatings` を呼び続けるので、先に消すと Step 3 で
「期待した 1 件」以外にも赤が出て、どれが期待した赤なのか判定できなくなる
（`metadata.spec.ts:404` の「保存した★がグリッドのタイルにも出る」は
`ratings.request` の応答で★を出しているため一緒に落ちる）。削除は Step 5。

`StubOptions.seedRatings` の doc コメントを実態に合わせる:

```ts
  /**
   * 特定の写真の★を既定パターンから差し替える。
   * 画像のインデックス（`list_images` が返す 0 始まりの並び順）→ 評価値（1〜5）。
   *
   * 既定は `RATING_PATTERN`（★あり・なしを混ぜたもの）。
   * 「index 0 に★が付いている」のような、既定と違う条件を作るときに使う
   */
  seedRatings?: Record<number, number>;
```

- [ ] **Step 2: 落ちる既存 e2e を先に直す**

`gui-frontend/e2e/gridFilter.spec.ts` の `tileNames`（`:19-26`）:

```ts
/**
 * 並び順の検査用にタイルの**名前だけ**を取り出す。
 *
 * `tileLabel()`（`gridMetrics.ts`）は★と未保存印を aria-label に連結するので、
 * 生の aria-label と完全一致させると、スタブの★が変わるたびに並び順のテストが
 * 巻き添えで落ちる。ここで見たいのは順序であって★ではない
 */
const tileNames = async (page: import("@playwright/test").Page, count: number) => {
  const options = gridOf(page).getByRole("option");
  const names: string[] = [];
  for (let at = 0; at < count; at += 1) {
    const label = (await options.nth(at).getAttribute("aria-label")) ?? "";
    names.push(label.split(" ")[0]);
  }
  return names;
};
```

`gui-frontend/e2e/metadata.spec.ts:404` の前提条件コメントを実態に合わせる:

```ts
  test("保存した★がグリッドのタイルにも出る（刷新 spec §4-3）", async ({ page }) => {
    const tile = page.getByRole("listbox", { name: "写真" }).getByRole("option").first();
    // 前提条件: この写真はスタブの既定パターンで未評価（＝★が出ていない）。
    // ★は list_images が返すので、付いていれば開いた瞬間から出ている
    await expect(tile.getByText("★", { exact: false })).toBeHidden();
```

`gui-frontend/e2e/metadata.spec.ts:427` の裁定 X1 のテストを**書き換える**。
★が `list_images` に乗ったことで、この主張は「モード切替で出る」から
「**開いた瞬間に出る**」へ強くなる。「情報モードに入る前は★が出ていない」という
前提条件はもう成立しないので、テストごと置き換える:

```ts
  test("フォルダーを開いた瞬間に★がタイルに出る（保存もモード切替も経由しない）", async ({
    page,
  }) => {
    // beforeEach とは別条件（index 0 に★を仕込む）なのでこのテスト内で貼り直す。
    // ★は `list_images` が返すので、情報モードへ入る必要も保存する必要も無い ──
    // 絞り込み spec §1 の「裁定 X1 の反転」。可視範囲起点の $effect は消えており、
    // この検査はその経路が戻ってこないことの歯止めでもある
    await installTauriStub(page, { imageCount: 12, seedRatings: { 0: 3 } });
    await page.goto("/");

    const tile = page.getByRole("listbox", { name: "写真" }).getByRole("option").first();
    // 前提条件: フォルダーを開く前はタイルそのものが無い
    // （「最初から出ていた」で緑にならないようにする）
    await expect(tile).toHaveCount(0);

    await page.getByRole("button", { name: "photos", exact: false }).first().click();

    // 変換モードのまま。モードは一度も切り替えていない
    await expect(tile.getByText("★★★")).toBeVisible();
  });
```

- [ ] **Step 3: e2e が落ちることを確認する**

Run: `cd gui-frontend && bun run e2e --grep "★"`
Expected: **「フォルダーを開いた瞬間に★がタイルに出る」だけが FAIL**
（変換モードでは `App.svelte:170` の `mode !== "metadata"` ガードで★を読まないため）。
同じ `--grep` に一致する `metadata.spec.ts` の
「保存した★がグリッドのタイルにも出る」と「タイトル・コメント・★は編集でき…」は
**PASS のまま**である（★はまだ `ratings` ストア経由で出ており、
`stub.ts` の `read_image_ratings` ハンドラも Step 5 まで残してある）。

**この 1 件だけが落ちることを目で確認してから Step 4 へ進む。**
2 件以上落ちたら Step 1 の「ハンドラを消さない」を守れているかを先に疑う。

- [ ] **Step 4: フロントの★の出所を `ImageEntry` に切り替える**

`gui-frontend/src/lib/browser/PhotoGrid.svelte`:
- Props（`:41`）から `ratingFor: (path: string) => number | null | undefined;` を削除
- 分割代入（`:70`）から `ratingFor,` を削除
- `:341` を差し替え:

```svelte
        {@const stars = image.rating}
```

`gui-frontend/src/App.svelte`:
- `import { createRatingStore } from "./lib/browser/ratings.svelte";`（`:24`）を削除
- `const ratings = createRatingStore();`（`:90`）を削除
- `let visibleRange = $state({ start: 0, end: -1 });` とその上のコメント（`:82-85`）を削除
- 可視範囲起点の★取得 `$effect`（`:169-178`）を**まるごと削除**
- `handleVisibleRangeChange`（`:275-278`）を縮める:

```ts
  function handleVisibleRangeChange(start: number, end: number) {
    thumbnails.setVisibleRange(start, end);
  }
```

- `loadFolder` の `ratings.forget();`（`:255`）を削除
- `<PhotoGrid ... ratingFor={ratings.get}`（`:414`）の行を削除
- `handleSaveMetadata`（`:340-357`）を差し替え:

```ts
  async function handleSaveMetadata(): Promise<boolean> {
    try {
      const didSave = await metadata.save();
      if (didSave) {
        toast.success("保存しました。");
        // ★の出所は `list_images` なので、**保存した 1 枚だけその場で書き換える**。
        // `listImages` を呼び直すとスクロール位置と選択が飛ぶ。
        //
        // **値は保存の応答（＝書いた後のファイルの実際の中身）から採る。**
        // `metadataDraft.save()` は応答を `values` に入れている。送信した下書きの
        // 値を採ると Rust 側の正規化（★の範囲外 → None）と食い違い、
        // グリッドの★がディスクに無い値になる
        const savedPath = metadata.path!;
        const stars = metadata.values.rating === 0 ? null : metadata.values.rating;
        images = images.map((img) =>
          img.path === savedPath ? { ...img, rating: stars } : img
        );
      }
      return didSave;
    } catch (e) {
      toast.error(`保存できませんでした: ${describeError(e)}`);
      return false;
    }
  }
```

- ストアを削除する（`git rm` は Step 8 のコミットでまとめて行う）:
  `gui-frontend/src/lib/browser/ratings.svelte.ts`

- [ ] **Step 5: 境界から `read_image_ratings` を落とす**

`gui-frontend/e2e/stub.ts` の `read_image_ratings` ハンドラ（`:320-324`）を削除する。
**ここまで残していたのは Step 3 の赤を 1 件に絞るため**（Step 1 の注記）。

`gui-frontend/src/lib/api.ts:147-149` の `readImageRatings` を削除し、
`import` から `RatingEntry` を外す。
`gui-frontend/src/lib/types.ts` の `RatingEntry` interface を削除。

`gui/src/commands.rs:788-826` の `read_image_ratings`（doc コメントごと）を削除。
`gui/src/main.rs:128` の `commands::read_image_ratings,` を削除。
`gui/src/types.rs` の `RatingEntry` struct（`:53-59`）と、`mod contract` の
`ratingEntry` / `ratingEntryUnrated` の 2 entry、型名リスト（`:558`）の `"RatingEntry",` を削除。

`gui/src/security.rs:48-53` の節を削除する。**`MAX_BATCH` に由来する自己 DoS の記述ごと
消えるのが正しい** ── 「パスの配列を受ける」コマンドが無くなり、`list_images` は
ディレクトリ 1 本しか受けないので、その脅威の入口自体が存在しなくなる。

- [ ] **Step 6: DTO 契約を再生成する**

```bash
UPDATE_DTO_CONTRACT=1 cargo test -p picture-tool-gui contract
```

`git diff gui-frontend/src/lib/dto.contract.ts` で `ratingEntry` / `ratingEntryUnrated` が
消えたことを確認する。

- [ ] **Step 7: 検証**

Run: `make check`
Expected: exit 0

Run: `cd gui-frontend && bun run e2e`
Expected: 全件 PASS（Step 2 で書き換えた 2 件を含む）。
落ちるものが出たら、**★が aria-label に連結されたせい**でないかを最初に疑う
（`tileLabel()` は `"photo-0000.jpg ★3"` を作る）。

- [ ] **Step 8: コミット**

**`git add -A` にしない。** この計画ファイル自身が未追跡なので、`-A` だと
Task 4 のコミットに巻き込まれ、Task 4 を revert すると計画ごと消える。

```bash
git add gui-frontend/src/lib/browser/PhotoGrid.svelte \
        gui-frontend/src/App.svelte \
        gui-frontend/src/lib/api.ts \
        gui-frontend/src/lib/types.ts \
        gui-frontend/src/lib/dto.contract.ts \
        gui-frontend/e2e/stub.ts \
        gui-frontend/e2e/metadata.spec.ts \
        gui-frontend/e2e/gridFilter.spec.ts \
        gui/src/commands.rs gui/src/types.rs gui/src/main.rs gui/src/security.rs
git rm gui-frontend/src/lib/browser/ratings.svelte.ts
git commit -m "$(cat <<'EOF'
refactor(gui-frontend): 写真グリッドの絞り込みと並び替え: 可視範囲起点の★取得を丸ごと落とす

★は list_images が返すようになったので、read_image_ratings コマンド・
RatingEntry 型・保持ストア（CHUNK / inFlight / 台帳）・可視範囲が動くたびに
★を要求する $effect が全部要らなくなった。

信頼境界は増えず減る。MAX_BATCH は「パスの配列を受ける」ことに由来する
防御で、ディレクトリ 1 本を受ける list_images には元々不要だった。
security.rs の自己 DoS の節も、その脅威の入口ごと消える。

保存後の反映は images の該当 ImageEntry.rating をその場で書き換える形にした。
list_images を呼び直すとスクロール位置と選択が飛ぶ。値は保存の応答（書いた後の
ファイルの実際の中身）から採る ── 送った下書きを採ると Rust 側の正規化と
食い違い、グリッドの★がディスクに無い値になる。

裁定 X1 は反転する。★は情報モードに入らなくても、フォルダーを開いた瞬間から
タイルに出る。★は「どれを変換するか」を選ぶ情報でもあり、変換モードで
見えない方が不自然である。並列化により変換モードだけの利用者も現行より速い。

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: 絞り込みの意味論（純関数）

`gridFilter.ts` に足す。**Phase 1 と同じく IPC を一切しない純関数のまま**
（`ImageEntry` に★が入っているので、その前提が保てる）。

**`min: 1` が「★で絞り込んでいない」である。** ★は 1〜5 しか取らない
（★0 は書き込まず、他アプリが書いた `xmp:Rating = 0` も `read_metadata` が `None` に落とす）。
だから `min` に「絞り込まない」専用の値は要らず、`null` を「★ありは出さない」に使える。

**★0 が万一届いた場合は「未評価」として扱う**（下の `filterByRating` のコメント）。
spec §3 は「ここに来ない」としか書いていないが、0 の意味は Rust 側で既に
「評価が無い」に決まっているので、フロントだけ別の意味を与えない。

**7 択の対応表は `gridFilter.ts` に置く**（`SORT_LABELS` / `DEFAULT_SORT` と同じ場所）。
`GridFilterBar.svelte` の中に置くと純関数のテストから参照できず、
「7 択それぞれを検査する」がラベルと値の手書きの写しになり、**写し間違えても緑になる**。

**Files:**
- Modify: `gui-frontend/src/lib/browser/gridFilter.ts`
- Test: `gui-frontend/src/lib/browser/gridFilter.test.ts`

**Interfaces:**
- Consumes: `ImageEntry.rating: number | null`（Task 3）
- Produces:
  - `export interface RatingFilter { min: 1 | 2 | 3 | 4 | 5 | null; includeUnrated: boolean }`
  - `export type RatingPresetId = "all" | "rated" | "min2" | "min3" | "min4" | "only5" | "unrated"`
  - `export const RATING_PRESETS: { id: RatingPresetId; label: string; filter: RatingFilter }[]`
  - `export const DEFAULT_RATING_PRESET: RatingPresetId`
  - `export function ratingFilterOf(id: RatingPresetId): RatingFilter`
  - `export function filterByRating(images: readonly ImageEntry[], filter: RatingFilter): ImageEntry[]`
  - `export function arrangeImages(images, query: string, rating: RatingFilter, sort: GridSort): ImageEntry[]`（**引数が 1 つ増える**）

- [ ] **Step 1: 失敗するテストを書く**

`gui-frontend/src/lib/browser/gridFilter.test.ts` の `image()` ヘルパに `rating` を足し、
`arrangeImages` の既存呼び出し（`:218`）を直し、★の `describe` を足す。

```ts
import {
  RATING_PRESETS,
  arrangeImages,
  filterByRating,
  filterImages,
  hiddenSelectedCount,
  ratingFilterOf,
  sortImages,
  type RatingPresetId,
} from "./gridFilter";

/** 日時と★を持つ 1 枚。どちらも「取れないことがある」ので既定は null にしておく */
function image(
  name: string,
  opts: { modified?: number | null; created?: number | null; rating?: number | null } = {}
): ImageEntry {
  return {
    name,
    path: `/photos/${name}`,
    width: 4000,
    height: 3000,
    size_bytes: 4_500_000,
    modified_ms: opts.modified ?? null,
    created_ms: opts.created ?? null,
    rating: opts.rating ?? null,
  };
}
```

`describe("arrangeImages")`（`:210-223`）の中の呼び出しを直す:

```ts
    expect(
      names(
        arrangeImages(images, "sunset", ratingFilterOf("all"), {
          key: "modified",
          desc: true,
        })
      )
    ).toEqual(["sunset-a.jpg", "sunset-b.jpg"]);
```

`describe("hiddenSelectedCount")` の**前**に足す:

```ts
describe("filterByRating", () => {
  const IMAGES = [
    image("r1.jpg", { rating: 1 }),
    image("r2.jpg", { rating: 2 }),
    image("r3.jpg", { rating: 3 }),
    image("r4.jpg", { rating: 4 }),
    image("r5.jpg", { rating: 5 }),
    image("none-a.jpg"),
    image("none-b.jpg"),
  ];

  /**
   * 画面に出す 7 択（設計 §3 の対応表がそのまま期待値）。
   *
   * **`RATING_PRESETS` を回して照合する。** ここで `{ min, includeUnrated }` を
   * 手で書き写すと、対応表を写し間違えても両方同じ間違いになって緑になる
   */
  const EXPECTED: Record<RatingPresetId, string[]> = {
    all: ["r1.jpg", "r2.jpg", "r3.jpg", "r4.jpg", "r5.jpg", "none-a.jpg", "none-b.jpg"],
    rated: ["r1.jpg", "r2.jpg", "r3.jpg", "r4.jpg", "r5.jpg"],
    min2: ["r2.jpg", "r3.jpg", "r4.jpg", "r5.jpg"],
    min3: ["r3.jpg", "r4.jpg", "r5.jpg"],
    min4: ["r4.jpg", "r5.jpg"],
    only5: ["r5.jpg"],
    unrated: ["none-a.jpg", "none-b.jpg"],
  };

  test("7 択がすべて対応表どおりの集合を返す", () => {
    // 前提条件: 選択肢が 7 つある（表から漏れた選択肢が黙って通らないように）
    expect(RATING_PRESETS).toHaveLength(7);
    for (const preset of RATING_PRESETS) {
      expect(names(filterByRating(IMAGES, preset.filter))).toEqual(EXPECTED[preset.id]);
    }
  });

  test("既定は「すべて」── フォルダーを開いた瞬間に何かが隠れていない", () => {
    expect(names(filterByRating(IMAGES, ratingFilterOf("all")))).toEqual(names(IMAGES));
  });

  test("閾値は「以上」── ★3 以上に★3 は入り、★2 は入らない", () => {
    const shown = names(filterByRating(IMAGES, ratingFilterOf("min3")));
    expect(shown).toContain("r3.jpg");
    // 前提条件: 除かれる側が母集合に実在する（「元から居ないから出ない」を防ぐ）
    expect(names(IMAGES)).toContain("r2.jpg");
    expect(shown).not.toContain("r2.jpg");
  });

  test("「★が付いているもの」は未評価を落とす", () => {
    const shown = names(filterByRating(IMAGES, ratingFilterOf("rated")));
    // 前提条件: 未評価が母集合に 2 枚ある
    expect(IMAGES.filter((i) => i.rating === null)).toHaveLength(2);
    expect(shown).not.toContain("none-a.jpg");
    expect(shown).not.toContain("none-b.jpg");
  });

  test("「未評価だけ」は★の付いたものを全部落とす", () => {
    const shown = names(filterByRating(IMAGES, ratingFilterOf("unrated")));
    // 前提条件: ★付きが母集合に 5 枚ある
    expect(IMAGES.filter((i) => i.rating !== null)).toHaveLength(5);
    expect(shown).toEqual(["none-a.jpg", "none-b.jpg"]);
  });

  test("0 件になってもエラーにならず空配列を返す", () => {
    const onlyRated = [image("r1.jpg", { rating: 1 })];
    expect(filterByRating(onlyRated, ratingFilterOf("unrated"))).toEqual([]);
  });

  /**
   * **★0 は「未評価」。** Rust 側が `xmp:Rating = 0` を `None` に落とすので
   * ここへは来ないが、その不変条件が破れたときに「すべて」が写真を隠さないことを
   * 固定しておく（0 が意味するのは「評価が無い」であって「評価が 0」ではない）
   */
  test("★0 は未評価と同じ扱いになる", () => {
    const withZero = [...IMAGES, image("zero.jpg", { rating: 0 })];
    // 前提条件: 0 を持つ写真が母集合に居る（型の上では来ない値なので明示する）
    expect(withZero.some((i) => i.rating === 0)).toBe(true);

    expect(names(filterByRating(withZero, ratingFilterOf("all")))).toContain("zero.jpg");
    expect(names(filterByRating(withZero, ratingFilterOf("unrated")))).toContain("zero.jpg");
    expect(names(filterByRating(withZero, ratingFilterOf("rated")))).not.toContain("zero.jpg");
  });

  test("元の配列を書き換えない", () => {
    const before = names(IMAGES);
    filterByRating(IMAGES, ratingFilterOf("only5"));
    expect(names(IMAGES)).toEqual(before);
  });
});

describe("arrangeImages と★の合成", () => {
  const IMAGES = [
    image("sunset-a.jpg", { rating: 5, modified: 1_000 }),
    image("sunset-b.jpg", { rating: 2, modified: 2_000 }),
    image("other.jpg", { rating: 5, modified: 3_000 }),
  ];

  test("名前で落ちた写真は★の条件を満たしても出ない", () => {
    // 前提条件: other.jpg は★5 なので、★だけで見れば残る側である
    expect(names(filterByRating(IMAGES, ratingFilterOf("only5")))).toContain("other.jpg");

    const shown = names(
      arrangeImages(IMAGES, "sunset", ratingFilterOf("only5"), { key: "name", desc: false })
    );
    expect(shown).toEqual(["sunset-a.jpg"]);
  });
});
```

- [ ] **Step 2: テストが失敗することを確認する**

Run: `cd gui-frontend && bun test src/lib/browser/gridFilter.test.ts`
Expected: `RATING_PRESETS` / `ratingFilterOf` / `filterByRating` が export されていない旨のエラー

- [ ] **Step 3: 実装を書く**

`gui-frontend/src/lib/browser/gridFilter.ts` のモジュールコメントの
「★や撮影日で絞り込むにはフォルダー全体のメタデータ走査が必要で、それは別の設計になる。」を
差し替える（**この 1 文はもう嘘である**）:

```ts
 * `list_images` が返す `ImageEntry` に必要な値（名前・更新日・作成日・★）は
 * すべて入っているので、ここは IPC を一切しない純関数で済む。★は
 * `core::metadata::read_metadata_fast`（JPEG の先頭だけを読む）を `list_images` に
 * 畳んだことで、フォルダーを開いた時点で全件揃っている。撮影日での絞り込みは
 * Exif `DateTimeOriginal` の解釈が別の設計になるので、まだ持っていない。
```

`SORT_LABELS` の直後に足す:

```ts
/**
 * ★の絞り込み。
 *
 * **`min: 1` が「★で絞り込んでいない」である。** ★は 1〜5 しか取らない
 * （★0 は書き込まず、他アプリが書いた `xmp:Rating = 0` も Rust 側の
 * `read_metadata` が `None` に落とす）。だから `min` に「絞り込まない」専用の値は
 * 要らず、**`null` を「★の付いた写真は出さない」に使える**。
 */
export interface RatingFilter {
  /** ★の下限。**null は「★の付いた写真は出さない」** */
  min: 1 | 2 | 3 | 4 | 5 | null;
  /** 未評価（rating === null）も出すか */
  includeUnrated: boolean;
}

export type RatingPresetId =
  | "all"
  | "rated"
  | "min2"
  | "min3"
  | "min4"
  | "only5"
  | "unrated";

/**
 * 画面に出す 7 択。**2 つのつまみを利用者に組み立てさせない。**
 *
 * `min` と `includeUnrated` をそのまま 2 つの部品にすると、「なし / ★1 / …」の
 * 「なし」が日常語で「★での絞り込みなし」と読めてしまう。実際の意味は
 * 「★ありを全部隠す」なので、**絞り込みを解除しようとした利用者の画面から
 * ★付きが全部消える**。7 択なら「両方切った状態＝常に 0 件」という
 * 意味の無い組み合わせも利用者が選べない。
 *
 * **この配列が唯一の対応表。** `GridFilterBar` はこれを `<select>` に流し、
 * テストも同じ配列を回す。部品の中に書くと純関数のテストから参照できず、
 * 写し間違えても緑になる。
 *
 * `min: n` ＋ `includeUnrated: true`（「★n 以上 ＋ 未評価」）は 7 択から漏れるが、
 * 内部表現には残るので、要望が出たら選択肢を足すだけで足りる。
 */
export const RATING_PRESETS: {
  id: RatingPresetId;
  label: string;
  filter: RatingFilter;
}[] = [
  { id: "all", label: "すべて", filter: { min: 1, includeUnrated: true } },
  { id: "rated", label: "★が付いているもの", filter: { min: 1, includeUnrated: false } },
  { id: "min2", label: "★2 以上", filter: { min: 2, includeUnrated: false } },
  { id: "min3", label: "★3 以上", filter: { min: 3, includeUnrated: false } },
  { id: "min4", label: "★4 以上", filter: { min: 4, includeUnrated: false } },
  { id: "only5", label: "★5", filter: { min: 5, includeUnrated: false } },
  { id: "unrated", label: "未評価だけ", filter: { min: null, includeUnrated: true } },
];

/** ★の絞り込みの既定。起動直後に何かが隠れている状態を作らない */
export const DEFAULT_RATING_PRESET: RatingPresetId = "all";

export function ratingFilterOf(id: RatingPresetId): RatingFilter {
  // 見つからないことは型の上で起きない。`!` ではなく既定へ落として、
  // 万一 id が増えたときにグリッドが空になるより「すべて」に倒す
  return (
    RATING_PRESETS.find((preset) => preset.id === id)?.filter ?? {
      min: 1,
      includeUnrated: true,
    }
  );
}
```

`filterImages` の直後に足す:

```ts
/**
 * ★で絞り込む。**元の配列は書き換えない。**
 *
 * **`rating === 0` は `null` と同じ「未評価」として扱う。**
 * ここへは来ない（Rust 側の `read_metadata` が `xmp:Rating = 0` を `None` に落とす。
 * `xmp.rs` の `parse_stars` と `mod.rs` の `percent_to_stars`）が、
 * 来た場合に「どの下限も満たさない」として全部落とすと、
 * **「すべて」という名前の選択肢が写真を静かに隠す**ことになる。
 * 0 が意味するのは「評価が無い」であって「評価が 0 である」ではないので、
 * **Rust 側と同じ規則をここでも守る**のが唯一の一貫した扱いである。
 */
export function filterByRating(
  images: readonly ImageEntry[],
  filter: RatingFilter
): ImageEntry[] {
  return images.filter((image) => {
    const stars = image.rating === null || image.rating === 0 ? null : image.rating;
    if (stars === null) return filter.includeUnrated;
    return filter.min !== null && stars >= filter.min;
  });
}
```

`arrangeImages` を差し替える:

```ts
/** 名前で絞る → ★で絞る → 並び替える。画面に出す一覧はこれ */
export function arrangeImages(
  images: readonly ImageEntry[],
  query: string,
  rating: RatingFilter,
  sort: GridSort
): ImageEntry[] {
  return sortImages(filterByRating(filterImages(images, query), rating), sort);
}
```

- [ ] **Step 4: テストが通ることを確認する**

Run: `cd gui-frontend && bun test src/lib/browser/gridFilter.test.ts`
Expected: 全件 PASS

- [ ] **Step 5: `App.svelte` を既定値で繋いで緑を保つ**

`arrangeImages` の引数が増えたので `App.svelte:104` が型エラーになる。
**ここでは既定を直に渡すだけ**にして、状態を持たせるのは Task 6 でやる
（赤いコミットを挟まないため。この時点では挙動は 1 ミリも変わらない）。

`gui-frontend/src/App.svelte:26-32` の import に `DEFAULT_RATING_PRESET` と
`ratingFilterOf` を足し、`:104` を差し替える:

```ts
  // ★の絞り込みはまだ既定に固定。利用者が選べるようにするのは次のコミット
  let visibleImages = $derived(
    arrangeImages(images, searchQuery, ratingFilterOf(DEFAULT_RATING_PRESET), gridSort)
  );
```

- [ ] **Step 6: 全体の検証とコミット**

Run: `make check`
Expected: exit 0

```bash
git add gui-frontend/src/lib/browser/gridFilter.ts gui-frontend/src/lib/browser/gridFilter.test.ts gui-frontend/src/App.svelte
git commit -m "$(cat <<'EOF'
feat(gui-frontend): 写真グリッドの絞り込みと並び替え: ★の絞り込みを純関数として足す

min: 1 が「★で絞り込んでいない」。★は 1〜5 しか取らないので、min に
専用の値は要らず、null を「★の付いた写真は出さない」に使える。

画面に出すのは名前付きの 7 択にする。min と includeUnrated をそのまま
2 つの部品にすると、「なし」が日常語で「★での絞り込みなし」と読めてしまい、
絞り込みを解除しようとした利用者の画面から★付きが全部消える。

対応表は gridFilter.ts に置く。GridFilterBar の中に置くと純関数のテストから
参照できず、「7 択それぞれを検査する」がラベルと値の手書きの写しになって、
写し間違えても緑になる。

App.svelte はまだ既定に固定してある。利用者が選べるようにするのは次のコミット。

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 6: ★の `<select>` を画面に繋ぐ

状態は `App.svelte` が持つ（`GridFilterBar` に持たせられない ── 一覧の順序は
情報モードの「次の写真」・サムネイル要求・変換対象の 3 箇所が同じ配列を index で
参照する前提なので、決めるのは App の `$derived` 1 箇所だけである）。

**スクロールを先頭へ戻す条件に★を足す。** 足さないと、下までスクロールした状態で
「★4 以上」に絞っても先頭に戻らず、一覧が短くなったぶん `scrollTop` がクランプされて
**末尾が表示される**。依存は `visibleImages` ではなく**絞り込み設定そのもの**に張る ──
`visibleImages` に張ると、保存 → その場で書き換え → グリッドから消える、でも
スクロールが飛ぶ。

**Files:**
- Modify: `gui-frontend/src/lib/browser/GridFilterBar.svelte`
- Modify: `gui-frontend/src/App.svelte:70-71,104,182-190,486-488`
- Test: `gui-frontend/e2e/gridFilter.spec.ts`

**Interfaces:**
- Consumes: `RATING_PRESETS` / `RatingPresetId` / `DEFAULT_RATING_PRESET` / `ratingFilterOf` / `arrangeImages`（Task 5）
- Produces: `GridFilterBar` の props が `{ query: string; sort: GridSort; ratingPreset: RatingPresetId }`（`query` と `ratingPreset` は `$bindable`）

- [ ] **Step 1: 失敗する e2e を書く**

`gui-frontend/e2e/gridFilter.spec.ts` に足す。
**スタブの★は `RATING_PATTERN`（Task 4）で決まる。** `imageCount: 20` のときの内訳:

| 選択肢 | id | 枚数 | 根拠（index） |
|---|---|---|---|
| すべて | `all` | 20 | 全部 |
| ★が付いているもの | `rated` | 12 | ★の付いた index すべて |
| ★2 以上 | `min2` | 10 | ★1 の 2 枚（4, 16）を除く |
| ★3 以上 | `min3` | 8 | ★2 の 2 枚（7, 19）をさらに除く |
| ★4 以上 | `min4` | 5 | ★3 の 3 枚（2, 10, 14）をさらに除く |
| ★5 | `only5` | 3 | index 1, 8, 13 |
| 未評価だけ | `unrated` | 8 | index 0, 3, 6, 9, 11, 12, 15, 18 |

```ts
import { RATING_PRESETS, type RatingPresetId } from "../src/lib/browser/gridFilter";

// ... 既存の describe の中に足す ...

  /**
   * スタブの `RATING_PATTERN`（`stub.ts`）を 20 枚に当てたときの内訳。
   * **選択肢のラベルではなく `RATING_PRESETS` を回す** ── 対応表の写しを
   * ここにもう 1 つ作ると、写し間違えても緑になる
   */
  const EXPECTED_COUNTS: Record<RatingPresetId, number> = {
    all: 20,
    rated: 12,
    min2: 10,
    min3: 8,
    min4: 5,
    only5: 3,
    unrated: 8,
  };

  test("★の 7 択それぞれでグリッドが絞られ、ヘッダーが分母付きになる", async ({ page }) => {
    // 前提条件: 絞り込む前は全件で、分母が出ていない
    await expect(page.getByText("20 枚", { exact: true })).toBeVisible();
    // 前提条件: スタブが★あり・なしを混ぜている（全部★なし／全部★ありだと
    // どの選択肢も同じ枚数になり、検査が成立しない）
    expect(EXPECTED_COUNTS.rated).toBeGreaterThan(0);
    expect(EXPECTED_COUNTS.unrated).toBeGreaterThan(0);

    const rating = page.getByLabel("評価");
    for (const preset of RATING_PRESETS) {
      await rating.selectOption(preset.id);
      const expected = EXPECTED_COUNTS[preset.id];
      if (expected === 20) {
        await expect(page.getByText("20 枚", { exact: true })).toBeVisible();
      } else {
        await expect(page.getByText(`${expected} / 20 枚`)).toBeVisible();
      }
    }
  });

  test("★の条件を変えるとスクロールが先頭へ戻る", async ({ page }) => {
    // 20 枚は viewport 1440x800 に収まってしまい、そもそもスクロールできない。
    // この検査だけ枚数を増やして貼り直す（★の内訳は見ないので枚数は自由）
    await installTauriStub(page, { imageCount: 300 });
    await page.goto("/");
    await page.getByRole("button", { name: "photos", exact: false }).first().click();
    await expect(gridOf(page).getByRole("option").first()).toBeVisible();

    const scroller = page.locator(".scroller");
    await scroller.evaluate((el) => (el.scrollTop = el.scrollHeight));
    // 前提条件: 実際に下までスクロールできている（0 のままだと自明に成立する）
    await expect
      .poll(() => scroller.evaluate((el) => el.scrollTop))
      .toBeGreaterThan(0);

    await page.getByLabel("評価").selectOption("min4");

    await expect.poll(() => scroller.evaluate((el) => el.scrollTop)).toBe(0);
  });

  test("★で隠れた選択も枚数が画面に出て、変換対象からは減らない", async ({ page }) => {
    const options = gridOf(page).getByRole("option");
    // photo-0000 は未評価（スタブの既定パターン）
    await options.nth(0).click();
    await expect(page.getByRole("button", { name: "1 枚を変換" })).toBeVisible();
    // 前提条件: 絞り込む前は非表示の注記が出ていない
    await expect(page.getByText("枚は非表示")).toHaveCount(0);

    await page.getByLabel("評価").selectOption("only5");

    // 前提条件: 選んだ 1 枚が実際にグリッドから消えていること
    await expect(options).toHaveCount(3);
    await expect(page.getByText("（1 枚は非表示）")).toBeVisible();
    // **減らない。** 選択は表示と独立している
    await expect(page.getByRole("button", { name: "1 枚を変換" })).toBeVisible();
  });

  test("絞り込み中に「保存して次の写真へ」を押すと、その写真は消えるがパネルは次へ送られる", async ({
    page,
  }) => {
    // photo-0001 は★5。「★5」に絞ると 3 枚（index 1, 8, 13）が残る
    await page.getByLabel("評価").selectOption("only5");
    const options = gridOf(page).getByRole("option");
    await expect(options).toHaveCount(3);
    await options.nth(0).click();
    await page
      .getByRole("navigation", { name: "モード" })
      .getByRole("button", { name: "情報" })
      .click();

    // canSave を満たす（承認済み ＋ 変更あり）。満たさないままボタンを押しても
    // 何も起きないので、検査そのものが成立しない
    await page.getByRole("button", { name: "このフォルダーへの書き込みを許可..." }).click();
    await page.getByLabel("タイトル").fill("夕暮れの港");
    // ★を 5 から 3 へ落とす。保存すると「★5」の条件から外れる
    const rating = page.getByRole("slider", { name: "レーティング" });
    await rating.focus();
    await rating.press("ArrowLeft");
    await rating.press("ArrowLeft");

    await page.getByRole("button", { name: "保存して次の写真へ" }).click();

    // グリッドから消える（3 → 2 枚）
    await expect(options).toHaveCount(2);
    // **パネルは次の写真へ送られている** ── 保存前に次を確定しているため
    // （`App.svelte` の `const next = nextImagePath;`）
    await expect(page.locator('[data-region="right"]').getByText("photo-0008.jpg")).toBeVisible();
  });

  test("絞り込み中に「保存」だけを押すと、写真は消えて両方のボタンが無効になる（仕様）", async ({
    page,
  }) => {
    await page.getByLabel("評価").selectOption("only5");
    const options = gridOf(page).getByRole("option");
    await expect(options).toHaveCount(3);
    await options.nth(0).click();
    await page
      .getByRole("navigation", { name: "モード" })
      .getByRole("button", { name: "情報" })
      .click();
    await page.getByRole("button", { name: "このフォルダーへの書き込みを許可..." }).click();
    await page.getByLabel("タイトル").fill("夕暮れの港");
    const rating = page.getByRole("slider", { name: "レーティング" });
    await rating.focus();
    await rating.press("ArrowLeft");
    await rating.press("ArrowLeft");

    const save = page.getByRole("button", { name: "保存", exact: true });
    const next = page.getByRole("button", { name: "保存して次の写真へ" });
    // 前提条件: 押す前はどちらも有効（＝「元から無効」で緑にならないように）
    await expect(save).toBeEnabled();
    await expect(next).toBeEnabled();

    await save.click();

    await expect(options).toHaveCount(2);
    // **「次へが有効であること」を検査してはならない。** 保存に成功すると
    // `draft.isDirty` が false になり `canSave` が false になるので、
    // hasNext の値に関係なく両方押せない（設計 §5 / R-16）。
    // これは絞り込みが持ち込む欠陥ではなく、保存ボタンの既存の振る舞いである
    await expect(save).toBeDisabled();
    await expect(next).toBeDisabled();
  });
```

- [ ] **Step 2: e2e が落ちることを確認する**

Run: `cd gui-frontend && bun run e2e --grep "★|絞り込み中"`
Expected: 新規 5 件がすべて FAIL（`getByLabel("評価")` が見つからない）。
**`--grep "★"` では足りない** ── 新規 5 件のうち「絞り込み中に…」の 2 件は
テスト名に★を含まず、逆に Task 4 で足した「フォルダーを開いた瞬間に★がタイルに出る」が
混ざって PASS するので、件数からも判定できなくなる。

- [ ] **Step 3: `GridFilterBar` に★の `<select>` を足す**

`gui-frontend/src/lib/browser/GridFilterBar.svelte`:

```svelte
  import {
    RATING_PRESETS,
    SORT_LABELS,
    type GridSort,
    type RatingPresetId,
    type SortKey,
  } from "./gridFilter";

  interface Props {
    query: string;
    /**
     * 並び順。**`bind:` ではなくプロパティを直接書き換える**
     * （`$state` のプロキシなので親まで届く）。ConvertPanel の config と同じ扱い
     */
    sort: GridSort;
    /** ★の絞り込み。7 択の id。対応表は `gridFilter.ts` が持つ */
    ratingPreset: RatingPresetId;
  }

  let { query = $bindable(), sort, ratingPreset = $bindable() }: Props = $props();

  const SORT_OPTIONS = (Object.keys(SORT_LABELS) as SortKey[]).map((value) => ({
    value,
    label: SORT_LABELS[value],
  }));

  // **対応表を写さない。** ラベルも値も `gridFilter.ts` の 1 箇所から取る
  const RATING_OPTIONS = RATING_PRESETS.map((preset) => ({
    value: preset.id,
    label: preset.label,
  }));
```

マークアップの `.sort` の直後（`IconButton` の後ろ）に足す:

```svelte
  <div class="rating">
    <Select label="評価" bind:value={ratingPreset} options={RATING_OPTIONS} />
  </div>
```

`<style>` に足す（幅は Task 7 で実測して確定する。ここでは設計 §6 の見積もり 140px）:

```css
  .rating {
    flex: 0 0 auto;
    width: 140px;
  }
```

- [ ] **Step 4: `App.svelte` に状態を持たせる**

import（`:26-32`）に `RATING_PRESETS` は要らない。`DEFAULT_RATING_PRESET` /
`ratingFilterOf` / `type RatingPresetId` を使う。

`gridSort` の宣言（`:71`）の直後に足す:

```ts
  /** ★の絞り込み。7 択の id を持つ（対応表は `gridFilter.ts`） */
  let ratingPreset = $state<RatingPresetId>(DEFAULT_RATING_PRESET);
  let ratingFilter = $derived(ratingFilterOf(ratingPreset));
```

`visibleImages`（Task 5 Step 5 で既定に固定したもの）を差し替える:

```ts
  let visibleImages = $derived(arrangeImages(images, searchQuery, ratingFilter, gridSort));
```

スクロールリセットの `$effect`（`:181-190`）に★を足す:

```ts
  // 絞り込みと並び順が変わったら先頭へ戻す。前のスクロール位置は別の写真を
  // 指しており、そのままだと「絞り込んだのに何も出ない」（一覧より下に居る）。
  // ★を足さないと、一覧が短くなったぶん scrollTop がクランプされて末尾が出る
  $effect(() => {
    searchQuery;
    gridSort.key;
    gridSort.desc;
    // **依存は絞り込み設定そのもの。`visibleImages` に張ってはならない** ──
    // 保存 → その場で★を書き換え → グリッドから消える、でもスクロールが飛ぶ
    ratingFilter.min;
    ratingFilter.includeUnrated;
    untrack(() => {
      gridScrollTop = 0;
    });
  });
```

snippet（`:486-488`）:

```svelte
{#snippet gridFilters()}
  <GridFilterBar bind:query={searchQuery} sort={gridSort} bind:ratingPreset />
{/snippet}
```

- [ ] **Step 5: 検証**

Run: `make check`
Expected: exit 0

Run: `cd gui-frontend && bun run e2e`
Expected: 全件 PASS

**`<option>` が 7 個増える。** スコープしていない `getByRole("option")` は
一斉に壊れるので、落ちたテストがあればまずそれを疑う
（既存はすべて `gridOf(page)` にスコープ済みか name 指定済みであることを確認してあるが、
新しく書いた検査が漏れていないかはここで見る）。

- [ ] **Step 6: コミット**

```bash
git add gui-frontend/src/lib/browser/GridFilterBar.svelte gui-frontend/src/App.svelte gui-frontend/e2e/gridFilter.spec.ts
git commit -m "$(cat <<'EOF'
feat(gui-frontend): 写真グリッドの絞り込みと並び替え: ★の 7 択をヘッダーに出す

状態は App が持つ。一覧の順序は情報モードの「次の写真」・サムネイル要求・
変換対象の 3 箇所が同じ配列を index で参照する前提なので、決めるのは
App の $derived 1 箇所だけである。

スクロールを先頭へ戻す条件に★を足した。足さないと、下までスクロールした
状態で絞っても先頭に戻らず、一覧が短くなったぶん scrollTop がクランプされて
末尾が表示される。依存は visibleImages ではなく絞り込み設定そのものに張る ──
visibleImages に張ると、保存 → その場で★を書き換え → グリッドから消える、
でもスクロールが飛ぶ。

「保存」だけを押したら操作はそこで終わる、を仕様として e2e に落とした。
保存に成功すると isDirty が false になり canSave が false になるので、
hasNext の値に関係なく両方のボタンが押せない。絞り込みが持ち込む欠陥では
なく、保存ボタンの既存の振る舞いである。

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 7: ヘッダーの段組みとレイアウト検査の作り直し

**Phase 1 のヘッダーは最小幅で既に破綻している**（spec §6 の実測。1100px 窓・右パネル開で
グリッドヘッダーの実効幅は 460px、「1 枚選択中」は max-content 58px に対して **12px** ＝
縦書き 1 文字ずつ、ヘッダーの高さが 97px の 3 行になる）。★の `<select>` を足すとさらに悪化する。

**`flex-wrap: wrap` 単独では出せない。** `.grid-header` の flex アイテムはソース順に
count → selected → 全解除 → filters → spacer → controls → primary → handle で、
`flex-wrap` はソース順にしか折り返せないので「1 段目 = 枚数群 ＋ サイズ ＋ ハンドル /
2 段目 = 検索群」は**絶対に出ない**（実測でヘッダー高さ 179px）。

3 つのグループに分け、`grid-template-areas` で段を明示する。切り替えは
**ヘッダー自身の幅**で決まる（窓幅ではない ── 左右のパネルは利用者がドラッグで変えられる）
ので、**コンテナクエリ**で切る。`@media` では正しく切れない。

**グループの容器は `.filters` にしない。** `GridHeader.svelte:64` の
`<div class="filters">` と、その中に描かれる `GridFilterBar.svelte:39` の
`<div class="filters">` は**入れ子で同名**である。Svelte のスコープ用ハッシュは
別クラスとして付くので CSS は衝突しないが、**Playwright の `page.locator(".filters")` は
2 つに一致して strict mode violation で例外**になり、レイアウト検査が
「現行の破綻を捕まえたのか」を判定する前に止まる。
本計画では**外側（`GridHeader` 側のグループ容器）を `.filters-area` に改名する**
── spec §6 の実測表（max-content 508px / min-content 380px）が数えているのは
検索 ＋ 並び順 ＋ 向き ＋ ★、すなわち**内側**（`GridFilterBar` の `.filters`）なので、
その名前を動かすと spec の数字と読み手の対応が切れる。

**Files:**
- Modify: `gui-frontend/src/lib/browser/GridHeader.svelte`
- Modify: `gui-frontend/src/lib/browser/PhotoGrid.svelte`（`.photo-grid` に `container-type`）
- Modify: `gui-frontend/src/lib/browser/GridFilterBar.svelte`（★の幅を実測値に）
- Test: `gui-frontend/e2e/gridFilter.spec.ts`

**Interfaces:**
- Consumes: `GridFilterBar` の★の `<select>`（Task 6）
- Produces: `.grid-header` の中が `.status` / `.filters-area` / `.tools` の 3 グループになる

- [ ] **Step 1: 新しいレイアウト検査を書く（既存の溢れ検査を置き換える）**

既存の「最小ウィンドウ幅でもヘッダーが横に溢れない」（`gridFilter.spec.ts:114-129`）は
`scrollWidth <= clientWidth` を見ており、**flex が縮んだだけでも成立する**ので現行の破綻を
検知できていない（実際に通っている）。**`scrollWidth` を別の値に置き換えるだけでも足りない**
── `.selected` はテキストが縦に折り返しているだけで溢れてはいないので、
`scrollWidth === clientWidth` が成立する。

`gridFilter.spec.ts` の該当テストを削除し、次を足す:

```ts
/**
 * グループの実幅と自然幅を測る。
 *
 * クローンに `width: max-content` / `min-content` を当てて測るのは、
 * **中身が潰れていることは `scrollWidth` に現れない**ため
 * （`.selected` の縦書きは溢れではなく折り返しなので `scrollWidth === clientWidth`）。
 */
const measureGroup = (page: import("@playwright/test").Page, selector: string, mode: string) =>
  page.locator(selector).evaluate(
    (el, sizing) => {
      const clone = el.cloneNode(true) as HTMLElement;
      clone.style.width = sizing;
      clone.style.position = "absolute";
      clone.style.left = "-99999px";
      clone.style.top = "0";
      el.parentElement!.appendChild(clone);
      const natural = clone.getBoundingClientRect().width;
      clone.remove();
      const style = getComputedStyle(el);
      return {
        actual: el.getBoundingClientRect().width,
        natural,
        height: el.getBoundingClientRect().height,
        lineHeight: parseFloat(style.lineHeight) || 20,
      };
    },
    mode
  );

test.describe("グリッドヘッダーの段組み", () => {
  test.beforeEach(async ({ page }) => {
    await installTauriStub(page, { imageCount: 20 });
    await clearStorageOnce(page);
    await page.goto("/");
    await page.getByRole("button", { name: "photos", exact: false }).first().click();
    await expect(gridOf(page).getByRole("option").first()).toBeVisible();
    // 「N 枚選択中 / 全解除」も出す ── `.selected` の縦書きはここでしか出ない
    await gridOf(page).getByRole("option").first().click();
  });

  /**
   * `.status` と `.tools` は縮んではいけない（中に伸縮する部品が無い）。
   * `.filters-area`（中身は `GridFilterBar` の `.filters`）は検索欄が
 * `min-width: 72px` まで縮むことを設計が許しているので、
   * **min-content と比べる**（設計 §6 の 380px）。max-content と比べると
   * 「検索欄が 200 → 128px に縮む」という設計そのものが落ちる
   */
  for (const width of [1100, 1600]) {
    test(`ヘッダー幅 ${width}px で各グループが潰れない`, async ({ page }) => {
      await page.setViewportSize({ width, height: 800 });

      for (const [selector, sizing] of [
        [".status", "max-content"],
        [".tools", "max-content"],
        // 内側の `.filters`（GridFilterBar）ではなくグループ容器を測る。
        // `.filters` は入れ子で 2 つ一致するので locator が例外になる
        [".filters-area", "min-content"],
      ] as const) {
        const m = await measureGroup(page, selector, sizing);
        // 前提条件: 実際に測れる幅がある（0 同士の比較で通らないように）
        expect(m.natural, `${selector} の自然幅`).toBeGreaterThan(0);
        expect(m.actual, `${selector} が ${sizing} を下回った`).toBeGreaterThanOrEqual(
          m.natural - 1
        );
        // 各グループは 1 行に収まる。`.selected` の縦書きはここで落ちる
        expect(m.height, `${selector} が 1 行に収まっていない`).toBeLessThan(
          m.lineHeight * 2 + 24
        );
      }
    });
  }

  /**
   * **変換モードでの検査である。** 実効幅は「窓幅 −（rail 80 ＋ 左カラム ＋ 右カラム）」で、
   * 右カラムの既定は変換 320px / 情報 360px。情報モードは 1600px の窓でも 920px にしかならず、
   * 1 段に要る幅を下回るので**常に 2 段**になる（許容。設計 §6 の 960px は変換モードの数字）
   */
  test("狭いと 2 段・広いと 1 段に切り替わる（変換モード）", async ({ page }) => {
    const rowOf = (selector: string) =>
      page.locator(selector).evaluate((el) => Math.round(el.getBoundingClientRect().top));

    await page.setViewportSize({ width: 1100, height: 800 });
    // 前提条件: 2 段になっている（`.filters-area` が `.status` より下の行にある）
    expect(await rowOf(".filters-area")).toBeGreaterThan(await rowOf(".status"));

    await page.setViewportSize({ width: 1600, height: 800 });
    await expect
      .poll(async () => (await rowOf(".filters-area")) - (await rowOf(".status")))
      .toBe(0);
  });

  test("スクリーンショットを残す（px を並べただけでは破綻が見えない）", async ({ page }) => {
    for (const width of [1100, 1600]) {
      await page.setViewportSize({ width, height: 800 });
      await page
        .locator(".grid-header")
        .screenshot({ path: `e2e/__screenshots__/grid-header-${width}.png` });
    }
  });
});
```

- [ ] **Step 2: 検査が現行の破綻を捕まえることを確認する**

Run: `cd gui-frontend && bun run e2e --grep "段組み"`
Expected: **FAIL**。少なくとも次が落ちる:
- 1100px で `.status` の実幅が max-content を下回る（「1 枚選択中」が 58px → 12px）
- `.status` の高さが 1 行に収まらない（縦書き）
- `.filters-area` / `.status` / `.tools` が存在しない（グループ分けがまだ無い）

**落ちることを目で確認してから Step 3 へ進む。** スクリーンショットも一度見る
（`e2e/__screenshots__/grid-header-1100.png`）── px を並べた時点では
「77px、まあ狭い」に見え、画像を見て初めて縦書きとラベル重なりが分かる。

- [ ] **Step 3: ★の `<select>` の実幅を測る（未検証 1 の消し込み）**

設計 §6 の 140px は**見積もり**で、段の切り替え閾値（約 930px）もそこから出ている。
**1600px / 右パネル開（実効 960px）の余裕は 34px しかない**ので、
**★が 175px を超えると 1600px の窓でも 2 段になり、「広い画面では 1 行」という
§6 の約束が静かに崩れる。**

`gui-frontend/e2e/gridFilter.spec.ts` に**一時的に**次を足して 1 回だけ走らせ、
出た数字を控えたら消す。**`bun run dev` は上げない** ── `playwright.config.ts` は
`reuseExistingServer: false` で必ず自分でサーバーを立てるので、
別に上げた dev サーバー（5173）は見に行かない。

```ts
  test("計測: ★の select の最小幅", async ({ page }) => {
    const m = await measureGroup(page, ".rating", "max-content");
    console.log("rating select max-content:", m.natural);
  });
```

```bash
cd gui-frontend && bun run e2e --grep "計測"
# 5174 が塞がっている環境では E2E_PORT=5199 bun run e2e --grep "計測"
```

判断:
- **175px 以下**: `.rating { width: <実測値を切り上げた値>px }` にする。ラベルはそのまま
- **175px 超**: 選択肢のラベルを詰める。`gridFilter.ts` の `RATING_PRESETS` で
  `"★が付いているもの"` → `"★あり"` に変え、`gridFilter.test.ts` は
  `RATING_PRESETS` を回しているので**変更不要**であることを確認する。
  詰めたうえで測り直し、`.rating` の幅と Step 4 の閾値を実測から出し直す

**実測値と、どちらを採ったかを実施メモに残す**（見積もりのまま次の工程へ渡さない）。

**測るのは Chromium（Playwright）だけでよい。** 実機は WebKitGTK で `<select>` の
内在幅はエンジンとフォントで変わるが、**実機で再計測はしない**（判断済み）。
根拠は、ずれても壊れ方が軽いこと ── 実幅が想定より広ければ段が 1 つ増えるだけで、
`.rating` は `flex: 0 0 auto` の固定幅なので**溢れはしない**。
**実施メモには「Chromium での実測値。WebKitGTK では未確認」と明記する。**

- [ ] **Step 4: `GridHeader` を 3 グループに分ける**

`gui-frontend/src/lib/browser/GridHeader.svelte` のマークアップを差し替える:

```svelte
<div class="grid-header">
  <div class="status">
    <!-- 絞り込み中は分母も出す。「3 枚」だけだとフォルダーが空なのか
         絞り込んだ結果なのか区別できない -->
    <span class="count">
      {#if unfilteredCount !== totalCount}{totalCount} / {unfilteredCount} 枚{:else}{totalCount} 枚{/if}
    </span>

    {#if selectionMode === "multi" && selectedCount > 0}
      <!-- 3,000 枚あれば選択中の写真は画面外に出る。✓ だけでは全容が分からないので
           ここに枚数と全解除を置く（spec §5-1） -->
      <span class="selected">
        {selectedCount} 枚選択中{#if hiddenSelectedCount > 0}<span class="hidden-note"
            >（{hiddenSelectedCount} 枚は非表示）</span
          >{/if}
      </span>
      <Button variant="text" onclick={onClearSelection}>全解除</Button>
    {/if}
  </div>

  {#if filters}
    <!-- **クラス名は `.filters` にしない。** 中に描かれる GridFilterBar の
         ルートが同じ `.filters` で、入れ子の同名は e2e の locator が
         2 つに一致して例外になる -->
    <div class="filters-area">{@render filters()}</div>
  {/if}

  <div class="tools">
    {#if controls}
      <div class="controls">{@render controls()}</div>
    {/if}

    {#if rightPanelCollapsed && primaryAction}
      <div class="primary">{@render primaryAction()}</div>
    {/if}

    <!-- 開閉ハンドルはパネルの外に置く。中にあると畳んだ瞬間に消える（spec §3-1） -->
    <IconButton
      label={rightPanelCollapsed ? "右パネルを開く" : "右パネルを畳む"}
      icon={rightPanelCollapsed ? "◧" : "◨"}
      toggle
      pressed={rightPanelCollapsed}
      onclick={onToggleRightPanel}
    />
  </div>
</div>
```

`<style>` を差し替える（`.spacer` は消える）:

```css
  /**
   * **段は `grid-template-areas` で明示する。`flex-wrap` では出せない。**
   *
   * flex アイテムはソース順にしか折り返せないので、「1 段目 = 枚数群 ＋ サイズ ＋
   * ハンドル / 2 段目 = 検索群」という並びは絶対に出ない（実測でヘッダー高さ 179px。
   * 現行の破綻した 97px よりさらに高い）。
   */
  .grid-header {
    display: grid;
    grid-template-columns: 1fr auto;
    grid-template-areas:
      "status tools"
      "filters filters";
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-3);
    border-bottom: 1px solid var(--md-sys-color-outline-variant);
    background: var(--md-sys-color-surface);
    font: var(--md-sys-typescale-body-sm);
    color: var(--md-sys-color-on-surface-variant);
  }

  /* **窓幅ではなくヘッダー自身の幅で切る。** 左右のパネルは利用者がドラッグで
     変えられるので、同じ窓幅でもヘッダーの実効幅は 460px にも 780px にもなる。
     コンテナは `.photo-grid`（PhotoGrid.svelte）が `container-type: inline-size` で作る */
  @container (min-width: 930px) {
    .grid-header {
      grid-template-columns: auto 1fr auto;
      grid-template-areas: "status filters tools";
    }
  }

  .status {
    grid-area: status;
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-width: 0;
  }

  .filters-area {
    grid-area: filters;
    min-width: 0;
  }

  .tools {
    grid-area: tools;
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }

  /* 絞り込みで桁が変わるたびに右の部品が折り返すのを防ぐ */
  .count {
    white-space: nowrap;
  }

  /* **`nowrap` が要る。** 入れないと縦書きに折り返すだけで溢れないので、
     破綻が `scrollWidth` に現れずテストで検知できない */
  .selected {
    color: var(--md-sys-color-primary);
    white-space: nowrap;
  }

  /* 主張は控えめに。選択枚数そのものより一段落とす */
  .hidden-note {
    color: var(--md-sys-color-on-surface-variant);
  }

  .controls,
  .primary {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }
```

`gui-frontend/src/lib/browser/PhotoGrid.svelte` の `.photo-grid`（`:412-418`）に足す:

```css
  .photo-grid {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
    background: var(--md-sys-color-surface);
    /* グリッドヘッダーの段組みが参照するコンテナ（GridHeader.svelte の @container）。
       inline 方向だけを封じるので高さの計算には影響しない。PhotoViewer は
       この div の外に描いているので、`contain: layout` が position: fixed の
       包含ブロックになる問題も起きない */
    container-type: inline-size;
  }
```

**閾値 930px は Step 3 の実測から出し直す。** ★が 140px なら
1 段の合計は `status 178 ＋ filters 508 ＋ tools 192 ＋ gap 12×2 ＋ padding 24 = 926px`
（設計 §6 の表）。★の実幅が変われば `filters` と合計が動くので、
**合計を切り上げた値**を `@container` の閾値にする。

`.photo-grid` の実効幅は**窓幅 − 640px**である（rail 80 ＋ 左カラム既定 240 ＋
右カラム既定 320。`.handle` は `width: 8px` ＋ `margin: 0 -4px` で正味 0 ──
`AppShell.svelte` / `columns.ts` の既定値から算出し、設計 §6 の 460 / 960 と一致することを確認済み）。
**ただしこれは変換モードの数字である**（右カラムが `convert` = 320px）。
情報モードは `metadata` = 360px なので実効幅は**窓幅 − 680px**になり、
1600px の窓でも 920px にしかならない ── 926px を下回るので 2 段になる。

**これは許容する**（判断済み）。閾値を 920px 未満へ下げると変換モードで 1 段に
詰め込む余裕が 6px 未満になり、フォントの差で溢れる方に倒れる。
2 段でも各グループは 1 行に収まり、失うのは約 44px の高さだけである。
**spec §6 の「1600px / 開 → 1 段」は変換モードの数字である**ことを実施メモに残す。

- [ ] **Step 5: 検査が通ることを確認する**

Run: `cd gui-frontend && bun run e2e --grep "段組み"`
Expected: 全件 PASS

`e2e/__screenshots__/grid-header-1100.png` と `grid-header-1600.png` を**目で見る** ──
縦書きもラベル重なりも無く、1100px は 2 段・1600px は 1 段になっていること。

- [ ] **Step 6: コンテナクエリが WebKitGTK で効くことを確認する（未検証 2 の消し込み）**

Playwright は Chromium なので、**ここは実機（Tauri = WebKitGTK）でしか確かめられない**。

```bash
make dev
```

窓を 1100px と 1600px にリサイズし、さらに**右パネルを畳む／開く**を試して、
段が切り替わることを目で見る（コンテナクエリが効いていなければ、
窓幅を変えても常に 2 段のまま、あるいは常に 1 段のまま溢れる）。

**効かなかった場合の代替**（設計 §6）: `@container` ブロックを消し、
`.grid-header` を幅に関係なく常に 2 段
（`grid-template-columns: 1fr auto` ＋ `"status tools" / "filters filters"`）にする。
約 44px を常に失うが機構は要らない。

**どちらを採ったかを実施メモに残す。**

- [ ] **Step 7: 全体の検証**

Run: `make check`
Expected: exit 0

Run: `cd gui-frontend && bun run e2e`
Expected: 全件 PASS

- [ ] **Step 8: コミット**

```bash
# `e2e/__screenshots__/` は .gitignore 済み。明示的に渡すと git add がエラーで
# 終了し、同じコマンドで並べた他のパスも 1 つも staged されない。
# spec が求めているのは「撮って目で見る」ことなので、無視したままでよい
git add gui-frontend/src/lib/browser/GridHeader.svelte gui-frontend/src/lib/browser/PhotoGrid.svelte gui-frontend/src/lib/browser/GridFilterBar.svelte gui-frontend/e2e/gridFilter.spec.ts
git commit -m "$(cat <<'EOF'
fix(gui-frontend): 写真グリッドの絞り込みと並び替え: ヘッダーの段を grid-template-areas で明示する

Phase 1 のヘッダーは最小幅で既に破綻していた。1100px 窓・右パネル開だと
「1 枚選択中」が max-content 58px に対して実幅 12px（縦書き 1 文字ずつ）、
ラベルが重なり、ヘッダーの高さが 3 行分の 97px になっていた。
★の select を足すとさらに悪化する。

flex-wrap では出せない。flex アイテムはソース順にしか折り返せないので、
「1 段目 = 枚数群 ＋ サイズ ＋ ハンドル / 2 段目 = 検索群」は絶対に出ない
（実測でヘッダー高さ 179px と、現行よりさらに高くなる）。

切り替えはヘッダー自身の幅で決まる。左右のパネルは利用者がドラッグで
変えられるので、同じ窓幅でもヘッダーの実効幅は 460px にも 780px にもなる。
@media では正しく切れないのでコンテナクエリで切る。

溢れ検査も作り直した。scrollWidth <= clientWidth は flex が縮んだだけでも
成立するので現行の破綻を検知できていなかった。中身が潰れていることは
scrollWidth に現れない（縦書きは溢れではなく折り返し）ので、
ブロックごとの実幅と自然幅を比べ、1 行に収まっているかを見て、
スクリーンショットも残す。

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 8: 未検証項目の消し込みとドキュメント

spec の「未検証（実装時に必ず潰すこと）」6 項目のうち、1・2 は Task 7、6 は Task 3/4 で
消し込み済み。残る 3・4・5 をここで潰す。**潰せなかったものは「潰せなかった」と
実施メモに書く**（黙って落とさない）。

**Files:**
- Modify: `core/tests/rating_scan_spike.rs`（`header_fallback_rate` と `drop_corpus_cache` の 2 つを追加）
- Modify: `docs/README.md`
- Modify: `docs/superpowers/specs/2026-09-08-grid-filtering-design.md`（状態の行のみ）
- Modify: `docs/superpowers/plans/2026-09-09-grid-rating-filter.md`（本ファイルに実施メモを追記）

- [ ] **Step 1: フォールバック率を数える spike を足す（未検証 3）**

軽量経路の**正しさ**はフォールバックが担保する。ここで確かめたいのは
**「フォールバックへ落ちる頻度が実用上ゼロか」**である ── 常時落ちるなら軽量経路の意味が無い。
実測は 1 機種（SOI〜SOS が 26.8KB）だけで、機種ごとの分布は分かっていない。

`core/tests/rating_scan_spike.rs` の末尾に足す:

```rust
/// **フォールバックへ落ちる頻度と、SOI〜SOS の大きさの分布**（絞り込み spec の未検証 3）。
///
/// `read_metadata_header` は手書きのセグメント走査なので、大きな MakerNote や
/// Extended XMP を持つファイルで失敗しうる。正しさは `read_metadata_fast` の
/// フォールバックが担保するが、**常時落ちるなら軽量経路の意味が無い。**
///
/// ```text
/// PICTURE_TOOL_SPIKE_DIR=/path/to/real/photos \
///   cargo test -p picture-tool-core --release --test rating_scan_spike \
///     -- --ignored --nocapture header_fallback_rate
/// ```
#[test]
#[ignore]
fn header_fallback_rate() {
    let dir = std::env::var("PICTURE_TOOL_SPIKE_DIR").expect("PICTURE_TOOL_SPIKE_DIR が要る");
    let files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("jpg") || e.eq_ignore_ascii_case("jpeg"))
        })
        .collect();
    assert!(!files.is_empty(), "JPEG が 1 枚も無い: {dir}");

    let mut failed: Vec<(PathBuf, String)> = Vec::new();
    let mut sizes: Vec<usize> = Vec::new();
    for path in &files {
        if let Err(error) = picture_tool_core::metadata::read_metadata_header(path) {
            failed.push((path.clone(), error.to_string()));
        }
        if let Ok(bytes) = header_jpeg(path) {
            sizes.push(bytes.len());
        }
    }
    sizes.sort_unstable();

    println!("枚数: {}", files.len());
    println!(
        "フォールバックへ落ちた: {} 枚（{:.2}%）",
        failed.len(),
        failed.len() as f64 * 100.0 / files.len() as f64
    );
    for (path, error) in failed.iter().take(10) {
        println!("  {} — {}", path.display(), error);
    }
    if !sizes.is_empty() {
        println!(
            "SOI〜SOS: 最小 {} / 中央 {} / 最大 {} バイト",
            sizes[0],
            sizes[sizes.len() / 2],
            sizes[sizes.len() - 1]
        );
    }
}
```

Run: 手持ちの実写真フォルダー（機種の違うものが混ざっているほど良い）に当てる。

```bash
PICTURE_TOOL_SPIKE_DIR=<実写真のフォルダー> \
  cargo test -p picture-tool-core --release --test rating_scan_spike \
    -- --ignored --nocapture header_fallback_rate
```

**判断**: 落ちた枚数が 0（または 1% 未満）なら軽量経路は実用に足る。
数割が落ちるなら、落ちた理由をログから読み、`header_bytes` の走査に
足りない分岐（未知のマーカーの扱いなど）が無いかを見る。
**数字と、当てたフォルダーの機種構成を実施メモに残す。**

- [ ] **Step 2: `list_images` の並列化が実機で効くことを確認する（未検証 4）**

実測は spike の合成コーパスで、`list_images` そのものを並列化して測ったわけではない。

コーパスは `/var/tmp/rating-spike`（1,000 枚 / 5.4GB）に残っている。

**コールドキャッシュは `sudo` を使わずに作る。** `echo 3 > /proc/sys/vm/drop_caches` は
昇格した権限が要るうえ**開発機のページキャッシュを全部落とす**ので、
対象ファイルだけに効く `posix_fadvise(POSIX_FADV_DONTNEED)` を使う ──
spike には既に `drop_page_cache`（`core/tests/rating_scan_spike.rs:194`。
`libc` は**そのための dev-dependency**）があるので、それを単独で叩く入口を足すだけでよい。

`core/tests/rating_scan_spike.rs` に足す:

```rust
/// **コーパスのページキャッシュだけを落とす**（絞り込み spec の未検証 4 の下ごしらえ）。
///
/// 実機（`make dev`）でフォルダーを開く直前に叩く。`/proc/sys/vm/drop_caches` と違い
/// **昇格した権限が要らず、開発機の他のキャッシュも巻き添えにしない。**
///
/// ```text
/// PICTURE_TOOL_SPIKE_DIR=/var/tmp/rating-spike ///   cargo test -p picture-tool-core --release --test rating_scan_spike ///     -- --ignored --nocapture drop_corpus_cache
/// ```
#[test]
#[ignore]
fn drop_corpus_cache() {
    let dir = spike_dir();
    let files = corpus(&dir);
    assert!(!files.is_empty(), "コーパスが空: {}", dir.display());
    drop_page_cache(&files);
    println!("{} 枚のページキャッシュを落とした", files.len());
}
```

手順:

```bash
PICTURE_TOOL_SPIKE_DIR=/var/tmp/rating-spike   cargo test -p picture-tool-core --release --test rating_scan_spike     -- --ignored --nocapture drop_corpus_cache
make dev
```

`/var/tmp/rating-spike` を開き、**フォルダーをクリックしてからグリッドにタイルが出るまで**を
体感で確認する。★がタイルに出ていることも同時に見る（変換モードのまま出るはず）。

比較対象が要るなら、`git stash` で Task 3 の `par_iter` を一時的に戻して同じ手順を踏む。
**数字（体感でよい）と、5,000 枚まで試したかを実施メモに残す。**
より大きなコーパスが要る場合は spike の `generate_corpus` で作れる
（`PICTURE_TOOL_SPIKE_COUNT=5000`。28.6GB の空きが要る）。

- [ ] **Step 3: Windows での挙動（未検証 5）**

`seek_relative` の効きとページキャッシュの振る舞いは Windows で測っていない。
**この開発環境（Fedora / Linux）では確かめられない。**

- Windows 機があるなら `make dev` で同じ確認をし、結果を実施メモに残す
- **無いなら「未確認」と、その理由（Windows 機が手元に無い）を実施メモに明記する。**
  推測で「たぶん動く」と書かない。フォールバックがあるので**正しさは Windows でも
  担保される**（`seek_relative` が遅くても結果は変わらない）ことも併記する

- [ ] **Step 4: ドキュメントを更新する**

`docs/superpowers/specs/2026-09-08-grid-filtering-design.md` の 3 行目:

```markdown
**状態**: Phase 1（ファイル名検索・並び替え）実装済み（`f0f12d3`）。Phase 2（★レーティング絞り込み）実装済み（実装計画と実施メモは [plans/2026-09-09-grid-rating-filter.md](../plans/2026-09-09-grid-rating-filter.md)）
```

`docs/README.md` の「現行仕様」の表で、grid-filtering-design の行から「Phase 2、未着手」を消す:

```markdown
| [specs/2026-09-08-grid-filtering-design.md](superpowers/specs/2026-09-08-grid-filtering-design.md) | **写真グリッドの絞り込み**（現行）。ファイル名検索・並び替え（Phase 1）と★レーティング絞り込み（Phase 2）。**一覧に何を出すかを決める規則**と、フォルダー全件の★を得る軽量読み取り経路。走査コストの実測値と、その計測の罠を持つ |
```

`docs/README.md` の「直近の実装計画」の表の**先頭**に足す:

```markdown
| [plans/2026-09-09-grid-rating-filter.md](superpowers/plans/2026-09-09-grid-rating-filter.md) | **★レーティング絞り込み**（Phase 2）の実装計画（全 8 タスク）と実施メモ。軽量読み取り経路のフォールバック率・`list_images` の並列化の実機確認・★の `<select>` の実幅とヘッダーの段の閾値・コンテナクエリが WebKitGTK で効いたか |
```

- [ ] **Step 5: 実施メモを書く**

本ファイルの末尾に `## 実施メモ` を足し、**spec と食い違った点**と
**実測で決めた数字**を残す。最低限、次を埋める:

| 項目 | 書くこと |
|---|---|
| ★の `<select>` の実幅（未検証 1） | 実測値。175px を超えたか。ラベルを詰めたか。**「Chromium での実測値。WebKitGTK では未確認」と明記する**（U-02） |
| コンテナクエリ（未検証 2） | WebKitGTK で効いたか。効かなければ「常に 2 段」を採ったこと |
| フォールバック率（未検証 3） | 当てたフォルダーの枚数・機種構成・落ちた枚数・SOI〜SOS の分布 |
| 並列化の実機確認（未検証 4） | 枚数とコールド／ウォーム、体感 |
| Windows（未検証 5） | 確認したか、できなかったならその理由 |
| 情報モードの段（U-04） | 1600px でも 2 段になることを許容した旨と、実測した実効幅 |
| 段の切り替え閾値 | 実測から出し直した値と、その内訳 |
| spec と食い違った点 | あれば全部。無ければ「無し」と書く |

- [ ] **Step 6: コーパスの後始末**

`/var/tmp/rating-spike`（1,000 枚 / 5.4GB）は Step 2 で使い終わる。
**Step 1〜3 がすべて終わってから**消す:

```bash
rm -rf /var/tmp/rating-spike
```

`docs/tmp/review.md` と `docs/tmp/review-2.md` は**消さない**（gitignore 対象だが
spec の「参照」節が指している）。

- [ ] **Step 7: 全体の検証**

Run: `make check`
Expected: exit 0

Run: `cd gui-frontend && bun run e2e`
Expected: 全件 PASS（Phase 1 時点の 102 件 ＋ 本計画で足した分）

- [ ] **Step 8: コミット**

```bash
git add core/tests/rating_scan_spike.rs docs/
git commit -m "$(cat <<'EOF'
docs: 写真グリッドの絞り込みと並び替え: Phase 2 の実施メモを残し、未検証を消し込む

spec の「未検証（実装時に必ず潰すこと）」6 項目の結果を実施メモに書いた。
★の select の実幅・コンテナクエリが WebKitGTK で効いたか・軽量経路が
フォールバックへ落ちる頻度・list_images の並列化の実機確認・Windows。

フォールバック率を数える #[ignore] テストを spike に常設した。軽量経路の
正しさはフォールバックが担保するが、常時落ちるなら軽量経路の意味が無い。
実測は 1 機種（SOI〜SOS が 26.8KB）だけで、機種ごとの分布は分かっていない。

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## セルフレビュー（spec との突き合わせ）

| spec の節 | 実装するタスク |
|---|---|
| §1 ★の出所は `list_images` / 並列化 | Task 3 |
| §1 消えるもの / 直すもの（7 項目） | Task 4（ストア・コマンド・型・`$effect`・`security.rs`・`gridFilter.ts` のコメントは Task 5） |
| §1 裁定 X1 の反転 | Task 4 Step 2（`metadata.spec.ts` の書き換え） |
| §1 `list_images` は `warnings` を捨てる | Task 3 Step 4 |
| §2 `read_metadata_header` | Task 1 ＋ Task 2 |
| §2 入口の 1 本化（呼び出し元 3 つ） | Task 2 Step 6（2 つ）＋ Task 3 Step 4（`list_images`） |
| §2 フォールバック | Task 2 Step 3・Step 1 のテスト |
| §2 非 JPEG の非対称 | Task 2 Step 1・Step 3 |
| §3 `RatingFilter` / 7 択 / `RATING_PRESETS` | Task 5 |
| §4 順序と index の不変条件 | Task 5（`arrangeImages` に足すだけ）・Task 6（App の `$derived` 1 箇所） |
| §4 スクロールを先頭へ戻す条件に★ | Task 6 Step 4 |
| §5 保存後の反映（値の出所・時点・失敗時） | Task 4 Step 4 |
| §5 条件から外れた写真は消える | Task 6 Step 1 の e2e 2 件 |
| §5「保存」だけを押したら操作はそこで終わる | Task 6 Step 1 の e2e |
| §6 ヘッダーの段 / コンテナクエリ / `nowrap` | Task 7（グループ容器は `.filters-area`） |
| テスト方針: レイアウト検査の作り直し | Task 7 Step 1・Step 2 |
| テスト方針: 純関数 | Task 5 Step 1 |
| テスト方針: e2e | Task 6 Step 1 |
| テスト方針: Rust | Task 1 Step 1・Task 2 Step 1・Task 3 Step 1 |
| テスト方針: スタブに★を持たせる | Task 4 Step 1 |
| 未検証 1（★の実幅） | Task 7 Step 3 |
| 未検証 2（コンテナクエリ） | Task 7 Step 6 |
| 未検証 3（SOI〜SOS の分布） | Task 8 Step 1 |
| 未検証 4（並列化の実機確認） | Task 8 Step 2 |
| 未検証 5（Windows） | Task 8 Step 3 |
| 未検証 6（DTO 契約の再生成） | Task 3 Step 7 ＋ Task 4 Step 6 |

### spec から意図的に外したこと

- **`rating === 0` の扱いを spec より 1 段だけ決め切った**（Task 5）。spec §3 は
  「`rating === 0` はここに来ない」としか書いていないが、来た場合に何もしないと
  「すべて」という名前の選択肢が写真を隠しうる。Rust 側が `xmp:Rating = 0` を
  `None` に落としているのと同じ規則（＝未評価）をフロントでも守る形にした（U-01）。

- **spec テスト方針の「max-content を各グループの実幅と比べる」を `.filters-area` だけ
  min-content に変えた**（Task 7 Step 1）。中身の `.filters` は検索欄が `min-width: 72px` まで
  縮むことを設計 §6 自身が織り込んでおり（「検索欄は 200 → 128px に縮む」）、
  max-content で比べると**その設計そのものが落ちる**。`.status` と `.tools` は
  伸縮する部品を持たないので max-content のまま。

---

## 未決だったものの決定（2026-09-09。ユーザー判断）

**未決はゼロになった。**

| ID | 決定 | 反映先 |
|---|---|---|
| **U-01** | `rating === 0` は **`null` と同じ「未評価」**として扱う。「★が付いているもの」からは消え、「すべて」「未評価だけ」には出る | Task 5 の `filterByRating` ＋ 検査 1 件 |
| **U-02** | ★の `<select>` の実幅は **Chromium での実測だけ**でよい。実機（WebKitGTK）では再計測しない。実施メモに「WebKitGTK では未確認」と明記する | Task 7 Step 3 |
| **U-03** | `sudo` は使わない。**`posix_fadvise(POSIX_FADV_DONTNEED)`** で対象ファイルだけコールドにする（spike の `drop_page_cache` を単独で叩く `#[ignore]` テストを足す） | Task 8 Step 2 ＋ `rating_scan_spike.rs` |
| **U-04** | 情報モードでは 1600px の窓でもヘッダーは 2 段。**許容する** | Task 7 Step 4 ／ Step 1 の検査名 |

### U-01 について（回答の理由に忠実に落とした結果、現案から挙動が変わった）

回答は「隠すでいい（レートがないということだと思うので）」。
**その理由をそのまま採ると、実装は「どの条件でも出さない」ではなく「未評価と同じ扱い」になる。**

- 「レートがない」＝ 未評価であり、未評価の置き場は既に `rating === null` にある
- Rust 側は `xmp:Rating = 0` を `None`（＝未評価）に落としている
  （`xmp.rs` の `parse_stars` / `mod.rs` の `percent_to_stars`）。
  **フロントだけ 0 に別の意味を与えると、同じ値が層をまたいで違う意味になる**
- 「★が付いているもの」「★n 以上」から消えるという点は、現案と**同じ結果**になる
- 違うのは「すべて」と「未評価だけ」で、**未評価扱いなら写真が消えない**。
  レビュー指摘 P-09 が問題にしていたのはまさにここである

### U-03 について（`sudo` を避けられる理由）

`/proc/sys/vm/drop_caches` は昇格した権限が要るうえ**開発機のページキャッシュを全部落とす**。
一方 `posix_fadvise(fd, 0, 0, POSIX_FADV_DONTNEED)` は**開いたファイルにだけ効き、権限も要らない**。
spike は最初からこちらを使っており（`rating_scan_spike.rs:194` の `drop_page_cache`、
`libc` はそのための dev-dependency）、実機確認でも同じ手が使える。

なお `rating_scan_spike.rs` は `std::os::unix::io::AsRawFd` を無条件に使っているため
**もともと Unix 専用**である（本計画で新しく持ち込む制約ではない）。
Windows での実機確認（未検証 5）は spike を介さず `make dev` で行う。

---

## レビュー対応記録（`docs/tmp/plan-review.md` / P-01〜P-10・Q-01〜Q-04）

**却下ゼロ。** P-01〜P-08・P-10 を反映。P-09 と Q-02〜Q-04 はユーザー判断を仰ぎ、
上の「未決だったものの決定」で U-01〜U-04 として閉じた（**未決はゼロ**）。

| ID | 判定 | 理由 |
|---|---|---|
| P-01 | 対応 | `GridHeader` と `GridFilterBar` の `<div class="filters">` が入れ子で同名なのを確認。外側を `.filters-area` に改名し、spec §6 にも「表の `.filters` は内側」を明記した |
| P-02 | 対応 | Task 3 Step 4 を「1 段ソートのまま切り出す（赤を見る）」→「2 段に直す」の 2 段に割った |
| P-03 | 対応 | スタブの `read_image_ratings` 削除を Step 5 へ移した。`metadata.spec.ts:404` が `ratings.request` の応答で★を出しており、先に消すと同時に落ちる |
| P-04 | 対応 | `--grep "★\|絞り込み中"` にした |
| P-05 | 対応（**理由は訂正**） | `bun run dev` を削除。ただし指摘の理由（ポート 5174 の衝突）は誤り ── `bun run dev` は 5173、e2e は 5174 で、衝突はしない。実際の欠陥は `reuseExistingServer: false` により **e2e が dev サーバーを一切見ない**ことで、`bun run dev` の手順が無意味だった点にある |
| P-06 | 対応 | `.gitignore:25` が `gui-frontend/e2e/__screenshots__/` を無視し、`git ls-files` も空であることを確認。`git add` から外した |
| P-07 | 対応 | spec テスト方針の取りこぼしを確認。あわせて **Q-01 を実測で解決**（下記）したので、足すテストは通ることが分かっている |
| P-08 | 対応 | `read_metadata_bytes` の doc から「spike 用（暫定）」を外す手順を Task 2 Step 5 として足した。`#[doc(hidden)]` は残す（外向けの入口を 2 つに保つため） |
| P-09 | 対応（**U-01**） | ユーザー判断により `rating === 0` を「未評価」として扱う。「すべて」から写真が消える経路が構造的に無くなり、Rust 側の 0 → `None` とも規則が揃う |
| P-10 | 対応 | 計画ファイル自身が未追跡であることを確認。パス明示に変えた |

### 疑問への回答（レビュアーが `[未確認]` としたもののうち、こちらで実測して閉じたもの）

- **Q-01（退化した JPEG を `read_metadata_bytes` が受け取れるか）— 解決。**
  `[FFD8 FFD9]` / `SOI + SOS ヘッダ + EOI`（APP1 も APP0 も無い）/ `SOI + APP0 + SOS + EOI` の
  3 形とも `Ok` かつ全項目 `None`、**警告も空**であることを実測した
  （`img_parts::Jpeg::from_bytes` と `little_exif::Metadata::new_from_vec` の両方を通る。
  `has_exif_segment` が偽なので「Exif を解析できませんでした」は積まれない）。
  Task 1 の「EOI で `Ok` を返す」判断と、P-07 で足すテストはどちらも成立する
- **Q-02（1600px での `.photo-grid` の実効幅）— 解決（派生した U-04 も決定済み）。**
  実効幅は**窓幅 −（rail 80 ＋ 左カラム ＋ 右カラム）**。`.handle` は
  `width: 8px` ＋ `margin: 0 -4px` で正味 0 なので幅を食わない
  （`AppShell.svelte:171-174`）。変換モードの既定（左 240 / 右 320）で
  1100 → 460px、1600 → 960px となり **spec §6 の実測値と一致する**。
  情報モードは右が 360 なので 1600 → 920px で、1 段に要る 926px を下回る → U-04
- **Q-03（`drop_caches` の sudo）— 解決（U-03）。** `sudo` は使わず、spike が既に持つ
  `drop_page_cache`（`posix_fadvise(POSIX_FADV_DONTNEED)`）を単独で叩く `#[ignore]` テストを足す。
  権限が要らず、開発機の他のキャッシュも巻き添えにしない
- **Q-04（★の実幅をどのエンジンで測るか）— 解決（U-02）。** Chromium での実測だけを採る。
  実施メモに「WebKitGTK では未確認」と明記する

---

## 実施メモ

| 項目 | 内容 |
|---|---|
| ★の `<select>` の実幅（未検証 1） | 実測 166px（設計の見積もり 140px から確定）。175px 以下なのでラベルは詰めていない（`RATING_PRESETS` は変更なし）。**Chromium（Playwright）での実測値。WebKitGTK では未確認**（U-02。実機での再計測はしない方針） |
| コンテナクエリ（未検証 2） | **未確認。** Task 7 Step 6（実機・WebKitGTK での目視確認、`make dev` が要る）は controller の裁定によりユーザーへ回されている。`@container (min-width: 952px)` は実装済みで、効かなかった場合の代替（`.grid-header` を常に 2 段にする）は採っていない。効くかどうかは実機確認待ち |
| フォールバック率（未検証 3） | **（未実施 — ユーザーの実機確認待ち）。** `header_fallback_rate` を `core/tests/rating_scan_spike.rs` に追加した（コードのみ）。`PICTURE_TOOL_SPIKE_DIR` に実写真フォルダーを渡して実行する必要があり、この環境には機種の異なる実写真が無いため実行していない |
| 並列化の実機確認（未検証 4） | **（未実施 — ユーザーの実機確認待ち）。** `drop_corpus_cache` を `core/tests/rating_scan_spike.rs` に追加した（コードのみ）。コーパス `/var/tmp/rating-spike`（1,000 枚 / 5.4GB、現存を確認済み）で `drop_corpus_cache` を叩いたのち `make dev` でフォルダーを開き、タイルが出るまでの体感と★の表示を目視確認する手順が残っている |
| Windows（未検証 5） | **未確認。** この開発環境は Fedora / Linux で、Windows 機が手元に無いため確認していない。`seek_relative` の効きとページキャッシュの振る舞いは未計測だが、`read_metadata_fast` にフォールバックがあるため、Windows で軽量経路が遅い・失敗するとしても**正しさ（返る★の値）は担保される**（結果は変わらない） |
| 情報モードの段（U-04） | 1600px の窓でも 2 段になることを許容した。情報モードの `.photo-grid` 実効幅は窓幅 − 680px（rail 80 ＋ 左カラム既定 240 ＋ 右カラム既定 360）で、1600px → 920px。段の切り替え閾値 952px を下回るため常に 2 段になる（変換モードは右カラム 320px なので 1600px → 960px となり 1 段になる） |
| 段の切り替え閾値 | 実測から出し直した **952px**（設計の見積もり 930px から確定）。内訳: `.status` の max-content 177.16px ＋ `.filters-area` の max-content 534.00px ＋ `.tools` の max-content 192.00px の合計に gap（`--space-3` × 2）と左右 padding を足した 951.16px を切り上げた値 |
| spec と食い違った点 | Task 8 の範囲（Step 1・2・4）で新たに見つかった食い違いは無い。計画全体を通じて生じた食い違いは既に「未決だったものの決定」節の U-01〜U-04 として決定済み（U-04 は「情報モードの段」の行を参照） |
