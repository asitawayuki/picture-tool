# メモリ予算 実装計画

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 画像を開く処理がすべて、開く前に「1 枚のピーク見積もり」をプロセス共通の予算（物理メモリの 1/2）から借りるようにし、メモリの少ない PC でもプロセスごと落ちないようにする。

**Architecture:** `core` に新しく `budget` モジュールを置く。`budget/mod.rs` が予算の型（`Mutex` ＋ `Condvar` ＋ 到着順の券）と RAII の借用、`budget/estimate.rs` が 1 枚の見積もり。`decode` に「ヘッダだけ読む」`probe` を足し、デコードはその結果（`Header`）を受け取って、見積もりから導いた上限で開き、寸法が変わっていれば `Err` にする。一括変換は専用の rayon プールで回し、各ワーカーが「取り消し確認 → 借りる → 取り消し確認 → 借りない内部版」の順に進む。

**Tech Stack:** Rust（`picture-tool-core`）、rayon 1.10（専用プール）、sysinfo 0.39（`default-features = false, features = ["system"]`。物理メモリの総量だけ）、jxl-oxide 0.12.6（`max_alloc`）、dav1d 0.11.1、ImageMagick（libjxl 0.11.2）／ffmpeg 8.1.3（計測用の入力の生成）

**Spec:** [`docs/superpowers/specs/2026-10-28-memory-budget-design.md`](../specs/2026-10-28-memory-budget-design.md)

## Global Constraints

- **検証は `make check`。** 各タスクの終わりで exit 0 にする。lint は `cargo clippy --workspace --all-targets -- -D warnings`
  （**未使用コードも失敗になる**）。Task 1〜4 の間、`budget` は本番コードから使われないので、`lib.rs` の `mod budget;` に
  `#[cfg_attr(not(test), allow(dead_code))]` を付けて通し、**Task 5 で外す**（外した状態で clippy が通ることが Task 5 の完了条件）
- **予算 = 物理メモリの総量 × 1/2、上限 = 総量 × 3/4。** 総量が取れない（`sysinfo` が 0 を返す）ときは総量を
  **7.5GiB（8,053,063,680 バイト）とみなす**。予算 3.75GiB（4,026,531,840）、上限 5.625GiB（6,039,797,760）。
  spec の「5.6GiB」はこの値の丸め（D-7）
- **GB は 10^9 バイト、GiB は 2^30 バイト**（spec §1）。エラー文言の GB も 10^9 で、小数 1 桁
- **`core` に `unsafe` を書かない。依存は sysinfo を `default-features = false, features = ["system"]` で 1 つだけ足す**
- **予算はプロセス全体で 1 つ（`budget::global()`）。テストは自分で作った `MemoryBudget` を渡し、全体を差し替える口は作らない**
- **入れ子で借りない。** 借りるのは「借りる場所」（下の表）の 1 回だけで、内部ではすべて借りない版を使う。
  同じスレッドで 2 度借りようとしたら `Err` を返す（D-5。デッドロックにしない）
- **1 枚の処理の中で rayon の並列 API（`par_iter`・`join`・`image` の `par_*`）を使わない。** 借りたワーカーが
  `join` の待ちで別のファイルの処理を盗み、同じスレッドで入れ子に借りる（D-6）。現状は使っていない（Task 6 で確認）
- **既存の 1 枚あたりの上限（出力 512MB、AVIF の宣言寸法による判定）とその文言は変えない。** 判定は `probe` へ移すだけ（D-1）
- **ユーザー向けの設定手段（CLI オプション・GUI 設定・環境変数）を作らない**
- **`core` は `eprintln!` しない**（CLAUDE.md）。計測用 example の出力は example 側で行う
- **計測は release ビルド**で、**5 回測って中央値**。値は全回分を実施メモに貼る（spec「完了の確かめ方」、
  memory: copy-measured-values-verbatim — 要約せず全要素、貼った後に件数を数えて照合する）。ピーク RSS は
  `/usr/bin/time -f "%M %e"`
- **x66（利用者の実物の JPEG XL）は `~/.local/share/Trash/files/20261010-DSC05534.jxl`。読み取り専用でスクラッチへコピーして使う。**
  消えていたら作業を止めて利用者に代わりを求める（合成画像では代わりにならない。memory: real-lrc-jxl-sample-location）
- **爆弾系の入力は中身が正当な単色画像で作り、ガードを外して落ちる（または理由が変わる）ことを実測する**
  （memory: bomb-fixtures-must-be-otherwise-valid）
- **否定形の検査（「追い越さない」「書かれない」「塞がない」）には前提条件の assert を添える**
  （memory: negative-assertion-tests-need-a-precondition）
- **テストを書く・直す前に `test-integrity` スキルを起動する**（グローバル指示）
- **この環境の `cargo test` は出力が要約される。** 出力が要るときはテストバイナリを直接実行してスクラッチへリダイレクトする
  （memory: cargo-test-output-is-summarized）。長いコマンドはフォアグラウンドで走らせる
- **`target/` を共有したまま並行ビルドしない**（memory: ci-clippy-drifts-ahead-of-local-toolchain）。比較用の main の
  ビルドは別の `--target-dir` を使う
- **コミットは `commit-message` スキルに従う。** `<対応の概要>` は直近 commit（`f9a2e15`）から逐語継承して **`メモリ予算`**。
  footer は `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`
- **push・PR は利用者の確認を取ってから**（memory: pr-merge-is-run-by-the-user）

### 借りる場所（spec §3）

| 入口 | 借りる見積もり | 内部で呼ぶもの |
|---|---|---|
| `process_batch`（CLI はこれだけ） | `Work::Convert` | `process_batch_in` → 各要素で借りて `process_image_unbudgeted` |
| `process_image`（公開） | `Work::Convert` | 自分で借りて `process_image_unbudgeted` |
| `open_image_oriented`（公開） | `Work::View` | `open_oriented_unbudgeted` |
| `generate_thumbnail_base64` | `Work::View` | `open_oriented_unbudgeted` |
| `generate_full_image_base64` | `Work::View` | `open_oriented_unbudgeted` |
| `ExifFramePreviewSource::load` | `Work::View` | `decode::open`（借りない） |

対象外: `image_dimensions_oriented`・`read_exif_info`・メタデータの読み書き・`generate_exif_frame_preview_base64`（下地は縮小済み）・
Exif フレームのロゴ（`exif_frame/logo.rs`）。

### エラーの文言

テストは `format!("{err:#}")` に次の文字列が含まれること（または `downcast_ref` で型）で理由を照合する。

| 判定 | 文言 | 置き場所 |
|---|---|---|
| 3/4 の上限を超える | `Image needs about {needed:.1} GB, over the memory limit of {limit:.1} GB`（型 `OverMemoryLimit`） | `budget/mod.rs` |
| 同じスレッドで 2 度借りた | `memory budget lease is already held by this thread` | `budget/mod.rs` |
| 見積もり後にファイルが変わった | `image changed between probing and decoding` | `decode/mod.rs` |
| 一括変換のプールが作れない | `Failed to start the worker pool` | `lib.rs` `process_batch_in` |
| 既存（変えない） | `AVIF declares a size over the 512MB output limit` ほか、既存の文言すべて | 既存のまま |

## Review Focus

spec が求めているがテストで直接は押さえにくい、利用者が踏みやすい順:

1. **一括変換の最中に GUI でフォルダーを開く** → 一覧（`collect_image_entries` の `par_iter`）が待たされずに出ること。
   → Task 6 の `waiting_batch_does_not_block_the_global_pool`
2. **予算待ちの最中に「取り消し」** → 待っていたワーカーは変換せずに抜け、出力が書かれないこと。進捗のコールバックは
   1 枚終わるまで呼ばれないので、取り消しは「1 枚目の完了」から始まる。→ Task 6 の `cancel_while_waiting_writes_nothing`
3. **見積もりの後、デコードの前にファイルが差し替わる**（同期フォルダー・上書き保存） → 上限が見積もりと無関係にならず `Err`。
   → Task 4 の `decode_rejects_a_file_changed_after_probe`（JPEG XL・AVIF・PNG の 3 形式）
4. **512MB を超える爆弾が、小さい PC で「単独処理の待ち」に入ってから弾かれる** → 待たずに既存の文言で弾かれること
   （D-1）。→ Task 4 の `over_output_limit_is_rejected_by_probe_before_borrowing`
5. **1 枚が `Err`・panic で終わる** → 借りた分が返り、後続が永遠に待たないこと。→ Task 1 の
   `lease_is_returned_on_panic`、Task 5 の `failed_open_returns_the_lease`

## spec を実装者の目で読んで決めたこと

- **D-1 既存の 512MB 判定を `probe` で先に行う。** 見積もり → 借りる → デコードの順だと、出力が 512MB を超える
  ファイルが、メモリの少ない PC では「単独処理」の待ちに入ってから弾かれる（全員の処理を止めて結局失敗）。また
  既存のテスト（`over_limit.jxl` など）の結果が実行する機械のメモリで変わる。`probe` はヘッダだけで 512MB 判定まで
  済ませ、通ったものだけを見積もる。spec「完了の確かめ方」の「出力 512MB の上限で先に弾かれないよう寸法を選ぶ」はこの順を前提にしている
- **D-2 AVIF の `probe` はファイル全体を読み、`avif_parse::read_avif` でシーケンスヘッダの宣言（主画像とアルファ）を得る。**
  デコードは読み直して宣言を比べ、違えば `Err`。`probe` の読んだバイト列を持ち越さないのは、待っているワーカーが
  予算外のバイト列を抱えたまま並ぶため（20 スレッドぶん）
- **D-3 JPEG / PNG / WebP の向きは `probe` で読まず、見積もりは縦横両方の向きの大きいほうを取る。** spec §2 が最悪値の
  仮定を許している。ビット深度とアルファは `into_decoder` の `color_type()` で読む（安い）
- **D-4 JPEG XL の `max_alloc` は見積もりの作業領域の項そのもの。** `estimate::jxl_working_bytes(&Header)` を
  `decode::jxl_decoder` と見積もりの両方が呼び、値を 2 箇所に書かない
- **D-5 同じスレッドの 2 度目の借用は `Err`。** スレッドローカルの印で検知する。`Lease` は `!Send`（印をスレッドに結びつけるため）
- **D-6 一括変換は呼び出しのたびに専用の rayon プールを作る**（スレッド数は既定 = グローバルプールと同じ）。
  作るコストは 1 回あたり数 ms（Task 6 で測って実施メモに書く）
- **D-7 総量が取れないときは「総量 7.5GiB」として `from_total` に通す。** 予算 3.75GiB と上限 5.625GiB が 1 つの式から出る
- **D-8 借用の記録（`Event`）は `MemoryBudget::with_log` で作ったものだけが取る。** プロセス全体の予算は取らない
  （GUI はサムネイルのたびに借りるので、記録が際限なく伸びる）
- **D-9 待ちの最中に取り消しで即座に抜ける仕組みは作らない**（spec「実装者に任せること」で任意）。③の再確認だけ
- **D-10 見積もりの「作業」は 2 種類。** `Work::Convert { mode, max_width, exif_frame }` と `Work::View`（開いて
  縮小・エンコードするだけ。サムネイル・ビューアー・プレビューの下地・`open_image_oriented`）

## ファイル構成

### 新規

| ファイル | 責務 |
|---|---|
| `core/src/budget/mod.rs` | `MemoryBudget`・`Lease`・`OverMemoryLimit`・`Event`、`from_total`、`global()`（sysinfo） |
| `core/src/budget/estimate.rs` | `Work`、`estimate(&Header, &Work) -> u64`、`jxl_working_bytes`、係数の定数と根拠のコメント |
| `core/examples/jxl_alloc_probe.rs` | 1 枚の JPEG XL が通る最小の `max_alloc` を二分探索して表示する（係数の再計測用） |
| `core/examples/budget_batch.rs` | 予算を引数で渡して一括変換し、借用の記録を CSV に書く。`--estimate` で見積もりだけ表示する |

### 変更

| ファイル | 変更 |
|---|---|
| `core/Cargo.toml` | `sysinfo` を追加（コメントで機能を絞った理由） |
| `core/src/decode/mod.rs` | `Header`・`probe` を追加。`open`/`open_with_warnings` は `&Header` を受ける。512MB 判定を `probe` へ移す。冒頭の「メモリの上限」節 |
| `core/src/lib.rs` | `mod budget`。借りる場所の表のとおり公開関数を包む。`process_batch_in`（専用プール）。`#[doc(hidden)]` の `process_batch_with_budget`・`estimate_conversion`・`MemoryBudget` の再公開。冒頭の公開 API の地図 |
| `README.md` | 「出力」節の既知の制限 |
| `docs/superpowers/specs/2026-10-14-avif-decoder-replacement-design.md` | 冒頭の「姉妹 spec（未作成）」をリンクに |
| `docs/superpowers/specs/2026-10-28-memory-budget-design.md` | 冒頭の「状態」を本計画へのリンクに |
| `docs/README.md` | 現行仕様と直近の実装計画の表に追加 |

---

## Task 1: 予算の型（借りる・待つ・返す）

**Files:**
- Create: `core/src/budget/mod.rs`
- Modify: `core/src/lib.rs`（`mod budget;` を足すだけ）

**Interfaces:**
- Produces:
  - `pub struct MemoryBudget`（`#[doc(hidden)]` で再公開するのは Task 7）
  - `MemoryBudget::new(budget: u64, limit: u64) -> Self`（`limit < budget` は `budget` に持ち上げる）
  - `MemoryBudget::with_log(budget: u64, limit: u64) -> Self`
  - `fn acquire(&self, estimate: u64) -> anyhow::Result<Lease<'_>>` — 待つ。`estimate > limit` は待たずに `Err(OverMemoryLimit)`
  - `fn budget(&self) -> u64`、`fn limit(&self) -> u64`、`fn in_use(&self) -> u64`
  - `fn events(&self) -> Vec<Event>`（`with_log` でなければ空）
  - `pub struct Lease<'a>`（Drop で返す。`!Send`）
  - `pub struct OverMemoryLimit { pub needed: u64, pub limit: u64 }`（`Display` は文言の表のとおり、`std::error::Error`）
  - `pub struct Event { pub at: Duration, pub kind: EventKind, pub estimate: u64, pub in_use: u64, pub holders: usize }`、
    `pub enum EventKind { Wait, Acquire, Release }`。`in_use`・`holders` はその事象の**後**の値。`at` は予算の作成からの経過

- [ ] **Step 1: `test-integrity` スキルを起動し、spec「待ちの性質をテストで確かめる」の 5 項目からテストを設計する**

- [ ] **Step 2: 失敗するテストを `budget/mod.rs` の `#[cfg(test)] mod tests` に書く**

待つ側は別スレッドで走らせ、「待ちに入った」ことは `events()` の `Wait` で確かめる（`sleep` で順序を作らない）。
共通の補助 `wait_until(|| cond, Duration::from_secs(10))` を置き、時間切れは panic。

```rust
#[test]
fn later_small_request_does_not_overtake_a_waiting_large_one() {
    let b = MemoryBudget::with_log(100, 150);
    let a = b.acquire(60).unwrap();
    // B(50) は入らない（60+50>100）。C(30) は追い越しを許せば入る
    assert!(60 + 30 <= 100, "前提: C は追い越しを許せば入る大きさ");
    std::thread::scope(|s| {
        s.spawn(|| drop(b.acquire(50).unwrap()));            // B
        wait_until(|| waits(&b) == 1);
        s.spawn(|| drop(b.acquire(30).unwrap()));            // C
        wait_until(|| waits(&b) == 2);
        assert_eq!(acquires(&b), vec![60], "B が待つ間、C は借りていない");
        drop(a);
    });
    assert_eq!(acquires(&b), vec![60, 50, 30], "到着順");
}

#[test]
fn over_budget_request_runs_alone() {
    let b = MemoryBudget::with_log(100, 150);
    let a = b.acquire(30).unwrap();
    std::thread::scope(|s| {
        s.spawn(|| drop(b.acquire(120).unwrap()));   // 予算超え・上限以下
        wait_until(|| waits(&b) == 1);
        s.spawn(|| drop(b.acquire(10).unwrap()));
        wait_until(|| waits(&b) == 2);
        drop(a);
    });
    // 120 は holders == 1 で借り、10 は 120 の Release より後に借りる
    let ev = b.events();
    let solo = position(&ev, EventKind::Acquire, 120);
    assert_eq!(ev[solo].holders, 1);
    assert!(position(&ev, EventKind::Acquire, 10) > position(&ev, EventKind::Release, 120));
}

#[test]
fn over_limit_request_fails_without_waiting() {
    let b = MemoryBudget::with_log(100, 150);
    let _held = b.acquire(100).unwrap();     // 待つなら永遠に待つ状態
    let err = b.acquire(151).unwrap_err();
    let over = err.downcast_ref::<OverMemoryLimit>().unwrap();
    assert_eq!((over.needed, over.limit), (151, 150));
    assert_eq!(waits(&b), 0);
}

#[test]
fn over_limit_message_names_both_sizes_in_gb() {
    let e = OverMemoryLimit { needed: 7_100_000_000, limit: 6_000_000_000 };
    assert_eq!(e.to_string(), "Image needs about 7.1 GB, over the memory limit of 6.0 GB");
}

#[test]
fn lease_is_returned_on_panic() {
    let b = MemoryBudget::new(100, 150);
    let r = std::thread::scope(|s| s.spawn(|| { let _l = b.acquire(100).unwrap(); panic!("x") }).join());
    assert!(r.is_err());
    assert_eq!(b.in_use(), 0);
}

#[test]
fn second_lease_on_the_same_thread_is_an_error() {
    let b = MemoryBudget::new(100, 150);
    let _l = b.acquire(10).unwrap();
    let err = b.acquire(10).unwrap_err();
    assert!(format!("{err:#}").contains("memory budget lease is already held by this thread"));
    // 別の予算でも同じ（入れ子の禁止は予算ごとではなくスレッドごと）
    assert!(MemoryBudget::new(100, 150).acquire(10).is_err());
}

#[test]
fn the_thread_can_borrow_again_after_returning() {
    let b = MemoryBudget::new(100, 150);
    drop(b.acquire(10).unwrap());
    assert!(b.acquire(10).is_ok());
}
```

`waits`・`acquires`・`position` はテスト内の補助（`events()` を絞るだけ）。

- [ ] **Step 3: 失敗を確かめる**

Run: `cargo test -p picture-tool-core budget::`
Expected: コンパイルエラー（`MemoryBudget` が無い）

- [ ] **Step 4: `budget/mod.rs` を実装する**

状態は `Mutex<State { in_use: u64, holders: usize, next_ticket: u64, serving: u64, log: Option<Vec<Event>> }>` ＋ `Condvar`。
`acquire` の判定（ここだけは式で決める）:

```text
estimate > limit                       → 待たずに Err(OverMemoryLimit)
スレッドローカルの印が立っている          → Err（文言の表）
ticket = next_ticket++ ; Wait を記録（すぐ借りられるときは記録しない）
待つ条件: ticket != serving || !(estimate <= budget - in_use || (estimate > budget && holders == 0))
借りたら: serving += 1, in_use += estimate, holders += 1, Acquire を記録, notify_all, 印を立てる
Drop:     in_use -= estimate, holders -= 1, Release を記録, notify_all, 印を下ろす
```

予算を超える 1 枚が借りている間は `in_use > budget` なので、後ろは自然に待つ（単独処理）。`Mutex` の poison は
`into_inner` で続行する（panic したワーカーの後も他が借りられるように）。`Condvar::notify_all` の起床順に頼らず、
券（`serving`）で順番を守る旨をコメントに書く（spec §3）。

- [ ] **Step 5: 通ることを確かめる**

Run: `cargo test -p picture-tool-core budget::` → 7 件 PASS。続けて `make check` → exit 0

- [ ] **Step 6: 追い越しを許す版に一時的に書き換え（`ticket != serving ||` を外す）、`later_small_request_does_not_overtake_a_waiting_large_one` が落ちることを確かめて戻す。** 結果を実施メモに書く

- [ ] **Step 7: Commit** — `feat(core): メモリ予算: 到着順に借りて単独処理もできる予算の型を足す`

---

## Task 2: 物理メモリの総量からプロセス全体の予算を決める

**Files:**
- Modify: `core/Cargo.toml`、`core/src/budget/mod.rs`

**Interfaces:**
- Consumes: `MemoryBudget::new`（Task 1）
- Produces:
  - `pub(crate) fn from_total(total: Option<u64>) -> (u64, u64)` — `(予算, 上限)`。`None` は 7.5GiB とみなす
  - `pub(crate) fn global() -> &'static MemoryBudget`（`OnceLock`。初回に 1 回だけ総量を読む）

- [ ] **Step 1: 失敗するテストを書く**

```rust
#[test]
fn budget_is_half_and_limit_is_three_quarters_of_total() {
    assert_eq!(from_total(Some(62_000_000_000)), (31_000_000_000, 46_500_000_000));
}

#[test]
fn unknown_total_is_treated_as_a_7_5_gib_machine() {
    assert_eq!(from_total(None), (4_026_531_840, 6_039_797_760));
    assert_eq!(from_total(None), from_total(Some(7 * (1 << 30) + (1 << 29))));
}
```

- [ ] **Step 2: 失敗を確かめる** — Run: `cargo test -p picture-tool-core budget::` / Expected: `from_total` が無い

- [ ] **Step 3: sysinfo を足し、`from_total` と `global` を実装する**

`core/Cargo.toml` に `sysinfo = { version = "0.39", default-features = false, features = ["system"] }` と、
「物理メモリの総量だけを読む。unsafe を core に書かずに Windows でも取るため。他の機能は切る」のコメント。
API は sysinfo 0.39 の docs で確かめる（context7 か docs.rs。想定は
`System::new_with_specifics(RefreshKind::nothing().with_memory(MemoryRefreshKind::nothing().with_ram())).total_memory()`、
0 は `None` 扱い）。`from_total` の掛け算は `u64` で溢れない範囲（総量 / 4 × 3）で行う。

- [ ] **Step 4: 通ることを確かめる** — `cargo test -p picture-tool-core budget::` PASS、`make check` exit 0、
  `cargo tree -p picture-tool-core -e features -i sysinfo` で `system` 以外の機能が有効でないことを確かめて実施メモに貼る

- [ ] **Step 5: Commit** — `feat(core): メモリ予算: 物理メモリの総量からプロセス全体の予算を決める`

---

## Task 3: 係数を決めるための計測（コードを書く前）

spec §2「係数は実装計画で実測して決める」。**このタスクの成果物は実施メモの表と、Task 4 の係数の値。**

**Files:**
- Create: `core/examples/jxl_alloc_probe.rs`

**Interfaces:**
- Produces: 実施メモ「Task 3」の表と、Task 4 に渡す係数（下の「決めるもの」）

- [ ] **Step 1: 入力を作る**（スクラッチパッドの `inputs/`。作り方は spec「実測」節の入力の欄をそのまま使う）

| 名前 | 作り方 |
|---|---|
| j33 | `cp docs/tmp/20260328-DSC05024.jpg` |
| p33 / w33 / x33 | `magick j33.jpg p33.png` / `magick j33.jpg -quality 85 w33.webp` / `magick j33.jpg -quality 85 x33.jxl` |
| a33 | `ffmpeg -i j33.jpg -c:v libaom-av1 -still-picture 1 -cpu-used 8 -crf 30 -pix_fmt yuv420p a33.avif` |
| x66 | `cp ~/.local/share/Trash/files/20261010-DSC05534.jxl x66.jxl && chmod a-w x66.jxl` |
| bomb | `magick -size 12000x14000 xc:'#806040' -depth 8 -type TrueColor -quality 85 bomb.jxl` |
| j33o6 | j33 に Orientation 6 を入れる（`exiftool -Orientation#=6 -o j33o6.jpg j33.jpg`。無ければ `magick j33.jpg -orient RightTop j33o6.jpg`） |
| p33a / w33a | アルファ付き: `magick j33.jpg -alpha set -channel A -evaluate set 60% +channel p33a.png`（WebP も同様に `-quality 85`） |
| p33x16 | 16bit PNG: `magick j33.jpg -depth 16 p33x16.png` |
| j33p | プログレッシブ JPEG: `magick j33.jpg -interlace JPEG -quality 92 j33p.jpg` |
| x66a | 16bit RGBA の JPEG XL: `magick x66.jxl -alpha set -channel A -evaluate set 60% +channel -depth 16 -quality 85 x66a.jxl`（`-fx` は 66MP で遅すぎるので使わない） |

作った後、`cargo build --release -p picture-tool-core --example avif_probe` の probe で全部を開き、寸法と色型を確かめる
（x66 は `6656x9984 Rgb16`、x66a は `6656x9984 Rgba16`、bomb は `12000x14000 Rgb8`）。出力を実施メモに貼る。
x66a が `Rgba16` にならなければ作り方を直す（ここで先へ進まない）。

- [ ] **Step 2: `jxl_alloc_probe` を書く**

`jxl_alloc_probe <path>`: `jxl_oxide::integration::JxlDecoder::with_thread_pool(BufReader::new(File::open(path)?), JxlThreadPool::none())`
に `max_alloc = m` を設定して `DynamicImage::from_decoder` が `Ok` になる最小の `m` を、1MiB 刻みの二分探索で求め、
`<path> <w>x<h> <color> min_max_alloc=<bytes> per_pixel=<bytes/画素, 小数 2 桁>` を 1 行出す。上端は `64 B/画素 × 画素数 + 64MiB`
（現行の値）から始め、それで通らなければその旨を出して終了コード 1。冒頭のコメントに「係数の再計測用。結果は
`docs/superpowers/plans/2026-10-28-memory-budget.md` の実施メモ」と書く。

- [ ] **Step 3: JPEG XL の作業領域を測る** — x33・x66・x66a・bomb・`core/tests/fixtures/lossy_24mp.jxl` で
  `jxl_alloc_probe` を release で 1 回ずつ（決定的なので 1 回でよい。理由を実施メモに書く）。表を実施メモに貼る

- [ ] **Step 4: 現行コードでピーク RSS を測る**（5 回の中央値）

- `avif_probe`（デコードのみ）: 全入力
- CLI（`target/release/picture-tool`、既定の品質 90・`--max-size 8`）の 1 枚の変換: 全入力 × crop / pad、x66・x66a・j33 は pad＋`-e` も
- 入力 1 枚だけを入れたフォルダーで測る。表は spec の実測表と同じ形で実施メモに貼る

- [ ] **Step 5: コードを読んで確かめる**（結果を実施メモに書く）
  - `DynamicImage::from_decoder(decoder)` は decoder を消費し、作業領域はデコードの後に解放される
  - AVIF: `av1::decode` の `Picture`（YUV 平面）は `yuv::to_image` が終わるまで生きている（デコード区間に出力 RGB と同時に数える）
  - AVIF: libdav1d が 1 アイテムのデコード中に持ちうるピクチャの枚数。既定は **参照 8 枚＋出力 1 枚＋グレインの複製 1 枚 = 10 枚**。
    `max_frame_delay = 1`・`n_threads = 1` でこれより少なくなると libdav1d のソースで言える場合だけ減らし、該当箇所を引用する
  - `jpeg-encoder` が出力キャンバス 1 画素あたりに確保する量（RSS の差から見積もり、ソースで裏を取る）
  - zune-jpeg（`image` の JPEG）がプログレッシブで係数を保持する量

- [ ] **Step 6: 係数を決める**（決めるもの。決め方の規則）

| 係数 | 決め方 |
|---|---|
| JPEG XL の作業領域（B/画素、深度・アルファで分けてよい）と固定分 | Step 3 の `per_pixel` の最大 × 1.25 を切り上げ。固定分は `JXL_WORKING_BASE`（64MiB）を残す |
| AVIF の 1 ピクチャ（宣言画素 × 平面 3 × サンプルのバイト数）と枚数 | Step 5 の枚数。間引き（4:2:0）は宣言から分からないので 4:4:4 とみなす |
| JPEG / PNG / WebP のデコーダ内部（B/画素） | 「avif_probe の RSS − 出力 − 起動時の RSS」の最大（プログレッシブ JPEG を含む）× 1.25 |
| ファイルの大きさの項 | AVIF 2 倍（全体の読み込み＋アイテムの複製）、他 1 倍。計測で足りなければ増やす |
| エンコーダ（出力キャンバスの B/画素） | Step 5 の値 × 1.25 |

**関門**: 決めた係数で、6656×9984 の JPEG XL（16bit RGB と 16bit RGBA、ファイル 5.1MB）の `Work::Convert { Pad, None, exif_frame: true }` の
見積もりを手計算し、どちらも **4,026,531,840 以下**であることを確かめる（spec §2 の不等式）。超えるなら、余裕（× 1.25）を
縮めて 1.10 まで下げてよい。1.10 でも超えるなら**作業を止めて利用者に報告する**（spec の前提が崩れている）。

- [ ] **Step 7: Commit** — 2 つに分ける: `feat(core): メモリ予算: JPEG XL の作業領域を測る example を足す`（`jxl_alloc_probe.rs`）と
  `docs(docs): メモリ予算: 係数を決めた計測を実施メモに残す`（本計画の実施メモ）

---

## Task 4: ヘッダを読む `probe` と 1 枚の見積もり

**Files:**
- Create: `core/src/budget/estimate.rs`
- Modify: `core/src/decode/mod.rs`、`core/src/lib.rs`（`decode::open` の呼び出し元を `probe` 経由に直す。借りるのは Task 5）

**Interfaces:**
- Consumes: Task 3 の係数
- Produces:
  - `pub(crate) struct Header { pub(crate) format: Format, pub(crate) width: u32, pub(crate) height: u32, pub(crate) color: image::ColorType, pub(crate) file_len: u64, pub(crate) avif: Option<AvifDeclared> }`
    — `width`/`height` は JPEG XL・AVIF では向き適用後、他は生（`decode::dimensions` と同じ意味）。`color` は出力の色型
    （AVIF は主画像の深度とアルファの有無から決める）
  - `pub(crate) struct AvifDeclared { pub(crate) primary_pixels: u64, pub(crate) alpha_pixels: Option<u64>, pub(crate) bit_depth: u8 }`
    （`#[derive(Debug, Clone, Copy, PartialEq, Eq)]`。`decode/mod.rs` に置く。`avif::Declared` は `pub(super)` のままにし、
    `decode` の外へ出さない）
  - `pub(crate) fn probe(path: &Path) -> Result<Header>` — 形式を判定し、ヘッダだけで寸法・色型を読み、**既存の 512MB 判定まで行う**（D-1）。
    読めなければ `Err`（fail-closed）。`guarded` で包み、文脈は `Failed to open image: {path}`
  - `pub(crate) fn open(path: &Path, header: &Header) -> Result<DynamicImage>`、`pub(crate) fn open_with_warnings(path: &Path, header: &Header) -> Result<(DynamicImage, Vec<String>)>`
    — デコーダが読んだ寸法・色型（AVIF は宣言）が `header` と違えば `image changed between probing and decoding`
  - `pub(crate) enum Work { Convert { mode: ConversionMode, max_width: Option<u32>, exif_frame: bool }, View }`
  - `pub(crate) fn estimate(header: &Header, work: &Work) -> u64`
  - `pub(crate) fn jxl_working_bytes(header: &Header) -> u64`（D-4。`jxl_decoder` の `max_alloc` にこの値を渡す）

- [ ] **Step 1: `test-integrity` を起動し、spec §2 の条件（実際のピーク以上・x66 の不等式・ファイルの大きさの項・寸法の食い違い・fail-closed）からテストを設計する**

- [ ] **Step 2: 失敗するテストを書く**

`estimate.rs` の単体テスト（`Header` を直接組み立てる。ファイルは要らない）:

```rust
const BUDGET_8GB_PC: u64 = 4_026_531_840; // 3.75GiB（spec §1）

fn x66(color: ColorType) -> Header {
    Header { format: Format::Jxl, width: 6656, height: 9984, color, file_len: 5_100_000, avif: None }
}
const FRAMED_PAD: Work = Work::Convert { mode: ConversionMode::Pad, max_width: None, exif_frame: true };

#[test]
fn x66_fits_the_budget_of_an_8gb_pc_in_rgb16_and_rgba16() {
    for color in [ColorType::Rgb16, ColorType::Rgba16] {
        let e = estimate(&x66(color), &FRAMED_PAD);
        assert!(e <= BUDGET_8GB_PC, "{color:?}: {e}");
    }
}

#[test]
fn jxl_estimate_covers_its_own_working_limit_plus_output() {
    // max_alloc が出力を数えないので、見積もりは「作業領域 + 出力」以上でなければならない
    let h = x66(ColorType::Rgba16);
    assert!(estimate(&h, &Work::View) >= jxl_working_bytes(&h) + 6656 * 9984 * 8);
}

#[test]
fn estimate_grows_with_file_size() {
    let small = Header { file_len: 1_000_000, ..avif_header() };
    let big = Header { file_len: 100_000_000, ..avif_header() };
    assert!(estimate(&big, &Work::View) >= estimate(&small, &Work::View) + 2 * 99_000_000);
}

#[test]
fn pad_estimate_covers_either_orientation_for_exif_oriented_formats() {
    // 7008x4672 の JPEG。向きは読まない（D-3）ので、縦に回っても横のままでも同じ値
    let land = jpeg_header(7008, 4672);
    let port = jpeg_header(4672, 7008);
    let pad = Work::Convert { mode: ConversionMode::Pad, max_width: None, exif_frame: false };
    assert_eq!(estimate(&land, &pad), estimate(&port, &pad));
}

#[test]
fn max_width_lowers_the_pad_estimate() {
    let h = jpeg_header(7008, 4672);
    let full = Work::Convert { mode: ConversionMode::Pad, max_width: None, exif_frame: false };
    let capped = Work::Convert { mode: ConversionMode::Pad, max_width: Some(1080), exif_frame: false };
    assert!(estimate(&h, &capped) < estimate(&h, &full));
}
```

補助: `avif_header()` は 4000×3000・8bit・アルファなし・`file_len` 1MB の AVIF（`avif: Some(AvifDeclared { primary_pixels: 12_000_000, alpha_pixels: None, bit_depth: 8 })`、`color: Rgb8`）、
`jpeg_header(w, h)` は `Format::Other`・`Rgb8`・`file_len` 5.5MB。

`decode/mod.rs` か `core/tests/jxl_avif_input.rs` の結合テスト:

```rust
#[test]
fn decode_rejects_a_file_changed_after_probe() {
    // 形式ごとに: A を probe → 同じパスを寸法の違う B で上書き → A の Header で open
    // JPEG XL: rotated.jxl → lossy_24mp.jxl、AVIF: orient_1.avif → avif/base_64x32.avif、PNG: 64x64 → 32x32（tempdir で生成）
    // 前提: B は単独なら開ける（open(B, &probe(B)) が Ok）
    // 期待: format!("{err:#}") に "image changed between probing and decoding"
}

#[test]
fn over_output_limit_is_rejected_by_probe_before_borrowing() {
    // over_limit.jxl / over_limit.png / avif/over_limit.avif の probe が既存の 512MB の理由で Err
    // AVIF は "AVIF declares a size over the 512MB output limit" を照合
}

#[test]
fn unreadable_dimensions_fail_closed() {
    // 中身がでたらめな .jpg、空の .jxl、mdat 手前で切った .avif のヘッダも無いもの: probe が Err
}
```

既存の `dimensions_are_read_from_the_header_alone`（ヘッダだけ残した JPEG XL / AVIF）は、`probe` が `Ok` で
`open` が `Err` になることも足して確かめる（fail-closed がヘッダの読める壊れたファイルを見積もり前に落とさないこと）。

- [ ] **Step 3: 失敗を確かめる** — `cargo test -p picture-tool-core` / Expected: コンパイルエラー

- [ ] **Step 4: `probe` と `Header` を実装し、`open`/`open_with_warnings` を `&Header` を受ける形にする**

- JPEG XL: `jxl_decoder(path)` は上限を設定しない形にする（ヘッダまで読んだ状態で返すだけ。`probe` と `dimensions` が使う）。
  `open` は `jxl_decoder(path)?` の寸法・色型を `header` と比べてから `set_limits(max_alloc = jxl_working_bytes(header))` する。
  上限は「デコードを始める前に設定する」ので、ヘッダを読んだ後に設定しても効く（現行も同じ順）。`JXL_WORKING_BYTES_PER_PIXEL` は
  `estimate.rs` の係数に置き換えて削除し、「上限値そのものの見直しは別の spec で扱う」のコメントを消す
- AVIF: D-2。`frame_size_limits` は `probe` から呼ぶ。`open` は読み直した宣言が `header.avif` と違えば `Err`
- 他: `ImageReader::open(path)?.with_guessed_format()?.into_decoder()?` の `dimensions()`・`color_type()`・`total_bytes()`
- 512MB 判定（`image::Limits::default().reserve(...)`）は `probe` へ移し、`open` からは消す（2 度判定しない）。
  `open` は寸法・色型が `header` と同じことを確かめるので、`probe` の判定が `open` にも効く

`lib.rs` の `decode::open` / `decode::open_with_warnings` の呼び出し元（`open_image_oriented`・`process_image`・
`ExifFramePreviewSource::load`）は `decode::probe(path)?` を先に呼ぶ形にする（借りるのは Task 5）。

- [ ] **Step 5: `estimate` を実装する**

**区間ごとの最大**（spec §2）。各区間は「その時点で同時に生きているバッファ」の和。`P` = 画素数、`O` = 出力の
画素バッファ（`P × color.bytes_per_pixel()`）、`F` = ファイルの大きさ、`C` = 変換後のキャンバスの画素数:

| 区間 | 和 | 備考 |
|---|---|---|
| デコード中 | 作業領域 ＋ 固定分 ＋ `O` ＋ ファイル項 | 作業領域: JPEG XL は `jxl_working_bytes`、AVIF はピクチャ × 枚数、他は B/画素 × `P` |
| デコード後 | `O` ＋ 向きの複製（`O`）／アルファの合成（`P × 4 ＋ P × 3`）の大きいほう | JPEG XL・AVIF の向きはデコード中の区間に入れる |
| 変換（Convert のみ） | 変換前の画像（`max(O, P × 3)`）＋ キャンバス | pad: `C × 3`、Exif フレーム: `C × 4`（RGBA）、crop: `C × 3`（`crop_imm` の複製）。`C` は `fit_to_4_5` を縦横両方で取った大きいほう（JPEG / PNG / WebP。D-3）、`max_width` があれば `target_canvas` の画素数で頭打ち。pad の前段縮小があるときは変換前の画像を縮小後の大きさで数えてよい |
| エンコード（Convert のみ） | `C × 3`（`into_rgb8`）＋ RGBA キャンバス（フレームのとき、`into_rgb8` の間だけ）＋ エンコーダ（B/画素 × `C`） | |
| View のエンコード | デコード後の画像 ＋ 縮小後の RGB（`max(P, 2560×1600) × 3`）＋ エンコーダ（B/画素 × `max(P, 2560×1600)`） | 縮小しないときは `to_rgb8` が原寸を複製するので `P`。2560×1600 は `generate_full_image_base64` の上限で、サムネイル（1024）とプレビュー（1024）もこれで押さえる |

各項の根拠（どの関数のどのバッファか、係数をどの計測から取ったか）を定数のコメントに書く。式の各項を
`process_image` の順に追えるよう、区間ごとに小さな関数に分けてよい。

- [ ] **Step 6: 通ることを確かめる** — `cargo test -p picture-tool-core` PASS、`make check` exit 0

- [ ] **Step 7: ガードを外して確かめる**（実施メモに書く）
  - 寸法の比較を外すと `decode_rejects_a_file_changed_after_probe` が落ちる
  - `open` が設定する `max_alloc` を一時的に `jxl_alloc_probe` で測った x66 の最小値の 1/2 にすると、x66 が `Err` になる
    （上限が本当に効いていることの確認）。戻した後に x66 が `Ok` になることも確かめる
  - x66・x66a の `Work::Convert { Pad, None, true }` の見積もり値（2 つ）を実施メモに残す（spec §2 が求める）

- [ ] **Step 8: Commit** — `feat(core): メモリ予算: ヘッダから 1 枚のピークを見積もり、デコードの上限を見積もりに合わせる`

---

## Task 5: 画像を開く公開関数で借りる

**Files:**
- Modify: `core/src/lib.rs`

**Interfaces:**
- Consumes: `budget::global()`、`MemoryBudget::acquire`、`decode::probe`、`estimate`
- Produces:
  - `fn open_budgeted<T>(budget: &MemoryBudget, path: &Path, work: &Work, f: impl FnOnce(&decode::Header) -> Result<T>) -> Result<T>`
    — `probe` → `acquire(estimate)` → `f(&header)`。借用は `f` の戻りまで
  - `fn open_oriented_unbudgeted(path: &Path, header: &decode::Header) -> Result<DynamicImage>`
  - `fn process_image_unbudgeted(input_path, header: &decode::Header, output_folder, config, exif_frame_config, assets) -> Result<ProcessResult>`
    （現在の `process_image` の本体。`decode::sniff`／`open_with_warnings` は `header` を使う）

- [ ] **Step 1: 失敗するテストを書く**（`lib.rs` の `tests`。自分で作った予算を渡す）

```rust
#[test]
fn failed_open_returns_the_lease() {
    // ヘッダだけ残した JPEG XL（probe は通り、デコードで Err）
    let b = MemoryBudget::with_log(u64::MAX / 4, u64::MAX / 2);
    let r = open_budgeted(&b, &truncated_jxl, &Work::View, |h| open_oriented_unbudgeted(&truncated_jxl, h));
    assert!(r.is_err());
    assert!(b.events().iter().any(|e| e.kind == EventKind::Acquire), "前提: 借りてから失敗した");
    assert_eq!(b.in_use(), 0);
}

#[test]
fn unreadable_file_is_rejected_without_borrowing() {
    // 中身がでたらめな .jpg
    let b = MemoryBudget::with_log(u64::MAX / 4, u64::MAX / 2);
    assert!(open_budgeted(&b, &garbage, &Work::View, |h| open_oriented_unbudgeted(&garbage, h)).is_err());
    assert!(b.events().is_empty());
}

#[test]
fn image_over_the_limit_fails_with_the_memory_message() {
    let est = estimate(&decode::probe(&jpeg_64).unwrap(), &Work::View);
    let b = MemoryBudget::new(est - 2, est - 1);
    let err = open_budgeted(&b, &jpeg_64, &Work::View, |h| open_oriented_unbudgeted(&jpeg_64, h)).unwrap_err();
    assert!(err.downcast_ref::<OverMemoryLimit>().is_some(), "{err:#}");
}
```

- [ ] **Step 2: 失敗を確かめる** — コンパイルエラー

- [ ] **Step 3: 借りる場所の表のとおりに包む**
  - `open_image_oriented`・`generate_thumbnail_base64`・`generate_full_image_base64`・`ExifFramePreviewSource::load` は
    `open_budgeted(budget::global(), path, &Work::View, ...)`。サムネイルとビューアーは**縮小・エンコードまで**を `f` に入れる
  - `process_image`（公開）は `open_budgeted(budget::global(), path, &Work::Convert{..}, |h| process_image_unbudgeted(..))`。
    `exif_frame` は `exif_frame_config.is_some() && assets.is_some() && mode == Pad`（実際に描く条件と同じ）
  - 既存の `with_context`（`Failed to open image for thumbnail: ...` など）はそのまま残す

- [ ] **Step 4: `lib.rs` の `mod budget;` の `#[cfg_attr(not(test), allow(dead_code))]` を外す**

- [ ] **Step 5: 通ることを確かめる** — `cargo test -p picture-tool-core` PASS（既存のテストもすべて）、`make check` exit 0

- [ ] **Step 6: Commit** — `feat(core): メモリ予算: 画像を開く公開関数で予算を借りる`

---

## Task 6: 一括変換を専用プールで回し、各要素で借りる

**Files:**
- Modify: `core/src/lib.rs`

**Interfaces:**
- Consumes: Task 5 の `process_image_unbudgeted`
- Produces:
  - `fn process_batch_in(budget: &MemoryBudget, threads: Option<usize>, files, output_folder, config, exif_frame_config, assets, on_progress) -> Vec<Result<ProcessResult>>`
    — `threads` は `None` で既定（テストだけが `Some` を渡す）。公開の `process_batch` は `process_batch_in(budget::global(), None, ...)`

- [ ] **Step 1: `test-integrity` を起動し、spec §3 の一括変換の手順（①〜④）・取り消し・グローバルプールからテストを設計する**

- [ ] **Step 2: 失敗するテストを書く**

```rust
#[test]
fn cancel_while_waiting_writes_nothing() {
    // 同じ大きさの JPEG 4 枚。予算は 1 枚ぶん（2 枚目は入らない）。プールは 2 スレッド
    let est = estimate(&decode::probe(&files[0]).unwrap(), &convert_work());
    let b = MemoryBudget::with_log(est, est);
    assert!(2 * est > b.budget(), "前提: 2 枚は同時に借りられない");
    // 1 枚目の完了で呼ばれるコールバックは、誰かが待ちに入るまで待ってから false を返す
    let cb = { let b = &b; ... wait_until(|| waits(b) >= 1); false };
    let results = process_batch_in(&b, Some(2), &files, out, &cfg, None, None, Some(cb));
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(cancelled_count(&results), 3);
    assert_eq!(std::fs::read_dir(out).unwrap().count(), 1, "待っていたワーカーは書いていない");
    // 前提: 待ちに入ったワーカーが、③の確認を通って抜けた（Acquire が 2 回以上 = 待った後に借りてから抜けた）
    assert!(acquires(&b).len() >= 2);
}

#[test]
fn waiting_batch_does_not_block_the_global_pool() {
    // 予算をテストが全部借りておき、グローバルプールのスレッド数 + 1 枚の一括変換を別スレッドで始める
    let n = rayon::current_num_threads();
    // 全ワーカー（n 本）が待ちに入ったのを events の Wait で確かめてから、グローバルプールで par_iter を回す
    // 期待: 10 秒以内に終わる（チャネルの recv_timeout）。その後で借りていた分を返し、一括変換も全件 Ok
}

#[test]
fn image_over_the_limit_fails_and_the_rest_of_the_batch_continues() {
    // 64x64 と 2000x2000 の JPEG。予算・上限を 2 つの見積もりの間に置く
    // 期待: 大きいほうは OverMemoryLimit、小さいほうは Ok で出力が 1 枚
}
```

`waiting_batch_does_not_block_the_global_pool` の前提として、**一括変換をグローバルプールで回す版（現行）に一時的に
戻すと時間切れで落ちる**ことを Step 6 で確かめる。

- [ ] **Step 3: 失敗を確かめる** — コンパイルエラー

- [ ] **Step 4: `process_batch_in` を実装する**

`rayon::ThreadPoolBuilder::new()`（`threads` があれば `num_threads`）で作ったプールの `install` の中で、現在の
`par_iter().map(...)` を回す。各要素は spec §3 の順:

```text
① cancelled なら Err(Cancelled)
   probe（失敗はその 1 枚の Err。借りない）
② budget.acquire(estimate(header, Work::Convert{..}))（Err はその 1 枚の Err）
③ cancelled なら Err(Cancelled)（借りた分は Lease の drop で返る）
④ process_image_unbudgeted(..)
   進捗のコールバック（既存どおり。③で抜けた要素は呼ばない — 既存の①と同じ扱い）
```

プールが作れなければ、全要素を `Failed to start the worker pool` の `Err` にして返す（panic しない）。
`process_batch` の doc に「専用プールで回す理由（グローバルプールを予算待ちで塞がない。GUI の `collect_image_entries`）」と
「1 枚の処理の中で rayon の並列 API を使わない理由（D-6）」を書く。`grep -rn "par_\|rayon::join\|rayon::scope" core/src` で
1 枚の処理の経路に並列 API が無いことを確かめ、結果を実施メモに書く。

- [ ] **Step 5: 通ることを確かめる** — `cargo test -p picture-tool-core` PASS、`make check` exit 0

- [ ] **Step 6: 罠を戻して確かめる**（実施メモに書く）
  - `install` を外す（グローバルプールで回す）→ `waiting_batch_does_not_block_the_global_pool` が時間切れで落ちる
  - ③を外す → `cancel_while_waiting_writes_nothing` が落ちる（出力が 2 枚以上）
  - 専用プールを作るコスト: 空のフォルダー（0 枚）と 1 枚で `process_batch` を 100 回呼んだ時間を測る

- [ ] **Step 7: Commit** — `feat(core): メモリ予算: 一括変換を専用プールで回し、1 枚ごとに予算を借りる`

---

## Task 7: 計測用の入口と example

**Files:**
- Modify: `core/src/lib.rs`
- Create: `core/examples/budget_batch.rs`

**Interfaces:**
- Consumes: `process_batch_in`、`estimate`、`decode::probe`
- Produces:
  - `#[doc(hidden)] pub use budget::{MemoryBudget, Event, EventKind, OverMemoryLimit};`
  - `#[doc(hidden)] pub fn process_batch_with_budget(budget: &MemoryBudget, files, output_folder, config, exif_frame_config, assets, on_progress) -> Vec<Result<ProcessResult>>`
    （`process_batch_in(budget, None, ...)`）
  - `#[doc(hidden)] pub fn estimate_conversion(path: &Path, config: &ProcessingConfig, exif_frame: bool) -> Result<u64>`
  - doc に「計測とテスト用。CLI・GUI からは使わない。利用者向けの設定手段ではない」（spec §6）

- [ ] **Step 1: `budget_batch` を書く**

```text
budget_batch <input_dir> <output_dir> --budget <bytes> [--limit <bytes>] [--mode crop|pad] [--exif-frame] [--log <csv>]
budget_batch --estimate <file>... [--mode crop|pad] [--exif-frame]
```

- `--limit` の既定は `budget / 2 * 3`（予算と上限の比 1/2 : 3/4 を保つ）
- 一括変換: `collect_image_files` → `MemoryBudget::with_log` → `process_batch_with_budget`。1 枚ごとに `Ok`/`Err {:#}` を出し、
  最後に `max_in_use=<bytes> files=<n> failed=<n>` を出す。`--log` には `at_ms,kind,estimate,in_use,holders` の CSV
- `--exif-frame` は `ExifFrameConfig::default()` と `ExifAssets::load(AssetDirs::default())`
- `--estimate`: 各ファイルの `estimate_conversion` を `<bytes> <path>` で出す
- 品質 90・`max_size_mb` 8 は CLI の既定と揃える

- [ ] **Step 2: 動くことを確かめる** — `cargo build --release -p picture-tool-core --example budget_batch`、テスト用の小さな
  フォルダーで一括変換と `--estimate` を 1 回ずつ。`make check` exit 0

- [ ] **Step 3: Commit** — `feat(core): メモリ予算: 予算を引数で渡して測る example を足す`

---

## Task 8: 完了の確かめ方（計測）

spec「完了の確かめ方」をそのまま実行する。**結果はすべて実施メモに全回分を貼る。** 入力は Task 3 のスクラッチを使う。

- [ ] **Step 1: 見積もりが実際のピーク以上であること**

Task 3 の全入力 × 変換モード（crop / pad、x66・x66a・j33 は pad＋`-e`）について、`budget_batch --estimate` の値と
CLI 1 枚変換のピーク RSS（5 回の中央値。**変更後のビルド**）を 1 つの表に並べる。1 つでも 見積もり < RSS なら、
その項の係数を直して Task 4 の Step 6 からやり直す。

- [ ] **Step 2: 予算 3.75GiB（`--budget 4026531840`）で**（5 回ずつ）
  - x66 × 4 枚の crop: 全件 Ok、`max_in_use ≤ 4026531840`、ピーク RSS ≤ 4,026,531,840 バイト（= 3,932,160 KB）
  - x66 の 1 枚と x66a の 1 枚: どちらも Ok で、`--log` の Acquire の `holders` に単独処理（予算超え）の印が無い
    （見積もりが予算以下）
  - 単独処理: 見積もりが予算（4,026,531,840）と上限（6,039,797,760）の間に入る寸法の単色 JPEG XL を bomb と同じ方法で作り、
    j33 × 3 枚と同じフォルダーで変換。`--log` で、その 1 枚の Acquire が `holders == 1` で、その Release まで他の Acquire が無いこと
  - 上限超え: 見積もりが 6,039,797,760 を超え、かつ出力が 512MB（8bit RGB で 178,956,970 画素）以下の寸法の単色 JPEG XL を作り、
    j33 × 3 枚と同じフォルダーで変換。その 1 枚が `Image needs about ... GB, over the memory limit of 6.0 GB` で、他は Ok
  - 寸法は `--estimate` で見積もりを確かめてから決め、作った寸法と見積もりを実施メモに書く。**作れる寸法が無い**
    （512MB 以下では上限を超えない）なら、その旨と最大の見積もりを書いて利用者に報告する
  - ピーク RSS が予算を超えたら: `--log` の `in_use` と並べ、`MALLOC_ARENA_MAX=1` で測り直して、見積もりの不足か
    アロケータの残留かを切り分ける（spec）

- [ ] **Step 3: 速度の比較の段取り（利用者に依頼する）**

この PC は普段、物理 62GB のうち 32〜41GB が使用中で空きは約 20GB、zram 8GB はほぼ満杯（2026-10-28 時点）。
x66 × 20 枚は現行で約 40GB 以上を使うので、**利用者に他のアプリを閉じてもらい、`free -g` の available が 45 以上に
なってから**測る。依頼の文面: 「速度の比較のために、空きメモリが 45GB 以上要ります。ブラウザーなど大きなアプリを
閉じて、終わったら教えてください（測定は 20〜30 分）」。`free -g` の出力を実施メモに貼る。

- [ ] **Step 4: 既定の予算での速度**（5 回ずつ。1 回目を捨てるかどうかを決めて書く）

比較元は main（`3b460d4`）を `git worktree add <scratch>/base 3b460d4` し、`cargo build --release -p picture-tool --target-dir <scratch>/base-target`
で作る（`target/` を共有しない）。同じ空きメモリの状態で、旧・新を交互に測る。

| フォルダー | モード |
|---|---|
| x33 × 8 枚 | crop |
| x66 × 4 枚 | crop |
| j33 × 20 枚 | crop |
| j33 × 20 枚 | pad |
| x66 × 20 枚 | crop |

各件の所要時間の悪化（中央値）が **5% 以内**であること。超えたら作業を止め、「予算の決め方（総量の 1/2）を見直すか」を
利用者に相談する（spec）。計測後に worktree を消す。

- [ ] **Step 5: Commit** — `docs(docs): メモリ予算: 完了の確かめ方の計測を実施メモに残す`

---

## Task 9: ドキュメント

**Files:**
- Modify: `README.md`、`core/src/decode/mod.rs`（冒頭）、`core/src/lib.rs`（冒頭の地図）、
  `docs/superpowers/specs/2026-10-14-avif-decoder-replacement-design.md`、`docs/superpowers/specs/2026-10-28-memory-budget-design.md`、`docs/README.md`

- [ ] **Step 1: 書き換える**
  - `README.md`「出力」節に既知の制限として 1 項目: 画像を開く処理は物理メモリの半分（**予算**）を分け合い、
    足りないときは順番を待つ。1 枚で予算を超える画像はほかを止めて単独で処理する（その間 PC が重くなりうる）。
    物理メモリの 3/4（**上限**）を超える 1 枚は「メモリの上限を超える」失敗になり、処理は続く。スワップの無い PC では、
    単独処理の最中に OS がプロセスを終了させることがある
  - `decode/mod.rs` 冒頭「メモリの上限」: 「姉妹 spec で扱う」を本 spec への参照に置き換え、JPEG XL の作業領域（`max_alloc` を
    見積もりから導く）・AVIF の参照フレーム・ファイルの大きさが見積もりに入ったこと、512MB 判定を `probe` で行うこと（D-1）を書く
  - `lib.rs` 冒頭の地図: 「呼び出し側が守ること」に「画像を開く公開関数はメモリ予算を借りる。予算待ちで呼び出し元のスレッドが
    止まることがある」を足す
  - AVIF spec 冒頭の「姉妹 spec（未作成）」を本 spec へのリンクにする
  - 本 spec 冒頭の「状態」を「実装済み（計画: 本計画へのリンク）」にする
  - `docs/README.md`: 現行仕様の表の先頭に本 spec、直近の実装計画の表の先頭に本計画を、既存の行と同じ粒度で足す

- [ ] **Step 2: `make check` exit 0**

- [ ] **Step 3: Commit** — `docs: メモリ予算: README と索引を更新する`

---

## セルフレビュー（spec との突き合わせ）

| spec | タスク |
|---|---|
| 成功の条件 1（8GB で使い切らない） | Task 4（不等式）・Task 8 Step 2 |
| 成功の条件 2（単独処理・上限超えは 1 枚の失敗・文言） | Task 1・Task 6・Task 8 Step 2 |
| 成功の条件 3（5% 以内） | Task 8 Step 3〜4 |
| 反目標（67MP が弾かれる／単独に入る・並列度の不当な低下・デッドロック） | Task 4 の不等式・Task 8 Step 4・Task 1（FIFO）／D-5（入れ子） |
| §1 予算の大きさ・単位・総量の fallback | Task 2 |
| §2 見積もり（区間・ファイル項・JPEG XL の `max_alloc`・AVIF の枚数・寸法の食い違い・fail-closed・係数の計測） | Task 3・Task 4 |
| §3 借りる場所・RAII・FIFO・単独・入れ子なし・グローバルプール | Task 1・Task 5・Task 6 |
| §4 文言と GUI での見え方（待ちは表示しない） | Task 1（文言）。GUI は変更なし |
| §5 変わらないもの | Global Constraints・D-1 |
| §6 予算の受け渡し（引数・example・`#[doc(hidden)]`） | Task 6・Task 7 |
| 完了の確かめ方 | Task 1・Task 4・Task 6（テスト）、Task 8（計測） |
| 更新するドキュメント | Task 9 |

### spec から意図的に外したこと・テストを置かなかったこと

- GUI の見え方（待ちの間サムネイルが止まって見える）は spec §4 が受け入れたもので、テストしない
- スワップの無い PC での挙動（spec 成功の条件 1 の対象外）

## 実施メモ

### Task 1

- Step 5: `cargo test -p picture-tool-core --lib budget::` → 7 passed。`make check` exit 0
- Step 6: `can_take` の `ticket == st.serving` を `(ticket == st.serving || true)` にすると
  `later_small_request_does_not_overtake_a_waiting_large_one` と `over_budget_request_runs_alone` の 2 件が落ちた
  （どちらも `wait_until` の 10 秒の時間切れ。後から来た小さい 1 枚が待たずに借りるので `Wait` が 2 件にならない）。戻して 7 件 PASS
- 計画からの変更: `mod budget;` の許可は `#[cfg_attr(not(test), allow(dead_code))]` ではなく `#[allow(dead_code)]`。
  test ビルドでも `budget()`・`limit()` が未使用になり、`clippy --all-targets -D warnings` が落ちるため（Task 5 で外すのは同じ）

### Task 2

### Task 3: 入力・作業領域・現行の RSS・係数

### Task 4: 見積もりの値とガードを外したとき

### Task 5

### Task 6: 罠を戻したとき・プールのコスト

### Task 8: 完了の確かめ方
