# 全体レビュー 修正計画（2026-08-04）

対象コミット: `dd5133b`（main / PR #1 `feature/exif-frame` マージ後）
このファイルだけで各セッションが自走できるように書いている。着手前に該当箇所を必ず再確認すること（行番号は上記コミット時点）。

---

## 0. 前提

### ビルド・検証コマンド

```bash
cargo test -p picture-tool-core -p picture-tool     # 78件（2026-08-04時点）
cargo clippy -p picture-tool-core -p picture-tool --all-targets
cargo fmt --all -- --check
cd gui-frontend && bun install && bunx svelte-check
```

### 既知の環境制約

- `cargo clippy --workspace` は GTK/WebKit 系（gobject-sys, gdk-sys, soup3-sys 等）の
  pkg-config が無いとビルド不可。
  **→ S2 で解消済み。** Fedora なら以下で導入する（Tauri v2 公式手順）:

  ```bash
  sudo dnf install -y webkit2gtk4.1-devel openssl-devel curl wget file \
    libappindicator-gtk3-devel librsvg2-devel libxdo-devel
  sudo dnf group install -y c-development
  ```

- `tauri-build` は `frontendDist`（`../gui-frontend/dist`）の実在を要求する。
  **`gui/` クレートに触る cargo コマンドの前に必ず `cd gui-frontend && bun run build` を通すこと。**
  CI もこの順序になっている。

### 現状のベースライン（S4 完了後 / 2026-08-05）

| 項目 | S2 前 | 現在 |
|---|---|---|
| `cargo test --workspace` | core のみ 78 passed | **107 passed**（S1 で +2、S3 で +10、S4 で +17） |
| `cargo clippy --workspace --all-targets -- -D warnings` | 21 warnings（gui は未検査） | **green**（gui にも2件あり修正済み） |
| `cargo fmt --all -- --check` | 失敗（実際は10ファイル） | **green** |
| `bunx svelte-check` | 未導入 | **0 errors / 0 warnings**（S5 で解消） |
| CI | **無し** | `.github/workflows/ci.yml`（ubuntu-22.04） |
| `make test` | core のみ | `cargo test --workspace` |

### 根本原因（全体を貫く所見）

exif-frame v2（コミット30本超、core の約半分）が**検証されないまま main に入っている**。
以下の個別バグはすべてその帰結。**S2（CI 整備）を先に終わらせないと同じことが再発する。**

---

## 1. セッション分割

依存関係を考慮した推奨順。各セッションは独立して完了でき、終了時に上記の検証コマンドが通ることをゴールとする。

| # | セッション | 主な対象 | 依存 |
|---|---|---|---|
| S1 | ライセンス・リポジトリ衛生 ✅ | ルート、`core/assets/` | なし |
| S2 | CI 整備 + lint/fmt + デッドコード掃除 ✅ | `.github/`, `Makefile`, 各所 | なし（最優先で実施） |
| S3 | core 画像処理の正しさ ✅ | `core/src/lib.rs` | S2 |
| S4 | exif_frame レイアウト・描画の正しさ ✅ | `core/src/exif_frame/` | S2 |
| S5 | フロントエンドの正しさ・安全性 ✅ | `gui-frontend/src/` | S2 |
| S6 | Tauri バックエンドのセキュリティ | `gui/src/`, `capabilities/` | S2 |
| S7 | ドキュメント整合 | `README.md`, `CLAUDE.md`, `docs/` | S1〜S6 完了後 |

---

## S1. ライセンス・リポジトリ衛生

法的リスクの解消が目的。

> **実施済み（2026-08-04）**。以下の実施メモも参照。

- [x] **C4-a** ルートに `LICENSE` を追加。プロジェクト自体のライセンスが未定義。
      → **MIT**（`Copyright (c) 2026 KOMO`）に決定。ルート `Cargo.toml` に
        `[workspace.package] license = "MIT"` を置き、core/cli/gui は `license.workspace = true` で継承。
- [x] **C4-b** `core/assets/fonts/NotoSansJP-Regular.ttf`（9.1MB, git 追跡下）は **SIL OFL 1.1**。
      OFL §2 は再配布時のライセンス全文同梱を要求しており、`rust-embed` でバイナリに埋め込む形態でも同様。
      → `core/assets/fonts/OFL.txt` を配置した（**rust-embed の対象に含める**。
        バイナリ単体配布でも OFL §2 を満たすため。`text.rs` の `.ttf/.otf` 検索は影響なし＝
        `load_bundled_font` テスト green）。
- [x] **C4-c** `core/assets/logos/{sony,sony_light,gmaster,gmaster_light,fujifilm}` は
      SONY / FUJIFILM の**登録商標**。
      → **ユーザー判断: バンドル維持 + README に免責明記**（2026-08-04）。
        README に「ライセンス」節を新設し、同梱アセット表・商標帰属・非提携の明示・
        ユーザーロゴでの差し替え手順（配置先 OS 別パス、`_light` 命名規則、
        `model_map_custom.json` の例）を記載した。
- [x] **REPO-1** `.serena/`（6ファイル）をリポジトリから削除し `.gitignore` に追加。
      → `git rm -r --cached .serena` で**追跡解除のみ**（ローカルファイルは残置）。
        内容が古いことは確認済み（`project_overview.md` が「単一ファイル構成: `src/main.rs`」のまま）。
        ローカルの stale な memories は Serena 側で再生成すること。
- [x] **REPO-2** `.gitignore` に `.claude/settings.local.json`、`.DS_Store`、`*.swp` を追加。
      → あわせて `.serena/` を追加し、全体をコメント付きでセクション分けした。
- [x] **REPO-3** `gui/icons/ios/`（18枚）と `android/`（15枚）を削除。
      → `tauri.conf.json` には `bundle` セクション自体が無く、Tauri のデフォルト
        （`32x32.png` / `128x128.png` / `128x128@2x.png` / `icon.icns` / `icon.ico`）だけが使われる。
        ルート直下の PNG 群は残し、`ios/` `android/` のみ削除した。

### S1 実施メモ（2026-08-04）

- **DEAD-5 を解消した**（S2 からの持ち越し）。C4-c で「バンドル維持」と決まったため、
  `model_map.json` に `FUJIFILM` / `Fujifilm` / `FUJIFILM Corporation` を配線し、
  `fujifilm_light.svg` を新規作成した（`fujifilm.svg` の `fill:#000000` 16箇所を
  `#ffffff` に置換。ブランドレッド `#ed1a3a` は保持。`sony_light.svg` と同じ生成規則）。
- **再発防止テストを追加**（`test-integrity` skill 適用、Rigor: Standard）:
  - `model_map::tests::maker_logo_fujifilm_variants` — FUJIFILM 3表記が `fujifilm.svg` を返す
  - `exif_frame::logo::tests::every_logo_referenced_by_model_map_is_bundled` —
    **`model_map.json` を実際に読んで**、参照される全ロゴが base と `_light` の両方で
    バンドルから解決できることを検証。JSON 駆動なので将来メーカーを追加しても自動でカバーされる。
    `resolve_and_load_logo` は light 欠落時に base へフォールバックしてしまうため、
    light の実在は `load_bundled_logo` で直接検証している。
    `fujifilm_light.svg` を一時削除して**実際に赤くなることを確認済み**。
- テスト数 78 → **80**。`make check` は green（clippy/fmt green, 80 passed,
  svelte-check 0 errors / 6 warnings = S5-M12 のまま）。
- **S2 の CI が green であることを確認した**（run 30910677408 / `80c9e1e` / conclusion: success）。
  ubuntu-22.04 のパッケージ名という唯一の未検証点は解消。
- **新規発見 → S4-L7 に起票**: 同梱フォントの中身がファイル名と食い違っている。

---

## S2. CI 整備 + lint/fmt + デッドコード掃除

**最優先。これが無い限り以降の修正も検証されない。**

> **実施済み（2026-08-04）**。以下の実施メモも参照。

- [x] **CI-1** `.github/workflows/ci.yml` を新設。ubuntu-22.04 で
      Tauri システム依存 → bun install → typecheck → **frontend build** →
      fmt → clippy(`-D warnings`) → test(`--workspace`) の順に実行する。
      frontend build を cargo より前に置くのは、`tauri-build` が
      `frontendDist`（`../gui-frontend/dist`）の存在を要求するため。
      → **S1 で `actions/checkout` を v4 → v5 に更新した**。v4 は Node.js 20 を対象としており
        runner 側で Node.js 24 に強制実行されるため、毎回 deprecation の annotation が出ていた。
- [x] **CI-2** `make test` を `cargo test --workspace` に変更。
      `make lint`（fmt --check + clippy -D warnings）、`make fmt`、`make typecheck`、
      `make check`（lint + test + typecheck）を追加。
- [x] **CI-3** `svelte-check` を devDependency に追加し `typecheck` スクリプトを定義。
      **`gui-frontend/svelte.config.js` の新設が必要だった**（無いと svelte-check が
      vite.config.ts から Svelte 設定を解決できず全ファイルでエラーになる）。
- [x] **CI-4** `@tauri-apps/cli@^2.11.4` を devDependencies に固定追加。
      `Makefile` の `dev` は `gui-frontend/node_modules/.bin/tauri` を直接叩く形にした。
      `bun run tauri dev` にしなかったのは、`tauri.conf.json` が `gui/` にあり
      `gui-frontend/` から実行すると設定を解決できないため。
- [x] **LINT-1** `cargo fmt --all` を実行（実際は10ファイルが対象だった）。
- [x] **LINT-2** clippy 21件を解消（`cargo clippy --fix` + 手動）。
      - `manual_div_ceil` は **S2 で先に解消した**。`div_ceil()` 化は機械的変換で
        S4-C1（モジュラスの左右逆転）とは独立であり、保留すると
        `-D warnings` を有効化した時点で CI が red になるため。S4 は当該行を書き換えるだけ。
      - `mod.rs:288,430` の引数11個のみ `#[allow(clippy::too_many_arguments)]` +
        `TODO(S4-H4)` で保留（S4-H4 のリファクタで allow を外すこと）。
- [x] **DEAD-1** `imageproc` を削除。
- [x] **DEAD-2** `resolve_placement` を削除。
- [x] **DEAD-3** **4層まとめて削除**（`commands.rs` / `main.rs` 登録 / `core` の `LogoInfo` /
      `types.ts` / `api.ts`）。常に空を返すスタブは未実装より有害（呼び出し側が
      「ロゴ0件」と誤認する）ため。UI を作る際に実装ごと足す。
- [x] **DEAD-4** **残す**方針に決定。`list_available_fonts` / `deletePreset` は
      バックエンドが正しく動作し、フォント選択は v2 spec:247 の要求機能。
      → **S5 に「フォント選択 UI とプリセット削除ボタンを追加する」タスクを追加すること。**
- [x] **DEAD-5** `core/assets/logos/fujifilm.svg` は**到達不能**。
      `model_map.json` の `logo_match` は `SONY`/`Sony`/`Sony Corporation` のみで FUJIFILM のエントリが無く、
      `fujifilm_light.svg` も存在しない（コミット `c8beb03` でアセットだけ追加してマッピングを忘れている）。
      → **S1 で解消**。C4-c が「バンドル維持」に決まったため、`model_map.json` に配線し
        `fujifilm_light.svg` を作成。再発防止テストも追加した（S1 実施メモ参照）。

### S2 で S5 から前倒しした項目

- **F5-F6**（`FolderTree.svelte:39` の `StoreOptions.defaults` 欠落）を S2 で修正した。
  svelte-check を CI に入れると**この型エラーだけで CI が red になる**ため、
  CI 整備の前提条件として不可分だった。修正は `{ defaults: {}, autoSave: false }` で
  ランタイム挙動は変えていない。
- **M12**（a11y）は svelte-check の *warning* であり exit code に影響しないので S5 のまま残した
  （現在 6 warnings）。CI を `--fail-on-warnings` にするのは S5-M12 を潰した後にすること。

---

## S3. core 画像処理の正しさ（`core/src/lib.rs`）

> **実施済み（2026-08-05）**。末尾の「S3 実施メモ」も参照。

- [x] **C2 並列バッチ処理で出力ファイルが衝突・破損する** `lib.rs:409-426`, `:441`

  ```rust
  while output_path.exists() { ... }                       // ロックなしの TOCTOU
  let temp_path = output_path.with_extension("tmp.jpg");   // 同名になりうる
  ```

  `process_batch` は常に `par_iter()`（`lib.rs:271`）。`collect_image_files` はサブディレクトリを
  再帰走査するため `sub1/photo.jpg` と `sub2/photo.jpg` は普通に存在する。
  両スレッドが `exists()` を false と判定し、同じ `photo_processed.jpg` と同じ一時ファイルを掴む。
  → 破損 JPEG か、片方の画像がサイレントに消失。

  **修正案**: `OpenOptions::new().create_new(true)` でパスを排他的に予約する、
  または一時ファイル名に UUID/スレッドIDを入れる。
  M1（下記）と併せて一時ファイル方式そのものを廃止するのが最善。

  **テスト必須**: 既存の `output_file_naming_handles_duplicate_names` は同一入力を**逐次**2回処理
  するだけでこの経路を通らない。別ディレクトリ同名ファイルを `process_batch` で並列処理する
  テストを追加すること。

- [x] **C3 EXIF Orientation が全パイプラインで未対応** `lib.rs:58-68`, `read_exif_info`, エンコード各所

  `grep -i orientation` が core/cli/gui/frontend で **0ヒット**（確認済み）。
  `image::open()` は Orientation を自動適用せず、出力は生ピクセルからの再エンコードで
  元 EXIF を一切引き継がない。Orientation=6/8 のカメラ・スマートフォン写真では:
  - `auto_placement` が縦横を誤判定し Exif バーが誤った辺に付く
  - 4:5 変換自体が誤った基準で行われる
  - 出力に Orientation タグが残らず **90度傾いたまま Instagram に上がる**

  **修正案**: `ExifInfo` に `orientation` を追加し、`read_exif_info` で読む。
  `image::open()` 直後に回転・反転を適用してから crop/pad/exif_frame パイプラインへ渡す。
  exif_frame 固有ではなく **core 全体の設計欠落**なので、`process_image` の入口で一度だけ適用する。

- [x] **H1 サイズ制限を満たせなくても呼び出し元に伝わらない** `lib.rs:428-462`

  `quality <= MIN_QUALITY(60)` に達した時点で、サイズ超過でも無条件に成功を返す。
  `ProcessResult`（`lib.rs:50-56`）には `final_quality` しかなく「制限を満たせなかった」を
  表すフィールドが無い。**主機能である最大ファイルサイズ制限がサイレントに破られる。**
  → `ProcessResult` に `size_limit_exceeded: bool`（または超過バイト数）を追加し、CLI/GUI で警告。
  → H2 の `warnings` と同時に設計すること。

- [x] **H2 core が `eprintln!` している（設計方針違反）** `lib.rs:217`, `:236-242`

  core はライブラリで、GUI には stderr が届かない。
  「Exif フレーム描画に失敗して pad にフォールバックした」「元ファイルの削除に失敗した」という
  重要情報が GUI ユーザーに完全に隠蔽される。
  CLAUDE.md の「core は Tauri 非依存（ProgressCallback で疎結合）」という原則を**警告経路にも適用**する。
  → `ProcessResult` に `warnings: Vec<String>` を追加し、CLI は stderr、GUI は Tauri イベントで表示。
  → H1 と同じ変更セットで実施する。

- [x] **H3 `follow_links(false)` が効いていない** `lib.rs:181-196`

  `WalkDir::follow_links(false)` を指定しているのに `path.is_file()`（`fs::metadata` ベースで
  リンクを常に解決する）で判定しているため、リンク先ファイルが設定と無関係にヒットする。
  → `entry.file_type().is_file()` を使う（`follow_links` 設定に従う）。

- [x] **M1 品質探索が毎回ディスクへ書き込み・削除している** `lib.rs:428-462`

  1段階下げるたびに `File::create` → `encode` → `metadata` → `remove_file`。
  初期90・下限60・step5 なら最大7回のディスクI/O。
  → `Vec<u8>` にエンコードして `len()` で判定し、確定した1回分だけ書き出す。
    一時ファイルのリネーム往復が不要になり **C2 の衝突リスクも同時に軽減**される。

- [x] **M2 `collect_image_files` の `Result` が事実上常に `Ok`** `lib.rs:181-196`

  `.filter_map(|e| e.ok())` で権限エラー等を全て捨てており `Err` を返す経路が存在しない。
  シグネチャが誤解を招き、「一部フォルダがスキップされて件数が減っている」ことに気づけない。
  → 戻り値を `Vec<PathBuf>` に簡素化するか、遭遇したエラーを併せて返す。

- [x] **M3 `read_exif_info` が EXIF エラーを一律 default に丸めている** `lib.rs:86-89`

  「EXIF が存在しない」（正常）と「ファイル破損 / I/O エラー」（異常）が区別できない。
  → H2 の `warnings` 経路に載せる。

- [x] **L1** `generate_output_path` の `to_string_lossy()`（`lib.rs:410-413`）は非UTF-8ファイル名を
      `U+FFFD` に丸め、異なる元ファイルが同一 stem になりうる（C2 の衝突を助長）。
- [x] **L2** `ProgressCallback`（`lib.rs:70-71`）に「rayon の複数ワーカースレッドから同時に呼ばれ、
      順序は保証されない」旨のドキュメントを追記。

### S3 実施メモ（2026-08-05）

**個別の対処ではなく3つの変更セットに束ねた。** 指摘同士が同じコードを共有しており、
別々に直すと互いを打ち消すため。

1. **C2 + M1 + L1 → 一時ファイル方式そのものを廃止**
   - `generate_output_path` + `save_with_size_limit` を `write_new_output_file` +
     `encode_within_size_limit` に置換。
   - パスの予約は `OpenOptions::create_new(true)`。「不在の確認」と「作成」が OS 側で不可分に
     行われるため、`exists()` の TOCTOU が原理的に消える。衝突時は `AlreadyExists` で連番へ進む。
   - 品質探索は `Vec<u8>` 上で行い、確定した1回だけ書き出す。最大7往復のディスクI/Oと
     `*.tmp.jpg` の衝突源が同時に消えた。
   - stem は `OsString` のまま組み立てる（`to_string_lossy` の U+FFFD 丸めを回避）。
2. **C3 → `apply_orientation` / `open_image_oriented` / `oriented_dimensions` を core に新設**
   - `image` 0.24 に自動適用は無いため自前実装（0.25 の `Orientation` と同じ 1-8 の対応表）。
   - **画像を開く入口を全て `open_image_oriented` に揃えた**: `process_image`、
     `generate_thumbnail_base64`、`generate_full_image_base64`、GUI の
     `render_exif_frame_preview`。揃えないとプレビューと出力で `auto_placement` が
     別の辺を選ぶ。GUI の `list_images` も `oriented_dimensions` で表示縦横を合わせた。
   - `process_image` は EXIF を先頭で1回だけ読み、Orientation 適用と Exif フレーム描画で共有する。
3. **H1 + H2 + M3 → `ProcessResult` に `size_limit_exceeded` と `warnings: Vec<String>` を追加**
   - core の `eprintln!` を全廃（横断的設計決定 1 のとおり）。CLI は stderr に出し、
     制限超過の件数を末尾に集計表示。GUI は `ProcessResult` 経由でフロントに届く
     （`types.ts` の型も更新済み。Svelte の表示 UI は **S5 に持ち越し**）。
   - `collect_image_files` は `CollectedImages { files, skipped }` を返す形に変更（M2）。
     常に `Ok` を返す欺瞞的な `Result` を廃し、走査失敗を呼び出し元が観測できるようにした。
     呼び出し元は CLI のみ（GUI は独自の `list_images` を使う）。

**テスト 80 → 90**（`test-integrity` skill 適用、Rigor: **Full** — 画像データの整合性に直結するため）:
- `process_batch_gives_each_input_a_distinct_output` — 計画が必須と定めたテスト。
  20個のサブディレクトリの同名 `photo.jpg` を並列処理し、出力パスが全て相異なることと、
  幅を1枚ずつ変えることで**内容が別画像に上書きされていない**ことまで検証する。
- `orientation_6_is_uprighted_before_conversion` / `read_exif_info_extracts_orientation` /
  `image_without_orientation_tag_is_unchanged` / `oriented_dimensions_swaps_only_for_rotated_orientations`
  — kamadak-exif は読み取り専用なので、SOI 直後に APP1 を差し込む
  `create_test_image_with_orientation` ヘルパーを自作した（Orientation タグ1件の最小 TIFF）。
- `size_limit_exceeded_is_reported_when_unreachable` / `..._is_false_when_limit_is_met` /
  `size_limit_failure_is_accompanied_by_a_warning` — 決定的な線形合同法で
  ほぼ圧縮できない画像を生成し、制限未達を再現する。
- `collect_image_files_does_not_follow_symlinks`（unix限定）/
  `collect_image_files_reports_unreadable_paths_as_skipped`

**ミューテーションテストで実効性を実測した**（Phase C）。修正を元のバグに戻すと
対応テストが実際に落ちることを4件すべてで確認:
`create_new`→`create` で並列テストが FAILED、Orientation=6 の回転を外すと向きテストが FAILED、
`entry.file_type()`→`path.is_file()` でリンクテストが FAILED、
`within_limit: false`→`true` でサイズ制限テスト2件が FAILED。

**検証**: `cargo fmt --all -- --check` green / `cargo clippy --workspace --all-targets -- -D warnings`
green / `cargo test --workspace` **90 passed** / `bunx svelte-check` 0 errors / 6 warnings（S5-M12 のまま）。
CLI 実機でも別ディレクトリ同名2枚が `photo_processed.jpg` と `photo_processed_1.jpg` に
分かれることを確認済み。

**S5 への申し送り**: `ProcessResult.warnings` と `size_limit_exceeded` はフロントに届いているが
UI で表示していない。F8（エラーの握りつぶし）と同時に扱うこと。

---

## S4. exif_frame レイアウト・描画の正しさ（`core/src/exif_frame/`）

> **実施済み（2026-08-05）**。末尾の「S4 実施メモ」も参照。

- [x] **C1 4:5 比の不変条件が破れる（最重要）** `layout.rs:225-244`

  ```rust
  // Bottom/Top
  let canvas_h_expanded = ((need_h + 3) / 4) * 4;    // 高さを「4」の倍数に ← 誤り
  let canvas_w_expanded = canvas_h_expanded * 4 / 5; // 割り切れず切り捨て
  // Right/Left
  let canvas_w_expanded = ((need_w + 4) / 5) * 5;    // 幅を「5」の倍数に ← 誤り
  let canvas_h_expanded = canvas_w_expanded * 5 / 4; // 割り切れず切り捨て
  ```

  **丸めるモジュラスが左右逆**。同ファイル `fit_to_4_5()`（`layout.rs:93-101`）が保証している
  `canvas = k*4 × k*5` を、このフォールバック分岐だけが破っている。

  実例: `photo=400x501`（普通のポートレート, Auto→Right）→ `canvas=405x506`。
  `405*5 = 2025 ≠ 506*4 = 2024`。

  **修正**: Bottom/Top は**高さを5の倍数**に丸めて `w = h*4/5`（割り切れる）、
  Right/Left は**幅を4の倍数**に丸めて `h = w*5/4`（割り切れる）。
  LINT-2 の `div_ceil` 化も同時に行う。

  **テスト必須（これが本質）**: 現行テストが検出できないのは構造的な問題。
  - `layout.rs` のユニットテストは `1200x800`, `800x1200`, `800x1000` など20の倍数しか使っておらず
    たまたま割り切れる
  - 統合テスト（`exif_frame_v2_integration.rs`）の比率検証は `(ratio - 0.8).abs() < 0.02` と
    許容2%で、1px単位のずれは**原理的に検出不能**
  → 全テストを `assert_eq!(canvas_w * 5, canvas_h * 4)` の**厳密比較**に変更し、
    `400x501`, `399x502` のような非round数値のケースを追加すること。
    これをやらないと同じバグが再発する。

- [x] **C5 `.expect()` が `Result` 返却関数の中でパニックする** `text.rs:19-28`（3箇所）, `model_map.rs:36-45`（2箇所）

  `load_font` はシグネチャが既に `Result<FontArc>` なのに、バンドルフォントのロード失敗時は
  `.expect()` で即パニックする。`process_image`（`lib.rs:214-220`）が用意している
  「描画失敗 → pad にフォールバック」という `match` を**完全にバイパス**する。
  さらに `process_batch` は rayon 上なのでバッチ全体を巻き込み、
  `OnceLock::get_or_init` はクロージャがパニックすると未初期化のまま残るため**以降ずっと失敗し続ける**。
  → `.ok_or_else(|| anyhow!(..))?` / `.context(..)?` に置き換えて `Result` として伝搬させる。

- [x] **H4 `ModelMap` を画像1枚ごとに再構築している（N+1）** `mod.rs:184-191`

  埋め込み JSON の `serde_json::from_slice` と、ユーザーファイルの `read_to_string` + パースを
  **画像ごとに毎回**実行。並列スレッドがそれぞれ独立に同じファイルを読み直す。
  さらに `let _ = model_map.merge_custom(&json_str);` で `anyhow::Result` を握りつぶしており、
  ユーザーのカスタム JSON が壊れていても無言で無視される。
  → `process_batch` の前で1回だけ構築し `&ModelMap` を注入する（`OnceLock` でも可）。
    エラーは S3-H2 の `warnings` に載せる。

- [x] **H5 横構図のセパレータ線が半透明にならない** `mod.rs:326-334`

  `alpha=100`（約39%）のつもりで `put_pixel` しているが、`put_pixel` は既存ピクセルとブレンドせず
  上書きする。最終的な `to_rgb8()` はアルファを単純に破棄する（背景合成しない）ため
  **100%不透明**で描画される。
  回転版（`mod.rs:551-555`）は明示的にブレンドしており正しい。
  → **同じ視覚要素が横構図と縦構図で挙動が違う**。H7 のリファクタで根本解決するのが望ましい。

- [x] **H6 レンズロゴとテキストが重なりうる** `mod.rs:338-406`, `:473-522`

  `auto_fit_text` に渡す `text_area_w` が右端いっぱいで、後から重ねる `lens_logo` の分を
  差し引いていない。テキストが領域幅いっぱいにフィットするとロゴが上に被る。
  水平版・回転版の両方に同じ問題がある。
  → `lens_logo` の有無とサイズを先に見積もり、`text_area_w` から幅+マージンを引いてから `auto_fit_text`。

- [x] **H7 `draw_exif_horizontal`(125行) と `draw_exif_rotated`(150行) の重複** `mod.rs:282-407`, `:411-560`

  ロゴ配置・セパレータ描画・テキストフィッティング・レンズロゴ配置がほぼそのままコピーされている。
  clippy も引数11個を検出。**H5（片方だけ不透明）はこの重複が直接生んだ不整合。**
  → 「幅=long side, 高さ=short side」の抽象バッファに対して1回だけ実装し、
    水平版はそのままキャンバスへ、垂直版は回転後に合成する形に統一する
    （回転版が既にバッファ方式なので、水平版を寄せれば実質1関数になる）。
    引数はコンテキスト構造体にまとめる。

- [x] **M4 プリセットのデフォルト値が2箇所で定義されている**

  `ExifFrameConfig::default()`（`mod.rs:90-100`, `DisplayItems::default` `:45-60`,
  `FontConfig::default` `:70-78`）と `core/assets/presets/default.json` が同じ内容を二重定義。
  さらに `cli/src/main.rs:124-127` はプリセットが見つからない場合に
  **JSON ではなく Rust の Default にフォールバック**する。
  現状は値が一致しているが、片方だけ更新される事故を待っている構造。
  → どちらかを単一の真実にする。

- [x] **M5 `logo_match` の完全一致マッチがスケールしない** `model_map.rs:58-60`

  `HashMap::get(make)` の完全一致で `"SONY"`/`"Sony"`/`"Sony Corporation"` を手動列挙している。
  Canon(`Canon`), Nikon(`NIKON CORPORATION`) と増やすたびに表記揺れを人力で列挙し続けることになる。
  **DEAD-5 の fujifilm 配線漏れは偶発的ミスではなくこの設計の必然。**
  → trim + 大文字化の正規化キーで比較する（非破壊な改善）。
  → `lens_brand_logo` 側の `contains` は逆に緩すぎ、空パターンが全マッチする。ロード時に拒否する。

- [x] **M6 `match_type` が stringly-typed で未知の値を無警告で無視** `model_map.rs:23-28`, `:62-74`
  → `#[serde(rename_all = "snake_case")] enum MatchType { Contains }` にして fail-fast。
- [x] **M7 `ModelMapJson` に `#[serde(default)]` が無い** `model_map.rs:12-16`
  → ユーザーが `logo_match` のみを上書きしたい場合でも両フィールド必須になっている。
- [x] **M8 ロゴのファイル名をサニタイズせず `dir.join()`** `logo.rs:68-99`, `:115-150`
  → `user_model_map` や `--preset-file` 経由で `"../../secret"` が入るとディレクトリを脱出しうる。
    `preset.rs:82-86` の `sanitize_filename` と同等の検証を入れる。
- [x] **M9 統合テストが「panic しないこと」しか確認していない** `exif_frame_v2_integration.rs`
  - `crop_mode_ignores_exif_frame_config`（`:109-127`）はテスト名に反して**「無視されたこと」を
    一切検証していない**（`is_ok()` のみ。exif フレームが誤って適用されても通る）
    → 出力アスペクト比（crop なら 0.8、quality なら元比率維持）を assert する
  - `skip_exif` パスの統合テストが無い（layout 単体テストのみ）
- [x] **L3** `mod.rs:525-559` の手書き90度回転は `image::imageops::rotate90` で置換可能。
- [x] **L4** `logo.rs:53` SVG の width/height が0だとゼロ除算で NaN/Inf。
- [x] **L5** `layout.rs:190-250` の最終フォールバックで Exif バーが `skip_exif` を経由せず
      静かに切り詰められる（`:214`, `:262` の `min(rem_h)`）。C1 修正後に到達可能性を再検証。
- [x] **L6** 9.1MB の CJK フォントが exif-frame を使わない CLI ユーザーのバイナリにも常に乗る
      （`text.rs:7-9` の無条件 `#[derive(Embed)]`）。Cargo feature での切り分けを検討。
- [x] **L7 同梱フォントの中身がファイル名と食い違っている**（S1 で発見 / 2026-08-04）

  `core/assets/fonts/NotoSansJP-Regular.ttf` の name テーブルは以下の通りで、
  実体は **Regular ではなく Thin ウェイト**。Exif テキストが意図より極細で描画されている。

  | name ID | 値 |
  |---|---|
  | 0 (copyright) | `(c) 2014-2021 Adobe (http://www.adobe.com/), with Reserved Font Name 'Source'.` |
  | 1 (family) | `Noto Sans JP Thin` |
  | 6 (postscript) | `NotoSansJP-Thin` |
  | 5 (version) | `Version 2.004-H2` |

  → Regular ウェイトを取得して差し替える（ファイル名は据え置きでよい）。
    差し替え時は `OFL.txt` の著作権表記が新ファイルの name ID 0 と一致するか再確認すること。
    L6 の feature 切り分けと同時にやると差分が読みやすい。

### S4 実施メモ（2026-08-05）

**S3 と同様、指摘を個別にではなく5つの変更セットに束ねた。**

1. **C1 + L5 → キャンバス拡張の丸め方を「k が整数になる辺」基準に統一**
   - Bottom/Top は**高さを5の倍数**に切り上げて `w = h/5*4`、Right/Left は**幅を4の倍数**に
     切り上げて `h = w/4*5`。どちらも割り切れるので 4:5 が 1px も崩れない。
   - `if canvas_*_expanded >= new_photo_*` の分岐は**到達不能になったので削除**した。
     拡張後のキャンバスは必ず `fit_to_4_5` の結果より大きくなる（証明はコード中のコメント）。
     これにより L5 の「Exif バーが `skip_exif` を経由せず静かに切り詰められる」経路が消え、
     `exif_bar_size.min(rem)` は防御的なクランプに退いた（`debug_assert!` で不変条件を明示）。
   - 「短辺の6%・最低30px」を `layout::exif_bar_size()` として公開した。テストが
     実装式を書き写さずに仕様値を参照できるようにするため。
2. **新規発見: Right/Left で写真が Exif バーの下に潜り込んでいた**
   不変条件テストが C1 とは別に検出した。`photo_x` をキャンバス全体で中央寄せしていたため、
   余白がバー幅ちょうどのとき（例 `400x501`）写真がバー領域に 15px 食い込み、
   Exif テキストが写真の上に重なっていた。Bottom/Top と同じく
   **バーを除いた領域で中央寄せ**に修正。
3. **C5 + M5 + M6 + M7 + M8 + H4 → `ModelMap` の全面的な作り直しと `ExifAssets` の新設**
   - `load_bundled()` は `Result` を返す。`text.rs` の `load_font` も
     `OnceLock::get_or_init` の中で `.expect()` していたのをやめた
     （panic すると `process_image` の pad フォールバックを飛び越え、
     rayon 上ではバッチ全体を巻き込み、さらに OnceLock が未初期化のまま残る）。
     `get()` → 失敗しうるパース → `get_or_init` の順にすることで、
     競合時に同じフォントを2度パースするだけで済み毒されない。
   - **`ExifAssets`（`dirs` + `model_map` + `warnings`）を新設**し、
     `render_exif_frame` / `process_image` / `process_batch` の引数を
     `AssetDirs` から `ExifAssets` に差し替えた。構築は CLI/GUI が**バッチ前に1回だけ**行う。
     カスタム model_map の読み込み・パース失敗は握りつぶさず `warnings` に載る
     （横断的設計決定 1。core は `eprintln!` しない）。
   - メーカー判定は trim + 大文字化の正規化キー。完全一致で外したら**先頭トークン**でも引く
     ので `"Sony Corporation"` / `"NIKON CORPORATION"` を列挙しなくてよい。
     `model_map.json` の 6 エントリは 2 エントリに減った。
   - `match_type` は `enum MatchType { Contains }`（未知の値は serde がエラーにする）。
     空パターンの `contains`（全レンズにマッチする）はロード時に拒否。
   - `validate_asset_filename` を新設し、`model_map` のロード時と
     `resolve_logo_file` / `resolve_and_load_logo` の両方で検証する
     （`pub` API なので JSON 側の検証だけに依存させない）。
4. **H5 + H6 + H7 + L3 → Exifバーを「1つの透明バッファに1回だけ描く」形へ統一**
   - `draw_exif_horizontal`(125行) と `draw_exif_rotated`(150行) を
     `render_exif_bar` + `draw_exif_area` に置換（引数は `ExifBar` 構造体にまとめ、
     `#[allow(clippy::too_many_arguments)]` と `TODO(S4-H4)` を除去）。
   - 合成は水平・回転とも `imageops::overlay`（アルファ合成）を通る。
     **H5（水平版だけセパレータが不透明）は「重複の解消」そのもので消えた**ので、
     H5 単独の回帰テストは書いていない。片方だけ壊す余地がもう無い。
   - 回転は手書きループをやめて `imageops::rotate90`（L3）。
   - H6 はレンズロゴを**テキストのフィッティング前に**確定させ、その幅+マージンを
     テキスト領域から差し引く形に変更した。
5. **L6 + L7 → 同梱フォント**
   - `NotoSansJP-Regular.ttf`（9.1MB）の中身は **Thin ウェイト**だった。
     noto-cjk の `Sans2.004` リリースから `NotoSansJP-Regular.otf`（4.3MB, Regular）に差し替え、
     `OFL.txt` の著作権表記も新ファイルの name ID 0 に合わせた。
     ファイルサイズは 9.1MB → 4.3MB に減る。README の同梱アセット表も更新。
   - `bundled-font` feature（default on）で埋め込み自体を切れるようにした（L6）。
     off でも `font_path` を明示すれば Exif フレームは動く。
     `cargo check -p picture-tool-core --no-default-features` が通ることを確認済み。
6. **M4 → デフォルト値の単一の真実は `ExifFrameConfig::default()`**
   - `core/assets/presets/default.json` を**削除**し、`load_bundled_presets()` は
     `vec![ExifFrameConfig::default()]` を返すだけにした。`rust-embed` の
     `PresetAssets` ごと不要になり、「JSON パース失敗で default プリセットが消える」
     という失敗モードも同時に無くなった。CLI の
     「プリセットが見つからなければ Rust の Default」というフォールバックとも定義上一致する。
   - → **S7 への申し送り**: `--preset-file` 用のサンプル JSON がリポジトリから
     無くなったので、README に例を載せること。

**テスト 90 → 107**（`test-integrity` skill 適用、Rigor: **Full**）:
- `layout::tests::layout_invariants_hold_for_non_round_sizes` — **これが本命**。
  4の倍数にも5の倍数にも揃っていない15サイズ × 5 placement について
  「厳密な4:5」「写真がキャンバスに収まる」「Exifバーが仕様どおりの太さ」
  「バーが写真に重ならない」を検査する。**2 の不具合はこのテストが見つけた。**
- `layout::tests::fallback_canvas_expansion_keeps_exact_4_5` — 計画書が挙げた `400x501` を単独で残置。
- `layout::tests::exif_bar_is_six_percent_of_short_side_with_30px_floor` — 比率と下限を固定値で。
- `exif_frame_v2_integration` の比率検証を `(ratio-0.8).abs() < 0.02` から
  `w*5 == h*4` の**厳密比較**に変更し、非round数値の網羅ケースを追加（M9）。
- `crop_mode_ignores_exif_frame_config` は出力を実際に開いて `640x800` を assert
  （旧テストは `is_ok()` だけで、Exif フレームが誤適用されても通っていた）。
  `quality_mode_ignores_exif_frame_config` は 4:5 でない `1200x800` を入力にして
  「寸法が変わらない」ことを assert する（旧テストの `800x1000` では
  「何もしない」と「4:5に変換した」を出力から区別できなかった）。
- `unloadable_font_falls_back_to_pad_with_a_warning` — C5 の本丸。フォントが読めなくても
  panic せず pad にフォールバックし、warnings に載ることを end-to-end で確認。
- `tiny_image_skips_exif_but_still_becomes_4_5` — skip_exif は「Exif を描かない」であって
  「4:5 変換を放棄する」ではない（統合テストが無かった経路）。
- `exif_frame::tests::lens_logo_never_overlaps_the_exif_text`（H6）— マゼンタ単色のロゴを描き、
  ロゴが占める帯にテキスト画素が1つも入らないことを検査。
  **「ロゴ無しなら帯までテキストが届く」という前提条件も同時に assert する**
  （テキストが短くて偶然重ならなかっただけ、という無力なテストになるのを防ぐため。
  実際、最初に書いた版はこの前提を満たしておらず回帰を検出できなかった）。
- `model_map` は表記揺れの許容・別メーカーの誤認防止・未知 match_type の拒否・
  空パターンの拒否・ディレクトリ脱出の拒否をそれぞれ独立に検査。
- `logo::tests::logo_names_cannot_escape_the_asset_directory` / `zero_sized_svg_is_rejected`。
- `text::tests::bundled_font_is_the_regular_weight_not_a_thin_one` — name テーブルを
  自前で読んで PostScript 名が `NotoSansJP-Regular` であることを確認する。
  ファイル名ではなく**フォント自身の申告**を見るのがポイント（L7 が数か月見過ごされた原因）。

**ミューテーションテストで実効性を実測した**（Phase C）:
- C1 のモジュラスを元の左右逆転に戻す → `layout_invariants_...` と
  `fallback_canvas_expansion_keeps_exact_4_5` の**2件が FAILED**（`405x506` を報告）。
- H6 のレンズロゴ幅の差し引きを外す → `lens_logo_never_overlaps_the_exif_text` が FAILED
  （`(736, 16)` のテキスト画素を検出）。
- 2 の写真中央寄せは、修正前の状態で不変条件テストが実際に落ちることを確認済み
  （overlap を報告して発見された）。
- L7 は差し替え前のファイルの PostScript 名が `NotoSansJP-Thin` であることを確認済み
  （＝新テストは旧ファイルで必ず落ちる）。

**検証**: `make check` green（`cargo fmt --all -- --check` / `cargo clippy --workspace
--all-targets -- -D warnings` / `cargo test --workspace` **107 passed** /
`bunx svelte-check` 0 errors / 6 warnings = S5-M12 のまま）。
`cargo check -p picture-tool-core --no-default-features` も green。

**実機確認**: CLI で 4枚（`400x501` / `1200x800` / `1000x1000` / `150x100`）を
`-m pad -e` で処理し、出力が全て厳密な 4:5 であることを確認した
（`400x501` → **404x505**。修正前は `405x506` で 4:5 ではなかった）。
Bottom と Right の描画結果を PNG に出して目視でも比較し、
セパレータの半透明・ロゴ位置・2段テキストが**両者で同一**になったことを確認済み。

**S7 への申し送り（新規発見）**: レンズブランドロゴが実質見えない。
`gmaster.png` は 2880x1748 だが、描画高さが `secondary_size * 1.2`（72px バーで約21px）
なので細線が潰れて数ピクセルの点になる。S4 の範囲は「テキストと重ならないこと」なので
サイズ規則は**変更していない**が、DOC-2 が挙げている
「レンズブランドロゴはレンズ型番の直前にインライン」という spec との乖離を
解消する際に、サイズも併せて設計し直すこと。

**S5 への申し送り**: `ExifAssets::warnings`（カスタム model_map の不備）は現在
GUI では `eprintln!` するだけで UI に出ていない。`ProcessResult.warnings` /
`size_limit_exceeded`（S3 から持ち越し）と併せて F8 で扱うこと。

---

## S5. フロントエンド（`gui-frontend/src/`）

- [x] **F1 プリセットのシャローコピーによる汚染** `lib/ExifFrameSettings.svelte:71-77`

  ```ts
  config = { ...preset };   // items / font が presets 配列内のオブジェクトと同一参照
  ```
  以降の `config.items[key] = !...`（`:154`）、`bind:value={config.font.primary_size}`（`:167`）が
  **`presets` state 内の元オブジェクトを直接ミューテートする**。
  プリセットを切り替えて戻すと編集が残る／別プリセットのつもりで保存すると別ファイルが汚染される。
  → `structuredClone(preset)` にする。

- [x] **F2 歯車から開いた設定に選択中プリセットが引き継がれない** `lib/ExifFrameSettings.svelte:5-13`, `:37-39`

  `Props` に選択中プリセット名を受け取る口が無く、`config` は常に `defaultConfig()`（`name: 'default'`）
  で初期化される。`SettingsPanel` で "portrait" を選んで歯車を押しても `default` の内容から始まり、
  そのまま保存すると `config.name`（=`"default"`）で書き込まれ
  **`default` プリセットを意図せず上書きする**（`preset.rs:63-70` は同名で上書き）。
  → 選択中プリセット名を prop で渡し、初期 `config` をそれに合わせる。

- [x] **F3 `delete_originals` に確認フローが無い** `lib/SettingsPanel.svelte:74-77` → `App.svelte:171-192`

  チェックボックス1つで元ファイル一括削除が走る。確認ダイアログも二段階確認も無い。
  CLAUDE.md の「`--delete-originals` で明示的に削除」という方針が GUI で担保されていない。
  → 実行前にモーダル確認を挟む（削除対象件数を明示する）。

- [x] **F4 レースコンディション（2箇所）**
  - `App.svelte:136-145` `handleSelectFolder`: フォルダー連打で古い `listImages` の応答が
    新しい一覧を上書きし、表示中フォルダーとサムネイルが食い違う
  - `lib/ImagePreview.svelte:44-47`: 矢印キー高速ナビで別画像の EXIF / 全体画像が表示される

  **構造的な問題**: `$effect` 内で非同期を発火して結果を直接 `$state` に代入するパターンが
  ガード無しで繰り返されている。`App.svelte:112-131` は `cancelled` フラグで正しく処理しているので、
  同じ方式（リクエストトークン or `path` 一致チェック）に揃える。

- [x] **F5 サムネイルキャッシュのキーに解像度が含まれない** `App.svelte:28`, `:53-77`

  フロントは `Map<path, base64>`、Rust 側（`gui/src/commands.rs:126-158`）は
  `format!("{}:{}", path, max_dimension)`。`handleRequestThumbnail` が `has(path)` で早期リターン
  するため、列数スライダーで `thumbSize` を変えても**再取得されず低解像度のまま引き伸ばされる**。
  → キーを `${path}:${maxDimension}` に揃える。

- [x] **F6 型エラーが混入している** `lib/FolderTree.svelte:39` — **S2 で修正済み**

  `load("favorites.json", { autoSave: false })` — `StoreOptions` の必須 `defaults` が欠落。
  svelte-check を CI に入れるとこの1件だけで CI が red になるため、S2 に前倒しした。
  `{ defaults: {}, autoSave: false }` に修正（ランタイム挙動は不変）。

- [x] **F12 フォント選択 UI とプリセット削除ボタンが無い**（S2-DEAD-4 から派生）

  `list_available_fonts` / `delete_preset` は Rust・`api.ts` まで実装済みだが UI が無い。
  S2 で「バックエンドを残す」と決定したので、ここで UI を作って配線を完成させる。
  フォント選択は v2 spec:247 の要求機能。

- [x] **F7 Exif プレビューで `bgColor` の変更が追跡されない** `lib/ExifFrameSettings.svelte:52-68`

  `bgColor` を `setTimeout` コールバック（非同期）内でのみ参照しているため、
  `$effect` の同期実行フェーズで読まれず**リアクティブ依存として追跡されない**。
  → 同期部分で `const bg = bgColor;` を読む。

- [x] **F8 エラーの握りつぶし**
  - `lib/FolderTree.svelte:109-125`, `:130-134`: `catch (e) { node.children = [] }` と
    `.catch(() => {})`。権限エラーでも「空のフォルダー」にしか見えず原因が分からない
  - `App.svelte:72` サムネイル取得の `.catch(() => {})`
  - `App.svelte:160-169`, `:194-196`: `handlePickOutputFolder` / `handleCancel` が try/catch なしで
    fire-and-forget（他のハンドラーは try/catch しており不統一）

- [x] **M10 プリセット一覧の状態が2箇所で重複管理** `App.svelte:33`, `:36-42` と
      `lib/ExifFrameSettings.svelte:38`, `:44-48`（それぞれ独立に `listPresets()` を呼ぶ）
      → props 経由で一本化。
- [x] **M11 `SelectionList.svelte:14-20` の `$effect` が `thumbnailCache` を読むため、
      サムネイル1枚ロードごとに選択済み全件をループ**（実質 O(n²)）。
- [x] **M12 アクセシビリティ**（`bun run build` で Svelte コンパイラが実際に警告を出力）
  - `lib/ImagePreview.svelte:208` `role="dialog"` に `tabindex` もフォーカストラップも無く、
    Escape 以外でキーボードから閉じられない
  - 同 `:253`, `:263` `<img>` にマウスイベントのみ
  - `lib/ExifFrameSettings.svelte:132` `<label>` がコントロールと未紐付け
  - `lib/FolderTree.svelte:68-85`, `:173` お気に入り追加/削除が右クリックのみでキーボード不可
- [x] **L7** `app.css:1-17` `--text-muted: #666` を `font-size: 10px` で多用（コントラスト・視認性）。
      ライト/ダーク切替も未対応（常に固定ダーク）。
- [x] **L8** `App.svelte:185`, `:187` の `alert()` をトースト等に置換。
- [x] **L9** `types.ts:14` `ImageEntry.thumbnail_base64` はデッドフィールド（Rust 側も常に `None`）。
- [x] **L10** `ThumbnailGrid.svelte:21` の `void columnCount;` は直後に読んでいるため冗長。
- [x] **L11** `App.svelte:36-42`, `FolderTree.svelte:141-144` の「マウント時1回」`$effect` は
      `onMount` の方が意図が明確。
- [x] **F13（着手時に追加）** 「横断的な設計決定 1」が S5 に持ち越すと明記していた
      `ProcessResult.warnings` / `size_limit_exceeded` の表示 UI。チェックリストから漏れていた。
      L8（`alert()` 置換）と一体で `ResultDialog` として実装。

### S5 実施メモ（2026-08-05）

**新規ファイル**（4つのモーダルで重複しないよう共通部品に集約した）

| ファイル | 目的 | 対応項目 |
|---|---|---|
| `lib/focusTrap.ts` | モーダル共通のフォーカストラップ Svelte action | M12 |
| `lib/toasts.svelte.ts` + `lib/Toast.svelte` | 握りつぶしていた例外と `alert()` の置換先 | F8, L8 |
| `lib/ConfirmDialog.svelte` | 破壊的操作の確認（初期フォーカスはキャンセル側） | F3 |
| `lib/ResultDialog.svelte` | 成功/失敗/サイズ超過/警告の内訳 | F13, L8 |

**判断のメモ**

- **F5 のキー変更に伴う副作用を潰した。** `path:maxDimension` にしただけだと、列数変更で
  要求解像度が変わっても `IntersectionObserver` が既に切断済みで再取得されない。
  action に `update` を持たせて再要求するようにし、併せて要求サイズを 64px 刻みに丸めて
  キャッシュの再利用率を確保した（`ThumbnailGrid.svelte`）。
- **`Map` の再代入ハックを `SvelteMap` に置換**（`svelte/reactivity`）。
  子には生の Map ではなく `thumbnailFor(path, size)` を渡し、キーの組み立てを App に閉じ込めた。
- **M12 のお気に入り操作は、右クリックにキーボード経路を足すのではなく常設トグルに置換した。**
  同じ操作の入口が2つある状態を避けるためコンテキストメニューは削除（約40行減）。
- **F12 に「プリセット名」入力を追加した。** F2 の修正（選択中プリセットを初期値にする）だけだと
  常に選択中を上書きすることになり、**新規プリセットを作る経路が存在しなくなる**ため。
  併せて削除ボタンは `default`（バンドル）に対しては無効化している。
- **L7 のライト/ダーク切替は S5 では実施しない。** コントラスト（`--text-secondary` / `--text-muted`
  を AA 準拠に引き上げ、10px 併用箇所を 11px 化）のみ対応。テーマ切替は新機能でありセッションの
  主旨（正しさ・安全性）から外れるため UX 改善側で扱う。
- `SettingsPanel` の未使用 prop `currentFolder` を削除（触っている箇所に隣接するデッドコード）。

**S6 への申し送り**

- `gui/src/commands.rs:174-258` に `TODO(S5-F8)` コメントがあるが、これは `ExifAssets::load` の
  警告を `eprintln!` している箇所で **バックエンド側の話**。S6 の M15 と併せて片付けること。
- `process_images` は成功分しか返さず、失敗は件数だけ `processing-error` で emit している。
  フロントはこのイベントを購読していない（`ResultDialog` が要求リストとの差分で失敗を復元するため）。
  S6 で `emit` を整理するならこの死に配線も一緒に判断すること。
- `types.ts` から `ImageEntry.thumbnail_base64` を削除した（L9）。Rust 側の同名フィールドは
  未削除なので S6 で落とすこと。

**検証**（2026-08-05）

| コマンド | 結果 |
|---|---|
| `bunx svelte-check` | **0 errors / 0 warnings**（S5 前は 0 errors / 6 warnings） |
| `bun run build` | 成功。Svelte コンパイラ警告なし |
| `cargo test --workspace` | 107 passed（ベースライン維持） |
| `cargo fmt --all -- --check` | green |

Tauri 無しでは到達できない経路は `vite dev` + Playwright で実機確認した。

- 起動時の4つの失敗（進捗購読 / ドライブ一覧 / プリセット / お気に入り）がすべてトースト表示に
  なることを確認（従来は `console.error` か完全な握りつぶし）
- `ConfirmDialog`: 初期フォーカスがキャンセル、Tab がダイアログ内で巻き戻る、
  Escape で閉じてトリガー要素へフォーカスが戻る
- `ResultDialog`: 要求リストとの差分から失敗ファイルを復元、`size_limit_exceeded` と
  `warnings` を表示、キャンセル時は「失敗」ではなく「未処理」と表記

---

## S6. Tauri バックエンド（`gui/src/`, `gui/capabilities/`）

- [x] **H8 全コマンドがパス検証なしで任意ファイルを読み書きできる** `gui/src/commands.rs` 全体

  **重要な前提**: Tauri v2 の capabilities/ACL は**プラグインコマンドのみ**を対象とし、
  `invoke_handler(generate_handler![...])` で登録した**自作コマンドは ACL の対象外**。
  つまりフロントエンドコード自身が唯一の防波堤で、webview が乗っ取られれば
  `~/.ssh/*` の読み取りも `delete_originals=true` での任意パス削除も通る。

  対象: `list_directory`(`:11-16`), `list_images`(`:77-81`), `get_thumbnail`(`:126-130`),
  `get_full_image`(`:161-165`), `get_exif_info`(`:267-269`),
  `render_exif_frame_preview`(`:279-286`), `process_images`(`:174-190`)

  → `fs::canonicalize` で正規化し、ユーザーがネイティブダイアログで選択したルート配下のみ許可する。
  → CSP（`tauri.conf.json:14` `default-src 'self'`）自体は適切に絞れている。

- [x] **M13 `dialog:default` が過剰権限** `gui/capabilities/default.json:8`
      `ask/confirm/message/save/open` を全許可するが、使っているのは `open()` のみ
      （`App.svelte:3`）。明示的な `dialog:allow-open` 行があるので `dialog:default` は削除できる。
- [x] **M14 `list_directory` は1エントリのエラーで全体が失敗** `gui/src/commands.rs:19-22`
      `entry.file_type()?` が壊れたシンボリックリンク等で即 return。
      CLAUDE.md の「失敗はスキップして継続」方針に反する。`list_images` 側は `.flatten()` で
      握りつぶしており一貫性も無い。→ `continue` でスキップ。
- [x] **M15 `emit()` の戻り値握りつぶし** `gui/src/commands.rs:215`, `:254`
      進捗が止まった時に原因を追えない。最低限 `eprintln!` する。
- [x] **M16 プレビュー生成ロジックが GUI に閉じ込められている** `gui/src/commands.rs:279-320`
      「400px リサイズ → render → JPEG 85% → base64」を丸ごと実装しており、
      GUI が `image`/`base64` を直接依存する原因になっている。CLI に `--preview` 相当が作れない。
      → core に `render_exif_frame_preview_base64` を置く。
- [x] **M17 設定ディレクトリのパス計算が4箇所に散在**
      `exif_frame/mod.rs:121-123`, `cli/src/main.rs:119-120`,
      `gui/src/commands.rs:324`, `:330-332`, `:338-340`（同じ文字列を直書き）。
      → core の `AssetDirs` に `user_presets_dir` を追加して一元化。
- [x] **M18 `process_images` が約85行** `gui/src/commands.rs:174-258` → 分割。
- [x] **M19 `[workspace.dependencies]` 未使用**。`anyhow`, `serde_json`, `dirs` が
      cli/core/gui にそれぞれ個別記述。→ ワークスペースに集約し `.workspace = true`。
- [x] **CLI-1 バッチ処理中に進捗が一切表示されない** `cli/src/main.rs:174-176`
      `on_progress` が `true` を返すだけ。大量処理時にフリーズして見える。
      → `eprint!("\r{}/{}", current, total)` 等を出す。
- [x] **CLI-2 clap レベルのバリデーション不足** `cli/src/main.rs:29-30`, `:33-34`
      `quality`/`max_size` を `value_parser!(u8).range(1..=100)` で弾くとエラーが分かりやすい。
- [x] **CLI-3 `ConversionMode` の enum が cli に複製されている** `cli/src/main.rs:61-66`, `:74-82`
      core の enum を変更しても**コンパイルエラーにならず片方だけ増える**。
      → core の enum に optional feature で `clap::ValueEnum` を derive するか、
        最低限「core を変えたら cli も」のコメントを残す。
- [x] **L12** `cli/src/main.rs:199`, `:202`, `:214` の `file_name().unwrap()`。
- [x] **L13** `cli/src/main.rs:171-172` の `AtomicUsize` は単一スレッドで不要。
- [x] **L14** `cli/src/main.rs:97`, `:106` で `ConversionMode::from(args.mode)` を二重に呼んでいる
      （fmt 崩れの一因）。`config.mode` を使う。
- [x] **L15** `[profile.release]` 未設定（`lto`, `codegen-units`, `strip`）。
- [x] **L16** `make release` は `tauri build` を呼ばずバンドル/インストーラを作らない。名前と実態が乖離。

### S6 実施メモ（2026-08-08〜09）

**新規ファイル**

| ファイル | 目的 |
|---|---|
| `gui/src/security.rs` | webview から渡された値（パス・フォント・プリセット名）の検証。境界の理由と脅威モデルをモジュール doc に記述 |

**境界の設計（H8）**

Tauri v2 の ACL は自作コマンドに適用されないため、コマンド自身が唯一の境界。
本アプリはツリーで FS 全体を閲覧する設計なので「1ルートに閉じる」ことはできず、操作種別で分けた。

| 操作 | 境界 |
|---|---|
| ディレクトリ一覧 | 実在するディレクトリであること |
| 画像の読み出し | 実体パスが対応画像の拡張子を持つこと |
| フォントの読み出し | 実体がユーザーフォントディレクトリ配下の ttf/otf であること |
| 書き込み | ネイティブダイアログで許可したルート配下 |
| 元ファイルの削除 | 実行ごとに OS ネイティブの確認ダイアログで承認されること |

判定はすべて `canonicalize` 済みの**実体**に対して行う。許可を与えられるのは Rust 側で
ダイアログを開く `pick_output_folder` だけで、webview から自分に許可を出す経路は無い。

**判断のメモ**

- **`dialog` に続いて `store` プラグインの権限も webview から落とした。** レビューで
  「`security.rs` の境界が store 経由で丸ごと迂回できる」と指摘され、実際に確認した:
  store プラグインの `load(path)` はパスを `AppData` 基準で解決するだけで `..` も絶対パスも
  正規化しない（`tauri/src/path/mod.rs` の `_up_` 変換は `BaseDirectory::Resource` 限定）。
  つまり `store:allow-load` があれば `load("../../../.ssh/x.json")` で任意 JSON を読み書きできた。
  用途はお気に入りの文字列配列だけなので、保存先を Rust に固定した `load_favorites` /
  `save_favorites` コマンドに置き換え、capabilities は `core:default` のみにした。
  → **教訓**: 自作コマンドを固めても、webview に与えたプラグイン権限が同じ強さの穴を開ける。
    境界を評価するときは capabilities も同じ面に並べて見ること。
- **元ファイル削除は「許可ルート配下」に縛らなかった。** 入力はツリーから自由に選ぶ設計なので、
  縛ると通常利用（入力 `~/photos` / 出力 `~/out`）で削除が一切できなくなる。代わりに
  webview が偽装できない OS のダイアログを毎回挟み、**枚数だけでなく削除先フォルダー一覧**を
  提示する（枚数だけでは、乗っ取られた webview がライブラリ全体を混ぜても気づけない）。
- **`ExifFrameConfig` も webview 由来のデータとして扱う。** `font.font_path` は無検証で
  `fs::read` に届いており、`/dev/zero` でメモリを枯渇させたり、エラー文面の差分で任意パスの
  存在を列挙できた。`readable_font` で「ユーザーフォントディレクトリ配下の ttf/otf」に限定。
  `save_preset` も同じ検証を通す（汚染されたプリセットは CLI からも読まれ、webview の
  生存期間を越えて残るため）。
- **プリセット名の検証を GUI 境界にも置いた。** core の `sanitize_filename` だけでも現状
  traversal は起きないが、core が許容文字を緩めた瞬間に GUI が無警告で穴を得る。
  契約は境界に書く。
- **キャンセルは「失敗」ではない。** `process_batch` は入力と同数の結果を返し、キャンセル後の
  分も `Err` になる。これをそのまま `failures` に載せると全件が赤字の失敗として並ぶため、
  `core::CANCELLED_ERROR` を公開定数にし、GUI では results にも failures にも載せない。
  フロントが要求リストとの差分から「未処理」として表示する（S5 の `ResultDialog` の想定どおり）。
- **入力の検証失敗でバッチ全体を落とさない。** 選択後に1枚消えていただけで全滅していた。
  `failures` に理由付きで載せ、残りは処理する（CLAUDE.md「失敗はスキップして継続」）。
- **M18 は `run_batch` / `progress_callback` / `confirm_delete_originals` / `validate_inputs` /
  `split_results` への分割まで。** `process_images` 本体は検証→確認→実行→整形の流れだけになった。

**受け入れた残リスク**（対応しないと決めたもの）

- **TOCTOU**: 検証と I/O の間にリンクを差し替えられる攻撃は防げない。ローカル権限を持つ
  攻撃者の話であり、本設計の脅威モデル（乗っ取られた webview）の外。
- **`pick_output_folder` の初期表示位置を webview が指定できる**: 許可されるのはダイアログの
  戻り値だけなので任意許可にはならないが、`/` を初期表示にして反射的な「開く」を誘える。
  ダイアログの表示内容自体は OS のもので偽装できないため許容した。
- **画像判定は実体パスの拡張子**（マジックナンバーではない）: `.jpg` という名前の非画像は
  下流のデコードで失敗する。`security.rs` の表現を実装に合わせて直した。

**外部レビューの指摘と対応**（rust-reviewer / security-reviewer）

| 指摘 | 重大度 | 対応 |
|---|---|---|
| store プラグイン権限で境界を迂回できる | Critical | 修正（権限削除 + Rust 側コマンド化） |
| `delete_originals` の対象が無制限 | High | 確認ダイアログに削除先フォルダー一覧を追加 |
| `font_path` が無検証で `fs::read` に到達 | High | `security::readable_font` を追加 |
| `..` を未作成成分の後ろに隠す経路がテストされていない | Medium | テスト追加（防御機構が実際に発火する形で） |
| 兄弟ディレクトリ（`photos-evil`）の誤許可テストが無い | Medium | テスト追加 |
| プリセット名の検証が core の実装詳細に暗黙依存 | Medium | `security::preset_name` を追加 |
| 入力1件の検証失敗でバッチ全滅 | Low | 修正 |
| サムネイルキャッシュキーがクランプ前の値 | Low | 修正（`core::THUMBNAIL_MAX_DIMENSION`） |
| `user_config_dir` が不要に公開されている | Low | `pub(crate)` へ |
| CLI 進捗表示の `let _ = write!` | Low | 意図的（壊れたパイプで処理を止めない）。対応せず |
| サムネイルキャッシュの `Arc<str>` 化 | Low | 現状ボトルネックではない。対応せず |

**検証**（2026-08-09）

| コマンド | 結果 |
|---|---|
| `cargo test --workspace` | **138 passed**（S6 前 107 / レビュー前 123） |
| `cargo clippy --workspace --all-targets -- -D warnings` | green |
| `cargo fmt --all -- --check` | green |
| `bunx svelte-check` | 0 errors / 0 warnings |
| `bun run build` | 成功 |

Tauri 無しでは到達できない経路は `vite dev` + Playwright で実機確認した。
起動時の4つの失敗（進捗購読 / ドライブ一覧 / プリセット / **お気に入り**）がすべて
トーストとして表示されることを確認。お気に入りは store プラグイン経由から `invoke` 経由に
変わったが、失敗時の見え方は変わっていない。

**S7 への申し送り**

- CLAUDE.md の「GUI 3カラムレイアウト」以外にバックエンドの記述が無い。**Tauri コマンドが
  唯一の信頼境界であること**と `gui/src/security.rs` の存在は、実装者が次に触るときに
  知っている必要がある（知らずに新しいコマンドを足すと境界が破れる）。
- `make release` が `tauri build` を呼ぶようになった（L16）。README のビルド手順に反映すること。
- CLI に `--exif-frame` 系のオプション表が無い件（DOC-1）は S6 でも解消していない。
- お気に入りの保存先が `favorites.json`（AppData 配下）である点はどこにも書かれていない。

---

## S7. ドキュメント整合

**S1〜S6 完了後に実施すること**（先にやると再度ズレる）。

> S1 で README 末尾に「ライセンス」節（同梱アセット表 / 商標免責 / ユーザーロゴ配置手順）を
> 追加済み。DOC-1 で exif-frame の記述を足すときに消さないこと。

- [x] **DOC-1 exif-frame 機能が README.md / CLAUDE.md に一切存在しない**（`grep -i exif` で0件）
  - 未記載の CLI オプション: `--exif-frame`/`-e`, `--preset`/`-p`, `--preset-file`, `--custom-text`
  - **「Pad モード限定」という v2 の最重要制約**がどこにも書かれていない
  - 「細かい制御は `--preset-file` で」という導線も未記載（CLI から位置・項目を個別指定できない）
- [x] **DOC-2 v2 spec を実装に追従させる**（`docs/superpowers/specs/2026-03-29-exif-frame-v2-design.md`）
  | spec | 実装 |
  |---|---|
  | 縦構図は「1行凝縮・90度回転」(`:70-89`) | 横構図と同じ2段構成（コミット `5273d30` で**設計判断が逆転**、spec 未更新） |
  | レンズブランドロゴはレンズ型番の直前にインライン(`:191-193`) | Exif エリアの右端（コード内コメントも「簡易実装」と自認） |
  | オーバーフロー時に優先度の低い項目から省略(`:91-94`) | **未実装**（フォント縮小＋`...` 切り詰めのみ） |
  | セパレータは半角パイプで統一(`:67`) | 1行目は `" \| "`、2行目はスペース2個 |
  | GUI にフォント選択を残す(`:247`) | UI 無し（DEAD-4） |
  | 背景はカスタム RGB にも対応(`:68`) | `BackgroundColor` は White/Black の2値 enum。輝度計算(`mod.rs:194-196`)は汎用 RGB 対応済みなのに入力側が塞がっている |
  特に「1行凝縮→2段」は設計判断の逆転なので、**理由付きで** spec に反映する。
- [x] **DOC-3** `README.md:80` 「テスト実行（23件）」→ 実際78件。数値の埋め込み自体をやめる。
- [x] **DOC-4** `CLAUDE.md:73` の主要関数リストが古い。`read_exif_info`, `generate_full_image_base64`,
      `is_supported_image`, `exif_frame::render_exif_frame` が欠落。
      `process_image`/`process_batch` の引数も増えている。
- [x] **DOC-5** `docs/` に索引が無い（specs 5本 / plans 6本がフラット）。
      v1 spec（`2026-03-25-exif-frame-design.md`）の冒頭に **Superseded by v2** を明記。
- [x] **DOC-6** `.claude/agents/`, `.claude/skills/` をコミットしている意図を CLAUDE.md に記載
      （チーム共有なら妥当だが、現状は意図不明）。

### S7 実施メモ（2026-08-09）

**新規ファイル**

| ファイル | 目的 |
|---|---|
| `docs/README.md` | ドキュメント索引。現行仕様 / 過去の仕様 / 読む順番を明示（DOC-5） |

**判断のメモ**

- **DOC-2 は「spec を実装に合わせて書き換える」ではなく「逆転した判断を理由付きで残す」形にした。**
  結論だけ書き換えると、次に同じ設計を検討したときに同じ議論をやり直すことになる。
  当初案の図と、なぜそれを捨てたか（計算が構図ごとに二重化する / テキスト中に画像を挟むと
  切り詰め・回転と絡んで破綻する）を並べて残している。
- **DOC-2 の「GUI にフォント選択を残す → UI 無し（DEAD-4）」は既に解消していた。**
  `ExifFrameSettings.svelte` にフォント選択が実装済みで、spec と実装は一致している。
  チェックリストの記述の方が古かった。
- **オーバーフロー時の項目省略は「未実装」と明記して残した。** 切り詰めで実用上は足りており、
  省略順を設定に出す価値が薄いと判断。spec から消すと「検討した上で見送った」情報まで消える。
- **DOC-3 は件数を書き直すのではなく、数値の埋め込み自体をやめた**（書いた瞬間に古くなるため）。
- **README 末尾に紛れ込んでいた `claude --resume ...` の1行を削除した。**
- **S6 の申し送りも取り込んだ**: CLAUDE.md に「Tauri コマンドが唯一の信頼境界」「新しい
  コマンドは必ず `security.rs` を通す」「webview にプラグイン権限を与えない」を明記。
  ユーザーデータの置き場所（logos / fonts / presets / model_map_custom / favorites.json）も表にした。
  `make release` が `tauri build` を呼ぶようになった点は README のコマンド説明に反映。
- **DOC-6 は利用者に意図を確認した上で記載**（チームと将来の自分で共有するため意図的にコミット）。

**残っている乖離**（記録のみ、対応せず）

- 背景色のカスタム RGB: 描画側（輝度計算）は任意 RGB に対応済みだが、入力の
  `BackgroundColor` は White/Black の2値 enum のまま。spec にその旨を注記した。

---

## 2. 横断的な設計決定（各セッションで参照）

修正時に判断がブレやすい点をここに集約する。決めたらこのセクションを更新すること。

1. **警告の伝達経路** — core は `eprintln!` しない。`ProcessResult.warnings: Vec<String>` に集約し、
   CLI は stderr、GUI は `ProcessResult` 経由でフロントに渡す。S3-H1/H2/M3、S4-H4 が全てこれに依存する。
   **→ S3 で実装済み（2026-08-05）**。`warnings: Vec<String>` と `size_limit_exceeded: bool` が
   `ProcessResult` にある。以降のセッションで新たな警告を出す必要が生じたら、`eprintln!` を
   足さずここに push すること。GUI の表示 UI は **S5-F13 で実装済み**（`lib/ResultDialog.svelte`）。
   新しい警告を足せば追加実装なしでそこに出る。
2. **default の単一の真実** — **`ExifFrameConfig::default()` を正とする**（S4-M4 で決定 / 2026-08-05）。
   `core/assets/presets/default.json` は削除し、`load_bundled_presets()` は
   `vec![ExifFrameConfig::default()]` を返す。以後、デフォルト値は Rust 側だけを直すこと。
3. **バンドルアセットの失敗方針** — **全て `Result` 伝搬に統一した**（S4-C5 / 2026-08-05）。
   `ModelMap::load_bundled` と `text::load_font` から `.expect()` を全廃。
   `OnceLock` のキャッシュは `get()` → 失敗しうる構築 → `get_or_init()` の順で行い、
   `get_or_init` のクロージャ内で panic して未初期化のまま毒されることが無いようにする。
4. **メーカー判定** — **trim + 大文字化の正規化キーに変更済み**（S4-M5 / 2026-08-05）。
   完全一致で外れたら先頭トークンでも引くので、`"Sony Corporation"` のような
   社名接尾辞を `model_map.json` に列挙しなくてよい。ロゴ参照名は
   `model_map::validate_asset_filename` で単純ファイル名に限定する（S4-M8）。
5. **テストは仕様から書く** — C1 が検出されなかったのは「実装が通る数値」でテストを書いたため。
   4:5 は `assert_eq!(w*5, h*4)` の厳密比較、crop の無視は出力比率で検証する。
   テストに触る前に `test-integrity` スキルを起動する（CLAUDE.md 規約）。
   **→ S4 で適用済み。加えて教訓が1つ増えた**（2026-08-05）: 「重ならないこと」のような
   否定形をテストするときは、**前提条件（重なりうる状況を作れているか）も同時に assert する**。
   S4-H6 の最初の版はテキストが短すぎて検査対象領域に届かず、バグを入れ直しても
   green のままだった。ミューテーションテストを回さなければ気づけなかった。

---

## 3. 完了条件

- [ ] `cargo test --workspace` が green（新規テスト含む）  ※ S4 時点 107 passed
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` が green
- [ ] `cargo fmt --all -- --check` が green
- [ ] `bunx svelte-check` が green
- [x] CI で上記すべてが自動実行され、**gui クレートが Linux でビルドされている**
      （run 30910677408 / `80c9e1e` で success 確認済み）
- [ ] README / CLAUDE.md / v2 spec が実装と一致
- [x] `LICENSE` と `OFL.txt` が存在し、商標素材の扱いが決定済み（S1 / バンドル維持 + 免責）
