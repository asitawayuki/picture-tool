# 出力幅の上限指定（`--max-width`）実装計画

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 出力 4:5 キャンバスの幅に上限（px）を指定できるようにし、pad / crop の出力を指定値以下へ縮小する。

**Architecture:** `ProcessingConfig` に `max_width: Option<u32>` を1つ足し、core のパイプラインに
「前段縮小（pad のみ・メモリ削減）」と「最終 `resize_exact`（契約の保証）」の2ステップを挿す。
正規化（4 の倍数への切り捨て）は core の1関数に閉じ込め、丸めの通知は入口（CLI は stderr、
GUI は入力確定時のスナップ）が行う。併せて `convert_aspect_ratio_pad` を `fit_to_4_5` に統一し、
「pad が作るキャンバス幅」の答えを1つにする。

**Tech Stack:** Rust（image 0.24 / rayon / clap derive / serde / anyhow）、Tauri v2、Svelte 5（runes）

**設計の出典:** [`docs/superpowers/specs/2026-08-12-output-width-limit-design.md`](../specs/2026-08-12-output-width-limit-design.md)
（以降「spec §N」と参照する）。**判断の根拠・却下した案・算術的導出はすべて spec にある。
迷ったら実装コードではなく spec を読むこと。**

## Global Constraints

これらは全タスクの要件に暗黙に含まれる。

- **core は `eprintln!` しない。** 利用者に伝えるべき事象は `ProcessResult.warnings` や
  戻り値に積み、CLI は stderr、GUI は結果ダイアログに出す（CLAUDE.md / spec §8）
- **4:5 は「おおむね 0.8」ではなく厳密な整数比。** キャンバスは常に `k*4 × k*5`。
  テストの比較も `w * 5 == h * 4` で行う（許容誤差つき比較は 1px のずれを原理的に検出できない）
- **丸めは切り捨てのみ。** `--max-width 1002` は 1000 になる。切り上げると「指定値を超えない」
  という機能の目的そのものを果たさない（spec §3）
- **拡大はしない。** 契約は等値ではなく不等式「出力キャンバス幅 ≤ 目標幅」。
  元画像が目標より小さければそのまま（spec §4）
- **`max_width` の範囲は 4..=20000 px。** clap と `validate_config` の両方で弾く
  （既存の `quality` 1..=100 / `max_size` 1..=1024 と同じく、両側に literal で書く）
- **リサイズフィルタは `Lanczos3`。** 前段は `resize`（ボックスに収める）、最終は `resize_exact`
- **quality モードは対象外。ただし黙って無視しない。** CLI は起動時に1回警告、
  GUI はトグルを無効化して理由を表示（spec §2）
- **新規 Tauri コマンドを追加しない。** `gui/src/security.rs` の検証対象（パス・フォント名・
  プリセット名）は増えない。もし途中でコマンドを足したくなったら、それは設計から外れている
- **テストコードを書く前に `test-integrity` スキルを起動する**（CLAUDE.md 規約 / spec §9）。
  各タスクのテストは spec §9 の番号（#1〜#10）に対応させてある。ケースは実装ではなく
  spec から導かれている
- **`gui` クレートに触る cargo コマンドの前に、必ずフロントエンドをビルドしておく**:
  `cd gui-frontend && bun install && bun run build`。`gui-frontend/dist` と `node_modules` は
  gitignore 済みで、`tauri-build` は `gui/tauri.conf.json:7` の `frontendDist` の実在を要求する
  （`beforeBuildCommand` は `tauri build` のときしか走らない）。新規 worktree では
  これを飛ばすと `cargo test --workspace` も `make check` も gui クレートのビルドで落ちる
  （姉妹 plan `2026-08-04-full-codebase-review-fixes.md` の「既知の環境制約」と同じ）
- 検証コマンド: `cargo test --workspace` / `cargo clippy --workspace --all-targets -- -D warnings` /
  `cargo fmt --all -- --check` / `cd gui-frontend && bun run typecheck`（`make check` で一括）。
  spec §10 の `bunx svelte-check` はこの `bun run typecheck`
  （`svelte-check --tsconfig ./tsconfig.json`）と同じ検査で、CI・Makefile と引数が揃う方を使う

## File Structure

| ファイル | 役割 | 変更 |
|---|---|---|
| `core/src/lib.rs` | `ProcessingConfig`、`process_image` パイプライン、pad/crop 変換、プレビュー生成 | 変更（本体） |
| `core/src/exif_frame/mod.rs` | `render_exif_frame`。戻り値に warnings を持たせる | 変更 |
| `core/src/exif_frame/layout.rs` | レイアウト計算。`MIN_SHORT_SIDE` を公開、不変条件テスト追加 | 変更 |
| `core/tests/exif_frame_v2_integration.rs` | Exif フレームの統合テスト。呼び出し6箇所＋プレビュー3箇所の追従、#7 / #9 追加 | 変更 |
| `cli/src/main.rs` | `--max-width` 引数と起動時の警告2種 | 変更 |
| `gui/src/commands.rs` | `render_exif_frame_preview` がプレビューの新しい戻り値を受ける | 変更（1箇所） |
| `gui-frontend/src/lib/types.ts` | `ProcessingConfig.max_width` | 変更 |
| `gui-frontend/src/App.svelte` | 既定値 `null` | 変更（1行） |
| `gui-frontend/src/lib/SettingsPanel.svelte` | トグル＋数値入力＋確定サイズ表示＋スナップ | 変更 |
| `README.md` / `CLAUDE.md` | CLI オプション表、Core API 表 | 変更 |

新規ファイルは作らない。

---

## Task 1: `max_width` フィールドと範囲検証

**Files:**
- Modify: `core/src/lib.rs:49-56`（`ProcessingConfig`）, `core/src/lib.rs:238-247`（`validate_config`）
- Test: `core/src/lib.rs` の `#[cfg(test)] mod tests`（同ファイル内）

**Interfaces:**
- Produces: `ProcessingConfig.max_width: Option<u32>`（`#[serde(default)]`）。
  以降のタスクと CLI / GUI がこのフィールドを読む。`validate_config(&config) -> Result<()>` は
  シグネチャ不変で、範囲外の `max_width` を `Err` にする

- [ ] **Step 1: 失敗するテストを書く**

`core/src/lib.rs` のテストモジュール末尾（`collect_image_files_reports_unreadable_paths_as_skipped`
の後、`mod tests` の閉じ括弧の前）に追加する。

```rust
    // =========================================================
    // max_width: 範囲検証と serde 互換（spec §7 / §9 #8）
    // =========================================================

    /// 仕様: 上限の指定は 4..=20000 px。
    /// 下限 4 はキャンバス幅が 4 の倍数であることの最小値、
    /// 上限 20000 は 20000x25000 の RGBA キャンバスが約 2GB に達するという実メモリ上の線。
    #[test]
    fn validate_config_accepts_max_width_boundaries() {
        let mut config = test_config();
        config.max_width = Some(4);
        assert!(validate_config(&config).is_ok(), "下限 4 は有効な指定");
        config.max_width = Some(20000);
        assert!(validate_config(&config).is_ok(), "上限 20000 は有効な指定");
        config.max_width = None;
        assert!(validate_config(&config).is_ok(), "無指定は無制限であって不正ではない");
    }

    #[test]
    fn validate_config_rejects_max_width_outside_the_supported_range() {
        let mut config = test_config();
        config.max_width = Some(3);
        assert!(validate_config(&config).is_err(), "3 は下限未満");
        config.max_width = Some(20001);
        assert!(validate_config(&config).is_err(), "20001 は上限超過");
    }

    /// 仕様: GUI から来る JSON に `max_width` が無くてもデシリアライズは壊れない。
    /// 無指定は「無制限」（従来どおり原寸）を意味する（spec §3）。
    #[test]
    fn processing_config_defaults_max_width_to_none_when_absent() {
        let json = r#"{
            "mode": "pad",
            "bg_color": "black",
            "quality": 75,
            "max_size_mb": 4,
            "delete_originals": true
        }"#;
        let config: ProcessingConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.max_width, None);
    }

    #[test]
    fn processing_config_reads_max_width_from_frontend_json() {
        let json = r#"{
            "mode": "pad",
            "bg_color": "white",
            "quality": 90,
            "max_size_mb": 8,
            "delete_originals": false,
            "max_width": 1080
        }"#;
        let config: ProcessingConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.max_width, Some(1080));
    }
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `cargo test -p picture-tool-core max_width`
Expected: コンパイルエラー `no field 'max_width' on type 'ProcessingConfig'`。
（Rust ではフィールド追加前のテストはコンパイルが通らない。これが RED。）

- [ ] **Step 3: フィールドと検証を実装**

`core/src/lib.rs:49-56` を置き換える:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingConfig {
    pub mode: ConversionMode,
    pub bg_color: BackgroundColor,
    pub quality: u8,
    pub max_size_mb: usize,
    pub delete_originals: bool,
    /// 出力 4:5 キャンバスの幅の上限 (px)。None なら無制限（元の画素数を保つ）。
    /// 実効値は 4 の倍数に切り捨てられる。quality モードでは無視される。
    ///
    /// `#[serde(default)]` は GUI から来る JSON にこのフィールドが無くても
    /// デシリアライズが壊れないようにするため。
    #[serde(default)]
    pub max_width: Option<u32>,
}
```

`core/src/lib.rs:238-247` の `validate_config` に範囲チェックを追加する:

```rust
/// 設定を検証する
pub fn validate_config(config: &ProcessingConfig) -> Result<()> {
    if config.quality == 0 || config.quality > 100 {
        anyhow::bail!("Quality must be between 1 and 100");
    }
    if config.max_size_mb == 0 {
        anyhow::bail!("max_size_mb must be at least 1");
    }
    // 上限そのものが壊れていると `k * 5` が u32 で溢れる。
    // 20000 は 20000x25000 の RGBA キャンバス（約 2GB）というメモリ側の線。
    if let Some(max_width) = config.max_width {
        if !(4..=20000).contains(&max_width) {
            anyhow::bail!("max_width must be between 4 and 20000");
        }
    }
    Ok(())
}
```

テスト用ヘルパー `core/src/lib.rs:684-692` の `test_config()` にも追加する:

```rust
    fn test_config() -> ProcessingConfig {
        ProcessingConfig {
            mode: ConversionMode::Crop,
            bg_color: BackgroundColor::White,
            quality: 90,
            max_size_mb: 8,
            delete_originals: false,
            max_width: None,
        }
    }
```

`core/tests/exif_frame_v2_integration.rs` の `ProcessingConfig` リテラル3箇所
（186行目付近 / 228行目付近 / 262行目付近）にも `max_width: None,` を足す。
`cli/src/main.rs:72` は Task 6 で扱うので、ここでは `max_width: None,` を仮に足してビルドを通す。

- [ ] **Step 4: テストが通ることを確認**

Run: `cargo test --workspace`
Expected: PASS（新規4件を含め全件）

- [ ] **Step 5: コミット**

```bash
git add core/src/lib.rs core/tests/exif_frame_v2_integration.rs cli/src/main.rs
git commit -m "feat(core): 出力幅の上限指定: ProcessingConfig に max_width を追加し範囲を検証"
```

---

## Task 2: `convert_aspect_ratio_pad` を `fit_to_4_5` に統一

**Files:**
- Modify: `core/src/lib.rs:532-560`（`convert_aspect_ratio_pad`）
- Test: `core/src/lib.rs` の `#[cfg(test)] mod tests`（既存 pad テスト2件の置き換え＋新規2件）

**Interfaces:**
- Consumes: `exif_frame::layout::fit_to_4_5(w, h) -> (u32, u32)`（`core/src/exif_frame/layout.rs:81`。既存 pub 関数）
- Produces: pad の出力キャンバスが `fit_to_4_5(元写真)` と厳密に一致する。
  Task 3 の前段スケール計算はこの一致に依存する

**なぜ必要か（spec §4「併せて直す1点」）:** 現在の `convert_aspect_ratio_pad` は
`width / 0.8` を自前計算しており 4:5 が厳密にならない。さらに `|ratio - 0.8| < 0.001` の
早期 return があり、この帯に入る入力はパディングされず素通りする。前段のスケール計算が
「pad が作るキャンバス幅」に依存する以上、その答えが2つある状態は事故のもと。

**過去バッチとの差分について:** 早期 return 帯の入力では出力寸法が変わる。
**利用者に確認のうえ許容と判断済み**（2026-08-12）。

- [ ] **Step 1: 失敗するテストを書く**

まず既存の2件を厳密比較へ置き換える。`core/src/lib.rs:891-940` の
`pad_mode_produces_4_5_and_preserves_original_content` と `pad_mode_with_black_background` の
assert 部分を次のように書き換える（比率の許容 0.02 では 1px のずれを検出できない）:

```rust
    #[test]
    fn pad_mode_produces_4_5_and_preserves_original_content() {
        let dir = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        let input = dir.path().join("wide.jpg");
        // 横長画像
        create_test_image(&input, 800, 400);

        let config = ProcessingConfig {
            mode: ConversionMode::Pad,
            bg_color: BackgroundColor::White,
            ..test_config()
        };
        let result = process_image(&input, out.path(), &config, None, None).unwrap();

        let output_img = image::open(&result.output_path).unwrap();
        let (w, h) = output_img.dimensions();
        // 4:5 は「おおむね 0.8」ではなく厳密な整数比（k*4 x k*5）
        assert_eq!((w, h), (800, 1000), "800x400 は 800x1000 にパディングされる");
        assert_eq!(w * 5, h * 4, "canvas must be exactly 4:5");
        // パディングは元画像以上のサイズになる（写真が欠けない）
        assert!(w >= 800 && h >= 400, "元画像がキャンバスに収まっていない");
    }

    #[test]
    fn pad_mode_with_black_background() {
        let dir = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        let input = dir.path().join("tall.jpg");
        create_test_image(&input, 400, 800);

        let config = ProcessingConfig {
            mode: ConversionMode::Pad,
            bg_color: BackgroundColor::Black,
            ..test_config()
        };
        let result = process_image(&input, out.path(), &config, None, None).unwrap();
        assert!(Path::new(&result.output_path).exists());

        let (w, h) = image::open(&result.output_path).unwrap().dimensions();
        assert_eq!((w, h), (640, 800), "400x800 は左右にパディングされて 640x800");
        assert_eq!(w * 5, h * 4, "canvas must be exactly 4:5");
    }
```

次に新規2件を `pad_mode_with_black_background` の直後に追加する。
**期待値は spec §4 の表と `k*4 × k*5` の定義から導いた固定値であり、
実装式の再計算ではない。**

```rust
    /// 仕様: pad の出力キャンバスは `fit_to_4_5` と同じ厳密な k*4 x k*5（spec §4 / §9 #5）。
    ///
    /// 400x501 は比率差 0.0016 で旧実装の早期 return 帯の外にあるが、
    /// `round(501 * 0.8) = 401` により 401x501（401*5=2005, 501*4=2004）を出していた。
    #[test]
    fn pad_mode_produces_an_exact_4_5_canvas_for_a_rounded_size() {
        let dir = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        let input = dir.path().join("near_4_5.jpg");
        create_test_image(&input, 400, 501);

        let config = ProcessingConfig {
            mode: ConversionMode::Pad,
            ..test_config()
        };
        let result = process_image(&input, out.path(), &config, None, None).unwrap();

        let (w, h) = image::open(&result.output_path).unwrap().dimensions();
        assert_eq!((w, h), (404, 505), "400x501 が収まる最小の 4:5 キャンバスは k=101");
        assert_eq!(w * 5, h * 4, "canvas must be exactly 4:5");
    }

    /// 仕様: 「ほぼ 4:5」の入力もパディングを省略されない（spec §4）。
    ///
    /// 800x1001 は比率差 0.0008 で旧実装の早期 return 帯に入り、
    /// 800x1001（800*5=4000, 1001*4=4004）のまま素通りしていた。
    #[test]
    fn pad_mode_does_not_pass_through_almost_4_5_input() {
        let dir = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        let input = dir.path().join("almost_4_5.jpg");
        create_test_image(&input, 800, 1001);

        let config = ProcessingConfig {
            mode: ConversionMode::Pad,
            ..test_config()
        };
        let result = process_image(&input, out.path(), &config, None, None).unwrap();

        let (w, h) = image::open(&result.output_path).unwrap().dimensions();
        assert_eq!((w, h), (804, 1005), "800x1001 が収まる最小の 4:5 キャンバスは k=201");
        assert_eq!(w * 5, h * 4, "canvas must be exactly 4:5");
    }
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `cargo test -p picture-tool-core pad_mode`
Expected: FAIL。`pad_mode_produces_an_exact_4_5_canvas_for_a_rounded_size` が
`(401, 501)` を返して `(404, 505)` と一致せず、
`pad_mode_does_not_pass_through_almost_4_5_input` が `(800, 1001)` を返して落ちる。
既存2件（800x400 / 400x800）は旧実装でも同じ値なので PASS のまま。

- [ ] **Step 3: 実装を書き換える**

`core/src/lib.rs:532-560` の `convert_aspect_ratio_pad` を丸ごと置き換える:

```rust
/// 4:5のアスペクト比に変換 (パディング)
///
/// キャンバスサイズは `fit_to_4_5` に一本化してある。以前はここだけ `width / 0.8` を
/// 自前計算し、比率差 0.001 未満は素通りしていたため、「pad が作るキャンバス幅」の
/// 答えが2つあった。`--max-width` の前段スケールはこの値に依存するので、
/// 食い違うと上限の保証が崩れる（spec 2026-08-12 §4）。
fn convert_aspect_ratio_pad(img: DynamicImage, bg_color: BackgroundColor) -> DynamicImage {
    let (width, height) = img.dimensions();
    let (new_width, new_height) = exif_frame::layout::fit_to_4_5(width, height);

    // 既に厳密な 4:5 ならコピーを作らない
    if (new_width, new_height) == (width, height) {
        return img;
    }

    let mut canvas = RgbaImage::from_pixel(new_width, new_height, bg_color.to_rgba());

    let x = (new_width.saturating_sub(width)) / 2;
    let y = (new_height.saturating_sub(height)) / 2;

    image::imageops::overlay(&mut canvas, &img.to_rgba8(), x.into(), y.into());

    DynamicImage::ImageRgba8(canvas)
}
```

- [ ] **Step 4: テストが通ることを確認**

Run: `cargo test --workspace`
Expected: PASS（全件）

- [ ] **Step 5: コミット**

```bash
git add core/src/lib.rs
git commit -m "fix(core): 出力幅の上限指定: pad のキャンバス計算を fit_to_4_5 に統一"
```

---

## Task 3: パイプライン統合（前段縮小と最終リサイズ）

**Files:**
- Modify: `core/src/lib.rs:282-338`（`process_image` の変換部）, `core/src/lib.rs:504` 付近（プライベートヘルパーに `target_canvas` を追加）
- Test: `core/src/lib.rs` の `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: `ProcessingConfig.max_width`（Task 1）、`fit_to_4_5` に統一された pad（Task 2）
- Produces: `fn target_canvas(max_width: Option<u32>) -> Option<(u32, u32)>`（core 内プライベート）。
  `process_image` の出力キャンバス幅が目標幅以下になる

**パイプライン（spec §4）:**

```
open → apply_orientation
     → [前段 / pad のみ] 目標があり fit_to_4_5(w, h).0 > target_w なら
                        写真を目標ボックスへ縮小（Lanczos3）
     → match mode { Crop | Pad(+frame) | Quality }   ← 中身は無変更
     → [最終] 目標があり canvas_w > target_w なら resize_exact(target)（Lanczos3）
     → encode
```

前段を pad だけに限定する理由と、pad では最終が no-op になる理由は spec §4 にある。
**最終ステップは保険ではなく契約**なので pad にも一律に適用する。

- [ ] **Step 1: 失敗するテストを書く**

`core/src/lib.rs` のテストモジュール、Task 1 で追加した max_width ブロックの後に追加する。

```rust
    // =========================================================
    // max_width: 出力キャンバス幅の上限（spec §9 #1〜#4, #6）
    // =========================================================

    /// テスト用: max_width つきの pad / crop 設定
    fn config_with_max_width(mode: ConversionMode, max_width: u32) -> ProcessingConfig {
        ProcessingConfig {
            mode,
            max_width: Some(max_width),
            ..test_config()
        }
    }

    /// 指定サイズの画像を1枚処理し、出力の (幅, 高さ) を返す
    fn process_and_measure(w: u32, h: u32, config: &ProcessingConfig) -> (u32, u32) {
        let dir = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        let input = dir.path().join(format!("in_{}x{}.jpg", w, h));
        create_test_image(&input, w, h);
        let result = process_image(&input, out.path(), config, None, None).unwrap();
        image::open(&result.output_path).unwrap().dimensions()
    }

    /// 仕様: 目標より大きい入力に対しては、pad の出力は目標幅ちょうどに着地する（#1）。
    /// 横位置・縦位置の両方を見る（spec §4 の導出は k を決める辺で場合分けしている）。
    #[test]
    fn pad_with_max_width_lands_exactly_on_the_target_canvas() {
        let config = config_with_max_width(ConversionMode::Pad, 1080);
        for (w, h) in [(3000, 2000), (2000, 3000)] {
            let (out_w, out_h) = process_and_measure(w, h, &config);
            assert_eq!(
                (out_w, out_h),
                (1080, 1350),
                "{}x{} + max_width=1080 は 1080x1350 になるべき",
                w,
                h
            );
            assert_eq!(out_w * 5, out_h * 4, "canvas must be exactly 4:5");
        }
    }

    /// 仕様: crop も同じ契約。crop には前段が無く、最終リサイズだけが上限を保証する（#2）。
    #[test]
    fn crop_with_max_width_lands_exactly_on_the_target_canvas() {
        let config = config_with_max_width(ConversionMode::Crop, 1080);
        for (w, h) in [(3000, 2000), (2000, 3000)] {
            let (out_w, out_h) = process_and_measure(w, h, &config);
            assert_eq!(
                (out_w, out_h),
                (1080, 1350),
                "{}x{} + max_width=1080 は 1080x1350 になるべき",
                w,
                h
            );
            assert_eq!(out_w * 5, out_h * 4, "canvas must be exactly 4:5");
        }
    }

    /// 仕様: 指定値は上限であって目標ではない。元が小さければ拡大しない（#3）。
    /// 契約は不等式なので、ここでは等値を要求しない。
    #[test]
    fn max_width_never_upscales_a_smaller_image() {
        // pad: 800x533 の 4:5 キャンバスは 800x1000 で、上限 1080 に既に収まっている
        let (w, h) = process_and_measure(800, 533, &config_with_max_width(ConversionMode::Pad, 1080));
        assert_eq!((w, h), (800, 1000), "上限より小さい入力は引き伸ばされない");

        // crop: 中央クロップの結果も元の高さを保ったまま
        let (w, h) = process_and_measure(800, 533, &config_with_max_width(ConversionMode::Crop, 1080));
        assert_eq!((w, h), (426, 533), "crop も拡大されない");
    }

    /// 仕様: 4 の倍数でない指定は切り捨てる。切り上げると指定値を超えてしまう（#4）。
    #[test]
    fn max_width_is_rounded_down_to_a_multiple_of_four() {
        let (w, h) = process_and_measure(3000, 2000, &config_with_max_width(ConversionMode::Pad, 1002));
        assert_eq!((w, h), (1000, 1250), "1002 は 1000 に切り捨てられる");
        assert!(w <= 1002, "実効値が指定値を超えてはならない");
    }

    /// 仕様: quality モードは 4:5 に変換しないため max_width の対象外（#6）。
    /// 「幅」を指定しても縦写真では長辺が幅*5/4 を大きく超え、上限の意味を持たない。
    #[test]
    fn quality_mode_ignores_max_width() {
        let config = config_with_max_width(ConversionMode::Quality, 1080);
        let (w, h) = process_and_measure(1600, 900, &config);
        assert_eq!((w, h), (1600, 900), "quality モードでは寸法が変わらない");
    }
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `cargo test -p picture-tool-core max_width`
Expected: FAIL。`pad_with_max_width_lands_exactly_on_the_target_canvas` は
`(3000, 3750)` を返し、crop 版は `(1600, 2000)` を返す（上限が効いていない）。
`quality_mode_ignores_max_width` と `max_width_never_upscales_a_smaller_image` は
この時点でも PASS（無視されているため）。

- [ ] **Step 3: `target_canvas` とパイプラインを実装**

`core/src/lib.rs` のプライベートヘルパー節の先頭（`// --- プライベートヘルパー ---` の直後、
`convert_aspect_ratio_crop` の前）に追加する:

```rust
/// 目標キャンバスサイズ。キャンバスは常に k*4 × k*5（S4 で確立した不変条件）。
///
/// 丸めは切り捨てのみ。切り上げて 1002 → 1004 になったら「指定値を超えない」という
/// 機能の目的を果たさない。
///
/// 範囲チェックの本線は `validate_config`（4..=20000）。`.max(1)` はそれを通さずに
/// core を直接使う利用者への安全網で、目標が 0x0 になって `resize_exact` が
/// 壊れるのを release ビルドでも防ぐ。`debug_assert!` では release で消える。
fn target_canvas(max_width: Option<u32>) -> Option<(u32, u32)> {
    let k = (max_width? / 4).max(1); // 切り捨て
    Some((k * 4, k * 5))
}
```

`core/src/lib.rs:309-338`（`let img = image::open(...)` から `let converted = match ... };` まで）を
次の内容に置き換える:

```rust
    let img = image::open(input_path)
        .with_context(|| format!("Failed to open image: {}", input_path.display()))?;
    let img = apply_orientation(img, exif.orientation);

    // 出力キャンバス幅の上限。quality モードはアスペクト比を変えないため対象外
    // （「幅」を指定しても縦写真の長辺を縛れない / spec §2）。
    let target = match config.mode {
        ConversionMode::Quality => None,
        _ => target_canvas(config.max_width),
    };

    // 前段: pad は巨大な RGBA キャンバスを確保してから文字とロゴを描くため、
    // その前に写真を目標ボックスへ縮小してメモリを抑える。crop に入れないのは、
    // 切り落として捨てる画素まで Lanczos3 で再サンプルすることになるため
    // （crop → 最終縮小の順の方が安く、丸めも一度で済む / spec §4）。
    // 縮小方向にしか働かない: ガードを満たさなければ何もしない。
    let img = match (target, config.mode) {
        (Some((target_w, target_h)), ConversionMode::Pad)
            if exif_frame::layout::fit_to_4_5(img.width(), img.height()).0 > target_w =>
        {
            img.resize(target_w, target_h, image::imageops::FilterType::Lanczos3)
        }
        _ => img,
    };

    let converted = match config.mode {
        ConversionMode::Crop => convert_aspect_ratio_crop(img),
        ConversionMode::Pad => {
            if let (Some(ef_config), Some(assets)) = (exif_frame_config, assets) {
                match exif_frame::render_exif_frame(
                    &img,
                    &exif,
                    ef_config,
                    &config.bg_color,
                    assets,
                ) {
                    Ok(framed) => framed,
                    Err(e) => {
                        warnings.push(format!(
                            "Exif frame failed, falling back to pad only: {}",
                            e
                        ));
                        convert_aspect_ratio_pad(img, config.bg_color)
                    }
                }
            } else {
                convert_aspect_ratio_pad(img, config.bg_color)
            }
        }
        ConversionMode::Quality => img,
    };

    // 最終: crop には前段が無いのでここがすべてを担う。pad では no-op になるが、
    // それはレイアウト実装に依存した不変条件なので、契約としてモードを問わず適用する。
    // 比較だけなので効いていないときの実行コストは無い（spec §4）。
    let converted = match target {
        Some((target_w, target_h)) if converted.width() > target_w => {
            converted.resize_exact(target_w, target_h, image::imageops::FilterType::Lanczos3)
        }
        _ => converted,
    };
```

- [ ] **Step 4: テストが通ることを確認**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS（全件、clippy 警告なし）

- [ ] **Step 5: コミット**

```bash
git add core/src/lib.rs
git commit -m "feat(core): 出力幅の上限指定: 前段縮小と最終リサイズで出力幅の上限を保証"
```

---

## Task 4: `render_exif_frame` の戻り値変更と `skip_exif` の通知

**Files:**
- Modify: `core/src/exif_frame/layout.rs:34-35`（`MIN_SHORT_SIDE` を公開）
- Modify: `core/src/exif_frame/mod.rs:188-310`（`render_exif_frame`）
- Modify: `core/src/lib.rs:313-338`（`process_image` の Exif フレーム分岐）, `core/src/lib.rs:440-469`（`generate_exif_frame_preview_base64`）
- Modify: `gui/src/commands.rs:553-565`（`render_exif_frame_preview`）
- Modify: `core/tests/exif_frame_v2_integration.rs`（呼び出し6箇所＋プレビュー3箇所の追従、#7 / #9 追加）

**Interfaces:**
- Produces:
  - `exif_frame::ExifFrameOutput { pub image: DynamicImage, pub warnings: Vec<String> }`
  - `render_exif_frame(...) -> Result<ExifFrameOutput>`（引数は不変）
  - `ExifFramePreview { pub base64: String, pub warnings: Vec<String> }`（`core` 直下）
  - `generate_exif_frame_preview_base64(...) -> Result<ExifFramePreview>`（引数は不変）
  - `exif_frame::layout::MIN_SHORT_SIDE: u32 = 200`
- Consumes: Task 3 の前段縮小（#9 の状況をこれが作る）

**なぜ必要か（spec §8）:** Exif フレームのレイアウト閾値は絶対 px（`MIN_SHORT_SIDE = 200`）。
前段縮小の結果は「同じ絵の縮小版」にはならず、パノラマ等では写真の短辺が 200px を割って
フレームが黙って消える。**この引き金は本機能が新設するもの**であり、現状は何も伝わらない。

**警告を GUI のプレビューに出さない理由（spec §8「捨てる判断は core ではなく境界で行う」）:**
プレビューは長辺 400px 固定なので、実出力ではフレームが出る写真でもプレビュー側は
`skip_exif` に落ちる。この握り潰しは GUI 固有の事情なので境界（`commands.rs`）で行う。
core に埋めると「core は判断せず warnings に積む」規約と逆向きになり、将来 CLI プレビューを
作ったときに理由の分からない握り潰しが残る。

- [ ] **Step 1: 失敗するテストを書く**

`core/tests/exif_frame_v2_integration.rs` の末尾に追加する:

```rust
// =========================================================
// max_width と Exif フレームの相互作用（spec §8 / §9 #7, #9）
// =========================================================

/// テスト用: pad + Exif フレーム + max_width の設定
fn pad_exif_config(max_width: Option<u32>) -> ProcessingConfig {
    ProcessingConfig {
        mode: ConversionMode::Pad,
        bg_color: BackgroundColor::Black,
        quality: 85,
        max_size_mb: 8,
        delete_originals: false,
        max_width,
    }
}

/// 仕様: Exif フレームつきでも出力キャンバスは目標サイズちょうどに着地する（#7）。
///
/// **前提条件として「フレームが実際に描かれている」ことも確かめる。**
/// skip_exif に落ちていると、この検査は「フレームを描かなかったから寸法が合った」
/// だけで green になり、何も守らない（S4 で踏んだ罠。#9 が skip 時の警告を
/// 固定しているので、警告が無いことがフレーム描画の witness になる）。
#[test]
fn exif_frame_with_max_width_lands_exactly_on_the_target_canvas() {
    let tmp = TempDir::new().unwrap();
    let input = write_test_jpeg(&tmp, 3000, 2000, "framed.jpg");

    let result = process_image(
        &input,
        tmp.path(),
        &pad_exif_config(Some(1080)),
        Some(&ExifFrameConfig::default()),
        Some(&default_assets()),
    )
    .expect("pad + exif frame + max_width must succeed");

    // 前提条件: フレームが省略されていない
    assert!(
        !result.warnings.iter().any(|w| w.contains("Exif frame skipped")),
        "前提が崩れている: フレームが描かれていないので寸法の検査に意味が無い ({:?})",
        result.warnings
    );

    let out = image::open(&result.output_path).unwrap();
    assert_eq!(
        (out.width(), out.height()),
        (1080, 1350),
        "max_width=1080 の出力は 1080x1350"
    );
    assert_exactly_4_5(&out, "framed output with max_width");
}

/// 仕様: 縮小の結果フレームが描けなくなったら、そのことを呼び出し元へ伝える（#9）。
///
/// 2376x400 のパノラマに max_width=1080 を指定すると写真が 1080x182 になり、
/// 短辺が MIN_SHORT_SIDE(200) を割ってフレームが消える。
/// max_width 無しなら短辺 400 でフレームは出るので、**この引き金は本機能が
/// 新設するもの**である。同じ入力の指定なし版を並べて、それを固定する。
#[test]
fn a_frame_dropped_by_the_width_limit_is_reported_in_warnings() {
    let tmp = TempDir::new().unwrap();
    let input = write_test_jpeg(&tmp, 2376, 400, "panorama.jpg");

    let without_limit = process_image(
        &input,
        tmp.path(),
        &pad_exif_config(None),
        Some(&ExifFrameConfig::default()),
        Some(&default_assets()),
    )
    .expect("pad + exif frame must succeed");
    assert!(
        !without_limit
            .warnings
            .iter()
            .any(|w| w.contains("Exif frame skipped")),
        "前提が崩れている: 上限なしでもフレームが出ていない ({:?})",
        without_limit.warnings
    );

    let with_limit = process_image(
        &input,
        tmp.path(),
        &pad_exif_config(Some(1080)),
        Some(&ExifFrameConfig::default()),
        Some(&default_assets()),
    )
    .expect("pad + exif frame + max_width must succeed");

    assert!(
        with_limit
            .warnings
            .iter()
            .any(|w| w.contains("Exif frame skipped")),
        "フレームが消えたことが呼び出し元に伝わっていない ({:?})",
        with_limit.warnings
    );
}
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `cargo test -p picture-tool-core --test exif_frame_v2_integration`
Expected: FAIL。両テストとも「`Exif frame skipped` を含む警告が無い」ため
`a_frame_dropped_by_the_width_limit_is_reported_in_warnings` が落ちる
（`exif_frame_with_max_width_lands_exactly_on_the_target_canvas` は PASS しうるが、
`skip_exif` の通知が無い時点で前提条件 assert が機能していない）。

- [ ] **Step 3: 戻り値と警告を実装**

**(a) `core/src/exif_frame/layout.rs:34-35` を公開する:**

```rust
/// 写真の短辺がこれ未満なら Exif フレームを描かない。
/// 呼び出し元が「なぜ消えたか」を利用者に伝えられるよう公開している。
pub const MIN_SHORT_SIDE: u32 = 200;
```

**(b) `core/src/exif_frame/mod.rs`**: `render_exif_frame` の直前（188行目付近）に型を追加:

```rust
/// `render_exif_frame` の結果。
///
/// core は `eprintln!` しないので、描画時に諦めた事象（バーを描けるだけの短辺が無い等）は
/// 呼び出し元に返して伝えてもらう。`process_image` はこれを `ProcessResult.warnings` に
/// 連結し、CLI は stderr、GUI は結果ダイアログに出す。
#[derive(Debug)]
pub struct ExifFrameOutput {
    pub image: DynamicImage,
    /// 描画は続行したが利用者に伝えるべき事象
    pub warnings: Vec<String>,
}
```

シグネチャを `-> Result<ExifFrameOutput>` に変え、`skip_exif` 分岐（203-213行目）と
最終 return（309行目）を差し替える:

```rust
pub fn render_exif_frame(
    image: &DynamicImage,
    exif: &crate::ExifInfo,
    config: &ExifFrameConfig,
    bg_color: &crate::BackgroundColor,
    assets: &ExifAssets,
) -> Result<ExifFrameOutput> {
```

```rust
    // 2. skip_exif: 4:5キャンバスに写真を中央配置して返す
    if layout.skip_exif {
        let bg_pixel = bg_color.to_rgba();
        let mut canvas = RgbaImage::from_pixel(layout.canvas_width, layout.canvas_height, bg_pixel);
        image::imageops::overlay(
            &mut canvas,
            image,
            layout.photo_x as i64,
            layout.photo_y as i64,
        );
        // フレームが消えたことは黙らない。`--max-width` の縮小でここに落ちる経路が
        // 新設されたため、気づけないと「指定したのにフレームが無い」になる（spec §8）。
        return Ok(ExifFrameOutput {
            image: DynamicImage::ImageRgba8(canvas),
            warnings: vec![format!(
                "Exif frame skipped: the photo is {}x{} and too small to draw the bar \
                 (short side must be at least {}px)",
                photo_w,
                photo_h,
                layout::MIN_SHORT_SIDE
            )],
        });
    }
```

```rust
    draw_exif_area(&mut canvas, &layout, &bar);

    Ok(ExifFrameOutput {
        image: DynamicImage::ImageRgba8(canvas),
        warnings: Vec::new(),
    })
}
```

**(c) `core/src/lib.rs` の `process_image`**: Task 3 で置き換えた `Ok(framed) => framed,` を
差し替える:

```rust
                    Ok(framed) => {
                        // core は自ら出力しない。フレームを諦めた等の事象は呼び出し元へ運ぶ。
                        warnings.extend(framed.warnings);
                        framed.image
                    }
```

**(d) `core/src/lib.rs` のプレビュー**: 440-469行目を差し替える:

```rust
/// `generate_exif_frame_preview_base64` の結果
#[derive(Debug)]
pub struct ExifFramePreview {
    /// data URI の prefix を含まない生の base64 JPEG
    pub base64: String,
    /// フレーム描画由来の警告。
    ///
    /// **GUI はこれを利用者に出さない。** プレビューは長辺 400px 固定なので、
    /// 実出力ではフレームが出る写真でも `skip_exif` に落ちて偽陽性になる。
    /// その判断は GUI 固有の事情なので境界（`gui/src/commands.rs`）が行う。
    /// core 側で握り潰すと、将来 CLI プレビューを作ったときに理由の分からない
    /// 握り潰しが残る（spec §8）。
    pub warnings: Vec<String>,
}

/// Exifフレームのプレビューをbase64エンコードされたJPEG文字列として生成
///
/// GUI 専用ではなく core に置く。以前は「縮小 → 描画 → JPEG → base64」の一連が
/// Tauri コマンドの中だけに書かれており、CLI からプレビューを作れず、
/// GUI が `image` / `base64` に直接依存する原因にもなっていた（S6-M16）。
pub fn generate_exif_frame_preview_base64(
    path: &Path,
    config: &exif_frame::ExifFrameConfig,
    bg_color: &BackgroundColor,
    assets: &exif_frame::ExifAssets,
    max_dimension: u32,
) -> Result<ExifFramePreview> {
    use base64::Engine as _;
    let max_dimension = max_dimension.clamp(1, 1024);

    // Orientation を適用してから縮小する。生の image::open だと縦横が実際の
    // 処理結果と食い違い、auto_placement が別の辺を選んでしまう。
    let img = open_image_oriented(path)?;
    let thumbnail = img.resize(
        max_dimension,
        max_dimension,
        image::imageops::FilterType::Triangle,
    );

    let exif = read_exif_info(path).unwrap_or_default();
    let framed = exif_frame::render_exif_frame(&thumbnail, &exif, config, bg_color, assets)?;
    let jpeg_bytes = encode_jpeg_rgb(&framed.image.to_rgb8(), 85)?;

    Ok(ExifFramePreview {
        base64: base64::engine::general_purpose::STANDARD.encode(&jpeg_bytes),
        warnings: framed.warnings,
    })
}
```

**(e) `gui/src/commands.rs:553-565`** の `spawn_blocking` の中身を差し替える:

```rust
    tokio::task::spawn_blocking(move || {
        let assets =
            exif_frame::ExifAssets::load(AssetDirs::default()).map_err(|e| format!("{:#}", e))?;

        let preview =
            core::generate_exif_frame_preview_base64(&file, &config, &bg_color, &assets, 400)
                .map_err(|e| format!("{:#}", e))?;

        Ok(PreviewImage {
            data_url: format!("data:image/jpeg;base64,{}", preview.base64),
            // フレーム描画由来の警告（preview.warnings）は載せない。プレビューは
            // 長辺 400px 固定なので、実出力ではフレームが出る写真でも skip_exif に
            // 落ち、出すと偽陽性になる。捨てる判断は境界の責務（spec §8）。
            warnings: assets.warnings,
        })
    })
```

**(f) 既存テストの追従（`core/tests/exif_frame_v2_integration.rs`）:**

- `render_exif_frame(...).unwrap()` の結果を `assert_exactly_4_5` / `result.width()` に
  渡している箇所（66, 81, 96, 127, 162行目付近の5箇所）は `.unwrap().image` にする。
  例: `let result = render_exif_frame(...).unwrap().image;`
- `pad_exif_no_exif_data_doesnt_crash`（144行目付近）は `result.is_ok()` のままでよい
- `generate_exif_frame_preview_base64(...)` の結果を `decode_preview` に渡している2箇所
  （308, 335行目付近）は `.expect("preview generation must succeed").base64` にし、
  `decode_preview(&base64)` をそのまま使う
- 360行目付近の `.is_err()` は変更不要

- [ ] **Step 4: テストが通ることを確認**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS（全件）

- [ ] **Step 5: コミット**

```bash
git add core/src/exif_frame/layout.rs core/src/exif_frame/mod.rs core/src/lib.rs \
        gui/src/commands.rs core/tests/exif_frame_v2_integration.rs
git commit -m "feat(core)!: 出力幅の上限指定: Exifフレームの省略を warnings で呼び出し元へ伝える"
```

---

## Task 5: `calculate_pad_exif_layout` の不変条件をテストで固定

**Files:**
- Test: `core/src/exif_frame/layout.rs` の `#[cfg(test)] mod tests`（実装は変更しない）

**Interfaces:**
- Consumes: `calculate_pad_exif_layout`, `fit_to_4_5`, `PadExifLayout`（すべて既存 pub）
- Produces: なし（テストのみ）

**なぜ必要か（spec §4 / §9 #10）:** 「pad では最終ステップが no-op になる」は
`calculate_pad_exif_layout` が返すキャンバスが常に `fit_to_4_5(元写真)` 以下である、という
レイアウト実装依存の不変条件に支えられている。記録に書くだけでは将来のレイアウト変更で
黙って壊れるので、4分岐すべてをテストで固定する。

**witness は spec §9 で検証済み**（自分で探すと「到達不能」と誤判断しやすい）。
`PadExifLayout` には `skip_exif` 以外の分岐フラグが無いため、判別は
「写真を縮小したか（`photo_width < 入力幅`）」と
「キャンバスを拡張したか（キャンバス != `fit_to_4_5(縮小後写真)`）」の組み合わせで行う。

- [ ] **Step 1: 失敗するテストを書く**

`core/src/exif_frame/layout.rs` のテストモジュール末尾（`fallback_canvas_expansion_keeps_exact_4_5`
の後）に追加する。**期待値はすべて spec §9 の witness 表から取った固定値**である。

```rust
    /// 仕様: `calculate_pad_exif_layout` が返すキャンバスは、どの分岐でも
    /// `fit_to_4_5(入力写真)` を超えない（spec 2026-08-12 §4 / §9 #10）。
    ///
    /// これは `--max-width` の設計が依存している不変条件そのものである。前段縮小で
    /// 写真を目標に合わせれば pad の最終リサイズは no-op になる、という前提が
    /// ここで崩れると上限の保証が「最終リサイズが実際に効いた」だけになる。
    ///
    /// 分岐フラグが無いので、写真の縮小有無とキャンバスの拡張有無で分岐を特定する。
    /// **どの分岐に入ったかを assert する**のは、4つのつもりが実は1つしか通っていない、
    /// という状態で green になるのを防ぐため（S4 で踏んだ罠）。
    #[test]
    fn pad_exif_layout_never_exceeds_the_original_4_5_canvas() {
        let config = crate::exif_frame::ExifFrameConfig::default();
        let bg = BackgroundColor::Black;

        // --- 分岐1: 余白が足りている（写真は原寸のまま） ---
        // 1200x800 → Bottom / bar 48 / fit_to_4_5 = 1200x1500 / pad_h 700 >= 48
        let l = calculate_pad_exif_layout(1200, 800, &config, &bg);
        assert!(!l.skip_exif, "1200x800 はフレームが描かれる分岐");
        assert_eq!(
            (l.photo_width, l.photo_height),
            (1200, 800),
            "余白が足りているので写真は縮小されない"
        );
        assert_eq!(
            (l.canvas_width, l.canvas_height),
            (1200, 1500),
            "キャンバスは fit_to_4_5(1200, 800) と一致する"
        );

        // --- 分岐2: 写真を縮小して収まった（拡張なし） ---
        // 500x660 → Right / bar 30 / fit_to_4_5 = 528x660 / available 28 < 30
        //   → deficit 2 → 写真 498x657 → new_available 30 >= 30
        let l = calculate_pad_exif_layout(500, 660, &config, &bg);
        assert!(!l.skip_exif, "500x660 はフレームが描かれる分岐");
        assert_eq!(
            (l.photo_width, l.photo_height),
            (498, 657),
            "この分岐は写真を縮小している"
        );
        assert_eq!(
            (l.canvas_width, l.canvas_height),
            fit_to_4_5(l.photo_width, l.photo_height),
            "縮小後の写真から求めたキャンバスのまま＝拡張していない"
        );
        assert_eq!(
            (l.canvas_width, l.canvas_height),
            (528, 660),
            "キャンバスは fit_to_4_5(500, 660) を超えない"
        );

        // --- 分岐3: 縮小しても足りずキャンバスを拡張した ---
        // 400x501 → Right / bar 30 / available 4 < 30 → deficit 26 → 写真 374x468
        //   → new_available 2 < 30 → 拡張して 404x505
        let l = calculate_pad_exif_layout(400, 501, &config, &bg);
        assert!(!l.skip_exif, "400x501 はフレームが描かれる分岐");
        assert_eq!(
            (l.photo_width, l.photo_height),
            (374, 468),
            "この分岐も写真を縮小している"
        );
        assert_ne!(
            (l.canvas_width, l.canvas_height),
            fit_to_4_5(l.photo_width, l.photo_height),
            "縮小後の写真では足りずキャンバスを拡張した分岐であること"
        );
        assert_eq!(
            (l.canvas_width, l.canvas_height),
            (404, 505),
            "拡張しても fit_to_4_5(400, 501) ちょうどに戻る（それを超えない）"
        );

        // --- 分岐4: skip_exif ---
        // 150x100 → 短辺 100 < MIN_SHORT_SIDE
        let l = calculate_pad_exif_layout(150, 100, &config, &bg);
        assert!(l.skip_exif, "150x100 は短辺が閾値未満で skip する分岐");
        assert_eq!(
            (l.canvas_width, l.canvas_height),
            (152, 190),
            "skip でも 4:5 変換は放棄せず fit_to_4_5(150, 100) を返す"
        );

        // 4分岐すべてで「キャンバス <= fit_to_4_5(入力写真)」が成り立っている
        // （上の固定値はいずれも fit_to_4_5(入力) と一致＝上界に等しい）。
        for (w, h) in [(1200, 800), (500, 660), (400, 501), (150, 100)] {
            let l = calculate_pad_exif_layout(w, h, &config, &bg);
            let (max_w, max_h) = fit_to_4_5(w, h);
            assert!(
                l.canvas_width <= max_w && l.canvas_height <= max_h,
                "canvas {}x{} exceeds fit_to_4_5({}, {}) = {}x{}",
                l.canvas_width,
                l.canvas_height,
                w,
                h,
                max_w,
                max_h
            );
        }
    }
```

- [ ] **Step 2: テストを実行して green を確認**

Run: `cargo test -p picture-tool-core pad_exif_layout_never_exceeds`
Expected: PASS。

**これは RED から始まらない唯一のテストである。** 現行実装が既に満たしている不変条件を
固定するのが目的なので、失敗を先に見ることはできない。代わりに **witness が本当に
意図した分岐を通っていることを、次のステップで確かめる**（テストが何も検査していない
状態で green になっていないことの確認）。

- [ ] **Step 3: 分岐 assert が実際に効いていることを確認（ミューテーション確認）**

`core/src/exif_frame/layout.rs:37` の `MAX_SHRINK_RATIO` を一時的に `0.001` に変えて
実行し、分岐2・分岐3 が `skip_exif` に落ちてテストが FAIL することを確かめる。

Run: `cargo test -p picture-tool-core pad_exif_layout_never_exceeds`
Expected: FAIL（`500x660 はフレームが描かれる分岐` で落ちる）

確認したら**必ず元の `0.20` に戻し**、再度実行して PASS に戻ることを確認する。

戻し忘れをコミットに混ぜないためのガード（このファイルには新規テストも足すので
`git diff --exit-code` は使えない。定数そのものを狙う）:

```bash
grep -q 'MAX_SHRINK_RATIO: f64 = 0.20;' core/src/exif_frame/layout.rs \
  && echo "OK: 定数は元に戻っている" || echo "NG: MAX_SHRINK_RATIO を戻していない"
```

- [ ] **Step 4: コミット**

```bash
git add core/src/exif_frame/layout.rs
git commit -m "test(core): 出力幅の上限指定: pad+Exifレイアウトのキャンバス上界を4分岐で固定"
```

---

## Task 6: CLI `--max-width`

**Files:**
- Modify: `cli/src/main.rs:15-59`（`Args`）, `cli/src/main.rs:69-87`（`main` の設定組み立てと警告）
- Test: 手動検証（CLI は既存もユニットテストを持たない。振る舞いは core 側で固定済み）

**Interfaces:**
- Consumes: `ProcessingConfig.max_width`（Task 1）、パイプライン（Task 3）
- Produces: `--max-width <px>` オプションと起動時の警告2種

**警告を「起動時に1回」にする理由（spec §3）:** core は `eprintln!` しない規約があり、
画像ごとの `ProcessResult.warnings` に積むと同じ文言が画像数だけ並ぶ。

- [ ] **Step 1: 引数を追加**

`cli/src/main.rs` の `Args` に、`max_size`（32-34行目）の直後へ追加する:

```rust
    /// 出力4:5キャンバスの幅の上限 (px)。無指定なら元の画素数を保つ
    #[arg(long, value_parser = clap::value_parser!(u32).range(4..=20000))]
    max_width: Option<u32>,
```

- [ ] **Step 2: 設定への反映と警告を実装**

`cli/src/main.rs:72-87` を差し替える（Task 1 で仮に置いた `max_width: None,` もここで確定する）:

```rust
    let config = ProcessingConfig {
        mode: args.mode,
        bg_color: args.bg_color,
        quality: args.quality,
        max_size_mb: args.max_size as usize,
        delete_originals: args.delete_originals,
        max_width: args.max_width,
    };

    core::validate_config(&config)?;

    // 上限を指定したのに効いていない、を黙って通さない。core は eprintln! しないので
    // 起動時に1回だけここで出す（画像ごとに出すと同じ文言が枚数分並ぶ / spec §3, §5）。
    if let Some(max_width) = args.max_width {
        if config.mode == ConversionMode::Quality {
            eprintln!(
                "Warning: --max-width is only supported with --mode crop or pad. Ignoring."
            );
        } else {
            // 4 の倍数へ切り捨てる。切り上げると指定値を超えてしまう。
            let effective = max_width / 4 * 4;
            if effective != max_width {
                eprintln!(
                    "Warning: --max-width {} is rounded down to {} \
                     (the output canvas width is always a multiple of 4).",
                    max_width, effective
                );
            }
        }
    }

    let exif_frame_requested = if args.exif_frame && config.mode != ConversionMode::Pad {
        eprintln!("Warning: --exif-frame is only supported with --mode pad. Ignoring.");
        false
    } else {
        args.exif_frame
    };
```

- [ ] **Step 3: ビルドと lint**

Run: `cargo build -p picture-tool && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS

- [ ] **Step 4: 実画像で手動検証（spec §10 の CLI 3項目）**

```bash
WORK=$(mktemp -d)
mkdir -p "$WORK/in" "$WORK/out"
magick -size 3000x2000 gradient:red-blue "$WORK/in/sample.jpg"

# 1) 出力が 1080x1350 であること
cargo run -q -p picture-tool -- -i "$WORK/in" -o "$WORK/out" -m pad --max-width 1080
identify -format "%wx%h\n" "$WORK/out/sample_processed.jpg"   # 期待: 1080x1350

# 2) 1002 で実効値 1000 の警告が「1回だけ」出ること（画像は2枚置いて確認する）
magick -size 3000x2000 gradient:green-yellow "$WORK/in/sample2.jpg"
rm -f "$WORK/out"/*
cargo run -q -p picture-tool -- -i "$WORK/in" -o "$WORK/out" -m pad --max-width 1002 2>&1 \
  | grep -c "rounded down to 1000"                            # 期待: 1
identify -format "%wx%h\n" "$WORK/out/sample_processed.jpg"   # 期待: 1000x1250

# 3) quality モードでは無視の警告が出て、寸法が変わらないこと
rm -f "$WORK/out"/*
cargo run -q -p picture-tool -- -i "$WORK/in" -o "$WORK/out" -m quality --max-width 1080 2>&1 \
  | grep "Ignoring"                                           # 期待: 無視の警告が出る
identify -format "%wx%h\n" "$WORK/out/sample_processed.jpg"   # 期待: 3000x2000

# 4) 範囲外は clap が弾くこと
cargo run -q -p picture-tool -- -i "$WORK/in" -o "$WORK/out" --max-width 3 ; echo "exit=$?"
# 期待: 非ゼロ終了 + "invalid value '3'" 相当のメッセージ
```

**実行した出力を計画の「実施メモ」に貼ること**（Task 8）。期待値と違ったら
先へ進まず原因を特定する。

- [ ] **Step 5: コミット**

```bash
git add cli/src/main.rs
git commit -m "feat(cli): 出力幅の上限指定: --max-width オプションと無視・丸めの警告を追加"
```

---

## Task 7: GUI（型・既定値・設定パネル）

**Files:**
- Modify: `gui-frontend/src/lib/types.ts:16-22`（`ProcessingConfig`）
- Modify: `gui-frontend/src/App.svelte:24-30`（既定値）
- Modify: `gui-frontend/src/lib/SettingsPanel.svelte`（script と markup、style に2クラス追加）

**Interfaces:**
- Consumes: `ProcessingConfig.max_width`（Rust 側は Task 1、JSON キーは `max_width`）
- Produces: 「出力幅を制限する」トグルと幅入力。**送信値は常に 4 の倍数**

**設計の前提（spec §6）:**
- **`step="4"` は予防にならない**（スピナーと HTML バリデーションにしか効かず、
  1002 を直接入力・貼り付けできる）。**入力確定時（change）に 4 の倍数へスナップする**
- スナップすれば表示値・送信値・Rust の正規化結果が一致し、`target_canvas` の切り捨てを
  TS 側に再実装して drift させることもない
- quality モードでは**トグルを隠さず無効化して理由を表示する**（背景色を pad 限定で
  隠しているのとは区別する。`max_width` は効いていないことに気づけないと目的を損なう）
- **プリセットボタン（1080 / 1440 等）は出さない。** 実際に使う幅が未確定なため保留（spec §3）

> **計画側の判断（spec に無い1点）:** トグルを on にした瞬間の初期値が必要になる。
> spec が唯一挙げている値である 1080（Instagram 推奨値 / spec §3 の表）を種にする。
> これは選択肢を並べる「プリセット」ではなく単なる初期値なので、上の保留と矛盾しない。

- [ ] **Step 1: 型と既定値を追加**

`gui-frontend/src/lib/types.ts:16-22` を置き換える:

```ts
export interface ProcessingConfig {
  mode: "crop" | "pad" | "quality";
  bg_color: "white" | "black";
  quality: number;
  max_size_mb: number;
  delete_originals: boolean;
  /** 出力4:5キャンバスの幅の上限 (px)。null は無制限。常に4の倍数を送る */
  max_width: number | null;
}
```

`gui-frontend/src/App.svelte:24-30` の初期値に1行足す:

```ts
  let config = $state<ProcessingConfig>({
    mode: "crop",
    bg_color: "white",
    quality: 90,
    max_size_mb: 8,
    delete_originals: false,
    max_width: null,
  });
```

- [ ] **Step 2: 型検査で漏れを確認**

Run: `cd gui-frontend && bun install && bun run typecheck`
Expected: エラーなし（`ProcessingConfig` を組み立てているのは App.svelte だけ）。
エラーが出たら、そのファイルにも `max_width` を足す。

- [ ] **Step 3: SettingsPanel に UI を実装**

`gui-frontend/src/lib/SettingsPanel.svelte` の `<script lang="ts">` 末尾
（`}: Props = $props();` の後）に追加する:

```ts
  const MAX_WIDTH_MIN = 4;
  const MAX_WIDTH_MAX = 20000;

  // トグルを on にしたときに入れる値。off にしても直前の値を覚えておく。
  let lastMaxWidth = $state(1080);

  // 確定サイズ表示。テンプレート内で null 絞り込みに頼らないよう $derived で持つ。
  let maxWidthLabel = $derived(
    config.max_width === null ? "" : `${config.max_width}×${(config.max_width * 5) / 4}`
  );

  /**
   * 4 の倍数へ切り捨てる（Rust 側 `target_canvas` と同じ丸め方向）。
   * 切り上げると指定値を超えてしまい、「上限」という機能の目的を果たさない。
   */
  function snapWidth(value: number): number {
    const clamped = Math.min(Math.max(value, MAX_WIDTH_MIN), MAX_WIDTH_MAX);
    return Math.floor(clamped / 4) * 4;
  }

  function toggleMaxWidth(enabled: boolean) {
    config.max_width = enabled ? lastMaxWidth : null;
  }

  /**
   * 入力確定時に値をスナップする。`step="4"` はスピナーと HTML バリデーションにしか
   * 効かず、1002 を直接入力・貼り付けできてしまうため予防にならない。
   *
   * DOM の value も明示的に書き戻す。スナップ結果が現在の状態と同じ値のとき
   * （例: 1000 のときに 1002 を入力）は state が変化せず再描画されないので、
   * 表示だけ 1002 のまま残ってしまう。
   */
  function commitMaxWidth(input: HTMLInputElement) {
    const raw = input.value.trim();
    const parsed = Number(raw);
    if (raw !== "" && Number.isFinite(parsed)) {
      lastMaxWidth = snapWidth(parsed);
    }
    config.max_width = lastMaxWidth;
    input.value = String(lastMaxWidth);
  }
```

markup は「最大サイズ」の `<label class="field">`（61-63行目）の直後に挿入する:

```svelte
    <div class="field">
      <label class="checkbox">
        <input
          type="checkbox"
          checked={config.max_width !== null}
          disabled={config.mode === "quality"}
          onchange={(e) => toggleMaxWidth((e.target as HTMLInputElement).checked)}
        />
        <span>出力幅を制限する</span>
      </label>
      {#if config.mode === "quality"}
        <p class="hint">
          Quality モードは 4:5 に変換しないため、出力幅の上限は適用されません。
        </p>
      {:else if config.max_width !== null}
        <div class="max-width-row">
          <input
            type="number"
            min={MAX_WIDTH_MIN}
            max={MAX_WIDTH_MAX}
            aria-label="出力幅の上限 (px)"
            value={config.max_width}
            onchange={(e) => commitMaxWidth(e.currentTarget)}
          />
          <span class="derived">→ {maxWidthLabel}</span>
        </div>
      {/if}
    </div>
```

`<style>` の `.danger-hint`（252-257行目）の後にクラスを追加する:

```css
  .hint {
    margin: 4px 0 0;
    font-size: 11px;
    line-height: 1.5;
    color: var(--text-secondary);
  }

  .max-width-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .max-width-row input[type="number"] {
    width: 90px;
    background: var(--bg-primary);
    border: 1px solid var(--border-color);
    color: var(--text-primary);
    padding: 4px 8px;
    border-radius: var(--radius-sm);
    font-size: 12px;
  }

  .derived {
    font-size: 11px;
    color: var(--text-secondary);
  }
```

- [ ] **Step 4: 型検査**

Run: `cd gui-frontend && bun run typecheck`
Expected: エラー 0 件

- [ ] **Step 5: ブラウザで手動検証（spec §10 の GUI 2項目）**

Tauri を起動せず `vite dev` + Playwright MCP で確認する。SettingsPanel は Tauri の
`invoke` に依存しないので、webview 外でも入力の振る舞いをそのまま検証できる
（起動直後にプリセット読み込み等の invoke が reject してトーストが出るが、これは想定内）。

```bash
cd gui-frontend && bun run dev    # http://localhost:5173（strictPort）
```

Playwright MCP で:
1. `browser_navigate` → `http://localhost:5173`
2. 「出力幅を制限する」チェックボックスをクリック → 入力欄と `→ 1080×1350` が出ること
3. 入力欄に `1002` を入力し、Tab で確定（change を発火させる）
   → **表示が `1000`、右が `→ 1000×1250` になること**
4. `20001` を入力して確定 → `20000` / `→ 20000×25000` にクランプされること
5. モードを `Quality (サイズのみ)` に変更
   → **チェックボックスが disabled になり、理由の文が表示されること**
6. モードを `Pad (パディング)` に戻す → 入力欄が **`20000` のまま**復帰すること
   （モード切替は `config.max_width` も `lastMaxWidth` もリセットしない。
   手順4 の値がそのまま残るのが正しい挙動）

`browser_snapshot` の該当部分を実施メモに残す。終わったら dev サーバーを停止する。

- [ ] **Step 6: コミット**

```bash
git add gui-frontend/src/lib/types.ts gui-frontend/src/App.svelte \
        gui-frontend/src/lib/SettingsPanel.svelte
git commit -m "feat(gui): 出力幅の上限指定: 出力幅トグルと4の倍数スナップを設定パネルに追加"
```

---

## Task 8: ドキュメント更新と完了条件の一括検証

**Files:**
- Modify: `README.md:47-48` 付近（使用例）, `README.md:74-86`（CLI オプション表）
- Modify: `CLAUDE.md`（CLI オプション表、変換モードの注記、Core API 表の下の説明）
- Modify: `docs/README.md`（「直近の実装計画」表に本 plan を登録）
- Modify: `docs/superpowers/plans/2026-08-12-output-width-limit.md`（本ファイル。実施メモを追記）

**Interfaces:**
- Consumes: Task 1〜7 の成果すべて

- [ ] **Step 1: README を更新**

使用例（`# 品質とサイズ上限を指定` のブロックの後）に追加:

```markdown
# 出力の画素数を抑える（4:5 キャンバスの幅を 1080px 以下に。crop / pad のみ）
cargo run -p picture-tool -- -i ./photos -o ./output -m pad --max-width 1080
```

CLI オプション表の `--max-size` 行の直後に追加:

```markdown
| `--max-width` | | (無制限) | 出力4:5キャンバスの幅の上限 (px, 4-20000)。`crop` / `pad` 限定 |
```

- [ ] **Step 2: CLAUDE.md を更新**

CLI オプション表の `--max-size` 行の直後に追加:

```markdown
| `--max-width` | | (無制限) | 出力4:5キャンバス幅の上限 (px, 4-20000)。**crop / pad 限定** |
```

「変換モード」節の末尾に追加:

```markdown
### 出力幅の上限（`--max-width`）
- **crop と pad 限定**。quality モードでは警告を出して無視する（4:5 に変換しないため
  「幅」で長辺を縛れない）
- 実効値は 4 の倍数へ**切り捨て**（1002 → 1000）。切り上げると指定値を超えてしまう
- **上限であって目標ではない**。元がそれより小さければ拡大しない
- 設計と算術的な導出は [`docs/superpowers/specs/2026-08-12-output-width-limit-design.md`](./docs/superpowers/specs/2026-08-12-output-width-limit-design.md)
```

Core API 表の下、`ExifAssets` の段落の後に追記:

```markdown
`render_exif_frame` は `ExifFrameOutput { image, warnings }` を、
`generate_exif_frame_preview_base64` は `ExifFramePreview { base64, warnings }` を返す。
どちらも `warnings` には「写真が小さすぎて Exif フレームを描けなかった」等が載る。
**GUI のプレビューはこの warnings を利用者に出さない**（プレビューは長辺 400px 固定で、
実出力ではフレームが出る写真でも `skip_exif` に落ちるため）。捨てる判断は core ではなく
境界（`gui/src/commands.rs`）の責務。
```

- [ ] **Step 3: docs/README.md の索引に本 plan を登録**

`docs/README.md` は「ここが唯一の入口」を宣言しており、spec は既に登録済み（commit `4c06e4a`）。
plan だけ載らないまま完了にしない。「## 直近の実装計画」表の先頭行として追加する:

```markdown
| [plans/2026-08-12-output-width-limit.md](superpowers/plans/2026-08-12-output-width-limit.md) | **出力幅の上限指定**（`--max-width`）の実装計画と実施メモ |
```

- [ ] **Step 4: spec §10 の完了条件を一括で検証**

```bash
cd gui-frontend && bun install && bun run build && cd ..   # tauri-build が dist の実在を要求する
make check     # fmt --check / clippy -D warnings / cargo test --workspace / typecheck
```

Expected: すべて PASS。CLI と GUI の手動検証は Task 6 Step 4 / Task 7 Step 5 で実施済み。
未実施なら**ここで実施する**（完了条件は spec §10 の全項目）:

- [ ] `cargo test --workspace` が green
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` が green
- [ ] `cargo fmt --all -- --check` が green
- [ ] `bun run typecheck`（= `svelte-check --tsconfig ./tsconfig.json`）が green
- [ ] CLI: `--max-width 1080` の出力が 1080×1350
- [ ] CLI: `--max-width 1002` で実効値 1000 の警告が1回だけ
- [ ] CLI: `--mode quality --max-width 1080` で無視の警告
- [ ] GUI: `1002` を確定 → `1000` / `→ 1000×1250`
- [ ] GUI: quality モードでトグルが無効化され理由が出る
- [ ] README / CLAUDE.md の CLI オプション表に `--max-width`
- [ ] CLAUDE.md の Core API 表が `render_exif_frame` の戻り値変更に追従
- [ ] `docs/README.md` の「直近の実装計画」表に本 plan が載っている

- [ ] **Step 5: 実施メモを本ファイル末尾に追記**

「なぜその形なのか」は spec にあるので、ここには**実装中に分かったことだけ**を書く:

- Task 6 / Task 7 の手動検証の実際の出力（`identify` の結果、警告の文面、
  Playwright スナップショットの該当部分）
- spec と食い違った点があれば、その内容とどちらを正としたか
- 却下・変更した実装判断（例: 前段を `resize` のボックス指定にしたので
  spec §4 のスケール式を手で実装しなくて済んだ、等）

- [ ] **Step 6: コミット**

```bash
git add README.md CLAUDE.md docs/README.md \
        docs/superpowers/plans/2026-08-12-output-width-limit.md
git commit -m "docs: 出力幅の上限指定: CLIオプション表とCore API表を更新し索引と実施メモを記録"
```

---

## 実施メモ

### Task 6 / Task 7 の手動検証は Task 8 でまとめて実施した

Task 6 Step 4・Task 7 Step 5 の手動検証は、各タスクのコミット（`ce5b4dc` / `2ea4357`）の
コミットメッセージに実行の痕跡が無く、当時の 実施メモ も空のままだった。
「未実施ならここで実施する」の指示どおり、spec §10 の全項目を Task 8 でまとめて実行した。

**CLI（`cargo run -p picture-tool`、入力は `magick -size 3000x2000 gradient:...` で生成した
3000x2000 の JPEG 2枚）:**

```
$ cargo run -q -p picture-tool -- -i in -o out -m pad --max-width 1080
[1/1] sample.jpg → sample_processed.jpg (0.1 MB) ✓
$ identify -format "%wx%h\n" out/sample_processed.jpg
1080x1350
```

```
$ cargo run -q -p picture-tool -- -i in -o out -m pad --max-width 1002   # 画像2枚
Warning: --max-width 1002 is rounded down to 1000 (the output canvas width is always a multiple of 4).
[1/2] sample2.jpg → sample2_processed.jpg (0.1 MB) ✓
[2/2] sample.jpg → sample_processed.jpg (0.1 MB) ✓
$ grep -c "rounded down to 1000" <ログ>
1
$ identify -format "%wx%h\n" out/sample_processed.jpg out/sample2_processed.jpg
1000x1250
1000x1250
```

警告が画像2枚に対して1回だけ出ることを確認（起動時に1回、の実装どおり）。

```
$ cargo run -q -p picture-tool -- -i in -o out -m quality --max-width 1080
Warning: --max-width is only supported with --mode crop or pad. Ignoring.
$ identify -format "%wx%h\n" out/sample_processed.jpg out/sample2_processed.jpg
3000x2000
3000x2000
```

```
$ cargo run -q -p picture-tool -- -i in -o out --max-width 3 ; echo exit=$?
error: invalid value '3' for '--max-width <MAX_WIDTH>': 3 is not in 4..=20000
exit=2
```

**GUI（`bun run dev` の vite dev サーバーに Playwright MCP で接続。Tauri を起動していないため
`invoke` は即 reject し、ドライブ一覧・プリセット・お気に入り・進捗購読の4件で警告トーストが
出る。これは想定内で、SettingsPanel の入力検証は webview だけで完結するため影響しない）:**

- 「出力幅を制限する」をチェック → `spinbutton "出力幅の上限 (px)": "1080"` と
  `→ 1080×1350` が表示された（トグル on の初期値 1080 が効いている）
- `1002` を入力して Tab で確定 → `spinbutton` の表示値が `"1000"` に、
  ラベルが `→ 1000×1250` に変わった（snapshot 確認済み）
- モードを `Quality (サイズのみ)` に切り替え →
  `checkbox "出力幅を制限する" [checked] [disabled]` になり、
  `Quality モードは 4:5 に変換しないため、出力幅の上限は適用されません。` が表示された

いずれも spec §10 の期待値と一致。

**追加確認（2026-08-18、同じく vite dev + Playwright MCP。上の3項目とは別セッション。
モードは既定の `Crop (中央クロップ)` から開始した）:**

- `20001` を入力して Tab で確定 → `spinbutton "出力幅の上限 (px)": "20000"` /
  `→ 20000×25000`（上限クランプが効いている）
- モードを `Quality (サイズのみ)` → `Pad (パディング)` と往復 →
  入力欄は `"20000"` のまま復帰した。モード切替は `config.max_width` も `lastMaxWidth` も
  リセットしないので、これが正しい挙動（Task 7 Step 5 手順6 の注記どおり）
- ついでにトグルを off → on → 入力欄が `"20000"` で戻った。off にしても
  `lastMaxWidth` が直前の値を覚えているという意図どおり

これで Task 7 Step 5 の手順1〜6 はすべて実測で確認済み。

### spec / 実装との食い違いは無かった

Task 1〜7 のコードは、この計画に書かれた実装（`target_canvas`、CLI の `Args`・警告文言、
`SettingsPanel.svelte` の型・関数）と実物を突き合わせて確認したが、差分は無かった。
ドキュメント（README / CLAUDE.md）は実物の `--help` 相当（`cli/src/main.rs` の `Args`）と
`core/src/lib.rs` / `core/src/exif_frame/mod.rs` の公開シグネチャを見てから書いた。

---

## 付録: 却下した案（spec に無い経緯）

計画の読者が「なぜこの形か」を再検討しなくて済むよう、設計時に落とした案を残す。

| 案 | 却下理由 |
|---|---|
| 最終リサイズ一本 | メモリが改善しない。巨大な RGBA キャンバスを確保してから縮小することになる |
| 前段のみ | crop に前段が無く、上限を保証できない |
| GUI にプリセット幅（1080 / 1440 等） | 実際に使う幅が未確定なので保留（spec §3） |
| プレビューと実出力の完全一致 | 一致させるにはプレビューを実寸法で描くしかなく、プレビューを軽く保つ目的と衝突（spec §6） |

spec は3巡のレビューを経て確定している。レビュー原本 `docs/tmp/review.md` は gitignore 済みで、
指摘の結論はすべて spec 本体へ取り込まれている（各巡の対応は commit `c2053a1..c502b80` の
メッセージに残っている）。
