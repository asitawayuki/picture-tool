# GUI デザイン刷新 実装計画

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 現行 GUI（Svelte 5 / 3 カラム固定 / 計 3,698 行）を、Material 3 系のトークン層と
11 個のプリミティブの上に、navigation rail による 3 モード構成として組み直す。

**Architecture:** `styles/` に CSS custom properties のトークン層を置き、`lib/ui/` の
プリミティブ 11 個だけがそれを消費する。アプリは `shell/`（rail + 可変カラム）・
`browser/`（フォルダーツリー・写真グリッド・プレビュー）・`panels/`（変換・メタデータ・
フレーム）に分かれ、`App.svelte` は 4 つの状態とパネルの差し替えだけを持つ。
**リアクティブでない純粋ロジック（幅のクランプ・グリッドの寸法計算・LRU・取得キュー）は
runes を持たない `.ts` に切り出し、`bun test` で単体検証する。** runes と DOM が絡む部分は
Playwright（`vite dev` に当てる）で検証する。

**Tech Stack:** Svelte 5（runes）/ Vite 5 / bun / TypeScript 5.7 / Tauri v2（設定のみ）/
`@material/material-color-utilities`（devDependency・色生成スクリプト用）/ `@playwright/test`（devDependency）

**Spec:** [`docs/superpowers/specs/2026-08-19-gui-redesign-design.md`](../specs/2026-08-19-gui-redesign-design.md)
（以降「spec §N」と参照する）。**採った手段・退けた手段・実測に基づく列数の導出はすべて spec にある。
迷ったら実装コードではなく spec を読むこと。** 本計画は spec §6 の段階 1〜9 を骨格にしている。

---

## Global Constraints

これらは全タスクの要件に暗黙に含まれる。

### 触ってよい範囲

- **`core/` と `cli/` は一切触らない**（spec スコープ）
- **`gui/src/` の Rust コードを触らない。新規 Tauri コマンドを 1 つも追加しない。**
  途中でコマンドを足したくなったら、それは設計から外れている
- **`gui-frontend/src/lib/api.ts` と `gui-frontend/src/lib/types.ts` を変更しない。**
  この 2 ファイルの差分が出たら、そのタスクは間違っている
- **`gui/tauri.conf.json` は `app.windows[0]` の寸法 4 値だけ変更してよい**（Task 9）。
  `security.csp`・`plugins`・`build` には触らない
- `gui/capabilities/default.json` を変更しない（`core:default` のまま）
- 触ってよいのは `gui-frontend/` 配下、`Makefile`、`.github/workflows/ci.yml`、
  `docs/`、`README.md`、`CLAUDE.md`

### 設計上の不変条件

- **`styles/` 以外のファイルに生の色を書かない。** 色は `var(--md-sys-color-*)` を使う。
  これが spec §1 の「場当たり的の再発防止線」であり、Task 18 Step 3 の grep で機械的に検査する
- **トークンがある属性は必ずトークンを使う。** 角丸は `var(--md-sys-shape-corner-*)`、
  **要素の間の余白**（`gap` / `margin` / `padding`）は `var(--space-N)`、
  文字は `var(--md-sys-typescale-*)`、時間と easing は `var(--md-sys-motion-*)`。
  **コンポーネント固有の内部寸法は各コンポーネント内に生の px で書いてよい**
  （`Button` の `min-height: 40px`、`Switch` のトラック 52×32px、
  `IconButton` の 40×40px、アイコンの `font-size` など）。
  これらは M3 の部品ごとの仕様値であって、他の部品と共有する設計値ではない。
  トークンにすると「使い回されないトークン」が 11 部品分増えるだけになる
- **レイアウトの構造的な寸法**（rail 幅 80px、カラムの min/max、グリッドの gap と padding）は
  spec §3-1 / §4-1 に数値が定義されている。`columns.ts` / `gridMetrics.ts` の定数と
  CSS の両方に現れるので、**片方だけ変えないこと**（Task 13 Step 4 の注記）
- **hover / focus / pressed は `::after` の全面オーバーレイ 1 パターンだけ**（spec §1-5）。
  `background` を直接書き換える実装は禁止。共有クラス `.state-layer` を使う
- **`backdrop-filter` を使わない**（spec 冒頭）。すりガラスは描画コストがグリッドのスクロールに直接効く
- **Web フォントを読み込まない**（spec §1-4）。`@font-face` も `@import url(https://…)` も書かない。
  CSP（`default-src 'self'`）でも落ちる
- **プリミティブは状態を持たない。** 値は `bind:value` / `bind:checked` で親が持つ（spec §2）
- **`Checkbox` を作らない。** 真偽値は `Switch` に統一する（spec §2）
- **プリミティブは 11 個で打ち止め。** 12 個目を作りたくなったら、それはパネルのローカル実装
- **プリセット JSON のスキーマを変更しない**（`ExifFrameConfig` は `types.ts` にある。上記のとおり不変）
- 国際化はしない（日本語のまま）

### 検証

- **各タスクの終わりで必ずビルドが通り、アプリが動くこと**（spec §6 の要件）。
  最低限 `cd gui-frontend && bun run typecheck && bun run build` が通ること
- 検証コマンド:
  - `cd gui-frontend && bun run typecheck`（`svelte-check --tsconfig ./tsconfig.json`）
  - `cd gui-frontend && bun test`（純粋ロジックの単体テスト。Task 1 で導入する）
    - **`bunfig.toml` の `[test] root = "src"` が前提**（Task 1 Step 2）。
      bun のテスト既定パターンは `*.test.*` と **`*.spec.*`** なので、
      これが無いと `e2e/*.spec.ts` が bun のランナーに読み込まれ、
      `@playwright/test` の `test()` がランナー外で呼ばれて落ちる
      （bun 1.3.14 で実測: `Ran 2 tests across 2 files` になる）
  - `cd gui-frontend && bun run build`
  - `cd gui-frontend && bun run e2e`（Playwright。Task 3 で導入する）
  - `make check`（Rust の lint / test ＋ フロントの typecheck。Task 1 で `bun test` を足す）
- **`gui` クレートに触る cargo コマンドの前に、必ずフロントエンドをビルドしておく**:
  `cd gui-frontend && bun install && bun run build`。`gui-frontend/dist` は gitignore 済みで、
  `tauri-build` は `gui/tauri.conf.json` の `frontendDist` の実在を要求する
- **テストコードを書く・直す前に `test-integrity` スキルを起動する**（CLAUDE.md 規約 / spec §7-3）。
  各タスクのテストは spec の記述から導いてある。実装を読んでテストを合わせない
- **Playwright を CI に足さない。** CI は `bun install --frozen-lockfile` しかせず、
  ブラウザバイナリを入れていない。e2e はローカル検証用に留める。
  `bun test`（純粋ロジック）だけを CI に足す
- **`bun.lock` は devDependency を足すたびにコミットする。**
  CI が `--frozen-lockfile` で入れるため、lockfile がずれると CI が落ちる

### 検証で偽陰性を踏まないための約束

- **runes（`$state` / `$derived`）を Node や `bun test` で動かして検証しない。**
  コンパイラを通らないので通る／落ちるが実挙動と一致しない。
  runes を含むのは `.svelte` と `.svelte.ts` だけに閉じ込め、テストは Playwright 側で行う
- **`bun test` の対象は runes を含まない `.ts` に限る**（`columns.ts`、`gridMetrics.ts`、
  `thumbnailCache.ts`、`requestQueue.ts`、`contrast.test.ts` が読む `color-tokens.css`）
- **「重ならない」「出ない」系の否定形テストには前提条件の assert を添える。**
  検査対象がそもそも発生していないだけで green になるため

---

## File Structure

### 新規作成

| ファイル | 責務 |
|---|---|
| `gui-frontend/scripts/generate-color-tokens.ts` | `material-color-utilities` から色ロールを生成し `color-tokens.css` を書き出す（dev 専用） |
| `gui-frontend/src/styles/color-tokens.css` | **生成物**。色ロールの値だけ。手で編集しない |
| `gui-frontend/src/styles/tokens.css` | 手書き。形状・余白・タイポ・状態レイヤー・elevation・モーション ＋ 色の 4 ブロック割り当て |
| `gui-frontend/src/styles/contrast.test.ts` | spec §7-1 のコントラスト検査（`bun test`） |
| `gui-frontend/bunfig.toml` | `bun test` の走査範囲を `src/` に限定する（`e2e/*.spec.ts` を拾わせない） |
| `gui-frontend/src/lib/ui/Button.svelte` ほか 11 個 | プリミティブ（spec §2） |
| `gui-frontend/src/Gallery.svelte` / `src/gallery.ts` / `gallery.html` | 部品確認用エントリ（dev 専用・ビルド成果物に入れない） |
| `gui-frontend/src/lib/shell/columns.ts` | **純粋**。カラムの min/max/既定値、クランプ、`localStorage` 文字列の解釈 |
| `gui-frontend/src/lib/shell/layout.svelte.ts` | runes。カラム幅と右パネル折りたたみの保持と永続化 |
| `gui-frontend/src/lib/shell/AppShell.svelte` | rail ＋ 可変カラムのレイアウト |
| `gui-frontend/src/lib/shell/NavigationRail.svelte` | 3 destination の rail |
| `gui-frontend/src/lib/browser/gridMetrics.ts` | **純粋**。列数・タイル寸法・行高・可視行範囲の計算 |
| `gui-frontend/src/lib/browser/thumbnailCache.ts` | **純粋**。バイト上限つき LRU |
| `gui-frontend/src/lib/browser/requestQueue.ts` | **純粋**。LIFO ＋ 初回 priming ＋ 範囲外の破棄 |
| `gui-frontend/src/lib/browser/thumbnailQueue.svelte.ts` | runes。上記 2 つと `getThumbnail` を繋ぐ |
| `gui-frontend/src/lib/browser/PhotoGrid.svelte` | 写真グリッド（仮想スクロール・listbox） |
| `gui-frontend/src/lib/browser/PhotoViewer.svelte` | 全画面プレビュー ＋ フィルムストリップ |
| `gui-frontend/src/lib/panels/ConvertPanel.svelte` | 変換設定 |
| `gui-frontend/src/lib/panels/MetadataPanel.svelte` | メタデータ（レイアウトのみ） |
| `gui-frontend/src/lib/panels/FramePanel.svelte` | Exif フレーム編集 |
| `gui-frontend/src/lib/panels/presets.svelte.ts` | プリセット一覧の保持と再読込 |
| `gui-frontend/src/lib/panels/convertRun.svelte.ts` | 変換実行・進捗・キャンセル・結果 |
| `gui-frontend/src/lib/panels/metadataDraft.svelte.ts` | メタデータの下書きと `isDirty`（本刷新では配線しない） |
| `gui-frontend/playwright.config.ts` / `e2e/*.spec.ts` / `e2e/stub.ts` | 検証（spec §7-2 / §7-3） |

### 移動

| 移動元 | 移動先 | タイミング |
|---|---|---|
| `src/lib/FolderTree.svelte` | `src/lib/browser/FolderTree.svelte` | Task 9 |

### 削除

| ファイル | タイミング | 吸収先 |
|---|---|---|
| `src/lib/ConfirmDialog.svelte`（130 行） | Task 6 | `ui/Dialog.svelte` |
| `src/lib/SelectionList.svelte`（168 行） | Task 10 | 廃止（spec §5-1） |
| `src/lib/SettingsPanel.svelte`（415 行） | Task 10 | `panels/ConvertPanel.svelte` |
| `src/lib/ThumbnailGrid.svelte`（319 行） | Task 13 | `browser/PhotoGrid.svelte` |
| `src/lib/ImagePreview.svelte`（482 行） | Task 14 | `browser/PhotoViewer.svelte` |
| `src/lib/ExifFrameSettings.svelte`（585 行） | Task 16 | `panels/FramePanel.svelte` |

### 変更するが残す

| ファイル | 扱い |
|---|---|
| `src/lib/focusTrap.ts` | **残す。** spec §2 の「`Dialog` の内部に取り込んで流用」は「Dialog が内部で使い、呼び出し側は意識しない」の意味に取る。`PhotoViewer` も同じ trap を要るため、モジュールとして共有する方が DRY。`FOCUSABLE` だけ Task 14 で直す |
| `src/lib/toasts.svelte.ts` | **無変更**（spec §2） |
| `src/lib/Toast.svelte` | 見た目のみ差し替え（Task 6） |
| `src/lib/ResultDialog.svelte` | `Dialog` + `Card` で組み直す（Task 6） |
| `src/lib/ProgressOverlay.svelte` | 進捗バーを `LinearProgress` に置換（Task 6） |
| `src/App.svelte` | 405 行 → 150 行程度（Task 7 で分解、Task 9 でシェル化） |
| `src/app.css` | 旧変数は Task 18 まで残す。リセットと `body` は残す |
| `src/main.ts` | `tokens.css` の読み込みを追加（Task 2） |
| `package.json` / `bun.lock` / `tsconfig.json` | devDependency とスクリプトの追加 |
| `Makefile` / `.github/workflows/ci.yml` | `bun test` の追加（Task 1） |

**`ResultDialog` / `ProgressOverlay` / `Toast` は `lib/` 直下に残す。**
spec §2 のファイル構成表はこの 3 つを挙げていないが、`ui/` は「11 個で打ち止め」の
汎用プリミティブの置き場であり、アプリ固有のこの 3 つを混ぜると境界が壊れる。

---

## Task 1: 色トークンの生成とコントラスト検査（段階 1 前半）

spec §1-1 / §1-2 / §7-1 / §8。**生成した 16 進値を spec に追記して確定させるところまでが本タスク。**

**Files:**
- Create: `gui-frontend/scripts/generate-color-tokens.ts`
- Create: `gui-frontend/src/styles/color-tokens.css`（生成物）
- Test: `gui-frontend/src/styles/contrast.test.ts`
- Create: `gui-frontend/bunfig.toml`
- Modify: `gui-frontend/package.json`（devDependency 3 件 ＋ スクリプト 2 件）, `gui-frontend/bun.lock`
- Modify: `Makefile`（`test-frontend` ターゲット追加、`check` に組み込み）
- Modify: `.github/workflows/ci.yml`（`bun test` ステップ追加）
- Modify: `docs/superpowers/specs/2026-08-19-gui-redesign-design.md`（§1-2 に生成値を追記）

**Interfaces:**
- Produces: `src/styles/color-tokens.css` が `:root` に
  `--_light-<role>` / `--_dark-<role>` を各 21 個定義する。ロール名は spec §1-1 の表と 1:1。
  以降のタスクはこの生の変数を直接使わず、Task 2 が定義する
  `--md-sys-color-<role>` 経由で読む

- [ ] **Step 1: devDependency を入れる**

```bash
cd gui-frontend
bun add -d @material/material-color-utilities@0.4.0 @types/bun
```

`@material/material-color-utilities` は**生成スクリプトだけが使う**。
アプリのバンドルには入らない（`src/` から import しない）。
`@types/bun` は `bun test` のグローバル（`describe` / `test` / `expect`）を
`svelte-check` に見せるために要る。`node_modules/@types/` 配下は tsconfig の
`types` 未指定時に自動で読まれるので、`tsconfig.json` の変更は要らない。

- [ ] **Step 2: `bunfig.toml` を置き、`package.json` と `tsconfig.json` を直す**

**先に `gui-frontend/bunfig.toml` を作る。**

```toml
# bun のテスト既定パターンは *.test.* と *.spec.* の両方。
# 走査範囲を src/ に限定しないと e2e/*.spec.ts が bun のランナーに
# 読み込まれ、@playwright/test の test() がランナー外で呼ばれて落ちる。
# Playwright 側は playwright.config.ts の testDir: "./e2e" で拾う（Task 3）。
[test]
root = "src"
```

**これを Task 1 で入れておくのが要点。** `bun test` を Makefile と CI に組み込むのは
このタスクだが、`e2e/` が生えるのは Task 3 で、そこで初めて落ちる。
順序の罠なので、先に閉じておく。

`package.json` の `"scripts"` に 2 行追加する（既存の `dev` / `build` / `preview` / `typecheck` は残す）:

```json
    "gen:colors": "bun run scripts/generate-color-tokens.ts",
    "test": "bun test"
```

`tsconfig.json` の `include` を差し替える。`src/` の外に置くスクリプトも
`svelte-check` の検査対象にしておかないと、型の壊れたまま放置される:

```json
  "include": ["src/**/*.ts", "src/**/*.svelte", "scripts/**/*.ts"]
```

- [ ] **Step 3: 生成スクリプトを書く**

`gui-frontend/scripts/generate-color-tokens.ts`:

```ts
/**
 * Material 3 の色ロールを生成して src/styles/color-tokens.css を書き出す。
 *
 * spec §1-2 の「面は無彩色、アクセントだけ色を持つ」を、neutral / neutralVariant の
 * chroma を 0 にしたカスタム DynamicScheme で実現する。M3 の標準スキーム
 * （tonal spot）は surface にも source color の色相が薄く乗り、写真ツールでは
 * 背景の色被りが色判断を狂わせるため採らない。
 *
 * 実行: cd gui-frontend && bun run gen:colors
 */
import {
  DynamicScheme,
  Hct,
  MaterialDynamicColors,
  TonalPalette,
  Variant,
  argbFromHex,
  hexFromArgb,
} from "@material/material-color-utilities";

/**
 * source color は現行 app.css の --accent-hover。
 * 現行の --accent (#818cf8) は同色相で明るいトーンであり、生成後は primary の
 * 明るいトーンとして再現される（spec §1-2）。
 */
const SOURCE_HEX = "#6366F1";

/** spec §1-1 の表で「使う」と決めた 21 ロール。これ以外は定義しない。 */
const ROLES: [string, { getArgb(scheme: DynamicScheme): number }][] = [
  ["primary", MaterialDynamicColors.primary],
  ["on-primary", MaterialDynamicColors.onPrimary],
  ["primary-container", MaterialDynamicColors.primaryContainer],
  ["on-primary-container", MaterialDynamicColors.onPrimaryContainer],
  ["surface", MaterialDynamicColors.surface],
  ["surface-container-lowest", MaterialDynamicColors.surfaceContainerLowest],
  ["surface-container-low", MaterialDynamicColors.surfaceContainerLow],
  ["surface-container", MaterialDynamicColors.surfaceContainer],
  ["surface-container-high", MaterialDynamicColors.surfaceContainerHigh],
  ["surface-container-highest", MaterialDynamicColors.surfaceContainerHighest],
  ["on-surface", MaterialDynamicColors.onSurface],
  ["on-surface-variant", MaterialDynamicColors.onSurfaceVariant],
  ["outline", MaterialDynamicColors.outline],
  ["outline-variant", MaterialDynamicColors.outlineVariant],
  ["error", MaterialDynamicColors.error],
  ["on-error", MaterialDynamicColors.onError],
  ["error-container", MaterialDynamicColors.errorContainer],
  ["on-error-container", MaterialDynamicColors.onErrorContainer],
  ["inverse-surface", MaterialDynamicColors.inverseSurface],
  ["inverse-on-surface", MaterialDynamicColors.inverseOnSurface],
  ["scrim", MaterialDynamicColors.scrim],
];

function buildScheme(isDark: boolean): DynamicScheme {
  const source = Hct.fromInt(argbFromHex(SOURCE_HEX));
  return new DynamicScheme({
    sourceColorHct: source,
    variant: Variant.TONAL_SPOT,
    contrastLevel: 0,
    isDark,
    // primary だけ source の chroma を保つ
    primaryPalette: TonalPalette.fromHueAndChroma(source.hue, source.chroma),
    // secondary / tertiary は spec §1-1 で「定義しない」と決めたロールにしか
    // 使われないが、DynamicScheme は必ず全パレットを持つ。無彩色にしておけば
    // 誤って参照しても色が漏れない。
    secondaryPalette: TonalPalette.fromHueAndChroma(source.hue, 0),
    tertiaryPalette: TonalPalette.fromHueAndChroma(source.hue, 0),
    // 面と線は完全な無彩色（spec §1-2）
    neutralPalette: TonalPalette.fromHueAndChroma(source.hue, 0),
    neutralVariantPalette: TonalPalette.fromHueAndChroma(source.hue, 0),
    // errorPalette は既定（M3 標準の赤）のまま。状態色まで無彩色にすると
    // 「危険」が伝わらない。
  });
}

function block(isDark: boolean): string {
  const scheme = buildScheme(isDark);
  const prefix = isDark ? "dark" : "light";
  return ROLES.map(
    ([name, color]) => `  --_${prefix}-${name}: ${hexFromArgb(color.getArgb(scheme))};`
  ).join("\n");
}

const css = `/* 生成物。編集しないこと。
 * 再生成: cd gui-frontend && bun run gen:colors
 * 生成元: scripts/generate-color-tokens.ts（source color ${SOURCE_HEX}）
 *
 * ここは値の置き場でしかない。--md-sys-color-* への割り当ては tokens.css が行う。
 */
:root {
  /* ライト */
${block(false)}

  /* ダーク */
${block(true)}
}
`;

await Bun.write(new URL("../src/styles/color-tokens.css", import.meta.url), css);
console.log("wrote src/styles/color-tokens.css");
```

- [ ] **Step 4: `test-integrity` スキルを起動する**

テストコードを書く前に必ず起動する（CLAUDE.md 規約）。
本タスクのテストは spec §7-1 の表から導いてある。**生成された値を見てから
基準を決めない**（それでは必ず通るテストになる）。

- [ ] **Step 5: 失敗するコントラスト検査を書く**

`gui-frontend/src/styles/contrast.test.ts`:

```ts
/**
 * spec §7-1 のコントラスト検査。
 *
 * 全ペアを検査する方式は成立しない。outline-variant や scrim は意図的に
 * 低コントラストで、M3 のトーン設計上 AA を満たさないため必ず赤になる。
 * 検査対象は「対になるロール」に限定する。
 *
 * color-tokens.css を直接読む。生成スクリプトの戻り値ではなく出荷される
 * ファイルを見ることで、手で編集された場合もここで落ちる。
 */
import { describe, expect, test } from "bun:test";

const SURFACES = [
  "surface",
  "surface-container-lowest",
  "surface-container-low",
  "surface-container",
  "surface-container-high",
  "surface-container-highest",
] as const;

interface Pair {
  fg: string;
  bg: string;
  /** WCAG の基準。本文は AA 4.5:1、境界線などの非テキストは 3:1 */
  min: number;
}

function pairsUnderTest(): Pair[] {
  const pairs: Pair[] = [];
  for (const bg of SURFACES) {
    pairs.push({ fg: "on-surface", bg, min: 4.5 });
    pairs.push({ fg: "on-surface-variant", bg, min: 4.5 });
  }
  pairs.push({ fg: "on-primary", bg: "primary", min: 4.5 });
  pairs.push({ fg: "on-primary-container", bg: "primary-container", min: 4.5 });
  pairs.push({ fg: "on-error", bg: "error", min: 4.5 });
  pairs.push({ fg: "on-error-container", bg: "error-container", min: 4.5 });
  pairs.push({ fg: "inverse-on-surface", bg: "inverse-surface", min: 4.5 });
  pairs.push({ fg: "outline", bg: "surface", min: 3 });
  return pairs;
}

/** WCAG 2.x の相対輝度。3 チャンネルすべてに同じガンマ補正を掛けること。 */
function channelLuminance(value8bit: number): number {
  const s = value8bit / 255;
  return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
}

function relativeLuminance(hex: string): number {
  const n = Number.parseInt(hex.slice(1), 16);
  return (
    0.2126 * channelLuminance((n >> 16) & 0xff) +
    0.7152 * channelLuminance((n >> 8) & 0xff) +
    0.0722 * channelLuminance(n & 0xff)
  );
}

function contrastRatio(a: string, b: string): number {
  const [hi, lo] = [relativeLuminance(a), relativeLuminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

const source = await Bun.file(new URL("./color-tokens.css", import.meta.url)).text();

const values: Record<"light" | "dark", Record<string, string>> = { light: {}, dark: {} };
for (const m of source.matchAll(/--_(light|dark)-([a-z-]+):\s*(#[0-9a-f]{6});/g)) {
  values[m[1] as "light" | "dark"][m[2]] = m[3];
}

for (const scheme of ["light", "dark"] as const) describe(`${scheme} スキーム`, () => {
  // 前提条件: そもそも値が読めていないと、以下の検査は「対象が無い」だけで
  // 素通りしうる。21 ロールが揃っていることを先に確かめる。
  test("spec §1-1 の 21 ロールがすべて定義されている", () => {
    expect(Object.keys(values[scheme]).sort()).toEqual(
      [
        "error", "error-container", "inverse-on-surface", "inverse-surface",
        "on-error", "on-error-container", "on-primary", "on-primary-container",
        "on-surface", "on-surface-variant", "outline", "outline-variant",
        "primary", "primary-container", "scrim", "surface",
        "surface-container", "surface-container-high", "surface-container-highest",
        "surface-container-low", "surface-container-lowest",
      ].sort()
    );
  });

  for (const { fg, bg, min } of pairsUnderTest()) {
    test(`${fg} / ${bg} が ${min}:1 以上`, () => {
      const ratio = contrastRatio(values[scheme][fg], values[scheme][bg]);
      expect(ratio).toBeGreaterThanOrEqual(min);
    });
  }
});

describe("面は無彩色である（spec §1-2）", () => {
  // 「背景の色被りが写真の色判断を狂わせる」を防ぐのが目的なので、
  // surface 系は R=G=B であることまで要求する。
  for (const role of SURFACES) {
    test(`${role} は R=G=B`, () => {
      for (const scheme of ["light", "dark"] as const) {
        const hex = values[scheme][role];
        expect(hex.slice(1, 3)).toBe(hex.slice(3, 5));
        expect(hex.slice(3, 5)).toBe(hex.slice(5, 7));
      }
    });
  }
});
```

- [ ] **Step 6: 落ちることを確認する**

```bash
cd gui-frontend && bun test src/styles/contrast.test.ts
```

期待: `color-tokens.css` がまだ無いので `ENOENT` で落ちる。

- [ ] **Step 7: 生成する**

```bash
cd gui-frontend && bun run gen:colors
```

- [ ] **Step 8: 通ることを確認する**

```bash
cd gui-frontend && bun test src/styles/contrast.test.ts
```

期待: PASS。落ちた場合、**値を手で直さないこと**。落ちるのは
「chroma 0 の neutral では M3 の既定トーンが AA を満たさない」という
設計側の事実なので、生成スクリプト側（`contrastLevel` を上げる等）で解く。

- [ ] **Step 9: `Makefile` に組み込む**

`.PHONY` 行に `test-frontend` を足し、`typecheck` ターゲットの直後に追加する:

```make
# フロントエンドの単体テスト（純粋ロジックのみ。runes / DOM は Playwright 側）。
# 走査範囲は bunfig.toml の [test] root = "src" で src/ に限定してある
test-frontend:
	cd gui-frontend && bun test
```

`check` ターゲットを変更する:

```make
check: lint test typecheck test-frontend
```

- [ ] **Step 10: CI に組み込む**

`.github/workflows/ci.yml` の `Typecheck frontend` ステップの直後に追加する:

```yaml
      # 純粋ロジックの単体テスト。走査範囲は bunfig.toml で src/ に限定してある。
      # Playwright（e2e）はブラウザバイナリを入れていないので CI では走らせない
      - name: Test frontend
        working-directory: gui-frontend
        run: bun test
```

- [ ] **Step 11: 生成値を spec に追記する**

`docs/superpowers/specs/2026-08-19-gui-redesign-design.md` の §1-2 末尾
（「手で書いた推測値は tonal palette の階調を壊すため。」の直後）に、
**実際に生成された値**を貼る。書式:

````markdown
#### 生成結果（段階 1 で確定）

source color `#6366F1` / `Variant.TONAL_SPOT` / `contrastLevel: 0` /
neutral・neutralVariant の chroma を 0 にしたカスタム `DynamicScheme`。
生成は `gui-frontend/scripts/generate-color-tokens.ts`、検査は
`gui-frontend/src/styles/contrast.test.ts`。

| ロール | ライト | ダーク |
|---|---|---|
| `primary` | `#xxxxxx` | `#xxxxxx` |
| …（21 行） | | |

コントラスト実測（spec §7-1 の対象ペア、最小値）: ライト `X.XX:1`（`on-surface-variant` / `surface-container-highest`）、
ダーク `X.XX:1`（同）。すべて基準を満たす。
````

**`#xxxxxx` はプレースホルダーではなく、Step 7 の出力そのものを貼ること。**
`bun test` の出力に比率が出ないので、値は次のワンライナーで取る:

```bash
cd gui-frontend && bun -e '
const s = await Bun.file("src/styles/color-tokens.css").text();
const v = {light:{},dark:{}};
for (const m of s.matchAll(/--_(light|dark)-([a-z-]+):\s*(#[0-9a-f]{6});/g)) v[m[1]][m[2]] = m[3];
for (const k of Object.keys(v.light).sort()) console.log(`| \`${k}\` | \`${v.light[k]}\` | \`${v.dark[k]}\` |`);
'
```

- [ ] **Step 12: コミット**

```bash
git add gui-frontend/package.json gui-frontend/bun.lock gui-frontend/tsconfig.json \
        gui-frontend/bunfig.toml \
        gui-frontend/scripts/generate-color-tokens.ts \
        gui-frontend/src/styles/color-tokens.css \
        gui-frontend/src/styles/contrast.test.ts \
        Makefile .github/workflows/ci.yml \
        docs/superpowers/specs/2026-08-19-gui-redesign-design.md
git commit -m "feat(gui): M3 色トークンを生成しコントラスト検査を追加

対応概要: material-color-utilities で neutral chroma 0 のカスタム
DynamicScheme から 21 ロールを生成し、spec 7-1 の対になるロードだけを
検査する bun test を追加。生成値は spec 1-2 に追記した。"
```

（コミットメッセージは `commit-message` スキルの規約に従うこと。上記は形式の例）

---

## Task 2: 静的トークンと読み込み配線（段階 1 後半）

spec §1-3 / §1-4 / §1-5 / §1-6 / §1-7。

**Files:**
- Create: `gui-frontend/src/styles/tokens.css`
- Modify: `gui-frontend/src/main.ts`

**Interfaces:**
- Produces: 以降の全コンポーネントが使う CSS custom properties。
  - 色: `--md-sys-color-<role>`（21 個。Task 1 の `--_light-*` / `--_dark-*` を 4 ブロックで割り当て）
  - 形状: `--md-sys-shape-corner-xs|sm|md|lg|full`
  - 余白: `--space-1` 〜 `--space-6`
  - タイポ: `--md-sys-typescale-title-md|title-sm|body-md|body-sm|label-lg`（`font:` 短縮形）と
    `--md-sys-typescale-label-lg-tracking`
  - 状態: `--md-sys-state-hover-opacity|focus-opacity|pressed-opacity`、
    `--md-sys-state-focus-ring`、`--md-sys-state-focus-ring-offset`
  - elevation: `--md-sys-elevation-shadow-0..3`、`--md-sys-elevation-surface-0..3`
  - モーション: `--md-sys-motion-duration-short|medium`、
    `--md-sys-motion-easing-standard|emphasized-decelerate`
- Produces: グローバルクラス `.state-layer`。hover / focus-visible / pressed の
  オーバーレイを 1 パターンで供給する。**プリミティブはこれを使い、`background` を
  直接書き換えない**

- [ ] **Step 1: `tokens.css` を書く**

`gui-frontend/src/styles/tokens.css`:

```css
/* Material 3 系デザイントークン。
 *
 * 色の値そのものは color-tokens.css（生成物）にある。ここは「どのロールを
 * どのテーマで使うか」の割り当てと、色以外のトークンを持つ。
 *
 * **このファイルと color-tokens.css 以外に生の色を書かないこと。**
 * それが「場当たり的」の再発防止線である（spec §1）。
 * 余白・角丸・タイポ・モーションもここのトークンを使う。
 * ただし部品固有の内部寸法（トラック 52×32px、アイコン 18px など）は
 * 各コンポーネント内に直接書いてよい（Global Constraints）。
 */
@import "./color-tokens.css";

/* ---- 色ロールの割り当て（spec §1-7） ----
 *
 * 4 ブロックで書く。ダークを :root だけで定義してライトをメディアクエリ内の
 * :root で上書きする構造にしてはならない。メディアクエリは詳細度を変えないので、
 * 将来 data-theme="dark" を足してもライトの OS では効かなくなる。
 * :root:not([data-theme="dark"]) がメディアクエリ側を降ろすこの形なら
 * 両方向の上書きが成立する。
 *
 * 本刷新で使うのは OS 追従の経路だけ（手動切替は入れない）。
 */
:root,
:root[data-theme="dark"] {
  --md-sys-color-primary: var(--_dark-primary);
  --md-sys-color-on-primary: var(--_dark-on-primary);
  --md-sys-color-primary-container: var(--_dark-primary-container);
  --md-sys-color-on-primary-container: var(--_dark-on-primary-container);
  --md-sys-color-surface: var(--_dark-surface);
  --md-sys-color-surface-container-lowest: var(--_dark-surface-container-lowest);
  --md-sys-color-surface-container-low: var(--_dark-surface-container-low);
  --md-sys-color-surface-container: var(--_dark-surface-container);
  --md-sys-color-surface-container-high: var(--_dark-surface-container-high);
  --md-sys-color-surface-container-highest: var(--_dark-surface-container-highest);
  --md-sys-color-on-surface: var(--_dark-on-surface);
  --md-sys-color-on-surface-variant: var(--_dark-on-surface-variant);
  --md-sys-color-outline: var(--_dark-outline);
  --md-sys-color-outline-variant: var(--_dark-outline-variant);
  --md-sys-color-error: var(--_dark-error);
  --md-sys-color-on-error: var(--_dark-on-error);
  --md-sys-color-error-container: var(--_dark-error-container);
  --md-sys-color-on-error-container: var(--_dark-on-error-container);
  --md-sys-color-inverse-surface: var(--_dark-inverse-surface);
  --md-sys-color-inverse-on-surface: var(--_dark-inverse-on-surface);
  --md-sys-color-scrim: var(--_dark-scrim);
  color-scheme: dark;
}

@media (prefers-color-scheme: light) {
  :root:not([data-theme="dark"]) {
    --md-sys-color-primary: var(--_light-primary);
    --md-sys-color-on-primary: var(--_light-on-primary);
    --md-sys-color-primary-container: var(--_light-primary-container);
    --md-sys-color-on-primary-container: var(--_light-on-primary-container);
    --md-sys-color-surface: var(--_light-surface);
    --md-sys-color-surface-container-lowest: var(--_light-surface-container-lowest);
    --md-sys-color-surface-container-low: var(--_light-surface-container-low);
    --md-sys-color-surface-container: var(--_light-surface-container);
    --md-sys-color-surface-container-high: var(--_light-surface-container-high);
    --md-sys-color-surface-container-highest: var(--_light-surface-container-highest);
    --md-sys-color-on-surface: var(--_light-on-surface);
    --md-sys-color-on-surface-variant: var(--_light-on-surface-variant);
    --md-sys-color-outline: var(--_light-outline);
    --md-sys-color-outline-variant: var(--_light-outline-variant);
    --md-sys-color-error: var(--_light-error);
    --md-sys-color-on-error: var(--_light-on-error);
    --md-sys-color-error-container: var(--_light-error-container);
    --md-sys-color-on-error-container: var(--_light-on-error-container);
    --md-sys-color-inverse-surface: var(--_light-inverse-surface);
    --md-sys-color-inverse-on-surface: var(--_light-inverse-on-surface);
    --md-sys-color-scrim: var(--_light-scrim);
    color-scheme: light;
  }
}

:root[data-theme="light"] {
  --md-sys-color-primary: var(--_light-primary);
  --md-sys-color-on-primary: var(--_light-on-primary);
  --md-sys-color-primary-container: var(--_light-primary-container);
  --md-sys-color-on-primary-container: var(--_light-on-primary-container);
  --md-sys-color-surface: var(--_light-surface);
  --md-sys-color-surface-container-lowest: var(--_light-surface-container-lowest);
  --md-sys-color-surface-container-low: var(--_light-surface-container-low);
  --md-sys-color-surface-container: var(--_light-surface-container);
  --md-sys-color-surface-container-high: var(--_light-surface-container-high);
  --md-sys-color-surface-container-highest: var(--_light-surface-container-highest);
  --md-sys-color-on-surface: var(--_light-on-surface);
  --md-sys-color-on-surface-variant: var(--_light-on-surface-variant);
  --md-sys-color-outline: var(--_light-outline);
  --md-sys-color-outline-variant: var(--_light-outline-variant);
  --md-sys-color-error: var(--_light-error);
  --md-sys-color-on-error: var(--_light-on-error);
  --md-sys-color-error-container: var(--_light-error-container);
  --md-sys-color-on-error-container: var(--_light-on-error-container);
  --md-sys-color-inverse-surface: var(--_light-inverse-surface);
  --md-sys-color-inverse-on-surface: var(--_light-inverse-on-surface);
  --md-sys-color-scrim: var(--_light-scrim);
  color-scheme: light;
}

:root {
  /* ---- 形状（spec §1-3） ---- */
  --md-sys-shape-corner-xs: 4px;    /* チップ、バッジ */
  --md-sys-shape-corner-sm: 8px;    /* テキストフィールド、サムネイル */
  --md-sys-shape-corner-md: 12px;   /* カード、パネル */
  --md-sys-shape-corner-lg: 16px;   /* ダイアログ */
  --md-sys-shape-corner-full: 999px; /* ボタン、セグメント、選択インジケータ */

  /* ---- 余白（spec §1-4）: 4px グリッドの 6 段 ---- */
  --space-1: 4px;
  --space-2: 8px;
  --space-3: 12px;
  --space-4: 16px;
  --space-5: 24px;
  --space-6: 32px;

  /* ---- タイポ（spec §1-4）: 実際に使う 5 段だけ ----
   * Web フォントは読み込まない。日本語は OS 同梱にフォールバックする。
   * font: 短縮形で持つので、使う側は font: var(--md-sys-typescale-body-md); と書く。
   */
  --md-ref-typeface-plain: system-ui, -apple-system, "Segoe UI", "Yu Gothic UI",
    "Hiragino Sans", "Noto Sans JP", Roboto, sans-serif;
  --md-sys-typescale-title-md: 500 16px/24px var(--md-ref-typeface-plain);
  --md-sys-typescale-title-sm: 500 14px/20px var(--md-ref-typeface-plain);
  --md-sys-typescale-body-md: 400 14px/20px var(--md-ref-typeface-plain);
  --md-sys-typescale-body-sm: 400 12px/16px var(--md-ref-typeface-plain);
  --md-sys-typescale-label-lg: 500 14px/20px var(--md-ref-typeface-plain);
  --md-sys-typescale-label-lg-tracking: 0.1px;

  /* ---- 状態レイヤー（spec §1-5） ---- */
  --md-sys-state-hover-opacity: 0.08;
  --md-sys-state-focus-opacity: 0.10;
  --md-sys-state-pressed-opacity: 0.10;
  --md-sys-state-focus-ring: 3px solid var(--md-sys-color-primary);
  --md-sys-state-focus-ring-offset: 2px;

  /* ---- elevation（spec §1-5）: 4 段のみ ----
   * ダークでは影がほぼ見えないため、surface-container 系の明度差を主、
   * box-shadow を従として併用する。backdrop-filter は使わない。
   */
  --md-sys-elevation-surface-0: var(--md-sys-color-surface);
  --md-sys-elevation-surface-1: var(--md-sys-color-surface-container-low);
  --md-sys-elevation-surface-2: var(--md-sys-color-surface-container);
  --md-sys-elevation-surface-3: var(--md-sys-color-surface-container-high);
  --md-sys-elevation-shadow-0: none;
  --md-sys-elevation-shadow-1: 0 1px 2px 0 rgb(0 0 0 / 0.30), 0 1px 3px 1px rgb(0 0 0 / 0.15);
  --md-sys-elevation-shadow-2: 0 1px 2px 0 rgb(0 0 0 / 0.30), 0 2px 6px 2px rgb(0 0 0 / 0.15);
  --md-sys-elevation-shadow-3: 0 1px 3px 0 rgb(0 0 0 / 0.30), 0 4px 8px 3px rgb(0 0 0 / 0.15);

  /* ---- モーション（spec §1-6） ---- */
  --md-sys-motion-duration-short: 150ms;
  --md-sys-motion-duration-medium: 250ms;
  --md-sys-motion-easing-standard: cubic-bezier(0.2, 0, 0, 1);
  --md-sys-motion-easing-emphasized-decelerate: cubic-bezier(0.05, 0.7, 0.1, 1);
}

/* ---- 状態レイヤーの唯一の実装（spec §1-5） ----
 *
 * これを付けた要素は position: relative になり、::after が全面を覆う。
 * 塗りは currentColor なので、対になる on-* 色が自動的に使われる。
 * hover と選択状態を混ぜないために、background を直接書き換える実装は禁止。
 *
 * tokens.css は Svelte の外側にある素の CSS なので、このクラスは
 * どのコンポーネントからでも素直に使える（スコープが付かない）。
 */
.state-layer {
  position: relative;
  isolation: isolate;
}

.state-layer::after {
  content: "";
  position: absolute;
  inset: 0;
  border-radius: inherit;
  background: currentColor;
  opacity: 0;
  pointer-events: none;
  transition: opacity var(--md-sys-motion-duration-short)
    var(--md-sys-motion-easing-standard);
}

.state-layer:hover:not(:disabled)::after {
  opacity: var(--md-sys-state-hover-opacity);
}

.state-layer:focus-visible::after {
  opacity: var(--md-sys-state-focus-opacity);
}

.state-layer:active:not(:disabled)::after {
  opacity: var(--md-sys-state-pressed-opacity);
}

/* フォーカスリングは状態レイヤーとは別に、輪郭として出す */
:focus-visible {
  outline: var(--md-sys-state-focus-ring);
  outline-offset: var(--md-sys-state-focus-ring-offset);
}

/* ---- prefers-reduced-motion（spec §1-6 / §7-3） ----
 * すべてのトランジションを止める。個別のコンポーネントで書かない。
 */
@media (prefers-reduced-motion: reduce) {
  *,
  *::before,
  *::after {
    transition-duration: 0.01ms !important;
    animation-duration: 0.01ms !important;
    animation-iteration-count: 1 !important;
    scroll-behavior: auto !important;
  }
}
```

- [ ] **Step 2: `main.ts` に読み込みを足す**

`gui-frontend/src/main.ts` の 1 行目を差し替える:

```ts
import "./styles/tokens.css";
import "./app.css";
import App from "./App.svelte";
import { mount } from "svelte";

const app = mount(App, { target: document.getElementById("app")! });

export default app;
```

**`app.css` を後に読む。** 旧変数（`--bg-primary` など）はまだ 5 コンポーネントが
参照しており、Task 18 まで残す。トークンと名前が衝突しないので順序は本来
どちらでもよいが、「新しい層が土台」という読み順に揃えておく。

- [ ] **Step 3: ビルドと型検査が通ることを確認する**

```bash
cd gui-frontend && bun run typecheck && bun run build
```

期待: どちらも成功。`dist/assets/*.css` に `--md-sys-color-primary` が入っていること:

```bash
grep -c -- "--md-sys-color-primary" gui-frontend/dist/assets/*.css
```

期待: 1 以上。

- [ ] **Step 4: 見た目が壊れていないことを確認する**

```bash
cd gui-frontend && bun run dev
```

ブラウザで `http://localhost:5173` を開く。**Tauri の外なので `invoke` は全部
reject し、トーストが数枚出る。それが正しい状態**（ドライブ一覧・お気に入りの
取得失敗）。画面の骨格（3 カラム）が現行どおり出ていればよい。

- [ ] **Step 5: コミット**

```bash
git add gui-frontend/src/styles/tokens.css gui-frontend/src/main.ts
git commit -m "feat(gui): 形状・余白・タイポ・状態・モーションのトークンを追加"
```

---

## Task 3: 部品確認用エントリと基礎プリミティブ 3 個（段階 2 の 1/3）

spec §2 / §6「部品確認用エントリについて」/ §7-3。
`Button` / `IconButton` / `Card` と、それを見るための `gallery.html`、Playwright を入れる。

**Files:**
- Create: `gui-frontend/src/lib/ui/Button.svelte`
- Create: `gui-frontend/src/lib/ui/IconButton.svelte`
- Create: `gui-frontend/src/lib/ui/Card.svelte`
- Create: `gui-frontend/gallery.html`, `gui-frontend/src/gallery.ts`, `gui-frontend/src/Gallery.svelte`
- Create: `gui-frontend/playwright.config.ts`, `gui-frontend/e2e/gallery.spec.ts`
- Modify: `gui-frontend/package.json`（`@playwright/test` ＋ `e2e` スクリプト）, `gui-frontend/bun.lock`, `gui-frontend/tsconfig.json`

**Interfaces:**
- Produces: `Button`
  `{ variant?: "filled"|"tonal"|"outlined"|"text", danger?: boolean, disabled?: boolean,
     icon?: string, full?: boolean, type?: "button"|"submit",
     onclick?: (e: MouseEvent) => void, children: Snippet }`
- Produces: `IconButton`
  `{ variant?: "standard"|"filled", toggle?: boolean, pressed?: boolean,
     label: string, icon: string, disabled?: boolean, onclick?: (e: MouseEvent) => void }`
  — `label` は必須。アイコンしか出ないので `aria-label` と `title` の両方に使う
- Produces: `Card`
  `{ level?: 0|1|2|3, padding?: string, title?: string, children: Snippet }`
- Produces: `Gallery.svelte` — 以降の Task 4 / 5 / 6 が節を足していく置き場

- [ ] **Step 1: Playwright を入れる**

```bash
cd gui-frontend
bun add -d @playwright/test
bunx playwright install chromium
```

`package.json` の `"scripts"` に追加:

```json
    "e2e": "playwright test"
```

`tsconfig.json` の `include` を差し替える:

```json
  "include": [
    "src/**/*.ts",
    "src/**/*.svelte",
    "scripts/**/*.ts",
    "e2e/**/*.ts",
    "playwright.config.ts"
  ]
```

- [ ] **Step 2: `playwright.config.ts` を書く**

```ts
import { defineConfig, devices } from "@playwright/test";

/**
 * 検証は vite dev サーバーに当てる（spec §7-3）。
 *
 * Tauri の外なので `window.__TAURI_INTERNALS__` が無く、`invoke` は全部 reject する。
 * エラー経路の検証にはこれがそのまま使える。見た目の検証にはスタブを注入する
 * （e2e/stub.ts。Task 9 で追加）。
 *
 * CI では走らせない。CI にブラウザバイナリを入れていないため。
 *
 * **ポートは e2e 専用の 5174 にして、必ず自分でサーバーを立てる。**
 * `vite.config.ts` は `port: 5173` / `strictPort: true` で、そこは
 * `make dev`（Tauri の dev サーバー）が使う。5173 を `reuseExistingServer: true`
 * で共有すると、`make dev` を上げたまま e2e を走らせたときに
 * **別のチェックアウト（worktree）の画面を検証してしまう**。
 * 落ちない代わりに嘘の緑が出る種類の事故なので、port ごと分ける。
 */
export default defineConfig({
  testDir: "./e2e",
  // 同じ vite dev サーバーを共有するので直列に走らせる
  fullyParallel: false,
  workers: 1,
  timeout: 30_000,
  use: {
    baseURL: "http://localhost:5174",
    // 既定ウィンドウ寸法（tauri.conf.json / spec §3-1）に合わせる
    viewport: { width: 1440, height: 800 },
  },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
  webServer: {
    // vite の CLI 引数は vite.config.ts の server.port を上書きする
    command: "bunx vite dev --port 5174 --strictPort",
    url: "http://localhost:5174",
    reuseExistingServer: false,
    timeout: 60_000,
  },
});
```

**`make dev` を落としてから e2e を走らせる必要は無い**（ポートが別なので共存できる）。

- [ ] **Step 3: `Button.svelte` を書く**

`gui-frontend/src/lib/ui/Button.svelte`:

```svelte
<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    variant?: "filled" | "tonal" | "outlined" | "text";
    /** 破壊的操作。primary ロールを error ロールに振り替える */
    danger?: boolean;
    disabled?: boolean;
    /** ラベルの左に置く記号。装飾なので aria-hidden にする */
    icon?: string;
    /** 幅を親いっぱいにする（パネル最下部の主ボタンなど） */
    full?: boolean;
    type?: "button" | "submit";
    onclick?: (event: MouseEvent) => void;
    children: Snippet;
  }

  let {
    variant = "filled",
    danger = false,
    disabled = false,
    icon,
    full = false,
    type = "button",
    onclick,
    children,
  }: Props = $props();
</script>

<button
  class="btn state-layer {variant}"
  class:danger
  class:full
  {type}
  {disabled}
  {onclick}
>
  {#if icon}<span class="icon" aria-hidden="true">{icon}</span>{/if}
  {@render children()}
</button>

<style>
  /* hover / focus / pressed は .state-layer（tokens.css）が ::after で供給する。
     ここで background を書き換えないこと。 */
  .btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-2);
    min-height: 40px;
    padding: 0 var(--space-5);
    border: none;
    border-radius: var(--md-sys-shape-corner-full);
    font: var(--md-sys-typescale-label-lg);
    letter-spacing: var(--md-sys-typescale-label-lg-tracking);
    cursor: pointer;
    white-space: nowrap;
  }

  .btn.full {
    width: 100%;
  }

  .btn:disabled {
    cursor: default;
    opacity: 0.38;
  }

  .filled {
    background: var(--md-sys-color-primary);
    color: var(--md-sys-color-on-primary);
  }

  .filled.danger {
    background: var(--md-sys-color-error);
    color: var(--md-sys-color-on-error);
  }

  .tonal {
    background: var(--md-sys-color-primary-container);
    color: var(--md-sys-color-on-primary-container);
  }

  .tonal.danger {
    background: var(--md-sys-color-error-container);
    color: var(--md-sys-color-on-error-container);
  }

  .outlined {
    background: transparent;
    color: var(--md-sys-color-primary);
    border: 1px solid var(--md-sys-color-outline);
  }

  .outlined.danger {
    color: var(--md-sys-color-error);
  }

  .text {
    background: transparent;
    color: var(--md-sys-color-primary);
    padding: 0 var(--space-3);
  }

  .text.danger {
    color: var(--md-sys-color-error);
  }

  .icon {
    font-size: 16px;
    line-height: 1;
  }
</style>
```

- [ ] **Step 4: `IconButton.svelte` を書く**

`gui-frontend/src/lib/ui/IconButton.svelte`:

```svelte
<script lang="ts">
  interface Props {
    variant?: "standard" | "filled";
    /** トグルとして使う。true のとき aria-pressed を出す */
    toggle?: boolean;
    pressed?: boolean;
    /** アイコンしか出ないので必須。aria-label と title の両方に使う */
    label: string;
    icon: string;
    disabled?: boolean;
    onclick?: (event: MouseEvent) => void;
  }

  let {
    variant = "standard",
    toggle = false,
    pressed = false,
    label,
    icon,
    disabled = false,
    onclick,
  }: Props = $props();
</script>

<button
  class="icon-btn state-layer {variant}"
  class:on={toggle && pressed}
  aria-label={label}
  aria-pressed={toggle ? pressed : undefined}
  title={label}
  {disabled}
  {onclick}
>
  <span aria-hidden="true">{icon}</span>
</button>

<style>
  .icon-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 40px;
    height: 40px;
    flex-shrink: 0;
    border: none;
    border-radius: var(--md-sys-shape-corner-full);
    font-size: 18px;
    line-height: 1;
    cursor: pointer;
  }

  .icon-btn:disabled {
    cursor: default;
    opacity: 0.38;
  }

  .standard {
    background: transparent;
    color: var(--md-sys-color-on-surface-variant);
  }

  .standard.on {
    color: var(--md-sys-color-primary);
  }

  .filled {
    background: var(--md-sys-color-surface-container-high);
    color: var(--md-sys-color-on-surface-variant);
  }

  .filled.on {
    background: var(--md-sys-color-primary);
    color: var(--md-sys-color-on-primary);
  }
</style>
```

- [ ] **Step 5: `Card.svelte` を書く**

`gui-frontend/src/lib/ui/Card.svelte`:

```svelte
<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    /** elevation の段。面の明度差が主、影が従（spec §1-5） */
    level?: 0 | 1 | 2 | 3;
    /** 既定は --space-4。行の並びなど余白を持たせたくない用途で "0" を渡す */
    padding?: string;
    /** 付けると title-sm の見出しが出る。パネル内のグループ分け用 */
    title?: string;
    children: Snippet;
  }

  let { level = 1, padding = "var(--space-4)", title, children }: Props = $props();
</script>

<section class="card level-{level}" style="padding: {padding};">
  {#if title}
    <h3 class="card-title">{title}</h3>
  {/if}
  {@render children()}
</section>

<style>
  .card {
    border-radius: var(--md-sys-shape-corner-md);
    color: var(--md-sys-color-on-surface);
    font: var(--md-sys-typescale-body-md);
  }

  .level-0 {
    background: var(--md-sys-elevation-surface-0);
    box-shadow: var(--md-sys-elevation-shadow-0);
  }

  .level-1 {
    background: var(--md-sys-elevation-surface-1);
    box-shadow: var(--md-sys-elevation-shadow-1);
  }

  .level-2 {
    background: var(--md-sys-elevation-surface-2);
    box-shadow: var(--md-sys-elevation-shadow-2);
  }

  .level-3 {
    background: var(--md-sys-elevation-surface-3);
    box-shadow: var(--md-sys-elevation-shadow-3);
  }

  .card-title {
    margin: 0 0 var(--space-3);
    font: var(--md-sys-typescale-title-sm);
    color: var(--md-sys-color-on-surface-variant);
  }
</style>
```

- [ ] **Step 6: ギャラリーのエントリを作る**

`gui-frontend/gallery.html`（**プロジェクトルート直下**。vite dev はルート直下の
任意の `.html` をそのまま配信する）:

```html
<!doctype html>
<html lang="ja">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Picture Tool — 部品ギャラリー</title>
  </head>
  <body>
    <div id="gallery"></div>
    <script type="module" src="/src/gallery.ts"></script>
  </body>
</html>
```

`gui-frontend/src/gallery.ts`:

```ts
/**
 * 部品確認用エントリ（dev 専用）。`vite dev` から /gallery.html で開く。
 *
 * vite.config.ts の build.rollupOptions.input には追加しないこと。
 * 追加すると dist/gallery.html とその chunk が生成され、frontendDist ごと
 * アプリのリソースに同梱されてしまう（spec §6）。
 */
import "./styles/tokens.css";
import "./app.css";
import Gallery from "./Gallery.svelte";
import { mount } from "svelte";

export default mount(Gallery, { target: document.getElementById("gallery")! });
```

`gui-frontend/src/Gallery.svelte`:

```svelte
<script lang="ts">
  import Button from "./lib/ui/Button.svelte";
  import IconButton from "./lib/ui/IconButton.svelte";
  import Card from "./lib/ui/Card.svelte";

  /**
   * ここだけは data-theme を手で切り替える。
   * アプリ本体には手動切替を入れない（spec §1-7）が、ギャラリーは dev 専用で
   * ビルド成果物に入らないため、明暗を並べて見るためにこの経路を使う。
   * tokens.css の 4 ブロック構造が両方向に効くことの確認も兼ねる。
   */
  let theme = $state<"system" | "light" | "dark">("system");

  $effect(() => {
    if (theme === "system") delete document.documentElement.dataset.theme;
    else document.documentElement.dataset.theme = theme;
  });

  /** each の式に `as const` を直接書くと Svelte の each 構文の `as` と衝突する。
      配列はスクリプト側で定義しておく。 */
  const THEMES = ["system", "light", "dark"] as const;
  const LEVELS = [0, 1, 2, 3] as const;

  let toggled = $state(false);
</script>

<div class="gallery">
  <header>
    <h1>部品ギャラリー</h1>
    <div class="theme-switch" role="group" aria-label="テーマ">
      {#each THEMES as t (t)}
        <button class:active={theme === t} onclick={() => (theme = t)}>{t}</button>
      {/each}
    </div>
  </header>

  <section class="specimen" data-specimen="Button">
    <h2>Button</h2>
    <div class="row">
      <Button variant="filled">filled</Button>
      <Button variant="tonal">tonal</Button>
      <Button variant="outlined">outlined</Button>
      <Button variant="text">text</Button>
    </div>
    <div class="row">
      <Button variant="filled" danger>filled danger</Button>
      <Button variant="tonal" danger>tonal danger</Button>
      <Button variant="outlined" danger>outlined danger</Button>
      <Button variant="text" danger>text danger</Button>
    </div>
    <div class="row">
      <Button variant="filled" disabled>filled disabled</Button>
      <Button variant="outlined" disabled>outlined disabled</Button>
      <Button variant="filled" icon="▶">icon 付き</Button>
    </div>
    <div class="row block">
      <Button variant="filled" full>full width</Button>
    </div>
  </section>

  <section class="specimen" data-specimen="IconButton">
    <h2>IconButton</h2>
    <div class="row">
      <IconButton label="閉じる" icon="✕" />
      <IconButton variant="filled" label="削除" icon="🗑" />
      <IconButton label="切替" icon="★" toggle pressed={toggled}
        onclick={() => (toggled = !toggled)} />
      <IconButton variant="filled" label="切替（塗り）" icon="★" toggle pressed={toggled}
        onclick={() => (toggled = !toggled)} />
      <IconButton label="無効" icon="✕" disabled />
    </div>
  </section>

  <section class="specimen" data-specimen="Card">
    <h2>Card</h2>
    <div class="row">
      {#each LEVELS as level (level)}
        <Card {level} title="level {level}">
          <p>面の明度差が主、影が従。</p>
        </Card>
      {/each}
    </div>
  </section>
</div>

<style>
  .gallery {
    min-height: 100vh;
    padding: var(--space-5);
    background: var(--md-sys-color-surface);
    color: var(--md-sys-color-on-surface);
    font: var(--md-sys-typescale-body-md);
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: var(--space-5);
  }

  h1 {
    font: var(--md-sys-typescale-title-md);
    margin: 0;
  }

  h2 {
    font: var(--md-sys-typescale-title-sm);
    color: var(--md-sys-color-on-surface-variant);
    margin: 0 0 var(--space-3);
  }

  .theme-switch button {
    border: 1px solid var(--md-sys-color-outline);
    background: transparent;
    color: var(--md-sys-color-on-surface);
    padding: var(--space-1) var(--space-3);
    cursor: pointer;
    font: var(--md-sys-typescale-body-sm);
  }

  .theme-switch button.active {
    background: var(--md-sys-color-primary);
    color: var(--md-sys-color-on-primary);
  }

  .specimen {
    margin-bottom: var(--space-6);
  }

  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-3);
    margin-bottom: var(--space-3);
  }

  .row.block {
    display: block;
    max-width: 320px;
  }
</style>
```

- [ ] **Step 7: `test-integrity` スキルを起動する**

- [ ] **Step 8: ギャラリーの Playwright 検査を書く**

`gui-frontend/e2e/gallery.spec.ts`:

```ts
import { expect, test } from "@playwright/test";

/**
 * 部品ギャラリーの検査。
 *
 * spec §6 の段階 2 の完了の目印は「全部品 × 全 state × 明暗を目視できる」。
 * 目視は人が行うが、その前提（節が存在する・明暗の両方が実際に別の色になる）は
 * 機械で確かめられる。ここはその前提だけを見る。
 *
 * 明暗の切り替えは data-theme で行う。tokens.css の 4 ブロック構造が
 * 両方向に効かないと、ここで同じ色が返る。
 */
const SPECIMENS = ["Button", "IconButton", "Card"];

test.describe("部品ギャラリー", () => {
  test("すべての節が描画される", async ({ page }) => {
    await page.goto("/gallery.html");
    for (const name of SPECIMENS) {
      await expect(page.locator(`[data-specimen="${name}"]`)).toBeVisible();
    }
  });

  test("data-theme でライトとダークが実際に別の面色になる", async ({ page }) => {
    await page.goto("/gallery.html");

    const surfaceOf = async (theme: "light" | "dark") => {
      await page.evaluate((t) => {
        document.documentElement.dataset.theme = t;
      }, theme);
      return page.locator(".gallery").evaluate(
        (el) => getComputedStyle(el).backgroundColor
      );
    };

    const light = await surfaceOf("light");
    const dark = await surfaceOf("dark");

    // 前提条件: そもそも色が解決できていないと両方 "rgba(0, 0, 0, 0)" になり、
    // 「違う」という主張が成り立たなくなる
    expect(light).not.toBe("rgba(0, 0, 0, 0)");
    expect(dark).not.toBe("rgba(0, 0, 0, 0)");
    expect(light).not.toBe(dark);
  });

  test("明暗のスクリーンショットを撮る", async ({ page }) => {
    await page.goto("/gallery.html");
    for (const theme of ["light", "dark"] as const) {
      await page.evaluate((t) => {
        document.documentElement.dataset.theme = t;
      }, theme);
      await page.screenshot({
        path: `e2e/__screenshots__/gallery-${theme}.png`,
        fullPage: true,
      });
    }
  });
});
```

`gui-frontend/e2e/__screenshots__/` は `.gitignore` に足す。
**これは「目視のための出力」であって、ベースライン比較ではない。**
`toHaveScreenshot()` の差分比較にするならベースライン画像をコミットすることになるが、
CI は Playwright を走らせない（ブラウザバイナリが無い）ので比較する場所が無く、
フォントと GPU の差で機械ごとに落ちる画像を抱えるだけになる。
spec §7-3 の「ライト／ダーク両方でのスクリーンショット比較」は、
Task 3 Step 10 / Task 18 Step 7 の**目視**で満たす:

```
# Playwright の出力
gui-frontend/e2e/__screenshots__/
gui-frontend/test-results/
gui-frontend/playwright-report/
```

- [ ] **Step 9: 走らせる**

```bash
cd gui-frontend && bun run typecheck && bun run e2e
```

期待: 3 テストとも PASS。

- [ ] **Step 10: 目視する**

```bash
cd gui-frontend && bun run dev
```

`http://localhost:5173/gallery.html` を開き、system / light / dark の 3 つを
切り替えて全部品を見る。**hover・focus（Tab）・pressed（押しっぱなし）を
実際に触って、3 つとも見た目が変わることを確認する。**
（`::after` オーバーレイが効いていないと hover で何も起きない。）

- [ ] **Step 11: 本体のビルドに `gallery` が混ざっていないことを確認する**

```bash
cd gui-frontend && bun run build && ls dist/
```

期待: `dist/gallery.html` が**存在しない**こと。`dist/index.html` だけがあること。

- [ ] **Step 12: コミット**

```bash
git add gui-frontend/package.json gui-frontend/bun.lock gui-frontend/tsconfig.json \
        gui-frontend/playwright.config.ts gui-frontend/e2e/gallery.spec.ts \
        gui-frontend/gallery.html gui-frontend/src/gallery.ts gui-frontend/src/Gallery.svelte \
        gui-frontend/src/lib/ui/ .gitignore
git commit -m "feat(gui): Button/IconButton/Card と部品ギャラリーを追加"
```

---

## Task 4: 入力プリミティブ 5 個（段階 2 の 2/3）

spec §2。`TextField` / `Switch` / `Slider` / `Select` / `SegmentedButton`。

**Files:**
- Create: `gui-frontend/src/lib/ui/TextField.svelte`
- Create: `gui-frontend/src/lib/ui/Switch.svelte`
- Create: `gui-frontend/src/lib/ui/Slider.svelte`
- Create: `gui-frontend/src/lib/ui/Select.svelte`
- Create: `gui-frontend/src/lib/ui/SegmentedButton.svelte`
- Modify: `gui-frontend/src/Gallery.svelte`（節を 5 つ追加）
- Modify: `gui-frontend/e2e/gallery.spec.ts`（`SPECIMENS` に 5 つ追加）

**Interfaces:**
- Consumes: なし（Task 3 とは独立）
- Produces: `TextField`（`generics="T extends string | number | null"`）
  `{ value: T (bindable), label: string, type?: "text"|"number",
     multiline?: boolean, rows?: number, suffix?: string, error?: string | null,
     hint?: string | null, placeholder?: string, disabled?: boolean,
     min?: number, max?: number,
     normalize?: (v: number) => number,
     onchange?: () => void }`
  — **数値は `change`（blur / Enter）で確定し、`normalize` を通してから DOM に書き戻す。**
    `step` 属性はスピナーと HTML バリデーションにしか効かず、1002 の直接入力・
    貼り付けを防げない（現行 `SettingsPanel.commitMaxWidth` が同じ問題を解いている）
- Produces: `Switch` `{ checked: boolean (bindable), label: string, danger?: boolean, disabled?: boolean }`
- Produces: `Slider` `{ value: number (bindable), label: string, min: number, max: number,
  step?: number, suffix?: string, disabled?: boolean, format?: (v: number) => string }`
- Produces: `Select`（`generics="T extends string"`）`{ value: T (bindable), label: string,
  options: { value: T; label: string }[], disabled?: boolean }`
- Produces: `SegmentedButton`（`generics="T extends string"`）`{ value: T (bindable), label: string,
  options: { value: T; label: string; icon?: string }[], disabled?: boolean }`

- [ ] **Step 1: `TextField.svelte` を書く**

```svelte
<!--
  value をジェネリックにしてある。`bind:` は双方向なので、prop の型と
  束縛する式の型が相互に代入可能でないと svelte-check が落ちる。
  `value: string | number | null` と固定すると、`bind:value={config.max_size_mb}`
  （number）も `bind:value={title}`（string）も通らなくなる。
-->
<script lang="ts" generics="T extends string | number | null">
  interface Props {
    value: T;
    /** 可視ラベル。id は $props.id() で自動生成して label と結ぶ */
    label: string;
    type?: "text" | "number";
    multiline?: boolean;
    rows?: number;
    /** 入力欄の右端に出す固定文字（"MB" / "px" など） */
    suffix?: string;
    /** 非 null のとき error ロールで表示し aria-invalid を立てる */
    error?: string | null;
    /** 補足文。error があるときは error が優先される */
    hint?: string | null;
    placeholder?: string;
    disabled?: boolean;
    min?: number;
    max?: number;
    /**
     * type="number" の確定時に値を通す。クランプや「4 の倍数に切り捨て」など。
     * ここに寄せることで、正規化後に値が変わらなかった場合の表示ずれ
     * （1000 のときに 1002 を入力すると state が動かず表示だけ 1002 が残る）を
     * このコンポーネント側で 1 回だけ潰せる。
     */
    normalize?: (value: number) => number;
    /** 確定後に呼ばれる。value は既に更新済み */
    onchange?: () => void;
  }

  let {
    value = $bindable(),
    label,
    type = "text",
    multiline = false,
    rows = 3,
    suffix,
    error = null,
    hint = null,
    placeholder,
    disabled = false,
    min,
    max,
    normalize,
    onchange,
  }: Props = $props();

  const id = $props.id();
  const describedById = `${id}-desc`;

  let description = $derived(error ?? hint);

  // ジェネリックの実体はマークアップ側の分岐（text/multiline は string、
  // number は number|null）で決まる。その対応をここの 2 箇所のキャストに閉じ込める。
  /** 文字入力は逐次反映する */
  function handleInput(event: Event) {
    const el = event.currentTarget as HTMLInputElement | HTMLTextAreaElement;
    value = el.value as T;
  }

  /** 数値は確定時にだけ反映する。空欄は null（＝未指定）とする */
  function handleNumberChange(event: Event) {
    const el = event.currentTarget as HTMLInputElement;
    const raw = el.value.trim();
    const parsed = Number(raw);
    let next: number | null =
      raw === "" || !Number.isFinite(parsed) ? null : parsed;
    if (next !== null && normalize) next = normalize(next);
    value = next as T;
    // 正規化の結果が現在値と同じでも DOM は元の入力のままなので、明示的に戻す
    el.value = next === null ? "" : String(next);
    onchange?.();
  }
</script>

<div class="field" class:has-error={error !== null}>
  <label class="field-label" for={id}>{label}</label>
  <div class="control" class:multiline>
    {#if multiline}
      <textarea
        {id}
        {rows}
        {placeholder}
        {disabled}
        aria-invalid={error !== null}
        aria-describedby={description ? describedById : undefined}
        value={value === null ? "" : String(value)}
        oninput={handleInput}
        onchange={() => onchange?.()}
      ></textarea>
    {:else if type === "number"}
      <input
        {id}
        type="number"
        {placeholder}
        {disabled}
        {min}
        {max}
        aria-invalid={error !== null}
        aria-describedby={description ? describedById : undefined}
        value={value === null ? "" : String(value)}
        onchange={handleNumberChange}
      />
    {:else}
      <input
        {id}
        type="text"
        {placeholder}
        {disabled}
        aria-invalid={error !== null}
        aria-describedby={description ? describedById : undefined}
        value={value === null ? "" : String(value)}
        oninput={handleInput}
        onchange={() => onchange?.()}
      />
    {/if}
    {#if suffix}<span class="suffix" aria-hidden="true">{suffix}</span>{/if}
  </div>
  {#if description}
    <p class="description" id={describedById}>{description}</p>
  {/if}
</div>

<style>
  .field {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .field-label {
    font: var(--md-sys-typescale-body-sm);
    color: var(--md-sys-color-on-surface-variant);
  }

  .control {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: 0 var(--space-3);
    background: var(--md-sys-color-surface-container-highest);
    border: 1px solid var(--md-sys-color-outline);
    border-radius: var(--md-sys-shape-corner-sm);
  }

  .control.multiline {
    align-items: stretch;
    padding: var(--space-2) var(--space-3);
  }

  .has-error .control {
    border-color: var(--md-sys-color-error);
  }

  input,
  textarea {
    flex: 1;
    min-width: 0;
    background: none;
    border: none;
    padding: var(--space-2) 0;
    color: var(--md-sys-color-on-surface);
    font: var(--md-sys-typescale-body-md);
  }

  textarea {
    resize: vertical;
    padding: 0;
  }

  input:focus,
  textarea:focus {
    outline: none;
  }

  /* フォーカスは枠で示す。:focus-visible の既定リングは内側の input に
     付くと枠から浮くため、ここだけ :focus-within で外枠に寄せる */
  .control:focus-within {
    outline: var(--md-sys-state-focus-ring);
    outline-offset: var(--md-sys-state-focus-ring-offset);
    border-color: var(--md-sys-color-primary);
  }

  .suffix {
    flex-shrink: 0;
    font: var(--md-sys-typescale-body-sm);
    color: var(--md-sys-color-on-surface-variant);
  }

  .description {
    margin: 0;
    font: var(--md-sys-typescale-body-sm);
    color: var(--md-sys-color-on-surface-variant);
  }

  .has-error .description {
    color: var(--md-sys-color-error);
  }

  input:disabled,
  textarea:disabled {
    opacity: 0.38;
  }
</style>
```

- [ ] **Step 2: `Switch.svelte` を書く**

```svelte
<script lang="ts">
  interface Props {
    checked: boolean;
    label: string;
    /** 不可逆な操作のトグル（元ファイル削除）。on のとき error ロールで塗る */
    danger?: boolean;
    disabled?: boolean;
    onchange?: () => void;
  }

  let {
    checked = $bindable(),
    label,
    danger = false,
    disabled = false,
    onchange,
  }: Props = $props();
</script>

<label class="switch" class:disabled>
  <input type="checkbox" bind:checked {disabled} onchange={() => onchange?.()} />
  <span class="track state-layer" class:danger>
    <span class="thumb"></span>
  </span>
  <span class="text">{label}</span>
</label>

<style>
  .switch {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    cursor: pointer;
    font: var(--md-sys-typescale-body-md);
    color: var(--md-sys-color-on-surface);
  }

  .switch.disabled {
    cursor: default;
    opacity: 0.38;
  }

  /* ネイティブの checkbox は消さずに透明にして重ねる。
     消すとキーボード操作とフォームの意味論を自前で作り直すことになる。 */
  input {
    position: absolute;
    width: 52px;
    height: 32px;
    margin: 0;
    opacity: 0;
    cursor: inherit;
  }

  .track {
    position: relative;
    flex-shrink: 0;
    width: 52px;
    height: 32px;
    border-radius: var(--md-sys-shape-corner-full);
    background: var(--md-sys-color-surface-container-highest);
    border: 2px solid var(--md-sys-color-outline);
    color: var(--md-sys-color-on-surface-variant);
    transition: background var(--md-sys-motion-duration-short)
      var(--md-sys-motion-easing-standard);
  }

  .thumb {
    position: absolute;
    top: 50%;
    left: 6px;
    width: 16px;
    height: 16px;
    transform: translateY(-50%);
    border-radius: var(--md-sys-shape-corner-full);
    background: var(--md-sys-color-outline);
    transition: left var(--md-sys-motion-duration-short)
        var(--md-sys-motion-easing-standard),
      width var(--md-sys-motion-duration-short) var(--md-sys-motion-easing-standard),
      height var(--md-sys-motion-duration-short) var(--md-sys-motion-easing-standard),
      background var(--md-sys-motion-duration-short)
        var(--md-sys-motion-easing-standard);
  }

  input:checked ~ .track {
    background: var(--md-sys-color-primary);
    border-color: var(--md-sys-color-primary);
    color: var(--md-sys-color-on-primary);
  }

  input:checked ~ .track.danger {
    background: var(--md-sys-color-error);
    border-color: var(--md-sys-color-error);
    color: var(--md-sys-color-on-error);
  }

  input:checked ~ .track .thumb {
    left: 26px;
    width: 24px;
    height: 24px;
    background: var(--md-sys-color-on-primary);
  }

  input:checked ~ .track.danger .thumb {
    background: var(--md-sys-color-on-error);
  }

  input:focus-visible ~ .track {
    outline: var(--md-sys-state-focus-ring);
    outline-offset: var(--md-sys-state-focus-ring-offset);
  }
</style>
```

**透明な `input` と `.track` は同じ場所に重なる。上に来る（ポインタを受ける）のは `.track` の方である。**
`input` は絶対配置（静的位置＝flex の先頭＝`.track` と同じ場所）、`.track` は
`.state-layer` によって `position: relative` になる。どちらも `z-index: auto` の
positioned 要素なので、**塗りと当たり判定は DOM 順で後ろにある `.track` が上**になる。
したがって `.state-layer` の `::after` は `.track` 自身の `:hover` / `:active` で立つ。
（Chromium で実測: `.track` の中心の `elementFromPoint` は `.track`、
hover 時の `::after` の `opacity` は `0.08`。）

`:focus-visible` だけは `input` 側に立つため、上のように
`input:focus-visible ~ .track` で明示的にリングを出す。

**この重なりの帰結として、Playwright の `getByRole("checkbox", …).click()` は通らない**
（`.track intercepts pointer events` で actionability チェックに落ちる。実測済み）。
e2e から Switch を切り替えるときは `e2e/stub.ts` の `toggleSwitch()`（Task 9 Step 7）を
使い、可視ラベル側をクリックすること。**`input` に `pointer-events: none` を足して
「直す」のは誤り** — そうすると今度は `.track:hover` が立たなくなり、状態レイヤーが死ぬ。

- [ ] **Step 3: `Slider.svelte` を書く**

```svelte
<script lang="ts">
  interface Props {
    value: number;
    label: string;
    min: number;
    max: number;
    step?: number;
    /** 現在値の後ろに出す単位（"%" / "px" など） */
    suffix?: string;
    /** 値の見せ方を変えたいとき（フレーム文字サイズの 0.025 → "2.5"） */
    format?: (value: number) => string;
    disabled?: boolean;
  }

  let {
    value = $bindable(),
    label,
    min,
    max,
    step = 1,
    suffix = "",
    format,
    disabled = false,
  }: Props = $props();

  const id = $props.id();
  let display = $derived((format ? format(value) : String(value)) + suffix);
</script>

<div class="slider">
  <div class="head">
    <label for={id}>{label}</label>
    <span class="value">{display}</span>
  </div>
  <input {id} type="range" {min} {max} {step} {disabled} bind:value />
</div>

<style>
  .slider {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .head {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: var(--space-2);
    font: var(--md-sys-typescale-body-sm);
    color: var(--md-sys-color-on-surface-variant);
  }

  .value {
    color: var(--md-sys-color-on-surface);
    font-variant-numeric: tabular-nums;
  }

  input {
    width: 100%;
    height: 20px;
    margin: 0;
    padding: 0;
    background: transparent;
    -webkit-appearance: none;
    appearance: none;
    cursor: pointer;
  }

  input:disabled {
    cursor: default;
    opacity: 0.38;
  }

  /* WebKit / Blink（WebKitGTK も含む） */
  input::-webkit-slider-runnable-track {
    height: 4px;
    border-radius: var(--md-sys-shape-corner-full);
    background: var(--md-sys-color-surface-container-highest);
  }

  input::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: 16px;
    height: 16px;
    margin-top: -6px;
    border: none;
    border-radius: var(--md-sys-shape-corner-full);
    background: var(--md-sys-color-primary);
  }

  /* Gecko（開発時のブラウザ差を埋めるために残す） */
  input::-moz-range-track {
    height: 4px;
    border-radius: var(--md-sys-shape-corner-full);
    background: var(--md-sys-color-surface-container-highest);
  }

  input::-moz-range-thumb {
    width: 16px;
    height: 16px;
    border: none;
    border-radius: var(--md-sys-shape-corner-full);
    background: var(--md-sys-color-primary);
  }
</style>
```

- [ ] **Step 4: `Select.svelte` を書く**

```svelte
<!-- SegmentedButton と同じ理由でジェネリック。
     bind:value={config.position}（ExifPosition）を受けられるようにする -->
<script lang="ts" generics="T extends string">
  interface Props {
    value: T;
    label: string;
    options: { value: T; label: string }[];
    disabled?: boolean;
    onchange?: () => void;
  }

  let {
    value = $bindable(),
    label,
    options,
    disabled = false,
    onchange,
  }: Props = $props();

  const id = $props.id();
</script>

<div class="select">
  <label for={id}>{label}</label>
  <div class="control">
    <select {id} {disabled} bind:value onchange={() => onchange?.()}>
      {#each options as option (option.value)}
        <option value={option.value}>{option.label}</option>
      {/each}
    </select>
    <span class="arrow" aria-hidden="true">▾</span>
  </div>
</div>

<style>
  .select {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  label {
    font: var(--md-sys-typescale-body-sm);
    color: var(--md-sys-color-on-surface-variant);
  }

  .control {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: 0 var(--space-3);
    background: var(--md-sys-color-surface-container-highest);
    border: 1px solid var(--md-sys-color-outline);
    border-radius: var(--md-sys-shape-corner-sm);
  }

  .control:focus-within {
    outline: var(--md-sys-state-focus-ring);
    outline-offset: var(--md-sys-state-focus-ring-offset);
    border-color: var(--md-sys-color-primary);
  }

  select {
    flex: 1;
    min-width: 0;
    padding: var(--space-2) 0;
    background: none;
    border: none;
    color: var(--md-sys-color-on-surface);
    font: var(--md-sys-typescale-body-md);
    -webkit-appearance: none;
    appearance: none;
  }

  select:focus {
    outline: none;
  }

  select:disabled {
    opacity: 0.38;
  }

  .arrow {
    flex-shrink: 0;
    color: var(--md-sys-color-on-surface-variant);
  }
</style>
```

- [ ] **Step 5: `SegmentedButton.svelte` を書く**

```svelte
<!-- value をジェネリックにしないと bind:value={config.mode}
     （"crop"|"pad"|"quality"）が svelte-check で落ちる -->
<script lang="ts" generics="T extends string">
  interface Props {
    value: T;
    /** グループのラベル。可視ラベルは親側が置く前提で aria-label に使う */
    label: string;
    options: { value: T; label: string; icon?: string }[];
    disabled?: boolean;
  }

  let { value = $bindable(), label, options, disabled = false }: Props = $props();

  /**
   * ネイティブの radio を隠して重ねる。
   * button + aria-pressed で組むと、矢印キーでの移動とグループの
   * 意味論を自前で作り直すことになる（ブラウザが radio に対して既にやっている）。
   */
  const groupName = $props.id();
</script>

<div class="segmented" role="radiogroup" aria-label={label}>
  {#each options as option (option.value)}
    <label class="segment state-layer" class:selected={value === option.value}>
      <input
        type="radio"
        name={groupName}
        value={option.value}
        bind:group={value}
        {disabled}
      />
      {#if option.icon}<span class="icon" aria-hidden="true">{option.icon}</span>{/if}
      <span class="text">{option.label}</span>
    </label>
  {/each}
</div>

<style>
  .segmented {
    display: flex;
    border: 1px solid var(--md-sys-color-outline);
    border-radius: var(--md-sys-shape-corner-full);
    overflow: hidden;
  }

  .segment {
    flex: 1;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-1);
    min-height: 36px;
    padding: 0 var(--space-3);
    cursor: pointer;
    font: var(--md-sys-typescale-label-lg);
    letter-spacing: var(--md-sys-typescale-label-lg-tracking);
    color: var(--md-sys-color-on-surface);
    white-space: nowrap;
  }

  .segment + .segment {
    border-left: 1px solid var(--md-sys-color-outline);
  }

  .segment.selected {
    background: var(--md-sys-color-primary-container);
    color: var(--md-sys-color-on-primary-container);
  }

  input {
    position: absolute;
    opacity: 0;
    pointer-events: none;
  }

  input:focus-visible ~ .text,
  input:focus-visible ~ .icon {
    outline: var(--md-sys-state-focus-ring);
    outline-offset: var(--md-sys-state-focus-ring-offset);
  }

  input:disabled ~ .text {
    opacity: 0.38;
  }
</style>
```

- [ ] **Step 6: ギャラリーに 5 節を足す**

`Gallery.svelte` の `<script>` に状態を足す:

```ts
  let text = $state("キャプション");
  let comment = $state("複数行のコメント\n2 行目");
  let numeric = $state<number | null>(1080);
  let toggleOn = $state(true);
  let dangerOn = $state(false);
  let quality = $state(90);
  let selected = $state("crop");
  let bg = $state("white");
  let font = $state("");
```

`Card` の節の後ろに追加する:

```svelte
  <section class="specimen" data-specimen="TextField">
    <h2>TextField</h2>
    <div class="row grid">
      <TextField bind:value={text} label="タイトル" placeholder="未設定" />
      <TextField bind:value={numeric} label="出力幅の上限" type="number" suffix="px"
        min={4} max={20000} normalize={(v) => Math.floor(Math.min(Math.max(v, 4), 20000) / 4) * 4}
        hint="4 の倍数へ切り捨てる" />
      <TextField bind:value={text} label="エラー状態" error="値が範囲外です" />
      <TextField bind:value={text} label="無効" disabled />
      <TextField bind:value={comment} label="コメント" multiline rows={4} />
    </div>
  </section>

  <section class="specimen" data-specimen="Switch">
    <h2>Switch</h2>
    <div class="row">
      <Switch bind:checked={toggleOn} label="Exif フレーム" />
      <Switch bind:checked={dangerOn} label="元ファイルを削除" danger />
      <Switch bind:checked={toggleOn} label="無効" disabled />
    </div>
  </section>

  <section class="specimen" data-specimen="Slider">
    <h2>Slider</h2>
    <div class="row grid">
      <Slider bind:value={quality} label="品質" min={1} max={100} suffix="%" />
      <Slider bind:value={quality} label="無効" min={1} max={100} disabled />
    </div>
  </section>

  <section class="specimen" data-specimen="Select">
    <h2>Select</h2>
    <div class="row grid">
      <Select bind:value={font} label="フォント"
        options={[{ value: "", label: "同梱フォント" }, { value: "/a.ttf", label: "Noto Sans JP" }]} />
      <Select bind:value={font} label="無効" disabled
        options={[{ value: "", label: "同梱フォント" }]} />
    </div>
  </section>

  <section class="specimen" data-specimen="SegmentedButton">
    <h2>SegmentedButton</h2>
    <div class="row grid">
      <SegmentedButton bind:value={selected} label="変換モード"
        options={[
          { value: "crop", label: "Crop" },
          { value: "pad", label: "Pad" },
          { value: "quality", label: "Quality" },
        ]} />
      <SegmentedButton bind:value={bg} label="背景色"
        options={[{ value: "white", label: "白" }, { value: "black", label: "黒" }]} />
    </div>
  </section>
```

`.row.grid` のスタイルを `<style>` に足す:

```css
  .row.grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    align-items: start;
  }
```

import も 5 行足すこと。

- [ ] **Step 7: `e2e/gallery.spec.ts` の `SPECIMENS` を更新する**

```ts
const SPECIMENS = [
  "Button",
  "IconButton",
  "Card",
  "TextField",
  "Switch",
  "Slider",
  "Select",
  "SegmentedButton",
];
```

- [ ] **Step 8: 数値入力の表示ずれが起きないことを検査する**

`gui-frontend/e2e/gallery.spec.ts` に追加する。
（spec §5-1 の「出力幅」で実際に踏む挙動。現行 `SettingsPanel` が
`commitMaxWidth` で潰しているのと同じ罠を、プリミティブ側で 1 回だけ潰す。）

```ts
test("TextField(number) は正規化の結果が同値でも表示を戻す", async ({ page }) => {
  await page.goto("/gallery.html");
  const input = page.getByLabel("出力幅の上限");

  // 前提条件: 初期値が 1080（4 の倍数）であること。ここが崩れていると
  // 「1002 を入れても 1000 に戻る」という主張の土台が消える
  await expect(input).toHaveValue("1080");

  // 1004 → 正規化で 1004（4 の倍数なのでそのまま）
  await input.fill("1004");
  await input.blur();
  await expect(input).toHaveValue("1004");

  // 1006 → 正規化で 1004。state は 1004 のまま動かないが、表示は戻る
  await input.fill("1006");
  await input.blur();
  await expect(input).toHaveValue("1004");
});
```

- [ ] **Step 9: 走らせる**

```bash
cd gui-frontend && bun run typecheck && bun run e2e
```

期待: 全 PASS。

- [ ] **Step 10: 目視する**

`http://localhost:5173/gallery.html` を system / light / dark で開き、
5 部品それぞれの通常・hover・focus・pressed・disabled・選択中を触って確認する。
`Switch` は on/off でつまみが動くこと、`SegmentedButton` は矢印キーで
選択が移ることを確かめる。

- [ ] **Step 11: コミット**

```bash
git add gui-frontend/src/lib/ui gui-frontend/src/Gallery.svelte gui-frontend/e2e/gallery.spec.ts
git commit -m "feat(gui): TextField/Switch/Slider/Select/SegmentedButton を追加"
```

---

## Task 5: 残りのプリミティブ 3 個（段階 2 の 3/3）

spec §2。`Rating` / `LinearProgress` / `Dialog`。**これで 11 個が揃い、打ち止め。**

**Files:**
- Create: `gui-frontend/src/lib/ui/Rating.svelte`
- Create: `gui-frontend/src/lib/ui/LinearProgress.svelte`
- Create: `gui-frontend/src/lib/ui/Dialog.svelte`
- Modify: `gui-frontend/src/Gallery.svelte`, `gui-frontend/e2e/gallery.spec.ts`

**Interfaces:**
- Consumes: `src/lib/focusTrap.ts` の `focusTrap`（既存・無変更）
- Produces: `Rating` `{ value: number (bindable, 0-5), label?: string, readonly?: boolean, disabled?: boolean }`
  — ★の再クリックで 0 に戻す挙動は内部で完結し、外向きの API は他部品と同じ `bind:value`
- Produces: `LinearProgress` `{ value?: number | null, max?: number, label?: string }`
  — `value` 未指定（`null`）で indeterminate
- Produces: `Dialog`
  `{ title: string, danger?: boolean, dismissible?: boolean, initialFocus?: string,
     onClose: () => void, children: Snippet, actions?: Snippet }`
  — 表示・非表示は**親が `{#if}` で行う**（内部に `open` を持たない）。
    `onClose` は Esc と scrim クリックと ✕ で呼ばれる。実際に閉じるのは親の責務

- [ ] **Step 1: `Rating.svelte` を書く**

```svelte
<script lang="ts">
  interface Props {
    /** 0〜5。0 は「未設定」 */
    value: number;
    label?: string;
    readonly?: boolean;
    disabled?: boolean;
  }

  let {
    value = $bindable(),
    label = "レーティング",
    readonly = false,
    disabled = false,
  }: Props = $props();

  const STARS = [1, 2, 3, 4, 5];

  let locked = $derived(readonly || disabled);

  /** 同じ★をもう一度押したら 0 に戻す（spec §2） */
  function pick(star: number) {
    if (locked) return;
    value = value === star ? 0 : star;
  }

  function clamp(next: number) {
    if (locked) return;
    value = Math.min(5, Math.max(0, next));
  }

  function handleKeydown(event: KeyboardEvent) {
    switch (event.key) {
      case "ArrowRight":
      case "ArrowUp":
        event.preventDefault();
        clamp(value + 1);
        break;
      case "ArrowLeft":
      case "ArrowDown":
        event.preventDefault();
        clamp(value - 1);
        break;
      case "Home":
        event.preventDefault();
        clamp(0);
        break;
      case "End":
        event.preventDefault();
        clamp(5);
        break;
    }
  }
</script>

<!-- role="slider" にするのは、0（未設定）を含む 0〜5 の連続量であり、
     radiogroup では「どれも選ばれていない」を表現できないため。 -->
<div
  class="rating"
  class:locked
  role="slider"
  aria-label={label}
  aria-valuemin={0}
  aria-valuemax={5}
  aria-valuenow={value}
  aria-valuetext={value === 0 ? "未設定" : `${value} / 5`}
  aria-readonly={readonly || undefined}
  aria-disabled={disabled || undefined}
  tabindex={locked ? -1 : 0}
  onkeydown={handleKeydown}
>
  {#each STARS as star (star)}
    <!-- aria-hidden の中にフォーカスを入れないため、mousedown の既定動作
         （クリックした要素へのフォーカス）を止める。支援技術に見えているのは
         親の role="slider" だけで、★は装飾として扱う -->
    <button
      class="star"
      class:filled={star <= value}
      type="button"
      tabindex="-1"
      aria-hidden="true"
      disabled={locked}
      onmousedown={(e) => e.preventDefault()}
      onclick={() => pick(star)}
    >★</button>
  {/each}
</div>

<style>
  .rating {
    display: inline-flex;
    gap: var(--space-1);
    border-radius: var(--md-sys-shape-corner-xs);
  }

  .rating.locked {
    opacity: 0.6;
  }

  .star {
    background: none;
    border: none;
    padding: 0;
    font-size: 22px;
    line-height: 1;
    cursor: pointer;
    color: var(--md-sys-color-outline-variant);
    transition: color var(--md-sys-motion-duration-short)
      var(--md-sys-motion-easing-standard);
  }

  .star.filled {
    color: var(--md-sys-color-primary);
  }

  .star:disabled {
    cursor: default;
  }
</style>
```

★は `aria-hidden` かつ `tabindex="-1"`。**支援技術に見えるのは外側の slider 1 つだけ**で、
星 5 個が別々に読み上げられない。値の読み上げは `aria-valuetext` が担う。

- [ ] **Step 2: `LinearProgress.svelte` を書く**

```svelte
<script lang="ts">
  interface Props {
    /** null（既定）で indeterminate */
    value?: number | null;
    max?: number;
    label?: string;
  }

  let { value = null, max = 100, label = "進捗" }: Props = $props();

  let percent = $derived(
    value === null || max <= 0 ? 0 : Math.min(100, Math.max(0, (value / max) * 100))
  );
</script>

<div
  class="track"
  role="progressbar"
  aria-label={label}
  aria-valuemin={value === null ? undefined : 0}
  aria-valuemax={value === null ? undefined : max}
  aria-valuenow={value === null ? undefined : value}
>
  {#if value === null}
    <div class="bar indeterminate"></div>
  {:else}
    <div class="bar" style="width: {percent}%"></div>
  {/if}
</div>

<style>
  .track {
    position: relative;
    width: 100%;
    height: 4px;
    overflow: hidden;
    border-radius: var(--md-sys-shape-corner-full);
    background: var(--md-sys-color-surface-container-highest);
  }

  .bar {
    height: 100%;
    border-radius: inherit;
    background: var(--md-sys-color-primary);
    transition: width var(--md-sys-motion-duration-medium)
      var(--md-sys-motion-easing-standard);
  }

  .bar.indeterminate {
    position: absolute;
    inset-block: 0;
    width: 40%;
    animation: slide 1.4s var(--md-sys-motion-easing-standard) infinite;
  }

  @keyframes slide {
    from {
      left: -40%;
    }
    to {
      left: 100%;
    }
  }
</style>
```

`prefers-reduced-motion: reduce` のときは `tokens.css` のグローバル規則が
`animation-duration` を潰すので、indeterminate は動かない棒になる。これで正しい。

- [ ] **Step 3: `Dialog.svelte` を書く**

```svelte
<script lang="ts">
  import type { Snippet } from "svelte";
  import { focusTrap } from "../focusTrap";

  interface Props {
    title: string;
    /** 破壊的操作の確認。alertdialog にし、初期フォーカスをキャンセル側へ置く */
    danger?: boolean;
    /** false なら scrim クリックと Esc で閉じない（進行中の処理など） */
    dismissible?: boolean;
    /**
     * 初期フォーカスを当てる要素の CSS セレクタ（ダイアログ内を querySelector する）。
     * 既定はダイアログ自身。最初のボタンに当てると Space / Enter が
     * キーハンドラーとボタン既定動作の両方を発火させてしまう。
     *
     * 破壊的操作では `"footer button"` を渡す。actions snippet の最初のボタンが
     * キャンセル、という並び順を守れば、これでフォーカスが安全側に落ちる。
     * **フォーカス可能な要素そのものを指すセレクタにすること**（ラッパーの
     * span などを指すと focusTrap の focus() が効かない）。
     */
    initialFocus?: string;
    onClose: () => void;
    children: Snippet;
    actions?: Snippet;
  }

  let {
    title,
    danger = false,
    dismissible = true,
    initialFocus,
    onClose,
    children,
    actions,
  }: Props = $props();

  const titleId = $props.id();

  function handleKeydown(event: KeyboardEvent) {
    if (event.key !== "Escape" || !dismissible) return;
    event.preventDefault();
    onClose();
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="scrim">
  <!-- 余白クリックで閉じるための背景。ダイアログ本体にクリックハンドラーを
       付けるとキーボード操作を持たない対話要素になるため分離する。 -->
  {#if dismissible}
    <div class="backdrop" role="presentation" onclick={onClose}></div>
  {/if}

  <div
    class="dialog"
    role={danger ? "alertdialog" : "dialog"}
    aria-modal="true"
    aria-labelledby={titleId}
    tabindex="-1"
    use:focusTrap={initialFocus}
  >
    <header>
      <h2 id={titleId}>{title}</h2>
      {#if dismissible}
        <button class="close state-layer" type="button" aria-label="閉じる" onclick={onClose}>✕</button>
      {/if}
    </header>

    <div class="body">
      {@render children()}
    </div>

    {#if actions}
      <footer>
        {@render actions()}
      </footer>
    {/if}
  </div>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 500;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .backdrop {
    position: absolute;
    inset: 0;
    background: var(--md-sys-color-scrim);
    opacity: 0.5;
  }

  .dialog {
    position: relative;
    display: flex;
    flex-direction: column;
    width: 90vw;
    max-width: 560px;
    max-height: 85vh;
    background: var(--md-sys-elevation-surface-3);
    color: var(--md-sys-color-on-surface);
    border-radius: var(--md-sys-shape-corner-lg);
    box-shadow: var(--md-sys-elevation-shadow-3);
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    padding: var(--space-5) var(--space-5) var(--space-3);
  }

  h2 {
    margin: 0;
    font: var(--md-sys-typescale-title-md);
  }

  .close {
    width: 32px;
    height: 32px;
    flex-shrink: 0;
    border: none;
    border-radius: var(--md-sys-shape-corner-full);
    background: none;
    color: var(--md-sys-color-on-surface-variant);
    cursor: pointer;
    font-size: 14px;
  }

  .body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 0 var(--space-5);
    font: var(--md-sys-typescale-body-md);
  }

  footer {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
    padding: var(--space-4) var(--space-5) var(--space-5);
  }
</style>
```

- [ ] **Step 4: ギャラリーに 3 節を足す**

`<script>` に:

```ts
  let rating = $state(3);
  let dialogOpen = $state(false);
  let dangerDialogOpen = $state(false);
```

節:

```svelte
  <section class="specimen" data-specimen="Rating">
    <h2>Rating</h2>
    <div class="row">
      <Rating bind:value={rating} />
      <Rating value={4} readonly />
      <Rating value={0} disabled />
      <span>value = {rating}</span>
    </div>
  </section>

  <section class="specimen" data-specimen="LinearProgress">
    <h2>LinearProgress</h2>
    <div class="row grid">
      <LinearProgress value={0} />
      <LinearProgress value={40} />
      <LinearProgress value={100} />
      <LinearProgress />
    </div>
  </section>

  <section class="specimen" data-specimen="Dialog">
    <h2>Dialog</h2>
    <div class="row">
      <Button onclick={() => (dialogOpen = true)}>通常のダイアログ</Button>
      <Button variant="filled" danger onclick={() => (dangerDialogOpen = true)}>
        危険なダイアログ
      </Button>
    </div>
  </section>

{#if dialogOpen}
  <Dialog title="変換結果" onClose={() => (dialogOpen = false)}>
    <p>本文。Card を並べて中身を作る。</p>
    {#snippet actions()}
      <Button variant="text" onclick={() => (dialogOpen = false)}>閉じる</Button>
    {/snippet}
  </Dialog>
{/if}

{#if dangerDialogOpen}
  <Dialog
    title="元ファイルを削除します"
    danger
    initialFocus="footer button"
    onClose={() => (dangerDialogOpen = false)}
  >
    <p>削除したファイルはゴミ箱に入らず、元に戻せません。</p>
    {#snippet actions()}
      <Button variant="text" onclick={() => (dangerDialogOpen = false)}>キャンセル</Button>
      <Button variant="filled" danger onclick={() => (dangerDialogOpen = false)}>
        削除して変換
      </Button>
    {/snippet}
  </Dialog>
{/if}
```

`{#if dialogOpen}` のブロックは `.gallery` の**外側**（`</div>` の後）に置く。
中に入れるとギャラリーのスクロールに巻き込まれる。

- [ ] **Step 5: `test-integrity` スキルを起動する**

- [ ] **Step 6: `Dialog` と `Rating` の挙動を検査する**

`e2e/gallery.spec.ts` に追加。`SPECIMENS` にも `"Rating"`, `"LinearProgress"`, `"Dialog"` を足す。

```ts
test("Dialog は Esc で閉じ、フォーカスを元の場所へ返す", async ({ page }) => {
  await page.goto("/gallery.html");
  const opener = page.getByRole("button", { name: "通常のダイアログ" });
  await opener.click();

  const dialog = page.getByRole("dialog", { name: "変換結果" });
  await expect(dialog).toBeVisible();

  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
  // focusTrap の destroy が元のフォーカスへ戻すこと
  await expect(opener).toBeFocused();
});

test("Dialog のフォーカスは中に閉じ込められる", async ({ page }) => {
  await page.goto("/gallery.html");
  await page.getByRole("button", { name: "通常のダイアログ" }).click();

  const inside = page.getByRole("dialog", { name: "変換結果" });
  // 前提条件: ダイアログの外にもフォーカス可能な要素が存在すること。
  // 存在しなければ「外へ出ない」は自明に成立してしまう
  expect(await page.getByRole("button", { name: "危険なダイアログ" }).count()).toBe(1);

  for (let i = 0; i < 8; i++) {
    await page.keyboard.press("Tab");
    expect(await inside.evaluate((el) => el.contains(document.activeElement))).toBe(true);
  }
});

test("Rating は同じ★の再クリックで 0 に戻る", async ({ page }) => {
  await page.goto("/gallery.html");
  const rating = page.getByRole("slider", { name: "レーティング" }).first();

  // 前提条件: 初期値が 3
  await expect(rating).toHaveAttribute("aria-valuenow", "3");

  await rating.locator("button").nth(2).click(); // 3 番目の★ = 現在値と同じ
  await expect(rating).toHaveAttribute("aria-valuenow", "0");

  await rating.locator("button").nth(4).click(); // 5 番目の★
  await expect(rating).toHaveAttribute("aria-valuenow", "5");
});

test("Rating は矢印キーで増減し 0〜5 で止まる", async ({ page }) => {
  await page.goto("/gallery.html");
  const rating = page.getByRole("slider", { name: "レーティング" }).first();
  await rating.focus();

  await page.keyboard.press("End");
  await expect(rating).toHaveAttribute("aria-valuenow", "5");
  await page.keyboard.press("ArrowRight");
  await expect(rating).toHaveAttribute("aria-valuenow", "5");

  await page.keyboard.press("Home");
  await expect(rating).toHaveAttribute("aria-valuenow", "0");
  await page.keyboard.press("ArrowLeft");
  await expect(rating).toHaveAttribute("aria-valuenow", "0");
});
```

- [ ] **Step 7: 走らせる**

```bash
cd gui-frontend && bun run typecheck && bun run e2e
```

- [ ] **Step 8: 段階 2 の完了確認（目視）**

`http://localhost:5173/gallery.html` を system / light / dark で開き、
**11 部品すべて × 全 state**（通常・hover・focus・pressed・disabled・選択中・エラー）が
明暗の両方で読めることを確認する。ここが spec §6 の段階 2 の完了の目印。

- [ ] **Step 9: コミット**

```bash
git add gui-frontend/src/lib/ui gui-frontend/src/Gallery.svelte gui-frontend/e2e/gallery.spec.ts
git commit -m "feat(gui): Rating/LinearProgress/Dialog を追加しプリミティブ 11 個が揃う"
```

---

## Task 6: 既存の周辺コンポーネント 5 個の移行（段階 3）

spec §6「段階 3 — 既存の周辺コンポーネントの移行」。
この 5 個は書き直しの対象ではないが、**旧 `app.css` 変数だけを見ている**（計 71 箇所）。
放置すると、段階 1 でライトのトークンを入れた時点から最後まで、ライトテーマで黒背景のまま残る。

**この段階の目視確認では `SettingsPanel` / `ThumbnailGrid` / `SelectionList` /
`ImagePreview` / `ExifFrameSettings` が「ダークのままの島」として残る。
これは想定どおりの中間状態であって不具合ではない**（spec §6）。
Task 10 / 13 / 14 / 16 で順に解消し、Task 18 で全面が揃う。

**旧 `app.css` の変数はここでは消さない。** 上記 5 個が Task 16 まで参照し続けるため。

**Files:**
- Delete: `gui-frontend/src/lib/ConfirmDialog.svelte`
- Modify: `gui-frontend/src/lib/ResultDialog.svelte`（`Dialog` + `Card` で組み直す）
- Modify: `gui-frontend/src/lib/ProgressOverlay.svelte`（`Dialog` + `LinearProgress`）
- Modify: `gui-frontend/src/lib/Toast.svelte`（`inverse-surface` ロールで見た目のみ）
- Modify: `gui-frontend/src/lib/FolderTree.svelte`（見た目のみ。構造は維持）
- Modify: `gui-frontend/src/App.svelte`（`ConfirmDialog` の呼び出しを `Dialog` に差し替え）

**Interfaces:**
- Consumes: `ui/Dialog.svelte`, `ui/Card.svelte`, `ui/Button.svelte`, `ui/LinearProgress.svelte`
- Produces: `ResultDialog` / `ProgressOverlay` / `Toast` / `FolderTree` の props は**すべて不変**。
  呼び出し側（`App.svelte`）の変更は `ConfirmDialog` の 1 箇所だけ

- [ ] **Step 1: `ConfirmDialog` の呼び出しを `Dialog` に置き換える**

`App.svelte` の import から `ConfirmDialog` を消し、`Dialog` と `Button` を足す:

```ts
  import Dialog from "./lib/ui/Dialog.svelte";
  import Button from "./lib/ui/Button.svelte";
```

`{#if showDeleteConfirm}` のブロックを差し替える:

```svelte
{#if showDeleteConfirm}
  <!-- 破壊的操作なので alertdialog にし、初期フォーカスはキャンセル側に置く -->
  <Dialog
    title="元ファイルを削除します"
    danger
    initialFocus="footer button"
    onClose={() => (showDeleteConfirm = false)}
  >
    <p>変換に成功した {selectedImages.length} 枚の元ファイルを削除します。</p>
    <p class="dialog-detail">削除したファイルはゴミ箱に入らず、元に戻せません。</p>
    {#snippet actions()}
      <Button variant="text" onclick={() => (showDeleteConfirm = false)}>キャンセル</Button>
      <Button variant="filled" danger onclick={runProcess}>削除して変換</Button>
    {/snippet}
  </Dialog>
{/if}
```

`App.svelte` の `<style>` に追加:

```css
  .dialog-detail {
    color: var(--md-sys-color-on-surface-variant);
    font: var(--md-sys-typescale-body-sm);
  }
```

- [ ] **Step 2: `ConfirmDialog.svelte` を消す**

```bash
git rm gui-frontend/src/lib/ConfirmDialog.svelte
```

参照が残っていないことを確認する:

```bash
grep -rn "ConfirmDialog" gui-frontend/src/
```

期待: 0 件。

- [ ] **Step 3: `ResultDialog.svelte` を組み直す**

`<script>` は**そのまま残す**（`baseName` / `results` / `failed` / `accountedPaths` /
`unprocessed` / `oversized` / `warnings` / `hasIssues` の導出は変えない）。
ただし `focusTrap` の import と `handleKeydown` と `<svelte:window>` は
`Dialog` が持つので消し、代わりに import を 3 行足す:

```ts
  import Button from "./ui/Button.svelte";
  import Card from "./ui/Card.svelte";
  import Dialog from "./ui/Dialog.svelte";
```

新しいマークアップ:

```svelte
<Dialog title="変換結果" onClose={onClose}>
  <div class="summary">
    <Card level={2} padding="var(--space-3)">
      <span class="value">{results.length}</span>
      <span class="key">成功</span>
    </Card>
    <Card level={2} padding="var(--space-3)">
      <span class="value" class:danger={failed.length > 0}>{failed.length}</span>
      <span class="key">失敗</span>
    </Card>
    {#if unprocessed.length > 0}
      <Card level={2} padding="var(--space-3)">
        <span class="value">{unprocessed.length}</span>
        <span class="key">{cancelled ? "未処理" : "不明"}</span>
      </Card>
    {/if}
    <Card level={2} padding="var(--space-3)">
      <span class="value">{requested.length}</span>
      <span class="key">対象</span>
    </Card>
  </div>

  {#if !hasIssues}
    <p class="all-ok">すべて正常に変換しました。</p>
  {/if}

  {#if failed.length > 0}
    <Card level={1} title="変換できなかったファイル ({failed.length})">
      <ul>
        {#each failed as f (f.path)}
          <li>{f.name} — {f.error}</li>
        {/each}
      </ul>
    </Card>
  {/if}

  {#if unprocessed.length > 0}
    <Card
      level={1}
      title="{cancelled ? 'キャンセルにより未処理' : '結果が返らなかったファイル'} ({unprocessed.length})"
    >
      <ul>
        {#each unprocessed as img (img.path)}
          <li>{img.name}</li>
        {/each}
      </ul>
    </Card>
  {/if}

  {#if oversized.length > 0}
    <Card level={1} title="最大サイズに収まらなかったファイル ({oversized.length})">
      <p class="note">品質を下限まで下げても指定サイズを超えています。</p>
      <ul>
        {#each oversized as r (r.input_path)}
          <li>{baseName(r.input_path)} — {r.final_size_mb.toFixed(2)}MB</li>
        {/each}
      </ul>
    </Card>
  {/if}

  {#if warnings.length > 0}
    <Card level={1} title="警告 ({warnings.length})">
      <ul>
        {#each warnings as w, i (i)}
          <li>{w.file ? `${w.file} — ` : ""}{w.message}</li>
        {/each}
      </ul>
    </Card>
  {/if}

  {#snippet actions()}
    <Button variant="filled" onclick={onClose}>閉じる</Button>
  {/snippet}
</Dialog>
```

`<style>` は全面差し替え（旧 `app.css` 変数への参照を残さない）:

```css
  .summary {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(88px, 1fr));
    gap: var(--space-2);
    margin-bottom: var(--space-4);
    text-align: center;
  }

  .value {
    display: block;
    font: var(--md-sys-typescale-title-md);
    font-variant-numeric: tabular-nums;
  }

  .value.danger {
    color: var(--md-sys-color-error);
  }

  .key {
    display: block;
    font: var(--md-sys-typescale-body-sm);
    color: var(--md-sys-color-on-surface-variant);
  }

  .all-ok {
    margin: 0 0 var(--space-4);
  }

  ul {
    margin: 0;
    padding-left: var(--space-5);
  }

  li {
    font: var(--md-sys-typescale-body-sm);
    line-height: 1.7;
    overflow-wrap: anywhere;
  }

  .note {
    margin: 0 0 var(--space-2);
    font: var(--md-sys-typescale-body-sm);
    color: var(--md-sys-color-on-surface-variant);
  }
```

- [ ] **Step 4: `ProgressOverlay.svelte` を組み直す**

props（`progress` / `onCancel`）は不変。

```svelte
<script lang="ts">
  import Dialog from "./ui/Dialog.svelte";
  import Button from "./ui/Button.svelte";
  import LinearProgress from "./ui/LinearProgress.svelte";
  import type { ProgressPayload } from "./types";

  interface Props {
    progress: ProgressPayload | null;
    onCancel: () => void;
  }

  let { progress, onCancel }: Props = $props();
</script>

{#if progress}
  <!-- 変換中は Esc や scrim クリックで閉じさせない。閉じても処理は止まらず、
       進捗の見えない状態になるだけなので -->
  <Dialog title="変換中..." dismissible={false} onClose={onCancel}>
    <LinearProgress
      value={progress.total > 0 ? progress.current : null}
      max={progress.total}
      label="変換の進捗"
    />
    <div class="info">
      <span>{progress.current} / {progress.total}</span>
      <span class="file">{progress.file_name}</span>
    </div>
    {#snippet actions()}
      <Button variant="outlined" danger onclick={onCancel}>キャンセル</Button>
    {/snippet}
  </Dialog>
{/if}

<style>
  .info {
    display: flex;
    justify-content: space-between;
    gap: var(--space-3);
    margin-top: var(--space-2);
    font: var(--md-sys-typescale-body-sm);
    color: var(--md-sys-color-on-surface-variant);
  }

  .file {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
```

`total` が 0 のときに `value` を `null` にして indeterminate に落とす。
現行は 0% の棒が出るだけで「動いていない」ように見えていた。

- [ ] **Step 5: `Toast.svelte` の見た目を差し替える**

`<script>` とマークアップは**そのまま**（`toasts` / `dismissToast` / `ICON` / role 分岐）。
`<style>` だけ差し替える:

```css
  .toast-stack {
    position: fixed;
    right: var(--space-4);
    bottom: var(--space-4);
    z-index: 600;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    max-width: min(420px, calc(100vw - var(--space-6) * 2));
    pointer-events: none;
  }

  /* トーストは M3 の snackbar。面の上に置く反転色でコントラストを稼ぐ */
  .toast {
    display: flex;
    align-items: flex-start;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-4);
    border-radius: var(--md-sys-shape-corner-xs);
    background: var(--md-sys-color-inverse-surface);
    color: var(--md-sys-color-inverse-on-surface);
    font: var(--md-sys-typescale-body-md);
    box-shadow: var(--md-sys-elevation-shadow-3);
    pointer-events: auto;
  }

  /* 種別は左端の帯で示す。反転面の上では error / warning の塗りが読めないため。
     warning と success が同じ primary なのは意図した劣化である ──
     spec §1-1 は使うロールを限定していて warning / success を定義しない。
     この 2 つはアイコン（⚠ / ✓）で区別する。退行ではない */
  .toast.error {
    border-left: 4px solid var(--md-sys-color-error);
  }

  .toast.warning {
    border-left: 4px solid var(--md-sys-color-primary);
  }

  .toast.success {
    border-left: 4px solid var(--md-sys-color-primary);
  }

  .icon {
    flex-shrink: 0;
    line-height: 1.5;
  }

  .message {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .dismiss {
    flex-shrink: 0;
    background: none;
    border: none;
    color: inherit;
    opacity: 0.7;
    cursor: pointer;
    font: var(--md-sys-typescale-body-sm);
    padding: 0 var(--space-1);
    line-height: 1.5;
  }

  .dismiss:hover {
    opacity: 1;
  }
```

`.toast.error .icon` などの旧規則は消すこと（色は帯で示す方針に変えたため）。
`z-index` を 400 → 600 に上げる。`Dialog` の scrim が 500 なので、
ダイアログの上でもトーストが見えるようにする。

- [ ] **Step 6: `FolderTree.svelte` の見た目を差し替える**

`<script>` とマークアップは**一切変えない**（構造は維持）。`<style>` の中の
旧変数をトークンに置き換える。対応:

| 旧 | 新 |
|---|---|
| `var(--bg-primary)` | `var(--md-sys-color-surface)` |
| `var(--bg-secondary)` | `var(--md-sys-color-surface-container-low)` |
| `var(--bg-hover)` | **使わない。** hover は `.state-layer` に任せる |
| `var(--border-color)` | `var(--md-sys-color-outline-variant)` |
| `var(--text-primary)` | `var(--md-sys-color-on-surface)` |
| `var(--text-secondary)` / `var(--text-muted)` | `var(--md-sys-color-on-surface-variant)` |
| `var(--accent)` / `var(--accent-hover)` | `var(--md-sys-color-primary)` |
| `var(--accent-bg)` | `var(--md-sys-color-primary-container)` |
| `var(--danger)` | `var(--md-sys-color-error)` |
| `var(--radius)` | `var(--md-sys-shape-corner-sm)` |
| `var(--radius-sm)` | `var(--md-sys-shape-corner-xs)` |
| 生の px（`8px` / `12px` / `16px` …） | `var(--space-2)` / `var(--space-3)` / `var(--space-4)` |
| 生の `font-size` | `font: var(--md-sys-typescale-body-sm)` など |

`.tree-item` に `state-layer` クラスを足し、`:hover { background: ... }` を消す。
選択中の行は `background: var(--md-sys-color-primary-container);
color: var(--md-sys-color-on-primary-container);` にし、
角丸は `var(--md-sys-shape-corner-full)` にする（rail のインジケータと形を揃える）。

**`padding-left: {12 + depth * 16}px` のインライン style はそのまま残す。**
深さに比例する値なので CSS 変数では書けない。ここは構造的な寸法である。

- [ ] **Step 7: 旧変数の残りを数える**

```bash
grep -rno -- "--bg-primary\|--bg-secondary\|--bg-hover\|--border-color\|--text-primary\|--text-secondary\|--text-muted\|--accent\|--danger\|--success\|--warning\|--radius" gui-frontend/src/ | grep -v "src/app.css" | cut -d: -f1 | sort | uniq -c
```

期待: `App.svelte` / `ConfirmDialog.svelte` / `ResultDialog.svelte` /
`ProgressOverlay.svelte` / `Toast.svelte` / `FolderTree.svelte` が**消えている**こと。
残るのは `SettingsPanel.svelte` / `ThumbnailGrid.svelte` / `SelectionList.svelte` /
`ImagePreview.svelte` / `ExifFrameSettings.svelte` の 5 ファイルだけ。

- [ ] **Step 8: 型検査とビルド**

```bash
cd gui-frontend && bun run typecheck && bun run build
```

- [ ] **Step 9: 明暗の両方で目視する**

```bash
cd gui-frontend && bun run dev
```

OS のテーマをライト／ダークで切り替えて `http://localhost:5173` を開き、
**フォルダーツリーとトーストが両方のテーマで読めること**を確認する。
右カラム（`SelectionList` / `SettingsPanel`）と中央（`ThumbnailGrid`）が
ダークのまま残るのは想定どおり。

`invoke` が reject するのでトーストが出る。それが「トーストの明暗確認」に使える。

- [ ] **Step 10: コミット**

```bash
git add -A gui-frontend/src
git commit -m "refactor(gui): 周辺コンポーネント 5 個をトークン層へ移行し ConfirmDialog を Dialog に吸収"
```

---

## Task 7: `App.svelte` の分解（段階 4 の 1/3）

spec §3-5。**挙動を一切変えない純粋なリファクタ。** 見た目も操作も同じままで、
405 行の `App.svelte` から 3 つのモジュールを切り出す。

**この時点では `thumbnailQueue` の仕様は現行のまま**（FIFO・eviction なし）。
LIFO 化・`discardable` / `pinned`・LRU は Task 12 で入れる（spec §4-2）。
先に「置き場所」だけ作っておくことで、Task 12 の差分が仕様変更だけになる。

**Files:**
- Create: `gui-frontend/src/lib/browser/thumbnailQueue.svelte.ts`
- Create: `gui-frontend/src/lib/panels/presets.svelte.ts`
- Create: `gui-frontend/src/lib/panels/convertRun.svelte.ts`
- Modify: `gui-frontend/src/App.svelte`

**Interfaces:**
- Produces: `createThumbnailQueue(): ThumbnailQueue`
  - `get(path: string, maxDimension: number): string | undefined`
  - `request(path: string, maxDimension: number): void`
- Produces: `createPresetStore(): PresetStore`
  - `readonly presets: ExifFrameConfig[]`
  - `selectedName: string`（読み書き両方）
  - `readonly active: ExifFrameConfig | null`
  - `reload(): Promise<void>` / `save(preset): Promise<boolean>` / `remove(name): Promise<void>`
  - （Task 16 で `rename(from, preset): Promise<boolean>` を足す。ここでは作らない）
- Produces: `createConvertRun(): ConvertRun`
  - `readonly processing: boolean` / `readonly progress: ProgressPayload | null`
  - `readonly result: { requested: ImageEntry[]; response: ProcessBatchResponse; cancelled: boolean } | null`
  - `subscribeProgress(): () => void`（`onMount` の返り値にそのまま使える）
  - `run(images, outputFolder, config, exifFrameConfig): Promise<void>`
  - `cancel(): Promise<void>` / `dismissResult(): void`

- [ ] **Step 1: `thumbnailQueue.svelte.ts` を作る**

`gui-frontend/src/lib/browser/thumbnailQueue.svelte.ts`。
**現行 `App.svelte` から次の識別子をそのまま移す**（行番号で指定しない。
Task 6 までの変更で動いているため）: `thumbnailCache` / `thumbnailKey` /
`thumbnailFor` / `activeRequests` / `MAX_CONCURRENT` / `pendingQueue` /
`failedThumbnails` / `thumbnailErrorReported` / `processQueue` /
`handleRequestThumbnail`。

```ts
import { SvelteMap } from "svelte/reactivity";
import { getThumbnail } from "../api";
import { describeError, toast } from "../toasts.svelte";

/**
 * サムネイルの取得キューとキャッシュ。
 *
 * サムネイルは解像度ごとに別物なので `path:maxDimension` をキーにする。
 * path だけで持つと列数を変えても再取得されず、低解像度が引き伸ばされる。
 *
 * 本モジュールは Task 7 では App.svelte からの移設のみで、仕様は現行どおり
 * （FIFO・eviction なし）。LIFO 化・可視範囲による破棄・LRU 上限は
 * spec §4-2 に従って Task 12 で入れる。
 */
export interface ThumbnailQueue {
  get(path: string, maxDimension: number): string | undefined;
  request(path: string, maxDimension: number): void;
}

const MAX_CONCURRENT = 3;

function keyOf(path: string, maxDimension: number): string {
  return `${path}:${maxDimension}`;
}

export function createThumbnailQueue(): ThumbnailQueue {
  const cache = new SvelteMap<string, string>();
  const pending: { path: string; maxDimension: number }[] = [];
  /** 同一キーの失敗を繰り返し再要求しないための記録 */
  const failed = new Set<string>();

  let active = 0;
  let errorReported = false;

  function pump() {
    while (active < MAX_CONCURRENT && pending.length > 0) {
      const { path, maxDimension } = pending.shift()!;
      const key = keyOf(path, maxDimension);
      if (cache.has(key)) continue;
      active++;
      getThumbnail(path, maxDimension)
        .then((base64) => {
          cache.set(key, base64);
        })
        .catch((e) => {
          failed.add(key);
          // 1 枚ごとにトーストを出すと壊れたフォルダーで埋め尽くされるため
          // 最初の 1 件だけ通知する
          if (!errorReported) {
            errorReported = true;
            toast.error(`サムネイルを生成できない画像があります: ${describeError(e)}`);
          }
        })
        .finally(() => {
          active--;
          pump();
        });
    }
  }

  return {
    get(path, maxDimension) {
      return cache.get(keyOf(path, maxDimension));
    },
    request(path, maxDimension) {
      const key = keyOf(path, maxDimension);
      if (cache.has(key) || failed.has(key)) return;
      if (pending.some((r) => r.path === path && r.maxDimension === maxDimension)) return;
      pending.push({ path, maxDimension });
      pump();
    },
  };
}
```

- [ ] **Step 2: `presets.svelte.ts` を作る**

`gui-frontend/src/lib/panels/presets.svelte.ts`。
**現行 `App.svelte` の `reloadPresets` / `activeExifFrameConfig` /
`handleSavePreset` / `handleDeletePreset` を移す。**

```ts
import { deletePreset, listPresets, savePreset } from "../api";
import { describeError, toast } from "../toasts.svelte";
import type { ExifFrameConfig } from "../types";

/**
 * Exif フレームのプリセット一覧の唯一の保持者。
 * パネル側は props で受け取るだけで、自分では一覧を持たない。
 */
export function createPresetStore() {
  let presets = $state<ExifFrameConfig[]>([]);
  let selectedName = $state("default");

  async function reload() {
    try {
      presets = await listPresets();
      // 選択中のプリセットが消えていたら先頭へ落とす
      if (!presets.some((p) => p.name === selectedName)) {
        selectedName = presets[0]?.name ?? "default";
      }
    } catch (e) {
      toast.error(`プリセットの読み込みに失敗しました: ${describeError(e)}`);
    }
  }

  return {
    get presets() {
      return presets;
    },
    get selectedName() {
      return selectedName;
    },
    set selectedName(name: string) {
      selectedName = name;
    },
    get active(): ExifFrameConfig | null {
      return presets.find((p) => p.name === selectedName) ?? null;
    },
    reload,
    /** 保存できたら true。呼び出し側はこれを見てモードを閉じるか決める */
    async save(preset: ExifFrameConfig): Promise<boolean> {
      try {
        await savePreset(preset);
        selectedName = preset.name;
        await reload();
        toast.success(`プリセット「${preset.name}」を保存しました`);
        return true;
      } catch (e) {
        toast.error(`プリセットの保存に失敗しました: ${describeError(e)}`);
        return false;
      }
    },
    async remove(name: string): Promise<void> {
      try {
        await deletePreset(name);
        await reload();
        toast.success(`プリセット「${name}」を削除しました`);
      } catch (e) {
        toast.error(`プリセットの削除に失敗しました: ${describeError(e)}`);
      }
    },
  };
}
```

- [ ] **Step 3: `convertRun.svelte.ts` を作る**

`gui-frontend/src/lib/panels/convertRun.svelte.ts`。
**現行 `App.svelte` の進捗購読・`runProcess` / `handleCancel` と、
`processing` / `progress` / `batchResponse` / `batchRequested` / `batchCancelled` /
`cancelRequested` を移す。**

```ts
import { listen } from "@tauri-apps/api/event";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { cancelProcessing, processImages } from "../api";
import { describeError, toast } from "../toasts.svelte";
import type {
  ExifFrameConfig,
  ImageEntry,
  ProcessBatchResponse,
  ProcessingConfig,
  ProgressPayload,
} from "../types";

export interface ConvertResult {
  /** 変換を依頼した画像。キャンセル分は results にも failures にも現れない */
  requested: ImageEntry[];
  response: ProcessBatchResponse;
  /** 利用者がキャンセルした場合、未処理分は「失敗」ではないので区別する */
  cancelled: boolean;
}

export function createConvertRun() {
  let processing = $state(false);
  let progress = $state<ProgressPayload | null>(null);
  let result = $state<ConvertResult | null>(null);
  let cancelRequested = false;

  return {
    get processing() {
      return processing;
    },
    get progress() {
      return progress;
    },
    get result() {
      return result;
    },

    /** onMount の返り値にそのまま渡せるクリーンアップを返す */
    subscribeProgress(): () => void {
      let unlisten: UnlistenFn | null = null;
      let disposed = false;

      listen<ProgressPayload>("processing-progress", (event) => {
        progress = event.payload;
      })
        .then((fn) => {
          if (disposed) fn();
          else unlisten = fn;
        })
        .catch((e) => {
          toast.error(`進捗の購読に失敗しました: ${describeError(e)}`);
        });

      return () => {
        disposed = true;
        unlisten?.();
      };
    },

    async run(
      requested: ImageEntry[],
      outputFolder: string,
      config: ProcessingConfig,
      exifFrameConfig: ExifFrameConfig | null
    ): Promise<void> {
      if (processing || requested.length === 0 || outputFolder === "") return;

      processing = true;
      cancelRequested = false;
      progress = { current: 0, total: requested.length, file_name: "" };

      try {
        const files = requested.map((img) => img.path);
        const response = await processImages(files, outputFolder, config, exifFrameConfig);
        result = { requested, response, cancelled: cancelRequested };
      } catch (e) {
        toast.error(`変換に失敗しました: ${describeError(e)}`);
      } finally {
        processing = false;
        progress = null;
      }
    },

    async cancel(): Promise<void> {
      try {
        cancelRequested = true;
        await cancelProcessing();
      } catch (e) {
        toast.error(`キャンセルに失敗しました: ${describeError(e)}`);
      }
    },

    dismissResult() {
      result = null;
    },
  };
}
```

**`run` の中で `exifFrameConfig` を組み立てない。**
現行の `config.mode === "pad" && exifFrameEnabled ? activeExifFrameConfig : null`
という判断は「変換設定の状態」であって「実行の仕組み」ではないので、
呼び出し側（`App.svelte` → 後に `ConvertPanel`）に残す。

- [ ] **Step 4: `App.svelte` を書き換える**

削除する定義: `thumbnailCache` / `thumbnailKey` / `thumbnailFor` / `activeRequests` /
`MAX_CONCURRENT` / `pendingQueue` / `failedThumbnails` / `thumbnailErrorReported` /
`processQueue` / `handleRequestThumbnail` / `exifFramePresets` / `selectedPresetName` /
`reloadPresets` / `activeExifFrameConfig` / `processing` / `progress` /
`batchResponse` / `batchRequested` / `batchCancelled` / `cancelRequested` /
`runProcess` の本体 / `handleCancel` / `handleSavePreset` / `handleDeletePreset` /
`onMount` 内の `listen` 呼び出し。

追加する:

```ts
  import { createThumbnailQueue } from "./lib/browser/thumbnailQueue.svelte";
  import { createPresetStore } from "./lib/panels/presets.svelte";
  import { createConvertRun } from "./lib/panels/convertRun.svelte";

  const thumbnails = createThumbnailQueue();
  const presets = createPresetStore();
  const convert = createConvertRun();

  onMount(() => {
    const unsubscribe = convert.subscribeProgress();
    presets.reload();
    return unsubscribe;
  });

  function handleProcess() {
    if (!canProcess) return;
    // 元ファイルの一括削除は取り消せないため必ず確認を挟む
    if (config.delete_originals) {
      showDeleteConfirm = true;
      return;
    }
    runProcess();
  }

  function runProcess() {
    showDeleteConfirm = false;
    const efConfig =
      config.mode === "pad" && exifFrameEnabled ? presets.active : null;
    convert.run(selectedImages, outputFolder, config, efConfig);
  }

  let canProcess = $derived(
    selectedImages.length > 0 && !convert.processing && outputFolder !== ""
  );
```

**`import { SvelteMap }` と `import { listen }` / `UnlistenFn` の import は消す**
（使われなくなる）。`import { listImages, pickOutputFolder }` だけが `api` から残る。

テンプレート側の差し替え:

| 旧 | 新 |
|---|---|
| `{thumbnailFor}` | `thumbnailFor={thumbnails.get}` |
| `onRequestThumbnail={handleRequestThumbnail}` | `onRequestThumbnail={thumbnails.request}` |
| `presets={exifFramePresets}` | `presets={presets.presets}` |
| `{selectedPresetName}` | `selectedPresetName={presets.selectedName}` |
| `onPresetChange={(name) => (selectedPresetName = name)}` | `onPresetChange={(name) => (presets.selectedName = name)}` |
| `<ProgressOverlay {progress} onCancel={handleCancel} />` | `<ProgressOverlay progress={convert.progress} onCancel={convert.cancel} />` |
| `{#if batchResponse}<ResultDialog requested={batchRequested} response={batchResponse} cancelled={batchCancelled} onClose={...} />` | `{#if convert.result}<ResultDialog requested={convert.result.requested} response={convert.result.response} cancelled={convert.result.cancelled} onClose={convert.dismissResult} />` |
| `onSave={handleSavePreset}` | `onSave={async (p) => { if (await presets.save(p)) showExifFrameSettings = false; }}` |
| `onDelete={handleDeletePreset}` | `onDelete={presets.remove}` |

- [ ] **Step 5: 行数を確認する**

```bash
wc -l gui-frontend/src/App.svelte
```

期待: 260 行前後（シェル化前なのでまだ 150 行にはならない。Task 9 で到達する）。

- [ ] **Step 6: 型検査とビルド**

```bash
cd gui-frontend && bun run typecheck && bun run build
```

- [ ] **Step 7: 挙動が変わっていないことを実機で確認する**

```bash
make dev
```

（`make dev` は Tauri 実機を起動する。ここは `invoke` が実際に通る必要がある。）

- フォルダーを選ぶ → サムネイルが出る
- 写真を選ぶ → 右の選択リストに出る、サムネイルが出る
- 列スライダーを動かす → サムネイルが取り直される
- 出力先を選び「変換実行」 → 進捗が出て、結果ダイアログが出る
- 変換中に「キャンセル」 → 結果ダイアログの「未処理」に出る
- Exif フレーム設定 → プリセットの保存・削除ができる

**1 つでも壊れていたら、それは移設の写し間違い。** 仕様変更はこのタスクでは一切していない。

- [ ] **Step 8: コミット**

```bash
git add gui-frontend/src
git commit -m "refactor(gui): App.svelte からサムネイルキュー・プリセット・変換実行を切り出す"
```

---

## Task 8: カラム幅の永続化（段階 4 の 2/3）

spec §3-1「カラム幅の永続化（`localStorage`）」。
**読み出した値は必ず検証してからクランプする。** `NaN`・負値・巨大値が入っていると
1 カラムが画面を占有し、他のカラムが操作不能になる。

**Files:**
- Create: `gui-frontend/src/lib/shell/columns.ts`（**純粋。runes を含まない**）
- Create: `gui-frontend/src/lib/shell/columns.test.ts`
- Create: `gui-frontend/src/lib/shell/layout.svelte.ts`（runes ＋ `localStorage` I/O）

**Interfaces:**
- Produces: `columns.ts`
  - `type ColumnKey = "folder" | "presets" | "convert" | "metadata" | "frame"`
  - `const COLUMN_KEYS: readonly ColumnKey[]`
  - `const COLUMN_SPECS: Record<ColumnKey, { default: number; min: number; max: number }>`
  - `type ColumnWidths = Record<ColumnKey, number>`
  - `clampWidth(key: ColumnKey, value: unknown): number`
  - `defaultWidths(): ColumnWidths`
  - `parseWidths(raw: string | null): ColumnWidths`
  - `serializeWidths(widths: ColumnWidths): string`
  - `parseCollapsed(raw: string | null): boolean`
  - `const WIDTHS_STORAGE_KEY` / `const RIGHT_COLLAPSED_STORAGE_KEY`
- Produces: `layout.svelte.ts`
  - `createLayout(): Layout`
    - `readonly widths: ColumnWidths`
    - `setWidth(key: ColumnKey, value: number): void`（クランプと永続化を内包）
    - `rightPanelCollapsed: boolean`（読み書き両方。setter が永続化する）

- [ ] **Step 1: `test-integrity` スキルを起動する**

- [ ] **Step 2: 失敗するテストを書く**

`gui-frontend/src/lib/shell/columns.test.ts`:

```ts
/**
 * spec §3-1「カラム幅の永続化」。
 *
 * 検査するのは「壊れた値が入っていても操作不能にならない」こと。
 * localStorage は信頼できない入力として扱う（利用者が devtools で書ける、
 * 別バージョンのアプリが書いた、途中で壊れた、のいずれもありうる）。
 */
import { describe, expect, test } from "bun:test";
import {
  COLUMN_KEYS,
  COLUMN_SPECS,
  clampWidth,
  defaultWidths,
  parseCollapsed,
  parseWidths,
  serializeWidths,
} from "./columns";

describe("clampWidth", () => {
  test("範囲内の値はそのまま（整数へ丸める）", () => {
    expect(clampWidth("folder", 300)).toBe(300);
    expect(clampWidth("folder", 300.6)).toBe(301);
  });

  test("下限未満は下限へ、上限超過は上限へ", () => {
    expect(clampWidth("folder", 0)).toBe(COLUMN_SPECS.folder.min);
    expect(clampWidth("folder", -1000)).toBe(COLUMN_SPECS.folder.min);
    expect(clampWidth("folder", 999999)).toBe(COLUMN_SPECS.folder.max);
  });

  test("数値でない値と NaN / Infinity は既定値へ落とす", () => {
    // 幅 0 のカラムを作らせないための線。既定値へ落とすのが正しく、
    // min へ落とすと「壊れた値」と「利用者が最小まで縮めた」が区別できなくなる
    expect(clampWidth("folder", Number.NaN)).toBe(COLUMN_SPECS.folder.default);
    expect(clampWidth("folder", Number.POSITIVE_INFINITY)).toBe(COLUMN_SPECS.folder.default);
    expect(clampWidth("folder", "300")).toBe(COLUMN_SPECS.folder.default);
    expect(clampWidth("folder", null)).toBe(COLUMN_SPECS.folder.default);
    expect(clampWidth("folder", undefined)).toBe(COLUMN_SPECS.folder.default);
    expect(clampWidth("folder", {})).toBe(COLUMN_SPECS.folder.default);
  });
});

describe("parseWidths", () => {
  test("未保存（null）なら既定値", () => {
    expect(parseWidths(null)).toEqual(defaultWidths());
  });

  test("JSON として壊れていたら既定値", () => {
    expect(parseWidths("{")).toEqual(defaultWidths());
    expect(parseWidths("")).toEqual(defaultWidths());
  });

  test("オブジェクトでない JSON なら既定値", () => {
    expect(parseWidths("42")).toEqual(defaultWidths());
    expect(parseWidths("null")).toEqual(defaultWidths());
    expect(parseWidths('"folder"')).toEqual(defaultWidths());
    expect(parseWidths("[240, 320]")).toEqual(defaultWidths());
  });

  test("既知のキーだけを採り、欠けている分は既定値で埋める", () => {
    const parsed = parseWidths(JSON.stringify({ folder: 300, unknown: 9999 }));
    expect(parsed.folder).toBe(300);
    expect(parsed.convert).toBe(COLUMN_SPECS.convert.default);
    expect(Object.keys(parsed).sort()).toEqual([...COLUMN_KEYS].sort());
  });

  test("壊れた値が混ざっていても他のカラムは生き残る", () => {
    const parsed = parseWidths(JSON.stringify({ folder: -5, convert: 400 }));
    expect(parsed.folder).toBe(COLUMN_SPECS.folder.min);
    expect(parsed.convert).toBe(400);
  });

  test("書いて読むと同じ値に戻る", () => {
    const widths = { ...defaultWidths(), folder: 300, metadata: 420 };
    expect(parseWidths(serializeWidths(widths))).toEqual(widths);
  });
});

describe("parseCollapsed", () => {
  test('"true" だけが true。それ以外はすべて false', () => {
    expect(parseCollapsed("true")).toBe(true);
    expect(parseCollapsed("false")).toBe(false);
    expect(parseCollapsed(null)).toBe(false);
    expect(parseCollapsed("1")).toBe(false);
    expect(parseCollapsed("")).toBe(false);
  });
});

describe("COLUMN_SPECS", () => {
  // 前提条件: これが崩れていると上のクランプ検査はすべて無意味になる
  test("すべてのカラムで min <= default <= max", () => {
    for (const key of COLUMN_KEYS) {
      const spec = COLUMN_SPECS[key];
      expect(spec.min).toBeLessThanOrEqual(spec.default);
      expect(spec.default).toBeLessThanOrEqual(spec.max);
      expect(spec.min).toBeGreaterThan(0);
    }
  });
});
```

- [ ] **Step 3: 落ちることを確認する**

```bash
cd gui-frontend && bun test src/lib/shell/columns.test.ts
```

期待: `Cannot find module './columns'` で落ちる。

- [ ] **Step 4: `columns.ts` を書く**

```ts
/**
 * カラム幅の仕様と、localStorage 文字列の解釈。
 *
 * runes を含まない純粋なモジュールにしてある。壊れた永続値を弾く規則は
 * ここでしか書かれておらず、UI を起動せずに検査できることに意味がある。
 *
 * 数値の px がここに直接書かれているのは、spec §3-1 が定めた
 * 「レイアウトの構造的な寸法」だからである（色や余白のトークンとは別物）。
 */
export type ColumnKey = "folder" | "presets" | "convert" | "metadata" | "frame";

export const COLUMN_KEYS = [
  "folder",
  "presets",
  "convert",
  "metadata",
  "frame",
] as const satisfies readonly ColumnKey[];

export interface ColumnSpec {
  default: number;
  min: number;
  max: number;
}

/**
 * 既定値は spec §3-1 のレイアウト表から。
 * min は「そのカラムが役目を果たせる最小」、max は「グリッドを潰さない最大」。
 * minWidth 1100 のウィンドウで rail 80 + 左 max + 右 max を引いても
 * グリッドに 1 列分（内側 200px 弱）が残るように取ってある。
 */
export const COLUMN_SPECS: Record<ColumnKey, ColumnSpec> = {
  folder: { default: 240, min: 180, max: 400 },
  presets: { default: 220, min: 160, max: 360 },
  convert: { default: 320, min: 260, max: 480 },
  metadata: { default: 360, min: 280, max: 520 },
  frame: { default: 360, min: 280, max: 520 },
};

export type ColumnWidths = Record<ColumnKey, number>;

export const WIDTHS_STORAGE_KEY = "picture-tool.layout.widths.v1";
export const RIGHT_COLLAPSED_STORAGE_KEY = "picture-tool.layout.right-collapsed.v1";

export function defaultWidths(): ColumnWidths {
  return {
    folder: COLUMN_SPECS.folder.default,
    presets: COLUMN_SPECS.presets.default,
    convert: COLUMN_SPECS.convert.default,
    metadata: COLUMN_SPECS.metadata.default,
    frame: COLUMN_SPECS.frame.default,
  };
}

/**
 * 数値でない値・NaN・Infinity は既定値へ落とす。
 * min へ落とさないのは、「壊れた値」と「利用者が最小まで縮めた」を
 * 区別できるようにしておくため。
 */
export function clampWidth(key: ColumnKey, value: unknown): number {
  const spec = COLUMN_SPECS[key];
  if (typeof value !== "number" || !Number.isFinite(value)) return spec.default;
  return Math.min(spec.max, Math.max(spec.min, Math.round(value)));
}

export function parseWidths(raw: string | null): ColumnWidths {
  if (raw === null) return defaultWidths();

  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return defaultWidths();
  }

  if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
    return defaultWidths();
  }

  const source = parsed as Record<string, unknown>;
  const widths = defaultWidths();
  for (const key of COLUMN_KEYS) {
    widths[key] = clampWidth(key, source[key]);
  }
  return widths;
}

export function serializeWidths(widths: ColumnWidths): string {
  return JSON.stringify(widths);
}

/** 折りたたみ状態は幅と別キーなので、クランプの対象外（spec §3-1） */
export function parseCollapsed(raw: string | null): boolean {
  return raw === "true";
}
```

- [ ] **Step 5: 通ることを確認する**

```bash
cd gui-frontend && bun test src/lib/shell/columns.test.ts
```

期待: 全 PASS。

- [ ] **Step 6: `layout.svelte.ts` を書く**

```ts
import {
  clampWidth,
  parseCollapsed,
  parseWidths,
  serializeWidths,
  RIGHT_COLLAPSED_STORAGE_KEY,
  WIDTHS_STORAGE_KEY,
  type ColumnKey,
  type ColumnWidths,
} from "./columns";

/**
 * カラム幅と右パネル折りたたみの保持と永続化。
 *
 * localStorage が使えない環境（WebKitGTK で永続が不安定な例が知られている）でも
 * 動く。読めなければ既定値、書けなければ黙って捨てる。
 * 失って困る情報ではないのでトーストも出さない（spec §3-1 / §8）。
 */
function read(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function write(key: string, value: string): void {
  try {
    localStorage.setItem(key, value);
  } catch {
    // 保存できないだけで操作は続けられる
  }
}

export function createLayout() {
  let widths = $state<ColumnWidths>(parseWidths(read(WIDTHS_STORAGE_KEY)));
  let collapsed = $state(parseCollapsed(read(RIGHT_COLLAPSED_STORAGE_KEY)));

  return {
    get widths(): ColumnWidths {
      return widths;
    },

    /** ドラッグ中に毎フレーム呼ばれる。クランプと永続化はここに閉じる */
    setWidth(key: ColumnKey, value: number): void {
      widths[key] = clampWidth(key, value);
      write(WIDTHS_STORAGE_KEY, serializeWidths($state.snapshot(widths)));
    },

    get rightPanelCollapsed(): boolean {
      return collapsed;
    },

    set rightPanelCollapsed(next: boolean) {
      collapsed = next;
      write(RIGHT_COLLAPSED_STORAGE_KEY, String(next));
    },
  };
}
```

- [ ] **Step 7: 型検査**

```bash
cd gui-frontend && bun run typecheck && bun test && bun run build
```

- [ ] **Step 8: コミット**

```bash
git add gui-frontend/src/lib/shell
git commit -m "feat(gui): カラム幅の仕様・クランプ・永続化を追加"
```

---

## Task 9: アプリシェル（段階 4 の 3/3）

spec §3-1 / §3-2 / §3-3 / §3-5。rail による 3 モード、可変カラム、ウィンドウ寸法。

**Files:**
- Create: `gui-frontend/src/lib/shell/modes.ts`
- Create: `gui-frontend/src/lib/shell/NavigationRail.svelte`
- Create: `gui-frontend/src/lib/shell/AppShell.svelte`
- Create: `gui-frontend/e2e/stub.ts`, `gui-frontend/e2e/shell.spec.ts`
- Move: `gui-frontend/src/lib/FolderTree.svelte` → `gui-frontend/src/lib/browser/FolderTree.svelte`
- Modify: `gui-frontend/src/App.svelte`
- Modify: `gui/tauri.conf.json`（`app.windows[0]` の `width` / `minWidth` のみ）

**Interfaces:**
- Consumes: `createLayout()`（Task 8）, `Card` / `Button`（Task 3）
- Produces: `modes.ts` — `type AppMode = "convert" | "metadata" | "frame"`,
  `const MODES: { value: AppMode; label: string; icon: string }[]`
- Produces: `NavigationRail` `{ mode: AppMode, onModeChange: (mode: AppMode) => void }`
- Produces: `AppShell`
  `{ mode: AppMode, onModeChange: (mode: AppMode) => void,
     layout: ReturnType<typeof createLayout>,
     left: Snippet, center: Snippet, right: Snippet }`
  — 左右どちらのカラムに `ColumnKey` を割り当てるかは `mode` から `AppShell` が導く
- Produces: `installTauriStub(page, options?)`（e2e 用）

- [ ] **Step 1: `gui/tauri.conf.json` の寸法を上げる**

`app.windows[0]` を差し替える。**`height` と `minHeight` は変えない**
（列数は幅だけで解けるうえ、高さを上げると縦の狭い画面で初回起動時に
はみ出る側のリスクだけが増えるため。spec §3-1）:

```json
      {
        "title": "Picture Tool",
        "width": 1440,
        "height": 800,
        "minWidth": 1100,
        "minHeight": 600
      }
```

`security` / `plugins` / `build` には触らないこと。

- [ ] **Step 2: `modes.ts` を書く**

```ts
export type AppMode = "convert" | "metadata" | "frame";

/**
 * rail の destination（spec §3-3）。
 *
 * ラベルは日本語。「メタデータ」ではなく「情報」にしてあるのは、
 * rail 幅 80px に収めるため（spec §8 の未確定項目に対する結論）。
 * 幅が変わる変更をしたら Step 9 の実測をやり直すこと。
 */
export const MODES: { value: AppMode; label: string; icon: string }[] = [
  { value: "convert", label: "変換", icon: "⇄" },
  { value: "metadata", label: "情報", icon: "ℹ" },
  { value: "frame", label: "フレーム", icon: "▣" },
];
```

**spec §3-3 は「選択中は pill 型インジケータ ＋ 塗りアイコン、非選択は outline
アイコン」と書いているが、塗り／outline の切り替えは入れない。** 意図的な逸脱で、理由は:

- **Web フォントを読み込まない**（spec §1-4 / Global Constraints）ため、
  Material Symbols のような塗り・輪郭のペアを持つアイコンフォントが使えない
- このアプリに SVG アイコンセットは無い。3 destination のために 6 個の
  path を手で起こすのは、この刷新が取りに行っている価値に対して割に合わない
- Unicode の記号グリフには、3 つの意味（変換・情報・フレーム）に揃った
  塗り／輪郭のペアが存在しない（`▢`/`▣` はあるが `⇄` と `ℹ` に相方が無い）

**代わりに、選択状態は pill 型インジケータ（`primary-container` の塗り）と
文字色の濃淡で示す。** 支援技術には `aria-current="page"` で伝わる（Step 3）。
この判断は Task 18 Step 11 の「spec と食い違った点」にも記録すること。

- [ ] **Step 3: `NavigationRail.svelte` を書く**

```svelte
<script lang="ts">
  import { MODES, type AppMode } from "./modes";

  interface Props {
    mode: AppMode;
    onModeChange: (mode: AppMode) => void;
  }

  let { mode, onModeChange }: Props = $props();
</script>

<nav class="rail" aria-label="モード">
  {#each MODES as destination (destination.value)}
    {@const selected = mode === destination.value}
    <button
      class="destination"
      class:selected
      type="button"
      aria-current={selected ? "page" : undefined}
      onclick={() => onModeChange(destination.value)}
    >
      <span class="indicator state-layer" aria-hidden="true">{destination.icon}</span>
      <span class="label">{destination.label}</span>
    </button>
  {/each}
</nav>

<style>
  .rail {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-3);
    /* rail 幅は 80px 固定（spec §3-1）。ここだけは構造的な寸法 */
    width: 80px;
    flex-shrink: 0;
    padding: var(--space-3) 0;
    background: var(--md-sys-color-surface);
    border-right: 1px solid var(--md-sys-color-outline-variant);
  }

  .destination {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-1);
    width: 100%;
    padding: 0;
    background: none;
    border: none;
    cursor: pointer;
    color: var(--md-sys-color-on-surface-variant);
  }

  .destination.selected {
    color: var(--md-sys-color-on-surface);
  }

  /* 選択インジケータは pill 形（spec §3-3） */
  .indicator {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 56px;
    height: 32px;
    border-radius: var(--md-sys-shape-corner-full);
    font-size: 18px;
    line-height: 1;
    transition: background var(--md-sys-motion-duration-short)
      var(--md-sys-motion-easing-standard);
  }

  .destination.selected .indicator {
    background: var(--md-sys-color-primary-container);
    color: var(--md-sys-color-on-primary-container);
  }

  .label {
    font: var(--md-sys-typescale-body-sm);
    white-space: nowrap;
  }
</style>
```

- [ ] **Step 4: `AppShell.svelte` を書く**

```svelte
<script lang="ts">
  import type { Snippet } from "svelte";
  import NavigationRail from "./NavigationRail.svelte";
  import { COLUMN_SPECS, type ColumnKey } from "./columns";
  import type { createLayout } from "./layout.svelte";
  import type { AppMode } from "./modes";

  interface Props {
    mode: AppMode;
    onModeChange: (mode: AppMode) => void;
    layout: ReturnType<typeof createLayout>;
    left: Snippet;
    center: Snippet;
    right: Snippet;
  }

  let { mode, onModeChange, layout, left, center, right }: Props = $props();

  /**
   * フレームモードだけ左 2 カラムが変わる（spec §3-1）。
   * プリセット編集にフォルダーツリーは不要で、必要なのは見本の写真 1 枚だけ。
   */
  let leftKey = $derived<ColumnKey>(mode === "frame" ? "presets" : "folder");
  let rightKey = $derived<ColumnKey>(
    mode === "convert" ? "convert" : mode === "metadata" ? "metadata" : "frame"
  );

  let shell: HTMLDivElement | undefined = $state();
  let dragging = $state<"left" | "right" | null>(null);

  const RAIL_WIDTH = 80;
  const KEYBOARD_STEP = 16;

  function startDrag(event: PointerEvent, side: "left" | "right") {
    const handle = event.currentTarget as HTMLElement;
    handle.setPointerCapture(event.pointerId);
    dragging = side;
  }

  function endDrag(event: PointerEvent) {
    const handle = event.currentTarget as HTMLElement;
    if (handle.hasPointerCapture(event.pointerId)) {
      handle.releasePointerCapture(event.pointerId);
    }
    dragging = null;
  }

  function drag(event: PointerEvent) {
    if (dragging === null || !shell) return;
    const rect = shell.getBoundingClientRect();
    if (dragging === "left") {
      layout.setWidth(leftKey, event.clientX - rect.left - RAIL_WIDTH);
    } else {
      layout.setWidth(rightKey, rect.right - event.clientX);
    }
  }

  /** ポインタを持たない利用者のための経路。左右キーで 16px ずつ動かす */
  function nudge(event: KeyboardEvent, side: "left" | "right") {
    const delta =
      event.key === "ArrowLeft" ? -KEYBOARD_STEP : event.key === "ArrowRight" ? KEYBOARD_STEP : 0;
    if (delta === 0) return;
    event.preventDefault();
    const key = side === "left" ? leftKey : rightKey;
    // 右のハンドルは「左へ動かすと広くなる」ので符号を反転する
    const signed = side === "left" ? delta : -delta;
    layout.setWidth(key, layout.widths[key] + signed);
  }
</script>

<div class="shell" bind:this={shell} class:dragging={dragging !== null}>
  <NavigationRail {mode} {onModeChange} />

  <div class="column" style="width: {layout.widths[leftKey]}px;">
    {@render left()}
  </div>

  <div
    class="handle"
    role="separator"
    aria-orientation="vertical"
    aria-label="左カラムの幅"
    aria-valuemin={COLUMN_SPECS[leftKey].min}
    aria-valuemax={COLUMN_SPECS[leftKey].max}
    aria-valuenow={layout.widths[leftKey]}
    tabindex="0"
    onpointerdown={(e) => startDrag(e, "left")}
    onpointermove={drag}
    onpointerup={endDrag}
    onpointercancel={endDrag}
    onkeydown={(e) => nudge(e, "left")}
  ></div>

  <div class="center">
    {@render center()}
  </div>

  {#if !layout.rightPanelCollapsed}
    <div
      class="handle"
      role="separator"
      aria-orientation="vertical"
      aria-label="右パネルの幅"
      aria-valuemin={COLUMN_SPECS[rightKey].min}
      aria-valuemax={COLUMN_SPECS[rightKey].max}
      aria-valuenow={layout.widths[rightKey]}
      tabindex="0"
      onpointerdown={(e) => startDrag(e, "right")}
      onpointermove={drag}
      onpointerup={endDrag}
      onpointercancel={endDrag}
      onkeydown={(e) => nudge(e, "right")}
    ></div>

    <div class="column right" style="width: {layout.widths[rightKey]}px;">
      {@render right()}
    </div>
  {/if}
</div>

<style>
  .shell {
    display: flex;
    height: 100vh;
    overflow: hidden;
    background: var(--md-sys-color-surface);
    color: var(--md-sys-color-on-surface);
    font: var(--md-sys-typescale-body-md);
  }

  /* ドラッグ中はカラム内のテキスト選択を止める */
  .shell.dragging {
    user-select: none;
    cursor: col-resize;
  }

  .column {
    flex-shrink: 0;
    overflow: hidden;
    background: var(--md-sys-color-surface-container-low);
  }

  .column.right {
    background: var(--md-sys-color-surface-container);
  }

  .center {
    flex: 1;
    min-width: 0;
    overflow: hidden;
  }

  /* 見た目は 1px の境界線、当たり判定は 8px */
  .handle {
    flex-shrink: 0;
    width: 8px;
    margin: 0 -4px;
    z-index: 1;
    cursor: col-resize;
    background: linear-gradient(
      to right,
      transparent 3px,
      var(--md-sys-color-outline-variant) 3px,
      var(--md-sys-color-outline-variant) 4px,
      transparent 4px
    );
  }

  .handle:hover,
  .handle:focus-visible {
    background: linear-gradient(
      to right,
      transparent 3px,
      var(--md-sys-color-primary) 3px,
      var(--md-sys-color-primary) 5px,
      transparent 5px
    );
  }
</style>
```

- [ ] **Step 5: `FolderTree.svelte` を移す**

```bash
git mv gui-frontend/src/lib/FolderTree.svelte gui-frontend/src/lib/browser/FolderTree.svelte
```

`FolderTree.svelte` 内の import を 1 段深くする:

```ts
  import { listDirectory, listDrives, loadFavorites, saveFavorites } from "../api";
  import { toast, describeError } from "../toasts.svelte";
  import type { FileEntry } from "../types";
```

- [ ] **Step 6: `App.svelte` をシェル化する**

`App.svelte` に足す状態（spec §3-2）:

```ts
  import { SvelteSet } from "svelte/reactivity";
  import type { AppMode } from "./lib/shell/modes";

  let mode = $state<AppMode>("convert");

  // 全モードで共有。rail の切替では破棄しない
  let currentFolder = $state("");
  let images = $state<ImageEntry[]>([]);

  // 最後にクリックした 1 枚。フレームの見本写真の出所（spec §3-2）
  let focusedPath = $state<string | null>(null);

  // メタデータの編集対象。未保存ガードはこれの変更にだけ掛かる。
  // 本刷新では読むだけで、ガードの配線は次工程（spec §5-2）
  let editingPath = $state<string | null>(null);

  const layout = createLayout();
```

**`selectedImages: ImageEntry[]` はこの Task ではそのまま残す。**
`SvelteSet<string>` への置き換えとフォルダー変更時のクリアは Task 10 で行う
（`SelectionList` と `SettingsPanel` がまだ `ImageEntry[]` を要求するため）。
`import { SvelteSet }` も Task 10 で足す。上の import 行はここでは書かない。

`handleSelectFolder` に `focusedPath = null` を足す（フォルダーを変えたら
最後に触った写真は無効）。

テンプレートを差し替える:

```svelte
<AppShell {mode} onModeChange={(next) => (mode = next)} {layout}>
  {#snippet left()}
    {#if mode === "frame"}
      <div class="placeholder">
        <Card level={1} title="プリセット一覧">
          <p>Task 16（段階 7）で実装する。</p>
        </Card>
      </div>
    {:else}
      <FolderTree onSelectFolder={handleSelectFolder} />
    {/if}
  {/snippet}

  {#snippet center()}
    <ThumbnailGrid
      {images}
      {selectedPaths}
      thumbnailFor={thumbnails.get}
      {currentPage}
      onToggleSelect={handleToggleSelect}
      onRequestThumbnail={thumbnails.request}
      onPreview={handlePreview}
      onPageChange={(page) => (currentPage = page)}
    />
  {/snippet}

  {#snippet right()}
    {#if mode === "convert"}
      <SettingsPanel … 現行のまま … />
    {:else if mode === "metadata"}
      <div class="placeholder">
        <Card level={1} title="メタデータ">
          <p>Task 17（段階 8）で実装する。</p>
        </Card>
      </div>
    {:else}
      <div class="placeholder">
        <Card level={1} title="フレーム設定">
          <p>Task 16（段階 7）で実装する。</p>
          <Button variant="outlined" onclick={() => (showExifFrameSettings = true)}>
            現行の Exif フレーム設定を開く
          </Button>
        </Card>
      </div>
    {/if}
  {/snippet}
</AppShell>
```

`.placeholder { padding: var(--space-4); }` を `<style>` に足す。
`.app` / `.left-panel` / `.center-panel` / `.right-panel` の規則は削除する
（`AppShell` が持つようになったため）。

**`SelectionList` はこの Task では右パネルから外す**（`SettingsPanel` だけを残す）。
Task 10 で正式に削除するが、シェル化の時点で置き場所が無くなるため。
`handleRemove` もここで消す。

**モード切替のアニメーション**（spec §3-3: 150ms のフェードのみ）は
`AppShell` の `.center` / `.column` ではなく、パネル側に入れる。
ここでは入れず、Task 10 / 16 / 17 で各パネルに `transition: opacity` を持たせる。

- [ ] **Step 7: Playwright のスタブを書く**

`gui-frontend/e2e/stub.ts`。**見た目の検証にはこれが要る。**
スタブ無しでは、フォルダーツリーも写真グリッドもプレビューもフレームも
空のままになる（spec §7-3）。

```ts
import type { Page } from "@playwright/test";

export interface StubOptions {
  /** list_images が返す枚数 */
  imageCount?: number;
}

/**
 * Tauri の IPC をスタブする。
 *
 * @tauri-apps/api の invoke は window.__TAURI_INTERNALS__.invoke へ委譲し、
 * listen は transformCallback + `plugin:event|listen` を経由する。
 * この 2 つを用意すれば webview の中の挙動をそのまま再現できる。
 */
export async function installTauriStub(page: Page, options: StubOptions = {}) {
  await page.addInitScript((imageCount: number) => {
    /** サムネイルは 24 種類だけ作って使い回す。
     *  全部同じにするとデコードが 1 回で済んでしまい、スクロール計測の
     *  負荷が実際より軽く出る。全部別にすると生成自体が計測を汚す。 */
    const POOL_SIZE = 24;
    const pool = new Map<string, string>();

    function jpegFor(index: number, size: number): string {
      const key = `${index % POOL_SIZE}:${size}`;
      const cached = pool.get(key);
      if (cached) return cached;

      const canvas = document.createElement("canvas");
      canvas.width = size;
      canvas.height = Math.round((size * 5) / 4);
      const ctx = canvas.getContext("2d")!;
      const hue = ((index % POOL_SIZE) * 360) / POOL_SIZE;
      const gradient = ctx.createLinearGradient(0, 0, canvas.width, canvas.height);
      gradient.addColorStop(0, `hsl(${hue} 70% 60%)`);
      gradient.addColorStop(1, `hsl(${(hue + 60) % 360} 70% 25%)`);
      ctx.fillStyle = gradient;
      ctx.fillRect(0, 0, canvas.width, canvas.height);
      ctx.fillStyle = "#fff";
      ctx.font = `${Math.round(size / 6)}px sans-serif`;
      ctx.fillText(String(index % POOL_SIZE), size / 8, size / 3);

      const base64 = canvas.toDataURL("image/jpeg", 0.8).split(",")[1];
      pool.set(key, base64);
      return base64;
    }

    function indexOfPath(path: string): number {
      const m = /(\d+)\.jpg$/.exec(path);
      return m ? Number(m[1]) : 0;
    }

    const images = Array.from({ length: imageCount }, (_, i) => ({
      name: `photo-${String(i).padStart(4, "0")}.jpg`,
      path: `/photos/${i}.jpg`,
      width: 4000,
      height: 3000,
      size_bytes: 4_500_000,
    }));

    const presets = [
      {
        name: "default",
        position: "auto",
        items: {
          maker_logo: true, lens_brand_logo: true, camera_model: true,
          lens_model: true, focal_length: true, f_number: true,
          shutter_speed: true, iso: true, date_taken: false, custom_text: false,
        },
        font: { font_path: null, primary_size: 0.025, secondary_size: 0.018 },
        custom_text: "",
      },
    ];

    const callbacks = new Map<number, (payload: unknown) => void>();
    let nextCallbackId = 1;

    const handlers: Record<string, (args: any) => unknown> = {
      list_drives: () => ["/"],
      list_directory: () => [
        { name: "photos", path: "/photos", is_dir: true, is_image: false },
        { name: "archive", path: "/archive", is_dir: true, is_image: false },
      ],
      list_images: () => images,
      get_thumbnail: (a) => jpegFor(indexOfPath(a.path), a.maxDimension),
      get_full_image: (a) => jpegFor(indexOfPath(a.path), 800),
      get_exif_info: () => ({
        camera_make: "SONY", camera_model: "ILCE-7M4", lens_model: "FE 35mm F1.4 GM",
        focal_length: "35mm", f_number: "f/1.4", shutter_speed: "1/250s",
        iso: 400, date_taken: "2026-08-19 10:00:00", orientation: 1,
      }),
      pick_output_folder: () => "/output",
      load_favorites: () => ["/photos"],
      save_favorites: () => null,
      process_images: () => ({ results: [], failures: [], warnings: [] }),
      cancel_processing: () => null,
      render_exif_frame_preview: () => ({
        data_url: `data:image/jpeg;base64,${jpegFor(0, 400)}`,
        warnings: [],
      }),
      // プリセットは読み書きの往復を検査したい（Task 16 の改名）ので、
      // 配列を実際に書き換える。引数名は api.ts に合わせること
      // （save_preset は { config }、delete_preset は { name }）
      list_presets: () => presets,
      save_preset: (a) => {
        const i = presets.findIndex((p) => p.name === a.config.name);
        if (i >= 0) presets[i] = a.config;
        else presets.push(a.config);
        return null;
      },
      delete_preset: (a) => {
        const i = presets.findIndex((p) => p.name === a.name);
        if (i >= 0) presets.splice(i, 1);
        return null;
      },
      list_available_fonts: () => [
        { display_name: "同梱フォント", path: null, is_bundled: true },
      ],
      "plugin:event|listen": () => nextCallbackId++,
      "plugin:event|unlisten": () => null,
    };

    (window as any).__TAURI_INTERNALS__ = {
      transformCallback(callback: (payload: unknown) => void) {
        const id = nextCallbackId++;
        callbacks.set(id, callback);
        return id;
      },
      async invoke(cmd: string, args: any) {
        const handler = handlers[cmd];
        if (!handler) throw new Error(`stub: 未対応のコマンド ${cmd}`);
        return handler(args ?? {});
      },
    };
  }, options.imageCount ?? 24);
}

/**
 * `localStorage` を**そのテストの最初のナビゲーションでだけ**空にする。
 *
 * `page.addInitScript(() => localStorage.clear())` を直に書いてはいけない。
 * init script は `page.reload()` を含む**すべてのナビゲーションで再実行される**ので、
 * 「幅を変えてリロードしても保たれる」「畳んだ状態がリロード後も残る」といった
 * 永続化の検証が、リロードのたびに消されて必ず落ちる。
 *
 * `sessionStorage` はリロードを跨いで残り、Playwright は 1 テスト 1 コンテキストなので、
 * これで「テストごとに 1 回だけ」が成立する。
 */
export async function clearStorageOnce(page: Page) {
  await page.addInitScript(() => {
    const FLAG = "__pt_storage_cleared";
    if (!sessionStorage.getItem(FLAG)) {
      localStorage.clear();
      sessionStorage.setItem(FLAG, "1");
    }
  });
}

/**
 * `Switch` を切り替える。
 *
 * `getByRole("checkbox", …).click()` は通らない ── `Switch` は透明な `input` の上に
 * `.track`（`position: relative`）が重なる構造で、当たり判定は DOM 順で後ろの
 * `.track` が取るため、Playwright の actionability チェックが
 * `.track intercepts pointer events` で落ちる（Task 4 Step 2 の注記）。
 * 可視ラベルをクリックすれば `label` の既定動作で `input` が切り替わる。
 */
export function toggleSwitch(page: Page, label: string) {
  return page.getByText(label, { exact: true }).click();
}
```

- [ ] **Step 8: `test-integrity` スキルを起動する**

- [ ] **Step 9: シェルの検査を書く**

`gui-frontend/e2e/shell.spec.ts`:

```ts
import { expect, test } from "@playwright/test";
import { clearStorageOnce, installTauriStub } from "./stub";

test.describe("アプリシェル", () => {
  test.beforeEach(async ({ page }) => {
    await installTauriStub(page);
    await clearStorageOnce(page);
  });

  test("rail の 3 destination が切り替わる", async ({ page }) => {
    await page.goto("/");
    const rail = page.getByRole("navigation", { name: "モード" });
    for (const label of ["変換", "情報", "フレーム"]) {
      await rail.getByRole("button", { name: label }).click();
      await expect(rail.getByRole("button", { name: label })).toHaveAttribute(
        "aria-current",
        "page"
      );
    }
  });

  test("rail のラベルが 80px に収まる（spec §8）", async ({ page }) => {
    await page.goto("/");
    const rail = page.getByRole("navigation", { name: "モード" });
    // 前提条件: rail の幅が仕様どおり 80px であること
    expect((await rail.boundingBox())!.width).toBe(80);
    for (const label of ["変換", "情報", "フレーム"]) {
      const box = await rail.getByText(label, { exact: true }).boundingBox();
      expect(box!.width, `${label} が rail からはみ出す`).toBeLessThanOrEqual(80);
    }
  });

  test("左カラムはドラッグで幅が変わり、リロード後も保たれる", async ({ page }) => {
    await page.goto("/");
    const handle = page.getByRole("separator", { name: "左カラムの幅" });

    // 前提条件: 既定幅は 240px（columns.ts の COLUMN_SPECS.folder.default）
    await expect(handle).toHaveAttribute("aria-valuenow", "240");

    const box = (await handle.boundingBox())!;
    const centerX = box.x + box.width / 2;
    const centerY = box.y + box.height / 2;
    // 前提条件: ハンドルの中心が rail(80) + 既定幅(240) の位置にあること。
    // ここがずれているならハンドルの配置か margin の相殺が間違っている
    expect(Math.round(centerX)).toBe(80 + 240);

    const targetX = centerX + 60;
    await page.mouse.move(centerX, centerY);
    await page.mouse.down();
    await page.mouse.move(targetX, centerY, { steps: 5 });
    await page.mouse.up();

    // 幅は AppShell の drag() が `clientX - rect.left - RAIL_WIDTH` で出す。
    // シェルは左端から始まるので rect.left は 0
    const expected = String(Math.round(targetX - 80));
    await expect(handle).toHaveAttribute("aria-valuenow", expected);

    await page.reload();
    await expect(page.getByRole("separator", { name: "左カラムの幅" })).toHaveAttribute(
      "aria-valuenow",
      expected
    );
  });

  test("localStorage に壊れた値が入っていても既定幅で起動する", async ({ page }) => {
    await page.addInitScript(() => {
      localStorage.setItem(
        "picture-tool.layout.widths.v1",
        JSON.stringify({ folder: -99999, convert: "wide" })
      );
    });
    await page.goto("/");
    await expect(page.getByRole("separator", { name: "左カラムの幅" })).toHaveAttribute(
      "aria-valuenow",
      "180" // COLUMN_SPECS.folder.min
    );
    await expect(page.getByRole("separator", { name: "右パネルの幅" })).toHaveAttribute(
      "aria-valuenow",
      "320" // COLUMN_SPECS.convert.default
    );
  });

  test("フレームモードだけ左カラムの幅が別に持たれる（spec §3-1）", async ({ page }) => {
    await page.goto("/");
    const rail = page.getByRole("navigation", { name: "モード" });
    const handle = () => page.getByRole("separator", { name: "左カラムの幅" });

    await expect(handle()).toHaveAttribute("aria-valuenow", "240"); // folder
    await rail.getByRole("button", { name: "フレーム" }).click();
    await expect(handle()).toHaveAttribute("aria-valuenow", "220"); // presets
  });
});
```

`page.mouse.move` の移動量と期待値の対応（60px 右へ動かすと 240 → 300）は、
ハンドルが左カラムの右端にあり `clientX - rect.left - 80` で幅を出すことから来る。
**ここが合わなければハンドルの位置計算が間違っている。**

- [ ] **Step 10: 走らせる**

```bash
cd gui-frontend && bun run typecheck && bun test && bun run e2e
```

- [ ] **Step 11: 実機で `localStorage` が保つことを確認する（spec §8）**

```bash
make dev
```

カラム幅を変える → **アプリを終了して起動し直す** → 幅が保たれているか見る。

**WebKitGTK（Linux）は `localStorage` の永続が不安定な例が知られている。**
保たないようなら、`layout.svelte.ts` の `write()` を no-op にして
**永続だけを落とす**（可変幅は維持する）。その判断をこの計画の末尾
「実施メモ」に記録し、spec §8 の該当行も更新すること。

- [ ] **Step 12: 既定 1440×800 が画面に収まることを確認する**

実機を起動して、ウィンドウが画面からはみ出さないこと。
はみ出す場合は `width` を下げるのではなく、その環境を実施メモに記録する
（spec §3-1 の実測は論理解像度 3072×1728 の開発機で取られている）。

- [ ] **Step 13: コミット**

```bash
git add gui-frontend gui/tauri.conf.json
git commit -m "feat(gui): navigation rail と可変カラムのアプリシェルを追加"
```

---

## Task 10: 変換パネルの再構築と `SelectionList` の廃止（段階 5 の 1/2）

spec §5-1 / §3-2。

**これは機能低下を伴う。** 現行は「複数のフォルダーから集めて一括変換する」ことができ、
`SelectionList` がその全容を見せていた。**この操作はできなくなる。**
選択を跨がせたまま一覧を廃止すると「N 枚を変換」の N に画面外の写真が混ざり、
解除する手段も消えるため、跨ぐのをやめる方を選んだ（spec §5-1）。

**Files:**
- Create: `gui-frontend/src/lib/panels/ConvertPanel.svelte`
- Delete: `gui-frontend/src/lib/SettingsPanel.svelte`, `gui-frontend/src/lib/SelectionList.svelte`
- Modify: `gui-frontend/src/App.svelte`

**Interfaces:**
- Consumes: `Card` / `Button` / `Switch` / `Slider` / `SegmentedButton` / `Select` / `TextField`
- Produces: `ConvertPanel`
  `{ config: ProcessingConfig (bindable), outputFolder: string,
     selectedCount: number, canProcess: boolean,
     exifFrameEnabled: boolean (bindable),
     presetNames: string[], selectedPresetName: string (bindable),
     onPickOutputFolder: () => void, onProcess: () => void, onEditFrame: () => void }`
- Produces: `App.svelte` の選択状態が `selectedPaths: SvelteSet<string>` になる。
  `selectedImages` は `$derived(images.filter((img) => selectedPaths.has(img.path)))`

- [ ] **Step 1: `ConvertPanel.svelte` を書く**

```svelte
<script lang="ts">
  import Button from "../ui/Button.svelte";
  import Card from "../ui/Card.svelte";
  import Select from "../ui/Select.svelte";
  import SegmentedButton from "../ui/SegmentedButton.svelte";
  import Slider from "../ui/Slider.svelte";
  import Switch from "../ui/Switch.svelte";
  import TextField from "../ui/TextField.svelte";
  import type { ProcessingConfig } from "../types";

  interface Props {
    config: ProcessingConfig;
    outputFolder: string;
    selectedCount: number;
    canProcess: boolean;
    exifFrameEnabled: boolean;
    presetNames: string[];
    selectedPresetName: string;
    onPickOutputFolder: () => void;
    onProcess: () => void;
    /** フレームモードへ切り替える。プリセットの編集はそちらで行う（spec §5-3） */
    onEditFrame: () => void;
  }

  let {
    config = $bindable(),
    outputFolder,
    selectedCount,
    canProcess,
    exifFrameEnabled = $bindable(),
    presetNames,
    selectedPresetName = $bindable(),
    onPickOutputFolder,
    onProcess,
    onEditFrame,
  }: Props = $props();

  const MAX_WIDTH_MIN = 4;
  const MAX_WIDTH_MAX = 20000;

  /** トグルを on にしたときに入れる値。off にしても直前の値を覚えておく */
  let lastMaxWidth = $state(1080);

  let maxWidthEnabled = $derived(config.max_width !== null);

  let maxWidthLabel = $derived(
    config.max_width === null ? "" : `${config.max_width}×${(config.max_width * 5) / 4}`
  );

  /**
   * 4 の倍数へ切り捨てる（Rust 側 `target_canvas` と同じ丸め方向）。
   * 切り上げると指定値を超えてしまい、「上限」という機能の目的を果たさない。
   * TextField の normalize に渡すので、DOM への書き戻しは TextField が行う。
   */
  function snapWidth(value: number): number {
    const clamped = Math.min(Math.max(value, MAX_WIDTH_MIN), MAX_WIDTH_MAX);
    return Math.floor(clamped / 4) * 4;
  }

  function toggleMaxWidth(enabled: boolean) {
    config.max_width = enabled ? lastMaxWidth : null;
  }

  function commitMaxWidth() {
    if (config.max_width !== null) lastMaxWidth = config.max_width;
    else config.max_width = lastMaxWidth; // 空欄にされたら直前の値へ戻す
  }
</script>

<div class="panel">
  <div class="scroll">
    <Card level={1} title="変換モード">
      <SegmentedButton
        bind:value={config.mode}
        label="変換モード"
        options={[
          { value: "crop", label: "Crop" },
          { value: "pad", label: "Pad" },
          { value: "quality", label: "Quality" },
        ]}
      />
      {#if config.mode === "pad"}
        <div class="sub">
          <SegmentedButton
            bind:value={config.bg_color}
            label="背景色"
            options={[
              { value: "white", label: "白" },
              { value: "black", label: "黒" },
            ]}
          />
        </div>
      {/if}
    </Card>

    <Card level={1} title="出力">
      <Slider bind:value={config.quality} label="品質" min={1} max={100} suffix="%" />
      <div class="sub">
        <TextField
          bind:value={config.max_size_mb}
          label="最大サイズ"
          type="number"
          suffix="MB"
          min={1}
          max={1024}
          normalize={(v) => Math.min(1024, Math.max(1, Math.round(v)))}
        />
      </div>
      <div class="sub">
        <Switch
          checked={maxWidthEnabled}
          label="出力幅を制限する"
          disabled={config.mode === "quality"}
          onchange={() => toggleMaxWidth(!maxWidthEnabled)}
        />
        {#if config.mode === "quality"}
          <p class="hint">
            Quality モードは 4:5 に変換しないため、出力幅の上限は適用されません。
          </p>
        {:else if config.max_width !== null}
          <div class="sub">
            <TextField
              bind:value={config.max_width}
              label="出力幅の上限"
              type="number"
              suffix="px"
              min={MAX_WIDTH_MIN}
              max={MAX_WIDTH_MAX}
              normalize={snapWidth}
              onchange={commitMaxWidth}
              hint={maxWidthLabel ? `→ ${maxWidthLabel}` : null}
            />
          </div>
        {/if}
      </div>
    </Card>

    {#if config.mode === "pad"}
      <Card level={1} title="Exif フレーム">
        <Switch bind:checked={exifFrameEnabled} label="Exif フレームを付ける" />
        {#if exifFrameEnabled}
          <div class="sub">
            <Select
              bind:value={selectedPresetName}
              label="プリセット"
              options={presetNames.map((name) => ({ value: name, label: name }))}
            />
          </div>
          <div class="sub">
            <Button variant="text" onclick={onEditFrame}>プリセットを編集...</Button>
          </div>
        {/if}
      </Card>
    {/if}

    <Card level={1} title="出力先">
      <p class="path" title={outputFolder || undefined}>
        {outputFolder || "未選択"}
      </p>
      <Button variant="outlined" onclick={onPickOutputFolder}>フォルダーを選択...</Button>
    </Card>

    <Card level={1} title="元ファイル">
      <Switch bind:checked={config.delete_originals} label="元ファイルを削除" danger />
      {#if config.delete_originals}
        <p class="hint danger">
          変換実行時に確認します。削除したファイルは元に戻せません。
        </p>
      {/if}
    </Card>
  </div>

  <!-- 主ボタンはパネル最下部に固定（spec §5-1） -->
  <div class="action">
    <Button variant="filled" full disabled={!canProcess} onclick={onProcess}>
      {selectedCount} 枚を変換
    </Button>
  </div>
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    /* rail の切替は 150ms のフェードのみ（spec §3-3） */
    animation: fade-in var(--md-sys-motion-duration-short)
      var(--md-sys-motion-easing-standard);
  }

  @keyframes fade-in {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }

  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-3);
  }

  .sub {
    margin-top: var(--space-3);
  }

  .hint {
    margin: var(--space-2) 0 0;
    font: var(--md-sys-typescale-body-sm);
    color: var(--md-sys-color-on-surface-variant);
  }

  .hint.danger {
    color: var(--md-sys-color-error);
  }

  .path {
    margin: 0 0 var(--space-3);
    font: var(--md-sys-typescale-body-sm);
    color: var(--md-sys-color-on-surface-variant);
    overflow-wrap: anywhere;
  }

  .action {
    flex-shrink: 0;
    padding: var(--space-3);
    border-top: 1px solid var(--md-sys-color-outline-variant);
    background: var(--md-sys-color-surface-container);
  }
</style>
```

**`max_size_mb` を `Slider` から `TextField` に変える**のは spec §5-1 の表どおり
（「最大サイズ `TextField`」）。現行はスライダーで、8MB のような具体値に
合わせづらかった。

- [ ] **Step 2: `App.svelte` の選択を `SvelteSet<string>` にする**

```ts
  import { SvelteSet } from "svelte/reactivity";

  // 変換モード専用。フォルダーを変えたらクリアする（spec §3-2）
  const selectedPaths = new SvelteSet<string>();

  let selectedImages = $derived(images.filter((img) => selectedPaths.has(img.path)));
```

`handleToggleSelect` を差し替える。**クリックは「選択のトグル ＋ `focusedPath` の移動」を
同時に行う**（spec §3-2）:

```ts
  function handleToggleSelect(image: ImageEntry) {
    if (selectedPaths.has(image.path)) selectedPaths.delete(image.path);
    else selectedPaths.add(image.path);
    focusedPath = image.path;
  }

  function handleClearSelection() {
    selectedPaths.clear();
  }
```

`handleSelectFolder` に `selectedPaths.clear()` を足す:

```ts
  async function handleSelectFolder(path: string) {
    currentFolder = path;
    currentPage = 0;
    focusedPath = null;
    // 選択は常に現在のフォルダー内に閉じる。SelectionList を廃止したので
    // 画面外の選択を可視化・解除する窓口がもう無い（spec §3-2 / §5-1）
    selectedPaths.clear();
    // …以下は現行のまま
  }
```

- [ ] **Step 3: `SettingsPanel` と `SelectionList` を消す**

```bash
git rm gui-frontend/src/lib/SettingsPanel.svelte gui-frontend/src/lib/SelectionList.svelte
grep -rn "SettingsPanel\|SelectionList" gui-frontend/src/
```

期待: 0 件。

`App.svelte` のテンプレートで `SettingsPanel` を `ConvertPanel` に差し替える:

```svelte
    {#if mode === "convert"}
      <ConvertPanel
        bind:config
        {outputFolder}
        selectedCount={selectedPaths.size}
        {canProcess}
        bind:exifFrameEnabled
        presetNames={presets.presets.map((p) => p.name)}
        bind:selectedPresetName={presets.selectedName}
        onPickOutputFolder={handlePickOutputFolder}
        onProcess={handleProcess}
        onEditFrame={() => (mode = "frame")}
      />
```

`bind:selectedPresetName={presets.selectedName}` は、`presets` が getter/setter を
持つオブジェクトなので成立する（Task 7 で `set selectedName` を定義済み）。

- [ ] **Step 4: `test-integrity` スキルを起動する**

- [ ] **Step 5: 変換パネルの検査を書く**

`gui-frontend/e2e/convert.spec.ts`:

```ts
import { expect, test } from "@playwright/test";
import { clearStorageOnce, installTauriStub, toggleSwitch } from "./stub";

test.describe("変換パネル", () => {
  test.beforeEach(async ({ page }) => {
    await installTauriStub(page, { imageCount: 12 });
    await clearStorageOnce(page);
    await page.goto("/");
    await page.getByRole("button", { name: "photos", exact: false }).first().click();
  });

  test("主ボタンが選択枚数を持つ", async ({ page }) => {
    await expect(page.getByRole("button", { name: "0 枚を変換" })).toBeDisabled();
    await page.getByRole("button", { name: /photo-0000/ }).click();
    await expect(page.getByRole("button", { name: "1 枚を変換" })).toBeVisible();
  });

  test("フォルダーを変えると選択がクリアされる（spec §3-2）", async ({ page }) => {
    await page.getByRole("button", { name: /photo-0000/ }).click();
    // 前提条件: いま実際に 1 枚選ばれていること。0 枚のままだと
    // 「クリアされた」は自明に成立してしまう
    await expect(page.getByRole("button", { name: "1 枚を変換" })).toBeVisible();

    await page.getByRole("button", { name: "archive", exact: false }).first().click();
    await expect(page.getByRole("button", { name: "0 枚を変換" })).toBeVisible();
  });

  test("quality モードでは出力幅の制限が無効になる", async ({ page }) => {
    await page.getByRole("radio", { name: "Quality" }).click();
    await expect(page.getByRole("checkbox", { name: "出力幅を制限する" })).toBeDisabled();
    await expect(
      page.getByText("Quality モードは 4:5 に変換しないため")
    ).toBeVisible();
  });

  test("出力幅は 4 の倍数へ切り捨てられる", async ({ page }) => {
    await page.getByRole("radio", { name: "Pad" }).click();
    // Switch の input は .track に覆われて直接クリックできない（Task 4 Step 2）
    await toggleSwitch(page, "出力幅を制限する");

    const input = page.getByLabel("出力幅の上限");
    await expect(input).toHaveValue("1080");
    await input.fill("1002");
    await input.blur();
    await expect(input).toHaveValue("1000");
    await expect(page.getByText("→ 1000×1250")).toBeVisible();
  });

  test("元ファイル削除は確認ダイアログを挟む", async ({ page }) => {
    await page.getByRole("button", { name: /photo-0000/ }).click();
    await page.getByRole("button", { name: "フォルダーを選択..." }).click();
    await toggleSwitch(page, "元ファイルを削除");
    await page.getByRole("button", { name: "1 枚を変換" }).click();

    const dialog = page.getByRole("alertdialog", { name: "元ファイルを削除します" });
    await expect(dialog).toBeVisible();
    // 破壊的操作なので初期フォーカスはキャンセル側
    await expect(dialog.getByRole("button", { name: "キャンセル" })).toBeFocused();
  });
});
```

`page.getByRole("button", { name: /photo-0000/ })` はこの時点ではまだ
現行 `ThumbnailGrid` のタイル（`<button>`）を指す。Task 13 で `option` に変わるので、
そのときこのセレクタも `getByRole("option", …)` に直すこと。

- [ ] **Step 6: 走らせる**

```bash
cd gui-frontend && bun run typecheck && bun test && bun run e2e
```

- [ ] **Step 7: 実機で変換が最後まで通ることを確認する（段階 5 の完了の目印）**

```bash
make dev
```

フォルダー選択 → 写真を数枚選ぶ → 出力先を選ぶ → 「N 枚を変換」→ 進捗 → 結果ダイアログ。
crop / pad / quality の 3 モード、出力幅の制限あり／なし、Exif フレームあり／なしを
それぞれ 1 回ずつ通す。

- [ ] **Step 8: コミット**

```bash
git add -A gui-frontend/src
git commit -m "feat(gui): 変換パネルをプリミティブで再構築し SelectionList を廃止"
```

---

## Task 11: グリッドヘッダーと右パネルの折りたたみ（段階 5 の 2/2）

spec §3-1「右パネルの折りたたみ」/ §5-1。

**折りたたみは幅とは別の boolean 状態として持つ。幅 0 で畳む実装にはしない**（spec §3-1）:
畳むためのボタンがパネル内にあると畳んだ瞬間に開くボタンごと消えるうえ、
主ボタン「N 枚を変換」もパネル最下部にあるため変換の主導線が消える。
さらにクランプ規則（範囲外の幅は既定値へ落とす）と幅 0 が正面から衝突する。

**Files:**
- Create: `gui-frontend/src/lib/browser/GridHeader.svelte`
- Modify: `gui-frontend/src/lib/ThumbnailGrid.svelte`（ヘッダーを差し替え。中身は据え置き）
- Modify: `gui-frontend/src/App.svelte`

**Interfaces:**
- Produces: `GridHeader`
  `{ totalCount: number, selectedCount: number, selectionMode: "multi" | "single",
     rightPanelCollapsed: boolean, onToggleRightPanel: () => void,
     onClearSelection: () => void,
     controls?: Snippet, primaryAction?: Snippet }`
  — `controls` はグリッド固有の操作（Task 11 では列スライダーとページ送り、
    Task 13 ではサイズスライダー）。`primaryAction` は**畳んでいる間だけ**出す

- [ ] **Step 1: `GridHeader.svelte` を書く**

```svelte
<script lang="ts">
  import type { Snippet } from "svelte";
  import Button from "../ui/Button.svelte";
  import IconButton from "../ui/IconButton.svelte";

  interface Props {
    totalCount: number;
    selectedCount: number;
    selectionMode: "multi" | "single";
    rightPanelCollapsed: boolean;
    onToggleRightPanel: () => void;
    onClearSelection: () => void;
    /** グリッド固有の操作（サイズスライダーなど） */
    controls?: Snippet;
    /** 右パネルを畳んでいる間だけ出す主アクション（spec §3-1） */
    primaryAction?: Snippet;
  }

  let {
    totalCount,
    selectedCount,
    selectionMode,
    rightPanelCollapsed,
    onToggleRightPanel,
    onClearSelection,
    controls,
    primaryAction,
  }: Props = $props();
</script>

<div class="grid-header">
  <span class="count">{totalCount} 枚</span>

  {#if selectionMode === "multi" && selectedCount > 0}
    <!-- 3,000 枚あれば選択中の写真は画面外に出る。✓ だけでは全容が分からないので
         ここに枚数と全解除を置く（spec §5-1） -->
    <span class="selected">{selectedCount} 枚選択中</span>
    <Button variant="text" onclick={onClearSelection}>全解除</Button>
  {/if}

  <div class="spacer"></div>

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

<style>
  .grid-header {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-3);
    border-bottom: 1px solid var(--md-sys-color-outline-variant);
    background: var(--md-sys-color-surface);
    font: var(--md-sys-typescale-body-sm);
    color: var(--md-sys-color-on-surface-variant);
  }

  .selected {
    color: var(--md-sys-color-primary);
  }

  .spacer {
    flex: 1;
    min-width: 0;
  }

  .controls,
  .primary {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }
</style>
```

- [ ] **Step 2: `ThumbnailGrid.svelte` のヘッダーを差し替える**

`<script>` に import を 2 行足す:

```ts
  import type { Snippet } from "svelte";
  import GridHeader from "./browser/GridHeader.svelte";
```

`.grid-header` の `<div>` ごと `<GridHeader>` に置き換え、
既存の列スライダーとページ送りを `controls` snippet に移す。
`<style>` から `.grid-header` / `.pagination` / `.toolbar-right` の規則は消す
（`GridHeader` 側に移った）。**`.grid` 以下は触らない。**

props を 5 つ増やす:

```ts
    selectedCount: number;
    rightPanelCollapsed: boolean;
    onToggleRightPanel: () => void;
    onClearSelection: () => void;
    primaryAction?: Snippet;
```

マークアップ:

```svelte
<div class="thumbnail-grid">
  <GridHeader
    totalCount={images.length}
    {selectedCount}
    selectionMode="multi"
    {rightPanelCollapsed}
    {onToggleRightPanel}
    {onClearSelection}
    {primaryAction}
  >
    {#snippet controls()}
      <div class="size-control">
        <label class="size-label" for="grid-columns">列</label>
        <input id="grid-columns" type="range" min="2" max="8"
          bind:value={columnCount} class="size-slider" />
      </div>
      {#if totalPages > 1}
        <div class="pagination">
          <button aria-label="前のページ"
            onclick={() => onPageChange(Math.max(0, currentPage - 1))}
            disabled={currentPage === 0}>←</button>
          <span>{currentPage + 1} / {totalPages}</span>
          <button aria-label="次のページ"
            onclick={() => onPageChange(Math.min(totalPages - 1, currentPage + 1))}
            disabled={currentPage >= totalPages - 1}>→</button>
        </div>
      {/if}
    {/snippet}
  </GridHeader>

  <div class="grid" …現行のまま…>
```

`.pagination` の規則は `ThumbnailGrid` の `<style>` に残す（`controls` の中身なので）。

- [ ] **Step 3: `App.svelte` から折りたたみ状態を配る**

```svelte
    <ThumbnailGrid
      …現行の props…
      selectedCount={selectedPaths.size}
      rightPanelCollapsed={layout.rightPanelCollapsed}
      onToggleRightPanel={() =>
        (layout.rightPanelCollapsed = !layout.rightPanelCollapsed)}
      onClearSelection={handleClearSelection}
      primaryAction={collapsedPrimaryAction}
    />
```

`App.svelte` に snippet を定義する（テンプレートの末尾、`</AppShell>` の後）:

```svelte
{#snippet collapsedPrimaryAction()}
  {#if mode === "convert"}
    <Button variant="filled" disabled={!canProcess} onclick={handleProcess}>
      {selectedPaths.size} 枚を変換
    </Button>
  {:else if mode === "metadata"}
    <!-- メタデータの保存は次工程で配線する（spec §5-2）。
         畳んでいる間に主導線が消えないよう、場所だけ先に確保しておく -->
    <Button variant="filled" disabled>保存</Button>
  {/if}
{/snippet}
```

- [ ] **Step 4: `test-integrity` スキルを起動する**

- [ ] **Step 5: 折りたたみの検査を書く**

`gui-frontend/e2e/shell.spec.ts` に追加:

```ts
test("右パネルを畳んでも開くボタンと主アクションが残る（spec §3-1）", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "photos", exact: false }).first().click();

  // 前提条件: 畳む前は右パネルの中に主ボタンがある
  await expect(page.getByRole("button", { name: "0 枚を変換" })).toHaveCount(1);
  await expect(page.getByRole("separator", { name: "右パネルの幅" })).toBeVisible();

  await page.getByRole("button", { name: "右パネルを畳む" }).click();

  // 幅 0 で畳む実装ではないので、リサイザーごと消える
  await expect(page.getByRole("separator", { name: "右パネルの幅" })).toHaveCount(0);
  // 開くボタンはグリッドヘッダー（パネルの外）にあるので残る
  await expect(page.getByRole("button", { name: "右パネルを開く" })).toBeVisible();
  // 主導線もヘッダーへ移る
  await expect(page.getByRole("button", { name: "0 枚を変換" })).toHaveCount(1);

  await page.getByRole("button", { name: "右パネルを開く" }).click();
  await expect(page.getByRole("separator", { name: "右パネルの幅" })).toBeVisible();
});

test("折りたたみ状態はリロード後も保たれ、幅とは別キーである", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "右パネルを畳む" }).click();
  await page.reload();
  await expect(page.getByRole("button", { name: "右パネルを開く" })).toBeVisible();

  // 幅のキーは壊れていない（開いたら既定幅で戻る）
  await page.getByRole("button", { name: "右パネルを開く" }).click();
  await expect(page.getByRole("separator", { name: "右パネルの幅" })).toHaveAttribute(
    "aria-valuenow",
    "320"
  );
});

test("選択枚数と全解除がグリッドヘッダーに出る（spec §5-1）", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "photos", exact: false }).first().click();

  // 前提条件: 選択が 0 のときは出ない
  await expect(page.getByText(/枚選択中/)).toHaveCount(0);

  await page.getByRole("button", { name: /photo-0000/ }).click();
  await page.getByRole("button", { name: /photo-0001/ }).click();
  await expect(page.getByText("2 枚選択中")).toBeVisible();

  await page.getByRole("button", { name: "全解除" }).click();
  await expect(page.getByText(/枚選択中/)).toHaveCount(0);
});
```

- [ ] **Step 6: 走らせる**

```bash
cd gui-frontend && bun run typecheck && bun test && bun run e2e
```

- [ ] **Step 7: コミット**

```bash
git add gui-frontend/src gui-frontend/e2e
git commit -m "feat(gui): グリッドヘッダーと右パネルの折りたたみを追加"
```

---

## Task 12: サムネイル取得キューの仕様変更（段階 6 の 1/4）

spec §4-2。**現行のページネーション（50 枚固定）は、意図せず 2 つの上限として
機能していた。廃止するなら、代わりの上限を明示的に入れる必要がある。**

| 現行が偶然抑えていたもの | 廃止後に起きること | 対策 |
|---|---|---|
| 取得キューの長さ | FIFO でスクロールアウトした要求を捨てない。3,000 枚を高速スクロールすると可視分の要求が過去の要求の後ろで待つ | **LIFO に変更し、可視範囲を外れた未処理要求は破棄する** |
| キャッシュの総量 | eviction が無い。サイズスライダーで解像度別のキーが増える | **LRU でバイト上限を設ける** |

**「範囲外を捨てる」をキュー全体に効かせてはならない。** サムネイルキャッシュとキューは
全モードで共有しており、グリッド以外にも要求元がある（フィルムストリップ、
メタデータパネルのサムネイル、フレームモードの見本写真）。素朴に実装すると
**フィルムストリップが埋まらない／メタデータパネルのサムネイルが出ない**が起きる。

**Files:**
- Create: `gui-frontend/src/lib/browser/thumbnailCache.ts`（**純粋**）
- Create: `gui-frontend/src/lib/browser/thumbnailCache.test.ts`
- Create: `gui-frontend/src/lib/browser/requestQueue.ts`（**純粋**）
- Create: `gui-frontend/src/lib/browser/requestQueue.test.ts`
- Modify: `gui-frontend/src/lib/browser/thumbnailQueue.svelte.ts`
- Modify: `gui-frontend/src/App.svelte`（`resetForFolder` の呼び出し、dev 用の統計公開）

**Interfaces:**
- Produces: `LruBudget`（`thumbnailCache.ts`）
  - `new LruBudget(byteLimit: number)`
  - `readonly bytes: number` / `readonly size: number` / `readonly byteLimit: number`
  - `has(key: string): boolean`
  - `touch(key: string): void` — 参照された。順序だけ更新する
  - `admit(key: string, bytes: number): string[]` — 追加し、追い出すべきキーを返す
  - `remove(key: string): void` / `clear(): void`
- Produces: `RequestQueue`（`requestQueue.ts`）
  - `type RequestKind = "discardable" | "pinned"`
  - `interface ThumbnailRequest { key: string; path: string; size: number; kind: RequestKind; index: number }`
  - `push(request: ThumbnailRequest): void` / `take(): ThumbnailRequest | undefined`
  - `setVisibleRange(start: number, end: number): number` — 捨てた件数を返す
  - `reset(primeCount: number): void` / `clear(): void`
  - `readonly pendingCount: number` / `has(key: string): boolean`
- Produces: `ThumbnailQueue`（`thumbnailQueue.svelte.ts`）— 既存の `get` / `request` に加えて
  - `request(path, maxDimension, kind?: RequestKind, index?: number)`（既定は `"pinned"`, `-1`）
  - `setVisibleRange(start: number, end: number): void`
  - `resetForFolder(primeCount: number): void`
  - `stats(): { bytes: number; entries: number }`（計測用。Task 15 が読む）

- [ ] **Step 1: `test-integrity` スキルを起動する**

- [ ] **Step 2: `thumbnailCache.test.ts` を書く**

```ts
/**
 * spec §4-2「キャッシュの総量」。
 *
 * LRU は「値の保管庫」ではなく「どのキーをどの順で持っているかの台帳」として
 * 実装する。値そのものは reactive な SvelteMap 側が持ち、ここは順序とバイト数
 * だけを見る。こうすると UI を起動せずに追い出し規則を検査できる。
 */
import { describe, expect, test } from "bun:test";
import { LruBudget } from "./thumbnailCache";

describe("LruBudget", () => {
  test("上限内なら何も追い出さない", () => {
    const lru = new LruBudget(100);
    expect(lru.admit("a", 30)).toEqual([]);
    expect(lru.admit("b", 30)).toEqual([]);
    expect(lru.bytes).toBe(60);
    expect(lru.size).toBe(2);
  });

  test("上限を超えたら古い順に追い出す", () => {
    const lru = new LruBudget(100);
    lru.admit("a", 40);
    lru.admit("b", 40);
    expect(lru.admit("c", 40)).toEqual(["a"]);
    expect(lru.has("a")).toBe(false);
    expect(lru.bytes).toBe(80);
  });

  test("touch した項目は新しい扱いになり、次の追い出しを免れる", () => {
    const lru = new LruBudget(100);
    lru.admit("a", 40);
    lru.admit("b", 40);
    // 前提条件: touch しなければ a が追い出される（上の test で確認済みの挙動）
    lru.touch("a");
    expect(lru.admit("c", 40)).toEqual(["b"]);
    expect(lru.has("a")).toBe(true);
  });

  test("上限を 1 件で超える項目は保持する（追い出して空にしない）", () => {
    // ここを「新入りごと捨てる」にすると、大きなサムネイルが永久にキャッシュ
    // されず毎回 IPC が走る。上限は目安であって不変条件ではない
    const lru = new LruBudget(100);
    lru.admit("small", 50);
    expect(lru.admit("huge", 500)).toEqual(["small"]);
    expect(lru.has("huge")).toBe(true);
    expect(lru.bytes).toBe(500);
  });

  test("同じキーを再度 admit してもバイト数が二重に積まれない", () => {
    const lru = new LruBudget(1000);
    lru.admit("a", 40);
    lru.admit("a", 60);
    expect(lru.size).toBe(1);
    expect(lru.bytes).toBe(60);
  });

  test("remove と clear がバイト数を戻す", () => {
    const lru = new LruBudget(1000);
    lru.admit("a", 40);
    lru.admit("b", 60);
    lru.remove("a");
    expect(lru.bytes).toBe(60);
    lru.clear();
    expect(lru.bytes).toBe(0);
    expect(lru.size).toBe(0);
  });

  test("存在しないキーへの touch / remove は無視される", () => {
    const lru = new LruBudget(100);
    lru.admit("a", 40);
    lru.touch("missing");
    lru.remove("missing");
    expect(lru.size).toBe(1);
    expect(lru.bytes).toBe(40);
  });
});
```

- [ ] **Step 3: `thumbnailCache.ts` を書く**

```ts
/**
 * サムネイルキャッシュの追い出し規則（LRU + バイト上限）。
 *
 * 値は持たない。「どのキーを、どの順で、何バイトで持っているか」だけを持つ台帳。
 * 値は thumbnailQueue.svelte.ts の SvelteMap 側にある。
 *
 * Map の反復順は挿入順なので、先頭が最も古い＝次に追い出す対象になる。
 */
export class LruBudget {
  readonly byteLimit: number;
  /** key -> bytes。反復順が LRU 順（先頭が最古） */
  #entries = new Map<string, number>();
  #bytes = 0;

  constructor(byteLimit: number) {
    this.byteLimit = byteLimit;
  }

  get bytes(): number {
    return this.#bytes;
  }

  get size(): number {
    return this.#entries.size;
  }

  has(key: string): boolean {
    return this.#entries.has(key);
  }

  /** 参照された。順序だけ最新へ動かす */
  touch(key: string): void {
    const bytes = this.#entries.get(key);
    if (bytes === undefined) return;
    this.#entries.delete(key);
    this.#entries.set(key, bytes);
  }

  /**
   * 追加し、上限を超えた分として追い出すべきキーを古い順に返す。
   *
   * 新しく入れた項目自体は追い出さない。1 件で上限を超える大きさでも保持する
   * （捨てると毎回 IPC が走るだけで、上限を守る意味が無い）。
   */
  admit(key: string, bytes: number): string[] {
    this.remove(key);
    this.#entries.set(key, bytes);
    this.#bytes += bytes;

    const evicted: string[] = [];
    for (const oldest of this.#entries.keys()) {
      if (this.#bytes <= this.byteLimit) break;
      if (oldest === key) continue;
      evicted.push(oldest);
      this.#bytes -= this.#entries.get(oldest)!;
      this.#entries.delete(oldest);
    }
    return evicted;
  }

  remove(key: string): void {
    const bytes = this.#entries.get(key);
    if (bytes === undefined) return;
    this.#entries.delete(key);
    this.#bytes -= bytes;
  }

  clear(): void {
    this.#entries.clear();
    this.#bytes = 0;
  }
}
```

**`for (const oldest of this.#entries.keys())` の中で `delete` している。**
`Map` の反復子は反復中の削除に対して安全（削除済みの要素をスキップして進む）。
`continue` で飛ばした `key` は末尾にあるので、次の反復で先頭には来ない。

- [ ] **Step 4: `requestQueue.test.ts` を書く**

```ts
/**
 * spec §4-2「取得キューの長さ」。
 *
 * 検査するのは 3 点:
 *  - LIFO であること（可視分の要求が過去の要求の後ろで待たない）
 *  - 初回の 1 画面分だけは投入順（上から）で流れること
 *  - 範囲外の破棄が discardable にだけ効き、pinned に効かないこと
 */
import { describe, expect, test } from "bun:test";
import { RequestQueue, type ThumbnailRequest } from "./requestQueue";

function req(
  index: number,
  kind: ThumbnailRequest["kind"] = "discardable"
): ThumbnailRequest {
  return { key: `p${index}:200`, path: `/photos/${index}.jpg`, size: 200, kind, index };
}

describe("RequestQueue", () => {
  test("priming が無ければ LIFO で取り出す", () => {
    const q = new RequestQueue();
    q.push(req(1));
    q.push(req(2));
    q.push(req(3));
    expect(q.take()?.index).toBe(3);
    expect(q.take()?.index).toBe(2);
    expect(q.take()?.index).toBe(1);
    expect(q.take()).toBeUndefined();
  });

  test("初回の 1 画面分は投入順（上から）で流れる", () => {
    // LIFO だけにすると最上行が最後に読まれ、下から埋まって見える。
    // 実害は無いが印象に効くので初回だけ FIFO にする（spec §4-2）
    const q = new RequestQueue();
    q.reset(3);
    q.push(req(1));
    q.push(req(2));
    q.push(req(3));
    q.push(req(4));
    q.push(req(5));
    expect([q.take()?.index, q.take()?.index, q.take()?.index]).toEqual([1, 2, 3]);
    // priming を使い切ったら LIFO に戻る
    expect(q.take()?.index).toBe(5);
    expect(q.take()?.index).toBe(4);
  });

  test("同じキーを二重に積まない", () => {
    const q = new RequestQueue();
    q.push(req(1));
    q.push(req(1));
    expect(q.pendingCount).toBe(1);
  });

  test("取り出したキーは再度 push できる", () => {
    const q = new RequestQueue();
    q.push(req(1));
    q.take();
    q.push(req(1));
    expect(q.pendingCount).toBe(1);
  });

  test("可視範囲を外れた discardable を捨てる", () => {
    const q = new RequestQueue();
    for (const i of [1, 2, 30, 31]) q.push(req(i));
    // 前提条件: 捨てる前は 4 件ある（0 件だと「捨てた」が自明に成立する）
    expect(q.pendingCount).toBe(4);

    expect(q.setVisibleRange(0, 10)).toBe(2);
    expect(q.pendingCount).toBe(2);
    expect(q.has("p30:200")).toBe(false);
    expect(q.has("p1:200")).toBe(true);
  });

  test("pinned は範囲外でも捨てない（spec §4-2）", () => {
    // フィルムストリップ・メタデータパネルのサムネイル・フレームの見本写真は
    // グリッドの index 範囲に入らない。捨てると永久に埋まらなくなる
    const q = new RequestQueue();
    q.push(req(500, "pinned"));
    q.push(req(501, "discardable"));
    expect(q.pendingCount).toBe(2);

    expect(q.setVisibleRange(0, 10)).toBe(1);
    expect(q.has("p500:200")).toBe(true);
    expect(q.has("p501:200")).toBe(false);
  });

  test("priming に積まれた discardable も範囲外なら捨てる", () => {
    const q = new RequestQueue();
    q.reset(5);
    q.push(req(1));
    q.push(req(99));
    q.setVisibleRange(0, 10);
    expect(q.has("p99:200")).toBe(false);
    expect(q.take()?.index).toBe(1);
  });

  test("reset で残った要求を捨て、priming を張り直す", () => {
    const q = new RequestQueue();
    q.push(req(1));
    q.push(req(2));
    q.reset(2);
    expect(q.pendingCount).toBe(0);
    q.push(req(10));
    q.push(req(11));
    q.push(req(12));
    expect([q.take()?.index, q.take()?.index]).toEqual([10, 11]);
  });
});
```

- [ ] **Step 5: 落ちることを確認してから `requestQueue.ts` を書く**

```bash
cd gui-frontend && bun test src/lib/browser/
```

期待: モジュールが無くて落ちる。その後で実装する。

```ts
export type RequestKind = "discardable" | "pinned";

export interface ThumbnailRequest {
  /** `path:size`。キャッシュのキーと同じもの */
  key: string;
  path: string;
  size: number;
  /**
   * discardable — グリッド由来。可視範囲を外れたら未処理のものを捨てる
   * pinned      — それ以外の要求元。範囲による破棄の対象外（spec §4-2）
   */
  kind: RequestKind;
  /** グリッド上の通し番号。pinned は -1 でよい */
  index: number;
}

/**
 * サムネイル取得の待ち行列。
 *
 * 基本は LIFO。最後に要求されたもの＝いま見えているものを先に処理する。
 * ただし初回の 1 画面分だけは投入順で流す（下から埋まって見えるのを避ける）。
 *
 * 可視範囲による破棄は setVisibleRange が呼ばれたときにだけ起きる。
 * グリッドが unmount している間（フレームモード）は呼ばれないので、
 * 最後の範囲を保ったまま破棄も起きない ── これが spec §4-2 の求める挙動。
 */
export class RequestQueue {
  /** 初回の 1 画面分。FIFO で流す */
  #priming: ThumbnailRequest[] = [];
  /** それ以降。LIFO で流す */
  #stack: ThumbnailRequest[] = [];
  #keys = new Set<string>();
  #primingRemaining = 0;

  get pendingCount(): number {
    return this.#priming.length + this.#stack.length;
  }

  has(key: string): boolean {
    return this.#keys.has(key);
  }

  push(request: ThumbnailRequest): void {
    if (this.#keys.has(request.key)) return;
    this.#keys.add(request.key);
    if (this.#primingRemaining > 0) {
      this.#primingRemaining--;
      this.#priming.push(request);
    } else {
      this.#stack.push(request);
    }
  }

  take(): ThumbnailRequest | undefined {
    const next = this.#priming.shift() ?? this.#stack.pop();
    if (next) this.#keys.delete(next.key);
    return next;
  }

  /**
   * グリッドの可視範囲を通知する。範囲外の未処理 discardable を捨てる。
   * 戻り値は捨てた件数（検査と計測のため）。
   */
  setVisibleRange(start: number, end: number): number {
    const keep = (r: ThumbnailRequest) =>
      r.kind === "pinned" || (r.index >= start && r.index <= end);

    let dropped = 0;
    const drop = (list: ThumbnailRequest[]): ThumbnailRequest[] =>
      list.filter((r) => {
        if (keep(r)) return true;
        this.#keys.delete(r.key);
        dropped++;
        return false;
      });

    this.#priming = drop(this.#priming);
    this.#stack = drop(this.#stack);
    return dropped;
  }

  /** フォルダーを変えたとき。残りを捨てて priming を張り直す */
  reset(primeCount: number): void {
    this.clear();
    this.#primingRemaining = Math.max(0, primeCount);
  }

  clear(): void {
    this.#priming = [];
    this.#stack = [];
    this.#keys.clear();
  }
}
```

- [ ] **Step 6: 通ることを確認する**

```bash
cd gui-frontend && bun test src/lib/browser/
```

- [ ] **Step 7: `thumbnailQueue.svelte.ts` を組み直す**

```ts
import { SvelteMap } from "svelte/reactivity";
import { getThumbnail } from "../api";
import { describeError, toast } from "../toasts.svelte";
import { LruBudget } from "./thumbnailCache";
import { RequestQueue, type RequestKind } from "./requestQueue";

/**
 * サムネイルの取得キューとキャッシュ。
 *
 * 値は SvelteMap（リアクティブ）、順序とバイト数は LruBudget、
 * 待ち行列は RequestQueue が持つ。3 つとも役割が分かれている。
 */
export interface ThumbnailQueue {
  get(path: string, maxDimension: number): string | undefined;
  request(path: string, maxDimension: number, kind?: RequestKind, index?: number): void;
  setVisibleRange(start: number, end: number): void;
  resetForFolder(primeCount: number): void;
  stats(): { bytes: number; entries: number };
}

const MAX_CONCURRENT = 3;

/**
 * サムネイルキャッシュのバイト上限。
 *
 * **暫定値。Task 15（spec §7-2）の実測で確定させ、spec §4-2 / §8 に追記する。**
 * 1 枚あたりの実バイト数 × 保持したい枚数から決める。
 */
export const CACHE_BYTE_LIMIT = 64 * 1024 * 1024;

function keyOf(path: string, maxDimension: number): string {
  return `${path}:${maxDimension}`;
}

export function createThumbnailQueue(): ThumbnailQueue {
  const values = new SvelteMap<string, string>();
  const budget = new LruBudget(CACHE_BYTE_LIMIT);
  const queue = new RequestQueue();
  /** 同一キーの失敗を繰り返し再要求しないための記録 */
  const failed = new Set<string>();
  /** 処理中のキー。範囲外の破棄はここには効かない（spec §4-2） */
  const inFlight = new Set<string>();

  let active = 0;
  let errorReported = false;

  function pump() {
    while (active < MAX_CONCURRENT) {
      const request = queue.take();
      if (!request) return;
      if (values.has(request.key)) continue;

      active++;
      inFlight.add(request.key);
      getThumbnail(request.path, request.size)
        .then((base64) => {
          values.set(request.key, base64);
          // base64 は ASCII なので、文字数がそのまま保持バイト数の目安になる
          for (const evicted of budget.admit(request.key, base64.length)) {
            values.delete(evicted);
          }
        })
        .catch((e) => {
          failed.add(request.key);
          // 1 枚ごとにトーストを出すと壊れたフォルダーで埋め尽くされるため
          // 最初の 1 件だけ通知する
          if (!errorReported) {
            errorReported = true;
            toast.error(`サムネイルを生成できない画像があります: ${describeError(e)}`);
          }
        })
        .finally(() => {
          inFlight.delete(request.key);
          active--;
          pump();
        });
    }
  }

  return {
    get(path, maxDimension) {
      const key = keyOf(path, maxDimension);
      const value = values.get(key);
      // 参照されたら LRU 上で新しい扱いにする。値そのものは変えないので
      // リアクティブな読み取りの中から呼んでも再描画は誘発しない
      if (value !== undefined) budget.touch(key);
      return value;
    },

    request(path, maxDimension, kind = "pinned", index = -1) {
      const key = keyOf(path, maxDimension);
      if (values.has(key) || failed.has(key) || inFlight.has(key)) return;
      queue.push({ key, path, size: maxDimension, kind, index });
      pump();
    },

    setVisibleRange(start, end) {
      queue.setVisibleRange(start, end);
    },

    resetForFolder(primeCount) {
      queue.reset(primeCount);
      // キャッシュは残す。同じフォルダーへ戻ったときに再取得しないため。
      // 溢れれば LRU が古いフォルダー分から順に落とす
    },

    stats() {
      return { bytes: budget.bytes, entries: budget.size };
    },
  };
}
```

**`request` の既定の `kind` は `"pinned"`。**
グリッド以外の要求元（フィルムストリップ、メタデータのサムネイル、
フレームの見本写真）が `kind` を書き忘れても捨てられない側に倒れる。
捨てる側を明示的に書かせる。

- [ ] **Step 8: `App.svelte` から `resetForFolder` を呼び、dev 用に統計を出す**

`handleSelectFolder` の中、`selectedPaths.clear()` の隣に:

```ts
    // 初回 1 画面分の目安。正確な可視枚数は PhotoGrid が出すが、
    // ここでは「上から順に流す枚数」の見積もりで足りる
    thumbnails.resetForFolder(30);
```

`onMount` の中に足す（**`import.meta.env.DEV` で囲む。本番ビルドでは消える**）:

```ts
    if (import.meta.env.DEV) {
      (window as unknown as Record<string, unknown>).__thumbnailStats = thumbnails.stats;
    }
```

- [ ] **Step 9: 走らせる**

```bash
cd gui-frontend && bun run typecheck && bun test && bun run build && bun run e2e
```

`bun run build` の後、本番バンドルに `__thumbnailStats` が残っていないこと:

```bash
grep -c "__thumbnailStats" gui-frontend/dist/assets/*.js || echo "残っていない"
```

- [ ] **Step 10: 実機で確認する**

```bash
make dev
```

サムネイルが従来どおり出ること。列スライダーを動かして解像度が上がったときに
取り直されること。**この時点ではまだ仮想スクロールが無いので、
LIFO の効果は体感しづらい。壊れていないことだけを見る。**

- [ ] **Step 11: コミット**

```bash
git add gui-frontend/src
git commit -m "feat(gui): サムネイルキューを LIFO 化し LRU バイト上限と要求種別を導入"
```

---

## Task 13: 写真グリッドの刷新（段階 6 の 2/4）

spec §4-1 / §4-2 / §4-3。列数算出・仮想スクロール・`listbox` 化・キー割り当ての変更。

**キーボードとマウスの割り当ては現行から変わる**（spec §4-1）:

| 操作 | 動作 |
|---|---|
| クリック | multi: 選択トグル ＋ フォーカス移動 / single: フォーカス移動 |
| ダブルクリック | 全画面プレビュー |
| **Space** | クリックと同じ |
| **Enter** | 全画面プレビュー |
| ← → ↑ ↓ | フォーカス移動 |

現行のタイルは `<button>` で、**Enter は選択トグルでありプレビューではない**。
`<button>` のまま Enter をプレビューに割り当てると選択のキーボード操作が消えるため、
タイルを `role="option"` に変更する。

**タイル上に★（設定済みレーティング）と未保存マークは出さない**（spec §4-3）。
表示に必要な `read_image_metadata` は次工程で追加されるコマンドであり
（`ExifInfo` に `rating` は無い）、本刷新の制約と両立しない。
メタデータモードでは**フォーカス中の 1 枚を太いアウトラインで示すところまで**。

**Files:**
- Create: `gui-frontend/src/lib/browser/gridMetrics.ts`（**純粋**）
- Create: `gui-frontend/src/lib/browser/gridMetrics.test.ts`
- Create: `gui-frontend/src/lib/browser/PhotoGrid.svelte`
- Delete: `gui-frontend/src/lib/ThumbnailGrid.svelte`
- Modify: `gui-frontend/src/App.svelte`, `gui-frontend/e2e/convert.spec.ts`

**Interfaces:**
- Produces: `gridMetrics.ts`
  - 定数 `GRID_GAP = 8` / `GRID_PADDING = 12` / `LABEL_HEIGHT = 18` /
    `OVERSCAN_ROWS = 2` / `SIZE_STEP = 64` / `MIN_THUMB_SIZE = 96` / `MAX_THUMB_SIZE = 512`
  - `interface GridMetrics { columns; tileWidth; rowHeight; totalRows; thumbnailSize }`
  - `computeGridMetrics(containerWidth: number, targetTileWidth: number, itemCount: number): GridMetrics`
  - `interface VisibleRange { firstRow; lastRow; startIndex; endIndex; paddingTop; paddingBottom }`
  - `computeVisibleRange(metrics: GridMetrics, scrollTop: number, viewportHeight: number, itemCount: number): VisibleRange`
- Produces: `PhotoGrid`
  `{ images, selectionMode: "multi" | "single", selectedPaths: Set<string>,
     focusedPath: string | null,
     thumbnailFor, onRequestThumbnail, onVisibleRangeChange,
     onToggleSelect, onFocus, onPreview,
     selectedCount, rightPanelCollapsed, onToggleRightPanel, onClearSelection,
     primaryAction?, scrollTop: number (bindable) }`

- [ ] **Step 1: `test-integrity` スキルを起動する**

- [ ] **Step 2: `gridMetrics.test.ts` を書く**

```ts
/**
 * spec §4-1「密度と操作」/ §4-2「仮想スクロール」。
 *
 * 列数の決定を JS の単一のソースに寄せる、というのが設計の要点。
 * auto-fill に任せると仮想スクロールの行位置と 1px でもずれてスクロールが飛ぶ。
 * ここはその「単一のソース」を、UI を起動せずに検査する。
 *
 * 数値は spec §3-1 の実測表から取っている。
 */
import { describe, expect, test } from "bun:test";
import {
  GRID_GAP,
  GRID_PADDING,
  MAX_THUMB_SIZE,
  MIN_THUMB_SIZE,
  OVERSCAN_ROWS,
  computeGridMetrics,
  computeVisibleRange,
} from "./gridMetrics";

describe("computeGridMetrics", () => {
  // spec §3-1 の「打ち消し後の実測値」の表と一致すること。
  // ここが合わないなら spec の列数の議論ごと成り立たない
  const CASES: [width: number, target: number, columns: number][] = [
    [800, 200, 3], // 新既定 1440・変換（右 320）
    [760, 200, 3], // 新既定 1440・メタデータ（右 360）
    [1120, 200, 5], // 新既定 1440・右パネル折りたたみ
    [460, 200, 2], // minWidth 1100・変換（右 320）
    [560, 200, 2], // 打ち消し前の 1200
  ];
  for (const [width, target, columns] of CASES) {
    test(`幅 ${width} / N=${target} で ${columns} 列`, () => {
      expect(computeGridMetrics(width, target, 200).columns).toBe(columns);
    });
  }

  test("列数は cols = floor((内側 + gap) / (N + gap))", () => {
    const inner = 800 - GRID_PADDING * 2;
    const expected = Math.floor((inner + GRID_GAP) / (200 + GRID_GAP));
    expect(computeGridMetrics(800, 200, 200).columns).toBe(expected);
  });

  test("タイル幅は gap を差し引いた等分", () => {
    const m = computeGridMetrics(800, 200, 200);
    const inner = 800 - GRID_PADDING * 2;
    expect(m.tileWidth).toBeCloseTo((inner - GRID_GAP * (m.columns - 1)) / m.columns, 5);
  });

  test("極端に狭くても 1 列を下回らない", () => {
    expect(computeGridMetrics(50, 200, 10).columns).toBe(1);
    expect(computeGridMetrics(0, 200, 10).columns).toBe(1);
  });

  test("要求解像度は 64px 刻みに丸め、96〜512 に収める", () => {
    // 生の列幅をそのままキャッシュキーにすると 1px の差で別エントリになる
    const m = computeGridMetrics(800, 200, 200);
    expect(m.thumbnailSize % 64).toBe(0);
    expect(m.thumbnailSize).toBeGreaterThanOrEqual(MIN_THUMB_SIZE);
    expect(m.thumbnailSize).toBeLessThanOrEqual(MAX_THUMB_SIZE);
    expect(computeGridMetrics(50, 200, 10).thumbnailSize).toBe(MIN_THUMB_SIZE);
    expect(computeGridMetrics(4000, 2000, 10).thumbnailSize).toBe(MAX_THUMB_SIZE);
  });

  test("行数は列数から出る", () => {
    const m = computeGridMetrics(800, 200, 10); // 3 列
    expect(m.columns).toBe(3);
    expect(m.totalRows).toBe(4);
    expect(computeGridMetrics(800, 200, 0).totalRows).toBe(0);
  });

  test("行高は 4:5 のタイル ＋ ファイル名 ＋ gap", () => {
    const m = computeGridMetrics(800, 200, 200);
    expect(m.rowHeight).toBeGreaterThan((m.tileWidth * 5) / 4);
  });
});

describe("computeVisibleRange", () => {
  const metrics = computeGridMetrics(800, 200, 3000); // 3 列

  test("先頭では前方の余白が 0 で、後方に残り全部が積まれる", () => {
    const r = computeVisibleRange(metrics, 0, 800, 3000);
    expect(r.firstRow).toBe(0);
    expect(r.startIndex).toBe(0);
    expect(r.paddingTop).toBe(0);
    expect(r.paddingBottom).toBeGreaterThan(0);
  });

  test("前後 OVERSCAN_ROWS 行分を余分に描く", () => {
    const r = computeVisibleRange(metrics, metrics.rowHeight * 10, 800, 3000);
    expect(r.firstRow).toBe(10 - OVERSCAN_ROWS);
  });

  test("前後の余白の合計 ＋ 描画分の高さが総高と一致する", () => {
    // ここがずれるとスクロールバーの長さが動き、スクロールが飛ぶ
    for (const scrollTop of [0, 500, 5000, 50_000]) {
      const r = computeVisibleRange(metrics, scrollTop, 800, 3000);
      const rendered = (r.lastRow - r.firstRow + 1) * metrics.rowHeight;
      expect(r.paddingTop + rendered + r.paddingBottom).toBeCloseTo(
        metrics.totalRows * metrics.rowHeight,
        5
      );
    }
  });

  test("最終行を超えてスクロールしても範囲が要素数を超えない", () => {
    const r = computeVisibleRange(metrics, 10_000_000, 800, 3000);
    expect(r.lastRow).toBe(metrics.totalRows - 1);
    expect(r.endIndex).toBe(2999);
    expect(r.paddingBottom).toBe(0);
  });

  test("要素が 0 件なら空の範囲を返す", () => {
    const empty = computeGridMetrics(800, 200, 0);
    const r = computeVisibleRange(empty, 0, 800, 0);
    expect(r.startIndex).toBe(0);
    expect(r.endIndex).toBe(-1);
    expect(r.paddingTop).toBe(0);
    expect(r.paddingBottom).toBe(0);
  });

  test("行高が 0 になっても無限ループや NaN を出さない", () => {
    // 初回描画で clientWidth が 0 の瞬間に通る経路
    const zero = computeGridMetrics(0, 0, 100);
    const r = computeVisibleRange(zero, 0, 0, 100);
    expect(Number.isFinite(r.startIndex)).toBe(true);
    expect(Number.isFinite(r.endIndex)).toBe(true);
    expect(r.endIndex).toBeGreaterThanOrEqual(-1);
  });
});
```

- [ ] **Step 3: 落ちることを確認してから `gridMetrics.ts` を書く**

```ts
/**
 * 写真グリッドの寸法計算。
 *
 * 列数の決定を JS の単一のソースに寄せる（spec §4-1）。
 * `auto-fill minmax(N, 1fr)` を CSS に任せる案は退けた ── 仮想スクロールの
 * 行高計算には列数が必須で、CSS 側が決めた折り返しを JS で再現すると
 * 1px のずれで行位置がずれてスクロールが飛ぶ。
 *
 * ここの px はレイアウトの構造的な寸法であり、トークン化の対象ではない。
 */
export const GRID_GAP = 8;
export const GRID_PADDING = 12;
/** タイル下のファイル名 1 行分 */
export const LABEL_HEIGHT = 18;
/** 可視行の前後に余分に描く行数 */
export const OVERSCAN_ROWS = 2;
/** サムネイル要求サイズの丸め幅。1px の差で別キーにしないため */
export const SIZE_STEP = 64;
export const MIN_THUMB_SIZE = 96;
export const MAX_THUMB_SIZE = 512;

export interface GridMetrics {
  columns: number;
  tileWidth: number;
  rowHeight: number;
  totalRows: number;
  /** getThumbnail に渡す maxDimension */
  thumbnailSize: number;
}

export function computeGridMetrics(
  containerWidth: number,
  targetTileWidth: number,
  itemCount: number
): GridMetrics {
  const inner = Math.max(0, containerWidth - GRID_PADDING * 2);
  const target = Math.max(1, targetTileWidth);
  const columns = Math.max(1, Math.floor((inner + GRID_GAP) / (target + GRID_GAP)));
  const tileWidth = Math.max(0, (inner - GRID_GAP * (columns - 1)) / columns);
  const rowHeight = (tileWidth * 5) / 4 + LABEL_HEIGHT + GRID_GAP;
  const totalRows = Math.ceil(itemCount / columns);
  const thumbnailSize = Math.min(
    MAX_THUMB_SIZE,
    Math.max(MIN_THUMB_SIZE, Math.ceil(tileWidth / SIZE_STEP) * SIZE_STEP)
  );
  return { columns, tileWidth, rowHeight, totalRows, thumbnailSize };
}

export interface VisibleRange {
  firstRow: number;
  lastRow: number;
  startIndex: number;
  /** 最後に描く要素の index。要素が 0 件なら -1 */
  endIndex: number;
  paddingTop: number;
  paddingBottom: number;
}

export function computeVisibleRange(
  metrics: GridMetrics,
  scrollTop: number,
  viewportHeight: number,
  itemCount: number
): VisibleRange {
  const { columns, rowHeight, totalRows } = metrics;

  if (itemCount === 0 || totalRows === 0) {
    return {
      firstRow: 0,
      lastRow: -1,
      startIndex: 0,
      endIndex: -1,
      paddingTop: 0,
      paddingBottom: 0,
    };
  }

  // 初回描画で幅が 0 の瞬間は行高も 0 になる。割り算を避けて全件描く
  // （次のフレームで正しい幅が来て縮む）
  if (!Number.isFinite(rowHeight) || rowHeight <= 0) {
    return {
      firstRow: 0,
      lastRow: totalRows - 1,
      startIndex: 0,
      endIndex: itemCount - 1,
      paddingTop: 0,
      paddingBottom: 0,
    };
  }

  const firstRow = Math.max(0, Math.floor(scrollTop / rowHeight) - OVERSCAN_ROWS);
  const lastRow = Math.min(
    totalRows - 1,
    Math.floor((scrollTop + viewportHeight) / rowHeight) + OVERSCAN_ROWS
  );

  return {
    firstRow,
    lastRow,
    startIndex: firstRow * columns,
    endIndex: Math.min(itemCount - 1, (lastRow + 1) * columns - 1),
    paddingTop: firstRow * rowHeight,
    paddingBottom: (totalRows - 1 - lastRow) * rowHeight,
  };
}
```

- [ ] **Step 4: `PhotoGrid.svelte` を書く**

**仮想スクロールと `listbox` を組むときの制約**（spec §4-1。実装時に必ず守ること）:

- DOM 上に一部の `option` しか存在しないため、**各タイルに `aria-setsize`（総枚数）と
  `aria-posinset`（1 始まりの通し番号）が必須**
- **スペーサー要素を `listbox` の直接の子にしてはならない**。上下の余白は
  コンテナの `padding-top` / `padding-bottom` で作る
- フォーカス管理は **roving tabindex**。仮想化でフォーカス中のタイルが DOM から
  消える場合に備え、消える直前にコンテナへフォーカスを退避させる
- **`tabindex` を出し分けるだけでは roving tabindex にならない。**
  `tabindex={index === rovingIndex ? 0 : -1}` は「Tab でどこに入るか」を決めるだけで、
  DOM のフォーカスは動かない。矢印キーで動くのは `focusedPath` だけになり、
  `document.activeElement` は最初にクリックしたタイルに残る。
  結果、フォーカスリングと `.focused` の表示がずれ、仮想化で元のタイルが
  消えたときにフォーカスが `body` へ落ちる。**`rovingIndex` のタイルへ
  実際に `focus()` を当てる `$effect` が要る**（下記）

```svelte
<script lang="ts">
  import type { Snippet } from "svelte";
  import GridHeader from "./GridHeader.svelte";
  import Slider from "../ui/Slider.svelte";
  import {
    GRID_GAP,
    GRID_PADDING,
    computeGridMetrics,
    computeVisibleRange,
  } from "./gridMetrics";
  import type { RequestKind } from "./requestQueue";
  import type { ImageEntry } from "../types";

  interface Props {
    images: ImageEntry[];
    /** multi: 変換モード（複数チェック） / single: メタデータモード（単一フォーカス） */
    selectionMode: "multi" | "single";
    selectedPaths: Set<string>;
    focusedPath: string | null;
    thumbnailFor: (path: string, size: number) => string | undefined;
    onRequestThumbnail: (
      path: string,
      size: number,
      kind: RequestKind,
      index: number
    ) => void;
    onVisibleRangeChange: (start: number, end: number) => void;
    onToggleSelect: (image: ImageEntry) => void;
    onFocus: (image: ImageEntry) => void;
    onPreview: (image: ImageEntry) => void;
    selectedCount: number;
    rightPanelCollapsed: boolean;
    onToggleRightPanel: () => void;
    onClearSelection: () => void;
    primaryAction?: Snippet;
    /**
     * スクロール位置。**親が持つ**（spec §3-2「rail の切替では破棄しない
     * ── スクロール位置も保つ」）。フレームモードでは PhotoGrid 自体が
     * unmount されるので、内部 state のままだと戻ったときに先頭へ飛ぶ。
     */
    scrollTop: number;
  }

  let {
    images,
    selectionMode,
    selectedPaths,
    focusedPath,
    thumbnailFor,
    onRequestThumbnail,
    onVisibleRangeChange,
    onToggleSelect,
    onFocus,
    onPreview,
    selectedCount,
    rightPanelCollapsed,
    onToggleRightPanel,
    onClearSelection,
    primaryAction,
    scrollTop = $bindable(),
  }: Props = $props();

  /** タイルの目標幅。既定 200px は「サムネイルが小さい」への回答（spec §4-1） */
  let targetTileWidth = $state(200);

  let scroller: HTMLDivElement | undefined = $state();
  let containerWidth = $state(0);
  let viewportHeight = $state(0);

  // mount 時に親が持っている位置へ戻す。以降は onscroll が親へ書き戻す
  $effect(() => {
    if (scroller && scroller.scrollTop !== scrollTop) scroller.scrollTop = scrollTop;
  });

  let metrics = $derived(
    computeGridMetrics(containerWidth, targetTileWidth, images.length)
  );
  let range = $derived(
    computeVisibleRange(metrics, scrollTop, viewportHeight, images.length)
  );
  let visible = $derived(
    range.endIndex < range.startIndex
      ? []
      : images.slice(range.startIndex, range.endIndex + 1)
  );

  let focusedIndex = $derived(
    focusedPath === null ? -1 : images.findIndex((img) => img.path === focusedPath)
  );

  /**
   * roving tabindex。可視の 1 枚だけ tabindex="0"。
   * フォーカス中のタイルが描画範囲の外なら、範囲の先頭を代役にする。
   */
  let rovingIndex = $derived(
    focusedIndex >= range.startIndex && focusedIndex <= range.endIndex
      ? focusedIndex
      : range.startIndex
  );

  // 可視範囲が変わるたびにキューへ通知する。可視範囲の持ち主はグリッド側
  // であり、キューがスクロール状態を二重に持つ理由が無い（spec §4-2）
  $effect(() => {
    onVisibleRangeChange(range.startIndex, range.endIndex);
  });

  // 描いているタイルの分だけ要求する。仮想スクロールが可視範囲を持っているので
  // IntersectionObserver は要らない
  $effect(() => {
    const size = metrics.thumbnailSize;
    const start = range.startIndex;
    visible.forEach((image, offset) => {
      onRequestThumbnail(image.path, size, "discardable", start + offset);
    });
  });

  /**
   * グリッド内にフォーカスがあったかを、**DOM が入れ替わる前に**捕まえる。
   *
   * 仮想化でフォーカス中のタイルが取り除かれると、その瞬間に
   * `document.activeElement` は `body` に落ちる。`$effect`（DOM 更新の後）で
   * 見ても「元からグリッドの外にあった」と区別できないため、
   * 退避の判断ができない。`$effect.pre` は DOM 更新の前に走る。
   */
  let focusInside = false;
  $effect.pre(() => {
    void range.startIndex;
    void range.endIndex;
    void rovingIndex;
    const active = document.activeElement;
    focusInside = active instanceof HTMLElement && !!scroller?.contains(active);
  });

  /**
   * roving tabindex の実体（spec §4-1）。**tabindex の出し分けだけでは
   * DOM のフォーカスは動かない**ので、ここで実際に移す。
   *
   * - グリッド内にフォーカスがあったときだけ動かす（外にあるなら奪わない）
   * - `rovingIndex` のタイルが描画範囲にあればそこへ移す
   * - 仮想化で消えていればコンテナへ退避させる（＝従来の退避処理）
   *
   * 退避と移動を別々の `$effect` に分けると、同じ 1 回の範囲変化に対して
   * 両方が走って互いのフォーカスを奪い合うので、1 本にまとめる。
   */
  $effect(() => {
    const target = rovingIndex;
    // 範囲が動いてもタイルの入れ替わりを拾えるよう、明示的に依存させる
    void range.startIndex;
    void range.endIndex;
    if (!scroller || !focusInside) return;
    const tile = scroller.querySelector<HTMLElement>(`[data-index="${target}"]`);
    if (tile) {
      // preventScroll: スクロール位置は scrollIndexIntoView が決める。
      // ブラウザ既定のスクロールが入ると仮想化の行位置と食い違う
      if (tile !== document.activeElement) tile.focus({ preventScroll: true });
    } else if (document.activeElement !== scroller) {
      scroller.focus();
    }
  });

  function activate(image: ImageEntry) {
    if (selectionMode === "multi") {
      // 選択のトグル ＋ focusedPath の移動を同時に行う（spec §3-2）
      onToggleSelect(image);
    } else {
      onFocus(image);
    }
  }

  function moveFocus(delta: number) {
    if (images.length === 0) return;
    const from = focusedIndex < 0 ? range.startIndex : focusedIndex;
    const next = Math.min(images.length - 1, Math.max(0, from + delta));
    onFocus(images[next]);
    scrollIndexIntoView(next);
  }

  function scrollIndexIntoView(index: number) {
    if (!scroller || metrics.rowHeight <= 0) return;
    const row = Math.floor(index / metrics.columns);
    const top = row * metrics.rowHeight;
    const bottom = top + metrics.rowHeight;
    if (top < scroller.scrollTop) scroller.scrollTop = top;
    else if (bottom > scroller.scrollTop + viewportHeight) {
      scroller.scrollTop = bottom - viewportHeight;
    }
  }

  function handleKeydown(event: KeyboardEvent) {
    const current = focusedIndex < 0 ? null : images[focusedIndex];
    switch (event.key) {
      case "ArrowRight":
        event.preventDefault();
        moveFocus(1);
        break;
      case "ArrowLeft":
        event.preventDefault();
        moveFocus(-1);
        break;
      case "ArrowDown":
        event.preventDefault();
        moveFocus(metrics.columns);
        break;
      case "ArrowUp":
        event.preventDefault();
        moveFocus(-metrics.columns);
        break;
      case "Home":
        event.preventDefault();
        moveFocus(-images.length);
        break;
      case "End":
        event.preventDefault();
        moveFocus(images.length);
        break;
      case " ":
        // Space はクリックと同じ（spec §4-1）
        event.preventDefault();
        if (current) activate(current);
        break;
      case "Enter":
        // Enter は全画面プレビュー。現行と変わる点
        event.preventDefault();
        if (current) onPreview(current);
        break;
    }
  }

  function isSelected(image: ImageEntry): boolean {
    return selectionMode === "multi"
      ? selectedPaths.has(image.path)
      : focusedPath === image.path;
  }
</script>

<div class="photo-grid">
  <GridHeader
    totalCount={images.length}
    {selectedCount}
    {selectionMode}
    {rightPanelCollapsed}
    {onToggleRightPanel}
    {onClearSelection}
    {primaryAction}
  >
    {#snippet controls()}
      <div class="size">
        <Slider bind:value={targetTileWidth} label="サイズ" min={96} max={512} step={8} suffix="px" />
      </div>
    {/snippet}
  </GridHeader>

  <!-- スペーサー要素は置かない。上下の余白はこのコンテナの padding で作る
       （option 以外の子はロール構造を壊す。spec §4-1） -->
  <div
    class="grid"
    role="listbox"
    aria-label="写真"
    aria-multiselectable={selectionMode === "multi"}
    tabindex="-1"
    bind:this={scroller}
    bind:clientWidth={containerWidth}
    bind:clientHeight={viewportHeight}
    onscroll={(e) => (scrollTop = e.currentTarget.scrollTop)}
    onkeydown={handleKeydown}
    style:grid-template-columns="repeat({metrics.columns}, 1fr)"
    style:padding-top="{GRID_PADDING + range.paddingTop}px"
    style:padding-bottom="{GRID_PADDING + range.paddingBottom}px"
  >
    {#each visible as image, offset (image.path)}
      {@const index = range.startIndex + offset}
      {@const thumb = thumbnailFor(image.path, metrics.thumbnailSize)}
      <div
        class="tile state-layer"
        class:selected={isSelected(image)}
        class:focused={focusedPath === image.path}
        role="option"
        aria-selected={isSelected(image)}
        aria-setsize={images.length}
        aria-posinset={index + 1}
        aria-label={image.name}
        tabindex={index === rovingIndex ? 0 : -1}
        data-index={index}
        onclick={(e) => {
          // tabindex="-1" の要素はクリックでフォーカスされるが、エンジンによって
          // 挙動が違う（出荷先は WebKitGTK）。上の $effect が「グリッド内に
          // フォーカスがある」を前提にしているので、ここで確実に入れておく
          e.currentTarget.focus({ preventScroll: true });
          activate(image);
        }}
        ondblclick={(e) => {
          e.preventDefault();
          onPreview(image);
        }}
      >
        <div class="thumb">
          {#if thumb}
            <img src="data:image/jpeg;base64,{thumb}" alt="" />
          {:else}
            <div class="placeholder" aria-hidden="true">📷</div>
          {/if}
          {#if selectionMode === "multi" && selectedPaths.has(image.path)}
            <span class="check" aria-hidden="true">✓</span>
          {/if}
        </div>
        <span class="filename">{image.name}</span>
      </div>
    {/each}
  </div>
</div>

<style>
  .photo-grid {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
    background: var(--md-sys-color-surface);
  }

  .size {
    width: 140px;
  }

  .grid {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: grid;
    gap: var(--space-2);
    align-content: start;
    padding-left: var(--space-3);
    padding-right: var(--space-3);
  }

  .tile {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-1);
    padding: var(--space-1);
    border-radius: var(--md-sys-shape-corner-sm);
    cursor: pointer;
    color: var(--md-sys-color-on-surface);
    /* 選択とフォーカスの枠がタイル幅を変えないよう、常に同じ太さの枠を持つ */
    border: 2px solid transparent;
  }

  .tile.selected {
    border-color: var(--md-sys-color-primary);
  }

  /* メタデータモードのフォーカスは太いアウトラインで示す（spec §4-3） */
  .tile.focused {
    border-color: var(--md-sys-color-primary);
    box-shadow: var(--md-sys-elevation-shadow-2);
  }

  .thumb {
    position: relative;
    width: 100%;
    aspect-ratio: 4 / 5;
    overflow: hidden;
    border-radius: var(--md-sys-shape-corner-sm);
    background: var(--md-sys-color-surface-container-high);
  }

  .thumb img {
    width: 100%;
    height: 100%;
    object-fit: contain;
  }

  .placeholder {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 100%;
    height: 100%;
    color: var(--md-sys-color-on-surface-variant);
    font-size: 24px;
  }

  /* サムネイルの選択チェックは PhotoGrid のローカル実装（spec §2）。
     写真の上に乗る円形マークで、汎用の部品にする理由が無い */
  .check {
    position: absolute;
    top: var(--space-1);
    right: var(--space-1);
    display: flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    border-radius: var(--md-sys-shape-corner-full);
    background: var(--md-sys-color-primary);
    color: var(--md-sys-color-on-primary);
    font: var(--md-sys-typescale-body-sm);
    font-weight: 700;
  }

  .filename {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font: var(--md-sys-typescale-body-sm);
    color: var(--md-sys-color-on-surface-variant);
  }
</style>
```

**`GRID_GAP` と `GRID_PADDING` は `gridMetrics.ts` と CSS の両方に現れる。**
CSS 側は `var(--space-2)`（8px）と `var(--space-3)`（12px）で、
定数と同じ値であることが前提になっている。**片方だけ変えると行位置がずれる。**
`gridMetrics.ts` の定数の上にこのことをコメントで書いておくこと。

- [ ] **Step 5: `App.svelte` を差し替える**

`ThumbnailGrid` を `PhotoGrid` に置き換える。`currentPage` / `PAGE_SIZE` /
`handlePageChange` と、`handleNavigatePreview` のページ計算は**すべて削除**する
（ページネーションは廃止）。

```svelte
  {#snippet center()}
    <PhotoGrid
      {images}
      selectionMode={mode === "convert" ? "multi" : "single"}
      {selectedPaths}
      {focusedPath}
      thumbnailFor={thumbnails.get}
      onRequestThumbnail={thumbnails.request}
      onVisibleRangeChange={thumbnails.setVisibleRange}
      bind:scrollTop={gridScrollTop}
      onToggleSelect={handleToggleSelect}
      onFocus={handleFocus}
      onPreview={handlePreview}
      selectedCount={selectedPaths.size}
      rightPanelCollapsed={layout.rightPanelCollapsed}
      onToggleRightPanel={() =>
        (layout.rightPanelCollapsed = !layout.rightPanelCollapsed)}
      onClearSelection={handleClearSelection}
      primaryAction={collapsedPrimaryAction}
    />
  {/snippet}
```

`App.svelte` にスクロール位置を持たせる（フレームモードで `PhotoGrid` が
unmount されても失わないため。spec §3-2）:

```ts
  let gridScrollTop = $state(0);
```

フォルダーを変えたら先頭へ戻す ── `handleSelectFolder` に `gridScrollTop = 0;` を足す。

`handleFocus` を足す。**`editingPath` が動くのはメタデータモード内のフォーカス移動の
ときだけ**（spec §3-2）:

```ts
  function handleFocus(image: ImageEntry) {
    focusedPath = image.path;
    // 変換モードのクリックは focusedPath と selectedPaths にしか触らない。
    // ここで editingPath を動かすと、変換モードで写真をチェックするたびに
    // 未保存ガードが誤発火する
    if (mode === "metadata") editingPath = image.path;
  }
```

`handleToggleSelect` の末尾も `handleFocus` を通すように変える:

```ts
  function handleToggleSelect(image: ImageEntry) {
    if (selectedPaths.has(image.path)) selectedPaths.delete(image.path);
    else selectedPaths.add(image.path);
    handleFocus(image);
  }
```

**メタデータモードへ入ったとき、`editingPath` が空なら `focusedPath` を初期値として採る**
（spec §3-2）:

```ts
  function handleModeChange(next: AppMode) {
    mode = next;
    if (next === "metadata" && editingPath === null) editingPath = focusedPath;
  }
```

`AppShell` の `onModeChange` をこれに差し替える。

```bash
git rm gui-frontend/src/lib/ThumbnailGrid.svelte
grep -rn "ThumbnailGrid\|currentPage\|PAGE_SIZE" gui-frontend/src/
```

期待: 0 件。

- [ ] **Step 6: e2e のセレクタを `option` に直す**

`e2e/convert.spec.ts` の `getByRole("button", { name: /photo-0000/ })` を
`getByRole("option", { name: /photo-0000/ })` に置換する。`e2e/shell.spec.ts` も同様。

- [ ] **Step 7: グリッドの検査を書く**

`gui-frontend/e2e/grid.spec.ts`:

```ts
import { expect, test } from "@playwright/test";
import { clearStorageOnce, installTauriStub } from "./stub";

test.describe("写真グリッド", () => {
  test.beforeEach(async ({ page }) => {
    await installTauriStub(page, { imageCount: 3000 });
    await clearStorageOnce(page);
    await page.goto("/");
    await page.getByRole("button", { name: "photos", exact: false }).first().click();
  });

  test("3,000 枚でも DOM 上のタイルは可視分だけ（spec §4-2）", async ({ page }) => {
    const grid = page.getByRole("listbox", { name: "写真" });
    await expect(grid.getByRole("option").first()).toBeVisible();

    const rendered = await grid.getByRole("option").count();
    // 前提条件: 3,000 枚が読み込まれていること
    expect(await grid.getByRole("option").first().getAttribute("aria-setsize")).toBe("3000");
    expect(rendered).toBeGreaterThan(0);
    expect(rendered).toBeLessThan(120);
  });

  test("各タイルが aria-setsize と aria-posinset を持つ（spec §4-1）", async ({ page }) => {
    const first = page.getByRole("listbox", { name: "写真" }).getByRole("option").first();
    await expect(first).toHaveAttribute("aria-setsize", "3000");
    await expect(first).toHaveAttribute("aria-posinset", "1");
  });

  test("listbox の直接の子は option だけ（spec §4-1）", async ({ page }) => {
    const kinds = await page
      .getByRole("listbox", { name: "写真" })
      .evaluate((el) => Array.from(el.children).map((c) => c.getAttribute("role")));
    expect(kinds.length).toBeGreaterThan(0);
    expect(new Set(kinds)).toEqual(new Set(["option"]));
  });

  test("Space は選択、Enter はプレビュー（現行から変わる。spec §4-1）", async ({ page }) => {
    const grid = page.getByRole("listbox", { name: "写真" });
    await grid.getByRole("option").first().click();
    await expect(page.getByRole("button", { name: "1 枚を変換" })).toBeVisible();

    await page.keyboard.press("Space");
    await expect(page.getByRole("button", { name: "0 枚を変換" })).toBeVisible();

    await page.keyboard.press("Enter");
    await expect(page.getByRole("dialog", { name: "画像プレビュー" })).toBeVisible();
  });

  test("矢印キーでフォーカスが動き、下キーは 1 行分動く", async ({ page }) => {
    const grid = page.getByRole("listbox", { name: "写真" });
    await grid.getByRole("option").first().click();

    const focusedPos = () =>
      page.evaluate(() => document.activeElement?.getAttribute("aria-posinset"));

    // 前提条件: クリックでタイル自身に DOM フォーカスが入っていること。
    // ここが null（body）だと、以降の期待は roving tabindex が
    // 効いていないことすら検出できずに落ちるだけになる
    expect(await focusedPos()).toBe("1");

    await page.keyboard.press("ArrowRight");
    expect(await focusedPos()).toBe("2");

    // 列数はウィンドウ幅から決まる。1 行下は「現在 + 列数」
    const columns = await grid.evaluate(
      (el) => getComputedStyle(el).gridTemplateColumns.split(" ").length
    );
    await page.keyboard.press("ArrowDown");
    expect(Number(await focusedPos())).toBe(2 + columns);
  });

  test("仮想化でタイルが消えてもフォーカスがグリッドの外へ落ちない（spec §4-1）", async ({ page }) => {
    const grid = page.getByRole("listbox", { name: "写真" });
    await grid.getByRole("option").first().click();
    // 前提条件: タイル自身に DOM フォーカスがあること。
    // ここが body だと「落ちなかった」は検査になっていない
    expect(
      await page.evaluate(() => document.activeElement?.getAttribute("role"))
    ).toBe("option");

    // 1 枚目が描画範囲から確実に外れるところまで飛ばす
    await grid.evaluate((el) => (el.scrollTop = el.scrollHeight));

    await expect
      .poll(() =>
        page.evaluate(() => {
          const el = document.activeElement;
          if (!el || el === document.body) return "body";
          return el.closest('[role="listbox"]') ? "grid" : "outside";
        })
      )
      .toBe("grid");
  });

  test("末尾までスクロールしても最後の 1 枚に到達できる", async ({ page }) => {
    const grid = page.getByRole("listbox", { name: "写真" });
    await grid.evaluate((el) => (el.scrollTop = el.scrollHeight));
    await expect(grid.getByRole("option").last()).toHaveAttribute("aria-posinset", "3000");
  });
});
```

- [ ] **Step 8: 走らせる**

```bash
cd gui-frontend && bun run typecheck && bun test && bun run e2e
```

- [ ] **Step 9: 実機で確認する**

```bash
make dev
```

- 数千枚のフォルダーを開いてスクロールが飛ばないこと
- サイズスライダーを動かして列数が変わり、解像度が取り直されること
- 高速スクロールで**いま見えている行から先に**埋まること（LIFO の効果）
- ダブルクリックでプレビューが開くこと

- [ ] **Step 10: コミット**

```bash
git add -A gui-frontend
git commit -m "feat(gui): 写真グリッドを仮想スクロール化し listbox へ変更"
```

---

## Task 14: 全画面プレビューとフィルムストリップ（段階 6 の 3/4）

spec §4-1「全画面プレビューにはフィルムストリップを追加する。現行は ← → で送れるが
『いま何枚目か』が見えない」。

**Files:**
- Create: `gui-frontend/src/lib/browser/PhotoViewer.svelte`
- Delete: `gui-frontend/src/lib/ImagePreview.svelte`
- Modify: `gui-frontend/src/App.svelte`, `gui-frontend/src/lib/focusTrap.ts`

**Interfaces:**
- Consumes: `thumbnailFor` / `onRequestThumbnail`（`"pinned"` で要求する。
  フィルムストリップはグリッドの index 範囲に入らないため、`"discardable"` にすると
  スクロールのたびに捨てられて永久に埋まらない。spec §4-2）
- Produces: `PhotoViewer`
  `{ image: ImageEntry, images: ImageEntry[], selectionMode: "multi" | "single",
     selectedPaths: Set<string>, thumbnailFor, onRequestThumbnail,
     onToggleSelect, onClose, onNavigate }`

- [ ] **Step 1: `ImagePreview.svelte` を `PhotoViewer.svelte` へ移して土台にする**

```bash
git mv gui-frontend/src/lib/ImagePreview.svelte gui-frontend/src/lib/browser/PhotoViewer.svelte
```

`<script>` の import を 1 段深くする（`"../api"` / `"../focusTrap"` /
`"../toasts.svelte"` / `"../types"`）。

**ズーム（矩形ドラッグで拡大、クリックで解除）のロジックは一切変えない。**
`zoomed` / `zoomTransform` / `selecting` / `selStart` / `selEnd` / `selectionRect` /
`handleImageMouseDown` / `handleMouseMove` / `handleMouseUp` / `handleZoomedClick` /
`resetZoom`、および `loadToken` による競合防止、`imageErrorReported` の 1 件だけ通知、
`formatExifLine1` / `formatExifLine2` はそのまま残す。

- [ ] **Step 2: props を増やす**

```ts
  interface Props {
    image: ImageEntry;
    images: ImageEntry[];
    selectionMode: "multi" | "single";
    selectedPaths: Set<string>;
    thumbnailFor: (path: string, size: number) => string | undefined;
    onRequestThumbnail: (
      path: string,
      size: number,
      kind: RequestKind,
      index: number
    ) => void;
    onToggleSelect: (image: ImageEntry) => void;
    onClose: () => void;
    onNavigate: (image: ImageEntry) => void;
  }
```

`import type { RequestKind } from "./requestQueue";` を足す。

- [ ] **Step 3: フィルムストリップを足す**

`<script>` に:

```ts
  /** フィルムストリップの高さ。4:5 なので幅はこの 0.8 倍 */
  const STRIP_THUMB = 96;
  /** 現在位置の前後どれだけを要求するか。全部要求すると 3,000 枚分の IPC が走る */
  const STRIP_WINDOW = 20;

  let stripElement: HTMLDivElement | undefined = $state();

  // 現在位置の前後だけを pinned で要求する。グリッドの可視範囲に入らないので
  // discardable にすると捨てられて埋まらない（spec §4-2）
  $effect(() => {
    const from = Math.max(0, currentIndex - STRIP_WINDOW);
    const to = Math.min(images.length - 1, currentIndex + STRIP_WINDOW);
    for (let i = from; i <= to; i++) {
      onRequestThumbnail(images[i].path, STRIP_THUMB, "pinned", -1);
    }
  });

  // 送るたびに現在位置をストリップの中央へ寄せる。
  // ストリップ内にフォーカスがあるときは、フォーカスも一緒に運ぶ
  // （roving tabindex。PhotoGrid と同じ理由。tabindex の出し分けだけでは
  //  DOM のフォーカスが前の枠に取り残される）
  $effect(() => {
    void image.path;
    const current = stripElement?.querySelector<HTMLElement>('[aria-current="true"]');
    if (!current) return;
    current.scrollIntoView({ block: "nearest", inline: "center" });
    const active = document.activeElement;
    if (
      active instanceof HTMLElement &&
      stripElement?.contains(active) &&
      active !== current
    ) {
      current.focus({ preventScroll: true });
    }
  });
```

マークアップの `.image-container` の後ろ（`</div>` の直前）に足す:

```svelte
  <div class="filmstrip" bind:this={stripElement}>
    <div class="position">{currentIndex + 1} / {images.length}</div>
    <!-- role="list" は付けない。付けると子に role="listitem" が要り、
         それは button ロールを上書きしてしまう（「押せるもの」として
         支援技術に伝わらなくなる）。枚数と位置は各ボタンの aria-label が持つ -->
    <div class="strip" aria-label="フィルムストリップ">
      {#each images as item, index (item.path)}
        {@const thumb = thumbnailFor(item.path, STRIP_THUMB)}
        {@const current = item.path === image.path}
        <button
          class="frame state-layer"
          class:current
          class:selected={selectedPaths.has(item.path)}
          type="button"
          aria-current={current}
          aria-label="{index + 1} 枚目 {item.name}"
          tabindex={current ? 0 : -1}
          onclick={() => onNavigate(item)}
        >
          {#if thumb}
            <img src="data:image/jpeg;base64,{thumb}" alt="" />
          {/if}
        </button>
      {/each}
    </div>
  </div>
```

**`tabindex` は現在位置の 1 枚だけ 0 にする（roving tabindex）。** 見た目のためではなく、
次の Step 4 の Tab 応答のためである。

**`{#each images}` で 3,000 個の `<button>` を作ることになる。**
サムネイルの取得は上の窓で絞っているので IPC は走らないが、DOM の要素数は
そのまま出る。1 要素あたり 77px 幅の空箱なので初回のレイアウトコストは残る。
**Task 15 の計測でストリップを開いた状態の `rAF` 間隔も 1 度見て、
悪化するようなら窓の外を描かない仮想化を入れること**（グリッドと同じ計算を
横方向に使えばよい）。判断は実施メモに残す。

- [ ] **Step 4: `focusTrap` の `FOCUSABLE` から `tabindex="-1"` を除く**

**初回のレイアウトコストとは別に、Tab を押すたびのコストがある。**
`PhotoViewer` は `focusTrap` を使っており、`focusTrap.focusable()` は **Tab のたびに**
`node.querySelectorAll(FOCUSABLE)` の結果すべてに `getClientRects()` を掛ける。
`FOCUSABLE` は `button:not([disabled])` を拾うので、上のストリップの 3,000 個が
毎回そこに入り、Tab 1 回ごとに 3,000 回の強制レイアウトが走る。

`gui-frontend/src/lib/focusTrap.ts` の `FOCUSABLE` を直す。
**`tabindex="-1"` の要素は Tab 順に入らない**のだから、そもそも列挙する必要が無い:

```ts
const FOCUSABLE = [
  "a[href]",
  "button",
  "input",
  "select",
  "textarea",
  "[tabindex]",
]
  .map((sel) => `${sel}:not([disabled]):not([tabindex="-1"])`)
  .join(",");
```

これで上のストリップから列挙されるのは現在位置の 1 枚だけになる
（`querySelectorAll` 自体は 3,000 ノードでも sub-ms で、重いのは
`getClientRects()` の方である）。

**この変更は既存の `Dialog` / `PhotoViewer` の Tab 挙動を変えない。**
`tabindex="-1"` の要素は元々 Tab で到達できず、`focusable()` の
先頭／末尾判定に混ざっていただけである（`Rating` の★のように
`aria-hidden` かつ `tabindex="-1"` の要素が混ざると、むしろ巻き戻しの
端がずれる）。`node` 自身の `tabindex="-1"` も列挙対象から外れるが、
`focusTrap` は `node` を `items` ではなく `active === node` で別に見ているので影響は無い。

- [ ] **Step 5: 選択ボタンを `selectionMode` で出し分ける**

single モード（メタデータ）では「選択する」ボタンは意味を持たない:

```svelte
  {#if selectionMode === "multi"}
    <button class="select-btn" class:selected={isSelected} onclick={() => onToggleSelect(image)}>
      {isSelected ? "✓ 選択済み" : "○ 選択する"}
    </button>
  {/if}
```

`handleKeydown` の `case " "` も同様に `selectionMode === "multi"` のときだけ動かす。

- [ ] **Step 6: `<style>` をトークンへ差し替える**

旧 `app.css` 変数を Task 6 Step 6 の対応表どおりに置き換える。
背景の暗幕は `background: var(--md-sys-color-scrim); opacity: …` ではなく、
**`color-mix()` を使わずに済むよう** `.backdrop` を分けて不透明度を持たせる
（`Dialog` と同じ作りにする）。

フィルムストリップのスタイル:

```css
  .filmstrip {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    padding: var(--space-2) var(--space-3);
    background: var(--md-sys-color-surface-container);
    border-top: 1px solid var(--md-sys-color-outline-variant);
  }

  .position {
    margin-bottom: var(--space-1);
    font: var(--md-sys-typescale-body-sm);
    color: var(--md-sys-color-on-surface-variant);
    font-variant-numeric: tabular-nums;
  }

  .strip {
    display: flex;
    gap: var(--space-1);
    overflow-x: auto;
    padding-bottom: var(--space-1);
  }

  .frame {
    flex-shrink: 0;
    width: 77px;   /* 96 * 4/5 */
    height: 96px;
    padding: 0;
    border: 2px solid transparent;
    border-radius: var(--md-sys-shape-corner-xs);
    background: var(--md-sys-color-surface-container-high);
    cursor: pointer;
    overflow: hidden;
  }

  .frame.selected {
    border-color: var(--md-sys-color-primary-container);
  }

  .frame.current {
    border-color: var(--md-sys-color-primary);
  }

  .frame img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
```

`.image-container` に `padding-bottom` を足して、フィルムストリップと重ならないようにする。

- [ ] **Step 7: `App.svelte` を差し替える**

```svelte
{#if previewImage}
  <PhotoViewer
    image={previewImage}
    {images}
    selectionMode={mode === "convert" ? "multi" : "single"}
    {selectedPaths}
    thumbnailFor={thumbnails.get}
    onRequestThumbnail={thumbnails.request}
    onToggleSelect={handleToggleSelect}
    onClose={handleClosePreview}
    onNavigate={(img) => {
      previewImage = img;
      handleFocus(img);
    }}
  />
{/if}
```

`handleNavigatePreview` は削除する（ページ計算が要らなくなったため）。

- [ ] **Step 8: `test-integrity` スキルを起動し、検査を書く**

`e2e/grid.spec.ts` に追加:

```ts
test("プレビューのフィルムストリップが現在位置を示し、クリックで送れる", async ({ page }) => {
  const grid = page.getByRole("listbox", { name: "写真" });
  await grid.getByRole("option").first().dblclick();

  const viewer = page.getByRole("dialog", { name: "画像プレビュー" });
  await expect(viewer).toBeVisible();
  await expect(viewer.getByText("1 / 3000")).toBeVisible();

  // ストリップの枠は button のまま（role="listitem" で上書きしない。Step 3）
  await viewer.getByRole("button", { name: /^5 枚目/ }).click();
  await expect(viewer.getByText("5 / 3000")).toBeVisible();
  await expect(viewer.getByRole("button", { name: /^5 枚目/ })).toHaveAttribute(
    "aria-current",
    "true"
  );
});

test("プレビューは ← → で送れ、Esc で閉じる", async ({ page }) => {
  const grid = page.getByRole("listbox", { name: "写真" });
  await grid.getByRole("option").first().dblclick();
  const viewer = page.getByRole("dialog", { name: "画像プレビュー" });

  await page.keyboard.press("ArrowRight");
  await expect(viewer.getByText("2 / 3000")).toBeVisible();
  await page.keyboard.press("ArrowLeft");
  await expect(viewer.getByText("1 / 3000")).toBeVisible();

  await page.keyboard.press("Escape");
  await expect(viewer).toHaveCount(0);
});
```

- [ ] **Step 9: 走らせる**

```bash
cd gui-frontend && bun run typecheck && bun test && bun run e2e
grep -rn "ImagePreview" gui-frontend/src/
```

- [ ] **Step 10: 実機で確認し、コミット**

```bash
make dev
```

ズームのドラッグ選択が壊れていないことを必ず確認する（このタスクで唯一
「移設したが触っていない」部分であり、壊れていたら写し間違い）。

```bash
git add -A gui-frontend
git commit -m "feat(gui): 全画面プレビューを刷新しフィルムストリップを追加"
```

---

## Task 15: スクロール検査と LRU 上限の確定（段階 6 の 4/4）

spec §7-2 / §8。**この 2 つの実測値を spec に追記するところまでが本タスク。**

**指標は `requestAnimationFrame` のフレーム間隔**（連続する `rAF` コールバックの
時刻差の分布）。`PerformanceObserver` の `longtask` は使わない ── Chromium 専用の
指標であり、実際の出荷先である WebKitGTK では取得できず、比較の土台にならない。

**比較に使う統計量**（spec §7-2）:
- `rAF` 間隔の **p95**
- **32ms を超えたフレームの割合**

平均や最大では結論が変わる（平均は詰まりを均し、最大は 1 回の外れ値で決まる）。
**各条件 3 回ずつ計測し、それぞれの中央値で比較する。**
2 つの統計量が**どちらもベースライン以下**であることを合格条件とする。

**Files:**
- Create: `gui-frontend/e2e/scrollPerf.ts`（計測の共通処理）
- Create: `gui-frontend/e2e/scroll.spec.ts`
- Create: `gui-frontend/e2e/scroll-baseline.json`（**コミットする**）
- Modify: `gui-frontend/src/lib/browser/thumbnailQueue.svelte.ts`（`CACHE_BYTE_LIMIT` の確定）
- Modify: `docs/superpowers/specs/2026-08-19-gui-redesign-design.md`（§7-2 と §8 に追記）

- [ ] **Step 1: 計測の共通処理を書く**

`gui-frontend/e2e/scrollPerf.ts`:

```ts
import type { Page } from "@playwright/test";

export interface ScrollSample {
  /** rAF 間隔の 95 パーセンタイル (ms) */
  p95: number;
  /** 32ms を超えたフレームの割合 (0-1) */
  jankRatio: number;
  frames: number;
}

/**
 * 一定速度で最上部から最下部までスクロールしながら rAF 間隔を測る。
 *
 * スクロールは rAF ごとに scrollTop を等量ずつ進める。wheel イベントだと
 * OS とブラウザのスムーススクロールが挟まり、条件を揃えられない。
 */
export async function measureScroll(
  page: Page,
  selector: string,
  durationMs = 6000
): Promise<ScrollSample> {
  return page.evaluate(
    async ({ selector, durationMs }) => {
      const el = document.querySelector<HTMLElement>(selector)!;
      el.scrollTop = 0;
      await new Promise((r) => requestAnimationFrame(() => r(null)));

      const distance = el.scrollHeight - el.clientHeight;
      const intervals: number[] = [];
      let last = performance.now();
      const start = last;

      await new Promise<void>((resolve) => {
        function tick(now: number) {
          intervals.push(now - last);
          last = now;
          const elapsed = now - start;
          if (elapsed >= durationMs) {
            resolve();
            return;
          }
          el.scrollTop = (distance * elapsed) / durationMs;
          requestAnimationFrame(tick);
        }
        requestAnimationFrame(tick);
      });

      // 最初の 1 フレームは計測開始のオーバーヘッドを含むので捨てる
      const samples = intervals.slice(1).sort((a, b) => a - b);
      const p95 = samples[Math.floor(samples.length * 0.95)] ?? 0;
      const janky = samples.filter((v) => v > 32).length;
      return { p95, jankRatio: janky / samples.length, frames: samples.length };
    },
    { selector, durationMs }
  );
}

export function median(values: number[]): number {
  const sorted = [...values].sort((a, b) => a - b);
  const mid = Math.floor(sorted.length / 2);
  return sorted.length % 2 === 1 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2;
}
```

- [ ] **Step 2: ベースラインを取る（現行実装 / 50 枚）**

**ベースラインは現行の `ThumbnailGrid` に対して取る必要がある。**
Task 13 で既に置き換わっているので、**その直前のコミットを worktree で取り出して測る。**

```bash
# ThumbnailGrid.svelte を最後に含んでいたコミット（＝ Task 13 の 1 つ前）
BASE=$(git log --format=%H -n 1 -- gui-frontend/src/lib/ThumbnailGrid.svelte)^
git worktree add /tmp/pt-baseline "$BASE"
cd /tmp/pt-baseline/gui-frontend && bun install
cp <本リポジトリ>/gui-frontend/e2e/scrollPerf.ts e2e/scrollPerf.ts
```

`/tmp/pt-baseline/gui-frontend/e2e/baseline.spec.ts` を作る。
**旧実装のスクロール要素は `role="listbox"` ではなく `div.grid`。**
セレクタを間違えると `null` 参照で落ちる。

```ts
import { test } from "@playwright/test";
import { installTauriStub } from "./stub";
import { measureScroll, median } from "./scrollPerf";

test.setTimeout(180_000);

test("ベースライン: 現行実装 / 50 枚", async ({ page }) => {
  await installTauriStub(page, { imageCount: 50 });
  await page.goto("/");
  await page.getByRole("button", { name: "photos", exact: false }).first().click();
  await page.locator(".grid .grid-item").first().waitFor();

  const p95: number[] = [];
  const jank: number[] = [];
  for (let i = 0; i < 3; i++) {
    const s = await measureScroll(page, ".grid");
    p95.push(s.p95);
    jank.push(s.jankRatio);
  }
  console.log(JSON.stringify({ p95: median(p95), jankRatio: median(jank) }, null, 2));
});
```

```bash
cd /tmp/pt-baseline/gui-frontend && bun run e2e -- e2e/baseline.spec.ts
```

**測定は本番の計測と同じマシン・同じ画面・他のアプリを閉じた状態で行う**
（絶対値に意味は無く、比較にしか使わない）。
出力の 2 値を Step 4 の `scroll-baseline.json` に書き写す。

終わったら片付ける:

```bash
git worktree remove --force /tmp/pt-baseline
```

- [ ] **Step 3: `test-integrity` スキルを起動する**

- [ ] **Step 4: 検査を書く**

`gui-frontend/e2e/scroll.spec.ts`:

```ts
import { expect, test } from "@playwright/test";
import baseline from "./scroll-baseline.json";
import { installTauriStub } from "./stub";
import { measureScroll, median } from "./scrollPerf";

/** 計測は重いので既定のタイムアウトを延ばす */
test.setTimeout(180_000);

const RUNS = 3;

async function sampleThrice(page: import("@playwright/test").Page) {
  const p95: number[] = [];
  const jank: number[] = [];
  for (let i = 0; i < RUNS; i++) {
    const s = await measureScroll(page, '[role="listbox"][aria-label="写真"]');
    p95.push(s.p95);
    jank.push(s.jankRatio);
  }
  return { p95: median(p95), jankRatio: median(jank) };
}

test.describe("スクロール性能（spec §7-2）", () => {
  test("新実装 / 50 枚 はベースラインを上回らない", async ({ page }) => {
    await installTauriStub(page, { imageCount: 50 });
    await page.goto("/");
    await page.getByRole("button", { name: "photos", exact: false }).first().click();
    await expect(page.getByRole("option").first()).toBeVisible();

    const result = await sampleThrice(page);

    // 前提条件: そもそもスクロールできる高さがあること。
    // 1 画面に収まっていたら「悪化しなかった」は自明に成立する
    const scrollable = await page
      .getByRole("listbox", { name: "写真" })
      .evaluate((el) => el.scrollHeight - el.clientHeight);
    expect(scrollable).toBeGreaterThan(100);

    expect(result.p95).toBeLessThanOrEqual(baseline.p95);
    expect(result.jankRatio).toBeLessThanOrEqual(baseline.jankRatio);
  });

  test("新実装 / 3,000 枚 の絶対値とキャッシュ実サイズを記録する", async ({ page }) => {
    await installTauriStub(page, { imageCount: 3000 });
    await page.goto("/");
    await page.getByRole("button", { name: "photos", exact: false }).first().click();
    await expect(page.getByRole("option").first()).toBeVisible();

    const result = await sampleThrice(page);
    const stats = await page.evaluate(() =>
      (window as unknown as { __thumbnailStats: () => { bytes: number; entries: number } })
        .__thumbnailStats()
    );

    // 判定はしない。値を出力して spec に転記する（spec §7-2 / §8）
    console.log(
      JSON.stringify(
        {
          scale: 3000,
          p95: result.p95,
          jankRatio: result.jankRatio,
          cacheBytes: stats.bytes,
          cacheEntries: stats.entries,
          bytesPerThumbnail: stats.entries > 0 ? stats.bytes / stats.entries : 0,
        },
        null,
        2
      )
    );
    expect(stats.entries).toBeGreaterThan(0);
  });
});
```

`e2e/scroll-baseline.json` は Step 2 の計測結果で作る:

```json
{
  "_comment": "spec §7-2 のベースライン。現行 ThumbnailGrid / 50 枚 / 一定速度スクロールを 3 回測った中央値。測定マシンに依存するので、別のマシンで測り直したらこのファイルごと更新する",
  "machine": "<uname -a と論理解像度をここに書く>",
  "measuredAt": "2026-08-19",
  "p95": 0,
  "jankRatio": 0
}
```

**`p95` と `jankRatio` の 0 はプレースホルダーではなく、Step 2 の実測値で
置き換えてから次へ進む。** 0 のままだと必ず落ちるので、埋め忘れは検査が教える。

`import baseline from "./scroll-baseline.json"` のために
`tsconfig.json` は `resolveJsonModule: true` を既に持っている。

- [ ] **Step 5: 走らせて 3,000 枚の値を得る**

```bash
cd gui-frontend && bun run e2e -- e2e/scroll.spec.ts
```

- [ ] **Step 6: LRU 上限を決める**

Step 5 の出力の `bytesPerThumbnail` から決める。方針:

> **上限 = 1 枚あたりの実バイト数 × 保持したい枚数。**
> 保持したい枚数は「3,000 枚のフォルダーを 1 往復スクロールしても、
> 戻ってきたときに再取得が起きない程度」＝ 1 解像度あたり 3,000 枚を目安に取り、
> サイズスライダーで解像度が 2〜3 種類できることを見込んで 2 倍する。

計算結果を `CACHE_BYTE_LIMIT` に入れる（`thumbnailQueue.svelte.ts`）。
**計算式をコメントとして残すこと。** 数値だけ書くと、次に画像や解像度が
変わったときに再計算できない。

例（`bytesPerThumbnail` が 18KB だった場合）:

```ts
/**
 * サムネイルキャッシュのバイト上限。
 *
 * 実測（Task 15 / spec §7-2）: 200px の base64 サムネイルが 1 枚あたり約 18KB。
 * 3,000 枚 × 18KB ≒ 54MB。サイズスライダーで解像度が 2 種類できることを見込んで
 * 2 倍し、110MB とする。base64 文字列は latin1 なので、文字数がおおむね
 * 保持バイト数になる。
 */
export const CACHE_BYTE_LIMIT = 110 * 1024 * 1024;
```

- [ ] **Step 7: 上限が実際に効くことを検査する**

`e2e/scroll.spec.ts` に追加:

```ts
test("キャッシュはバイト上限を大きく超えない（spec §4-2）", async ({ page }) => {
  await installTauriStub(page, { imageCount: 3000 });
  await page.goto("/");
  await page.getByRole("button", { name: "photos", exact: false }).first().click();
  await expect(page.getByRole("option").first()).toBeVisible();

  const grid = page.getByRole("listbox", { name: "写真" });
  // サイズを変えて解像度別のキーを増やしながら往復する
  for (const size of ["512", "96", "256"]) {
    await page.getByLabel("サイズ").fill(size);
    await grid.evaluate((el) => (el.scrollTop = el.scrollHeight));
    await page.waitForTimeout(2000);
    await grid.evaluate((el) => (el.scrollTop = 0));
    await page.waitForTimeout(2000);
  }

  const stats = await page.evaluate(() =>
    (window as unknown as { __thumbnailStats: () => { bytes: number; entries: number } })
      .__thumbnailStats()
  );
  const limit = 110 * 1024 * 1024; // CACHE_BYTE_LIMIT と揃える

  // 前提条件: 上限を試すだけの量が実際に溜まっていること。
  // 溜まっていなければ「超えない」は自明に成立する
  expect(stats.bytes).toBeGreaterThan(limit * 0.1);
  // 1 件だけ上限を超える項目は保持する仕様なので、わずかな超過は許す
  expect(stats.bytes).toBeLessThanOrEqual(limit * 1.05);
});
```

`limit` の値は Step 6 で決めた `CACHE_BYTE_LIMIT` と同じ数にすること。

- [ ] **Step 8: spec に追記する**

`docs/superpowers/specs/2026-08-19-gui-redesign-design.md` の §7-2 末尾に追記:

````markdown
#### 計測結果（段階 6 で確定）

測定機: `<uname -a / 論理解像度>`。Playwright（Chromium）/ `vite dev` /
`e2e/stub.ts` のスタブサムネイル。各条件 3 回の中央値。

| 条件 | `rAF` 間隔 p95 | 32ms 超のフレーム割合 |
|---|---|---|
| ベースライン（現行実装 / 50 枚） | `X.X ms` | `X.X %` |
| 新実装 / 50 枚 | `X.X ms` | `X.X %` |
| 新実装 / 3,000 枚 | `X.X ms` | `X.X %` |

サムネイルキャッシュの実測: 3,000 枚を 1 往復した時点で `N` 件 / `M` MB、
1 枚あたり約 `K` KB。
````

§8 の「サムネイルキャッシュの LRU 上限」の行を、決めた値と根拠に書き換える。

- [ ] **Step 9: コミット**

```bash
git add gui-frontend/e2e gui-frontend/src/lib/browser/thumbnailQueue.svelte.ts \
        docs/superpowers/specs/2026-08-19-gui-redesign-design.md
git commit -m "test(gui): スクロール性能を計測し LRU 上限を実測値から確定"
```

---

## Task 16: フレームモード（段階 7）

spec §5-3。現行 `ExifFrameSettings.svelte`（585 行、モーダル）を解体し、独立したモードにする。

| 変更 | 内容 |
|---|---|
| プレビューが主役になる | 中央の最も広い場所を占め、実際の余白込みで見える |
| プリセットが左に一覧で並ぶ | 切り替えるたびにプレビューが更新され、見比べられる |
| 「プリセット名」入力欄を廃止 | 一覧の項目をダブルクリックで**改名**（複製ではない。下記） |
| 表示項目 10 個をチップに | チェックボックス 10 個 → チップ 10 個 |
| 背景色をパネル内にも出す | **値は変換設定と同じ `config.bg_color` にバインドする** |
| フォント選択 | **プリセットに残す**（プリセット JSON のスキーマは変更しない） |

**背景色をパネル専用の独立した値にしてはならない。** そうすると、フレームパネルで
black を見ながら詰めたプリセットが、変換設定が white のままなら white で出力される、
という黙って食い違う経路ができる（spec §5-3）。値は 1 つ、置き場所が 2 つ。

**「改名」は改名であって複製ではない。** spec §5-3 の表は「『プリセット名』入力欄を廃止 /
一覧の項目をダブルクリックで改名」であり、`draft.name` を書き換えて保存するだけでは
**新しい名前のファイルが増えて元のプリセットが残る**（＝複製）。
`api.ts` には rename に当たるコマンドが無く、あるのは `savePreset` / `deletePreset` だけなので、
**新しい名前で保存してから旧名を削除する**（この順序で行う。逆にすると保存に失敗したときに
プリセットが消えるだけになる）。新規コマンドは要らないので Global Constraints は崩れない。

改名先が既存の別プリセットと同じ名前になる場合は**保存させない**。
そのまま通すと「上書き ＋ 旧名の削除」で 2 つが 1 つになる（黙ってプリセットが 1 つ消える）。

**警告の扱いは現行の挙動を変えない**（spec §5-3）:

| 警告 | 扱い |
|---|---|
| フレーム描画由来（`preview.warnings`） | Rust 側で捨てている。**変更なし**（`gui/src/commands.rs` は触らない） |
| アセット由来（`assets.warnings`） | 返ってくるので**従来どおり toast する。同じ重複抑止も維持する** |

**Files:**
- Create: `gui-frontend/src/lib/panels/frameDraft.svelte.ts`
- Create: `gui-frontend/src/lib/panels/PresetList.svelte`
- Create: `gui-frontend/src/lib/panels/FramePreview.svelte`
- Create: `gui-frontend/src/lib/panels/FramePanel.svelte`
- Delete: `gui-frontend/src/lib/ExifFrameSettings.svelte`
- Modify: `gui-frontend/src/App.svelte`, `gui-frontend/src/lib/panels/presets.svelte.ts`（`rename` を足す）

**spec §2 のファイル構成表からの逸脱を 1 つ入れる。**
`panels/` のモジュールとして `presets` / `convertRun` / `metadataDraft` の 3 つが
挙げられているが、フレーム編集の下書きは**左・中央・右の 3 カラムにまたがって共有される**
ため、`App.svelte` に置くと 3-5 の「`App.svelte` は 4 状態とパネルの差し替えのみ」が崩れる。
4 つ目のモジュールとして `frameDraft.svelte.ts` を足す。

**Interfaces:**
- Produces: `createFrameDraft()`
  - `readonly draft: ExifFrameConfig | null`
  - `readonly editingName: string`（**ディスク上のどのプリセットを編集中か**。新規なら `""`）
  - `readonly isNew: boolean` / `readonly isRenamed: boolean` / `readonly renamedFrom: string | null`
  - `readonly nameConflict: boolean` / `readonly canSave: boolean` / `readonly canDelete: boolean`
  - `select(name: string, presets: ExifFrameConfig[]): void`
  - `rename(name: string): void`
  - `createNew(presets: ExifFrameConfig[]): void`
  - `snapshot(): ExifFrameConfig`
- Consumes / extends: `createPresetStore()`（Task 7）に
  `rename(from: string, preset: ExifFrameConfig): Promise<boolean>` を足す
- Produces: `PresetList` `{ presets, editingName, onSelect, onRename, onCreate, onDelete }`
- Produces: `FramePreview` `{ config: ExifFrameConfig | null, bgColor: "white"|"black", imagePath: string | null }`
- Produces: `FramePanel`
  `{ config: ExifFrameConfig (bindable), bgColor: "white"|"black" (bindable), fonts: FontInfo[],
     isNew: boolean, isRenamed: boolean, nameConflict: boolean, canSave: boolean, canDelete: boolean,
     sampleName: string | null, onSave: () => void, onDelete: () => void, onPickSample: () => void }`

- [ ] **Step 1: `frameDraft.svelte.ts` を書く**

```ts
import type { ExifFrameConfig } from "../types";

/** バンドルプリセット名。ユーザーファイルが無くても常に存在するため削除させない */
export const BUNDLED_PRESET_NAME = "default";

export function defaultFrameConfig(): ExifFrameConfig {
  return {
    name: BUNDLED_PRESET_NAME,
    position: "auto",
    items: {
      maker_logo: true,
      lens_brand_logo: true,
      camera_model: true,
      lens_model: true,
      focal_length: true,
      f_number: true,
      shutter_speed: true,
      iso: true,
      date_taken: false,
      custom_text: false,
    },
    font: { font_path: null, primary_size: 0.025, secondary_size: 0.018 },
    custom_text: "",
  };
}

/**
 * プリセットは必ず深いコピーを取ってから編集する。
 * シャローコピーだと items / font が一覧側のオブジェクトと同一参照になり、
 * 編集がそのまま一覧を書き換えてしまう。
 */
function clone(preset: ExifFrameConfig): ExifFrameConfig {
  return structuredClone($state.snapshot(preset)) as ExifFrameConfig;
}

export function createFrameDraft() {
  let draft = $state<ExifFrameConfig | null>(null);
  /**
   * **編集中の下書きがディスク上のどのプリセットか。** 新規作成中は `""`。
   * `draft.name` は改名で先に動くので、旧名をここに残しておく必要がある
   * （保存後にこの名前を削除するのが「改名」の実体）。
   */
  let editingName = $state("");
  let knownNames = $state<string[]>([]);

  /** 一覧の項目をダブルクリックして名前を変えた状態 */
  function renamed(): boolean {
    return draft !== null && editingName !== "" && draft.name.trim() !== editingName;
  }

  /** 別のプリセットと同じ名前になっている */
  function conflicting(): boolean {
    if (draft === null) return false;
    const name = draft.name.trim();
    return name !== editingName && knownNames.includes(name);
  }

  return {
    get draft() {
      return draft;
    },
    get editingName() {
      return editingName;
    },
    /** ディスク上に対応するプリセットが無い（＝新規作成中） */
    get isNew(): boolean {
      return draft !== null && editingName === "";
    },
    get isRenamed(): boolean {
      return renamed();
    },
    /** 保存後に削除すべき旧名。改名でなければ null */
    get renamedFrom(): string | null {
      return renamed() ? editingName : null;
    },
    /**
     * 既存の別プリセットと名前がぶつかっている。
     * 通すと「上書き ＋ 旧名の削除」で 2 つが 1 つになるため保存させない
     */
    get nameConflict(): boolean {
      return conflicting();
    },
    get canSave(): boolean {
      return draft !== null && draft.name.trim().length > 0 && !conflicting();
    },
    get canDelete(): boolean {
      return editingName !== "" && editingName !== BUNDLED_PRESET_NAME;
    },

    select(name: string, presets: ExifFrameConfig[]) {
      knownNames = presets.map((p) => p.name);
      const found = presets.find((p) => p.name === name);
      draft = found ? clone(found) : defaultFrameConfig();
      // 見つからなければディスク上の実体が無い＝新規扱い
      editingName = found ? found.name : "";
    },

    /** 一覧の項目をダブルクリックしての改名（spec §5-3）。
     *  ここでは下書きの名前を変えるだけで、旧名の削除は保存時に行う */
    rename(name: string) {
      if (draft) draft.name = name;
    },

    createNew(presets: ExifFrameConfig[]) {
      knownNames = presets.map((p) => p.name);
      draft = defaultFrameConfig();
      // 既存と衝突しない名前を作る
      let n = 1;
      while (knownNames.includes(`preset-${n}`)) n++;
      draft.name = `preset-${n}`;
      editingName = "";
    },

    snapshot(): ExifFrameConfig {
      const snap = structuredClone($state.snapshot(draft!)) as ExifFrameConfig;
      snap.name = snap.name.trim();
      return snap;
    },
  };
}
```

- [ ] **Step 2: `presets.svelte.ts` に `rename` を足す**

`createPresetStore()`（Task 7 Step 2）に 1 メソッド足す。`api.ts` は無変更で、
使うのは既存の `savePreset` / `deletePreset` だけである:

```ts
    /**
     * プリセットの改名。**新しい名前で保存してから旧名を消す。**
     * 逆順にすると、保存に失敗したときにプリセットが消えるだけになる。
     * `from === preset.name` のときは単なる保存として振る舞う。
     */
    async rename(from: string, preset: ExifFrameConfig): Promise<boolean> {
      try {
        await savePreset(preset);
        if (from !== preset.name) await deletePreset(from);
        selectedName = preset.name;
        await reload();
        toast.success(`プリセット「${from}」を「${preset.name}」に変更しました`);
        return true;
      } catch (e) {
        toast.error(`プリセットの改名に失敗しました: ${describeError(e)}`);
        // 保存だけ通って削除で落ちた場合、ディスク上は 2 件になっている。
        // 実際の状態を見せるために読み直す
        await reload();
        return false;
      }
    },
```

**`remove` を続けて呼ぶ形にはしない。** `remove` は「プリセット『旧名』を
削除しました」と toast するので、改名という 1 つの操作に対して削除の通知が出る。

- [ ] **Step 3: `FramePreview.svelte` を書く**

現行 `ExifFrameSettings.svelte` のライブプレビューの `$effect` を**そのまま**移す
（依存は同期フェーズで読む・300ms のデバウンス・警告の重複抑止まで含めて）。

```svelte
<script lang="ts">
  import { renderExifFramePreview } from "../api";
  import { describeError, toast } from "../toasts.svelte";
  import type { ExifFrameConfig } from "../types";

  interface Props {
    config: ExifFrameConfig | null;
    bgColor: "white" | "black";
    imagePath: string | null;
  }

  let { config, bgColor, imagePath }: Props = $props();

  let src = $state("");
  let loading = $state(false);

  let debounceTimer: ReturnType<typeof setTimeout>;
  /** プレビューは設定を触るたびに再生成されるため、同じ警告を毎回出さないよう記録する */
  const reportedWarnings = new Set<string>();

  $effect(() => {
    // 依存は $effect の同期フェーズで読む必要がある。
    // 非同期コールバック内でしか参照しないと依存として追跡されない
    const snapshot = config === null ? null : ($state.snapshot(config) as ExifFrameConfig);
    const bg = bgColor;
    const path = imagePath;
    if (!path || !snapshot) return;

    clearTimeout(debounceTimer);
    debounceTimer = setTimeout(async () => {
      loading = true;
      try {
        const preview = await renderExifFramePreview(path, snapshot, bg);
        src = preview.data_url;
        // アセット由来の警告（カスタム model_map の不備など）は返ってくるので
        // 従来どおり toast する。フレーム描画由来の警告は Rust 側で捨てている
        // （プレビューは長辺 400px 固定で偽陽性になるため。spec §5-3）
        for (const warning of preview.warnings) {
          if (reportedWarnings.has(warning)) continue;
          reportedWarnings.add(warning);
          toast.error(warning);
        }
      } catch (e) {
        toast.error(`プレビューの生成に失敗しました: ${describeError(e)}`);
      } finally {
        loading = false;
      }
    }, 300);
    return () => clearTimeout(debounceTimer);
  });
</script>

<div class="preview">
  {#if !imagePath}
    <!-- フレームモードに写真グリッドは出ないので「グリッドで選べ」とは書かない。
         選び直しの導線は右パネルの「写真を選ぶ」（Step 5） -->
    <p class="status">見本にする写真がありません。右の「見本写真」から選んでください。</p>
  {:else if loading && !src}
    <p class="status">読み込み中...</p>
  {:else if src}
    <img {src} alt="Exif フレームのプレビュー" class:stale={loading} />
  {:else}
    <p class="status">プレビューを生成できませんでした。</p>
  {/if}
</div>

<style>
  .preview {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    padding: var(--space-5);
    background: var(--md-sys-color-surface);
  }

  img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
    border-radius: var(--md-sys-shape-corner-sm);
    box-shadow: var(--md-sys-elevation-shadow-2);
    transition: opacity var(--md-sys-motion-duration-short)
      var(--md-sys-motion-easing-standard);
  }

  /* 再生成中も直前の絵を出したままにする。消すとちらつく */
  img.stale {
    opacity: 0.6;
  }

  .status {
    margin: 0;
    color: var(--md-sys-color-on-surface-variant);
  }
</style>
```

- [ ] **Step 4: `PresetList.svelte` を書く**

```svelte
<script lang="ts">
  import Button from "../ui/Button.svelte";
  import IconButton from "../ui/IconButton.svelte";
  import type { ExifFrameConfig } from "../types";
  import { BUNDLED_PRESET_NAME } from "./frameDraft.svelte";

  interface Props {
    presets: ExifFrameConfig[];
    editingName: string;
    onSelect: (name: string) => void;
    /** ダブルクリックでの改名。名前を変えるだけで、旧名の削除は保存時（Step 6） */
    onRename: (name: string) => void;
    onCreate: () => void;
    onDelete: (name: string) => void;
  }

  let { presets, editingName, onSelect, onRename, onCreate, onDelete }: Props = $props();

  let renaming = $state<string | null>(null);
  let renameValue = $state("");

  function startRename(name: string) {
    if (name === BUNDLED_PRESET_NAME) return; // 組み込みは改名できない
    renaming = name;
    renameValue = name;
  }

  function commitRename() {
    const next = renameValue.trim();
    if (next.length > 0) onRename(next);
    renaming = null;
  }
</script>

<div class="preset-list">
  <div class="head">
    <span>プリセット</span>
    <IconButton label="新規プリセット" icon="＋" onclick={onCreate} />
  </div>

  <ul>
    {#each presets as preset (preset.name)}
      <li>
        {#if renaming === preset.name}
          <!-- svelte-ignore a11y_autofocus -->
          <input
            class="rename"
            autofocus
            bind:value={renameValue}
            onblur={commitRename}
            onkeydown={(e) => {
              if (e.key === "Enter") commitRename();
              if (e.key === "Escape") renaming = null;
            }}
          />
        {:else}
          <button
            class="item state-layer"
            class:active={preset.name === editingName}
            type="button"
            aria-current={preset.name === editingName}
            onclick={() => onSelect(preset.name)}
            ondblclick={() => startRename(preset.name)}
          >
            {preset.name}
          </button>
          {#if preset.name !== BUNDLED_PRESET_NAME}
            <IconButton
              label="{preset.name} を削除"
              icon="🗑"
              onclick={() => onDelete(preset.name)}
            />
          {/if}
        {/if}
      </li>
    {/each}
  </ul>

  <p class="hint">項目をダブルクリックで改名できます。</p>
</div>

<style>
  .preset-list {
    display: flex;
    flex-direction: column;
    height: 100%;
    padding: var(--space-3);
    gap: var(--space-2);
  }

  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font: var(--md-sys-typescale-title-sm);
    color: var(--md-sys-color-on-surface-variant);
  }

  ul {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  li {
    display: flex;
    align-items: center;
    gap: var(--space-1);
  }

  .item {
    flex: 1;
    min-width: 0;
    text-align: left;
    padding: var(--space-2) var(--space-3);
    border: none;
    border-radius: var(--md-sys-shape-corner-full);
    background: none;
    color: var(--md-sys-color-on-surface);
    font: var(--md-sys-typescale-body-md);
    cursor: pointer;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .item.active {
    background: var(--md-sys-color-primary-container);
    color: var(--md-sys-color-on-primary-container);
  }

  .rename {
    flex: 1;
    min-width: 0;
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--md-sys-color-primary);
    border-radius: var(--md-sys-shape-corner-sm);
    background: var(--md-sys-color-surface-container-highest);
    color: var(--md-sys-color-on-surface);
    font: var(--md-sys-typescale-body-md);
  }

  .hint {
    margin: 0;
    font: var(--md-sys-typescale-body-sm);
    color: var(--md-sys-color-on-surface-variant);
  }
</style>
```

- [ ] **Step 5: `FramePanel.svelte`（右カラム）を書く**

現行の表示項目 10 個・配置位置 5 個・フォント・フォントサイズ・カスタムテキストを、
`Card` と `SegmentedButton` とチップで組み直す。

要点だけ挙げる（残りは現行 `ExifFrameSettings.svelte` の該当セクションを写す）:

```svelte
<script lang="ts">
  // …import…
  import type { DisplayItems, ExifPosition, ExifFrameConfig, FontInfo } from "../types";

  interface Props {
    config: ExifFrameConfig;
    bgColor: "white" | "black";
    fonts: FontInfo[];
    isNew: boolean;
    /** 一覧でダブルクリックして名前を変えた状態。保存で旧名が消える */
    isRenamed: boolean;
    /** 既存の別プリセットと名前がぶつかっている */
    nameConflict: boolean;
    canSave: boolean;
    canDelete: boolean;
    /** 見本にしている写真のファイル名。未選択なら null */
    sampleName: string | null;
    onSave: () => void;
    onDelete: () => void;
    /** 見本写真を選び直す（spec §5-3「選び直しはパネル内のボタンから」） */
    onPickSample: () => void;
  }

  let {
    config = $bindable(),
    bgColor = $bindable(),
    fonts,
    isNew,
    isRenamed,
    nameConflict,
    canSave,
    canDelete,
    sampleName,
    onSave,
    onDelete,
    onPickSample,
  }: Props = $props();

  const POSITIONS: { value: ExifPosition; label: string }[] = [
    { value: "auto", label: "自動" },
    { value: "bottom", label: "下" },
    { value: "top", label: "上" },
    { value: "right", label: "右" },
    { value: "left", label: "左" },
  ];

  const ITEMS: { key: keyof DisplayItems; label: string }[] = [
    { key: "maker_logo", label: "ロゴ" },
    { key: "lens_brand_logo", label: "レンズブランド" },
    { key: "camera_model", label: "カメラ" },
    { key: "lens_model", label: "レンズ" },
    { key: "focal_length", label: "焦点距離" },
    { key: "f_number", label: "F値" },
    { key: "shutter_speed", label: "SS" },
    { key: "iso", label: "ISO" },
    { key: "date_taken", label: "日時" },
    { key: "custom_text", label: "テキスト" },
  ];

  let fontOptions = $derived([
    ...fonts.map((f) => ({ value: f.path ?? "", label: f.display_name })),
    // プリセットが参照するフォントが見つからない場合も選択状態を失わせない
    ...(config.font.font_path && !fonts.some((f) => f.path === config.font.font_path)
      ? [{ value: config.font.font_path, label: `${config.font.font_path}（見つかりません）` }]
      : []),
  ]);

</script>
```

**フォントの値をローカルの `$state` に写して `$effect` で書き戻してはならない。**
`let fontValue = $state(config.font.font_path ?? "")` は初期化 1 回きりで、
`frame.select()` が `draft` を新しいオブジェクトに差し替えても `FramePanel` は
再マウントされないため、**プリセットを切り替えても前のフォントを表示し続ける**。
さらに、その状態で一度でも `Select` を触ると `$effect` が新しい draft の
`font.font_path` を古い値で上書きして保存する。

`Select` には**関数バインディング**で `config.font.font_path` を直接読み書きさせる
（Svelte 5.9 以降。このリポジトリは 5.54.1 で、実コンパイル確認済み）。
`null` と `""` の変換だけをここで吸収する:

```svelte
      <Select
        bind:value={
          () => config.font.font_path ?? "",
          (v) => (config.font.font_path = v === "" ? null : v)
        }
        label="フォント"
        options={fontOptions}
      />
```

マークアップの骨格:

```svelte
<div class="panel">
  <div class="scroll">
    <!-- rail の destination として常時見えるようになるため、
         crop / quality しか使わない利用者への注記を出す（spec §5-3） -->
    <p class="note">Exif フレームは pad モードでのみ出力されます。</p>

    <!-- 見本写真の出所は focusedPath（spec §3-2 / §5-3）。フレームモードには
         グリッドが無いので、選び直しの導線をパネル内に置く（spec §5-3
         「選び直しはパネル内のボタンから」）。押すと変換モードへ移る -->
    <Card level={1} title="見本写真">
      <p class="sample">{sampleName ?? "未選択"}</p>
      <Button variant="outlined" onclick={onPickSample}>
        {sampleName ? "別の写真を選ぶ" : "写真を選ぶ"}
      </Button>
    </Card>

    <Card level={1} title="背景色">
      <!-- 値は変換設定と同じ config.bg_color。置き場所が 2 つあるだけ（spec §5-3） -->
      <SegmentedButton
        bind:value={bgColor}
        label="背景色"
        options={[{ value: "white", label: "白" }, { value: "black", label: "黒" }]}
      />
    </Card>

    <Card level={1} title="配置位置">
      <SegmentedButton bind:value={config.position} label="配置位置" options={POSITIONS} />
    </Card>

    <Card level={1} title="表示項目">
      <div class="chips" role="group" aria-label="表示項目">
        {#each ITEMS as item (item.key)}
          <button
            class="chip state-layer"
            class:on={config.items[item.key]}
            type="button"
            aria-pressed={config.items[item.key]}
            onclick={() => (config.items[item.key] = !config.items[item.key])}
          >{item.label}</button>
        {/each}
      </div>
    </Card>

    <Card level={1} title="フォント">
      <!-- 値は config.font.font_path が唯一の持ち主。ローカルに写さない（上記） -->
      <Select
        bind:value={
          () => config.font.font_path ?? "",
          (v) => (config.font.font_path = v === "" ? null : v)
        }
        label="フォント"
        options={fontOptions}
      />
      <div class="sub">
        <Slider bind:value={config.font.primary_size} label="メイン" min={0.015} max={0.05}
          step={0.001} suffix="%" format={(v) => (v * 100).toFixed(1)} />
      </div>
      <div class="sub">
        <Slider bind:value={config.font.secondary_size} label="サブ" min={0.01} max={0.035}
          step={0.001} suffix="%" format={(v) => (v * 100).toFixed(1)} />
      </div>
    </Card>

    <Card level={1} title="カスタムテキスト">
      <TextField bind:value={config.custom_text} label="テキスト" placeholder="@username" />
    </Card>
  </div>

  <div class="action">
    {#if nameConflict}
      <p class="conflict" role="alert">同じ名前のプリセットが既にあります。</p>
    {/if}
    {#if canDelete}
      <Button variant="text" danger onclick={onDelete}>削除</Button>
    {/if}
    <Button variant="filled" full disabled={!canSave} onclick={onSave}>
      {isNew ? "新規保存" : isRenamed ? "名前を変えて保存" : "保存"}
    </Button>
  </div>
</div>
```

**保存ボタンの文言を 3 通りに分けるのは、押した結果が違うため。**
「新規保存」は増える、「名前を変えて保存」は旧名が消える、「保存」は上書きする。

`.panel` / `.scroll` / `.sub` / `.action` のスタイルは
**`ConvertPanel.svelte` のものをそのまま写す**（モード切替の 150ms フェードも
そこに入っている。spec §3-3）。`.sample` と `.conflict` は:

```css
  .sample {
    margin: 0 0 var(--space-2);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font: var(--md-sys-typescale-body-sm);
    color: var(--md-sys-color-on-surface-variant);
  }

  .conflict {
    margin: 0;
    font: var(--md-sys-typescale-body-sm);
    color: var(--md-sys-color-error);
  }
```

`.note` は:

```css
  .note {
    margin: 0;
    padding: var(--space-2) var(--space-3);
    border-radius: var(--md-sys-shape-corner-sm);
    background: var(--md-sys-color-primary-container);
    color: var(--md-sys-color-on-primary-container);
    font: var(--md-sys-typescale-body-sm);
  }
```

チップのスタイル:

```css
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  .chip {
    padding: var(--space-1) var(--space-3);
    border: 1px solid var(--md-sys-color-outline);
    border-radius: var(--md-sys-shape-corner-xs);
    background: none;
    color: var(--md-sys-color-on-surface-variant);
    font: var(--md-sys-typescale-body-sm);
    cursor: pointer;
  }

  .chip.on {
    background: var(--md-sys-color-primary-container);
    border-color: transparent;
    color: var(--md-sys-color-on-primary-container);
  }
```

- [ ] **Step 6: `App.svelte` を配線する**

```ts
  import { listAvailableFonts } from "./lib/api";

  const frame = createFrameDraft();
  let fonts = $state<FontInfo[]>([]);

  onMount(() => {
    // …既存…
    listAvailableFonts()
      .then((f) => (fonts = f))
      .catch((e) => toast.error(`フォント一覧の取得に失敗しました: ${describeError(e)}`));
  });
```

`handleModeChange` にフレームモードの初期化を足す:

```ts
    if (next === "frame" && frame.draft === null) {
      frame.select(presets.selectedName, presets.presets);
    }
```

3 つのスニペットに差し込む:

```svelte
  {#snippet left()}
    {#if mode === "frame"}
      <PresetList
        presets={presets.presets}
        editingName={frame.editingName}
        onSelect={(name) => frame.select(name, presets.presets)}
        onRename={frame.rename}
        onCreate={() => frame.createNew(presets.presets)}
        onDelete={presets.remove}
      />
    {:else}
      <FolderTree onSelectFolder={handleSelectFolder} />
    {/if}
  {/snippet}

  {#snippet center()}
    {#if mode === "frame"}
      <FramePreview config={frame.draft} bgColor={config.bg_color} imagePath={focusedPath} />
    {:else}
      <PhotoGrid … />
    {/if}
  {/snippet}
```

右カラムのフレーム分岐。**`bind:config` は使えない** ──
`frame.draft` は getter しか持たず、型も `ExifFrameConfig | null` で
`FramePanel` の `ExifFrameConfig` と相互代入可能にならない。
`{@const}` で絞ってから非 `bind:` で渡す。`FramePanel` 側は `config` を
`$bindable()` として受け、**プロパティを直接書き換える**（`$state` のプロキシなので
親まで届く。`$bindable()` にしておくのは所有権の警告を出さないため）:

```svelte
    {:else if mode === "frame"}
      {@const draft = frame.draft}
      {#if draft}
      <FramePanel
        config={draft}
        bind:bgColor={config.bg_color}
        {fonts}
        isNew={frame.isNew}
        isRenamed={frame.isRenamed}
        nameConflict={frame.nameConflict}
        canSave={frame.canSave}
        canDelete={frame.canDelete}
        sampleName={images.find((img) => img.path === focusedPath)?.name ?? null}
        onSave={async () => {
          const snap = frame.snapshot();
          // 改名なら「新しい名前で保存 → 旧名を削除」。それ以外は普通の保存
          const from = frame.renamedFrom;
          const ok = from
            ? await presets.rename(from, snap)
            : await presets.save(snap);
          if (ok) frame.select(snap.name, presets.presets);
        }}
        onDelete={() => presets.remove(frame.editingName)}
        onPickSample={() => handleModeChange("convert")}
      />
      {/if}
    {/if}
```

**見本写真は `focusedPath`（最後にクリックした 1 枚）を使う**（spec §5-3）。
フレームモードにはグリッドが無いので、**選び直しの導線はパネル内のボタン**
（spec §5-3「選び直しはパネル内のボタンから」）とし、押したら変換モードへ移す。
移った先でグリッドの写真をクリックすると `focusedPath` が動き（spec §3-2）、
rail でフレームへ戻ると新しい見本でプレビューが出る。

**`onSave` の後に `frame.select(snap.name, …)` を呼ぶのは、`editingName` を
保存後の名前へ合わせ直すため。** これをしないと、改名の直後にもう一度保存したときに
`renamedFrom` が消えた旧名を指し、存在しないプリセットを削除しようとする。

`showExifFrameSettings` とそれに紐づくモーダルの呼び出しは**すべて削除**する。

```bash
git rm gui-frontend/src/lib/ExifFrameSettings.svelte
grep -rn "ExifFrameSettings\|showExifFrameSettings" gui-frontend/src/
```

期待: 0 件。

- [ ] **Step 7: `test-integrity` スキルを起動し、検査を書く**

`gui-frontend/e2e/frame.spec.ts`:

```ts
import { expect, test } from "@playwright/test";
import { clearStorageOnce, installTauriStub } from "./stub";

test.describe("フレームモード", () => {
  test.beforeEach(async ({ page }) => {
    await installTauriStub(page, { imageCount: 12 });
    await clearStorageOnce(page);
    await page.goto("/");
    await page.getByRole("button", { name: "photos", exact: false }).first().click();
    await page.getByRole("option").first().click();
    await page.getByRole("navigation", { name: "モード" })
      .getByRole("button", { name: "フレーム" }).click();
  });

  test("pad モード限定の注記が出る（spec §5-3）", async ({ page }) => {
    await expect(
      page.getByText("Exif フレームは pad モードでのみ出力されます。")
    ).toBeVisible();
  });

  test("背景色は変換設定と同じ値を指す（spec §5-3）", async ({ page }) => {
    const rail = page.getByRole("navigation", { name: "モード" });

    // 前提条件: フレームパネルの背景色は初期状態で「白」
    await expect(page.getByRole("radio", { name: "白" })).toBeChecked();
    await page.getByRole("radio", { name: "黒" }).click();

    // 変換モードへ戻ると、変換設定側の背景色も黒になっている
    await rail.getByRole("button", { name: "変換" }).click();
    await page.getByRole("radio", { name: "Pad" }).click();
    await expect(page.getByRole("radio", { name: "黒" })).toBeChecked();
  });

  test("プレビューが出る", async ({ page }) => {
    await expect(page.getByRole("img", { name: "Exif フレームのプレビュー" })).toBeVisible();
  });

  test("表示項目チップの入切が押下状態に出る", async ({ page }) => {
    const chip = page.getByRole("button", { name: "日時" });
    // 前提条件: 既定では日時は切
    await expect(chip).toHaveAttribute("aria-pressed", "false");
    await chip.click();
    await expect(chip).toHaveAttribute("aria-pressed", "true");
  });

  test("組み込みプリセットは削除も改名もできない", async ({ page }) => {
    await expect(page.getByRole("button", { name: "default を削除" })).toHaveCount(0);
    await page.getByRole("button", { name: "default" }).dblclick();
    await expect(page.locator("input.rename")).toHaveCount(0);
  });

  test("新規プリセットを作ると一覧に増え、保存ボタンが新規保存になる", async ({ page }) => {
    await page.getByRole("button", { name: "新規プリセット" }).click();
    await expect(page.getByRole("button", { name: "新規保存" })).toBeEnabled();
    await page.getByRole("button", { name: "新規保存" }).click();
    await expect(page.getByRole("button", { name: "preset-1", exact: true })).toBeVisible();
  });

  test("改名は改名であって複製ではない（spec §5-3）", async ({ page }) => {
    // 改名できるプリセットを 1 つ作る（組み込みの default は改名できない）
    await page.getByRole("button", { name: "新規プリセット" }).click();
    await page.getByRole("button", { name: "新規保存" }).click();
    // 前提条件: 保存されて一覧に出ていること。ここが無いと
    // 「旧名が消えた」は「そもそも作られていない」でも成立してしまう
    await expect(page.getByRole("button", { name: "preset-1", exact: true })).toBeVisible();

    await page.getByRole("button", { name: "preset-1", exact: true }).dblclick();
    await page.locator("input.rename").fill("夜景");
    await page.locator("input.rename").press("Enter");

    await expect(page.getByRole("button", { name: "名前を変えて保存" })).toBeEnabled();
    await page.getByRole("button", { name: "名前を変えて保存" }).click();

    await expect(page.getByRole("button", { name: "夜景", exact: true })).toBeVisible();
    // 旧名は残らない。ここが 1 件のままなら「複製」になっている
    await expect(page.getByRole("button", { name: "preset-1", exact: true })).toHaveCount(0);
  });

  test("既存の名前に改名しようとすると保存できない", async ({ page }) => {
    await page.getByRole("button", { name: "新規プリセット" }).click();
    await page.getByRole("button", { name: "新規保存" }).click();
    await page.getByRole("button", { name: "preset-1", exact: true }).dblclick();
    await page.locator("input.rename").fill("default");
    await page.locator("input.rename").press("Enter");

    await expect(page.getByText("同じ名前のプリセットが既にあります。")).toBeVisible();
    await expect(page.getByRole("button", { name: "名前を変えて保存" })).toBeDisabled();
  });
});
```

**アセット由来の警告が toast されることの検査**は `installTauriStub` に
警告を返させて確かめる。`stub.ts` の `render_exif_frame_preview` を差し替えられるよう、
`StubOptions` に `frameWarnings?: string[]` を足し、既定は `[]` とする:

```ts
test("アセット由来の警告は toast される（spec §5-3 / S6-M15）", async ({ page }) => {
  await installTauriStub(page, { imageCount: 4, frameWarnings: ["model_map の書式が不正です"] });
  await page.goto("/");
  // …フレームモードまで進める…
  await expect(page.getByText("model_map の書式が不正です")).toBeVisible();
});
```

（この test は `beforeEach` のスタブ設定と衝突するので、別 `describe` に置くこと。）

- [ ] **Step 8: 走らせる**

```bash
cd gui-frontend && bun run typecheck && bun test && bun run e2e
```

- [ ] **Step 9: 実機で確認する（段階 7 の完了の目印）**

```bash
make dev
```

- プリセットの**作成・編集・削除**が通る
- 一覧をダブルクリックして改名 → 保存すると**旧名が消えて新しい名前だけになる**
  （`~/.config/picture-tool/presets/` を直接見て確認する。複製になっていたら実装が違う）
- フレームモードで「写真を選ぶ」を押すと変換モードへ移り、
  写真をクリックしてフレームへ戻ると見本が入れ替わる
- 背景色をフレームパネルで変えると変換設定にも反映される
- カスタム `model_map` を壊した状態で開くと toast が出る（`assets.warnings`）

- [ ] **Step 10: コミット**

```bash
git add -A gui-frontend
git commit -m "feat(gui): Exif フレーム設定モーダルを独立したフレームモードへ解体"
```

---

## Task 17: メタデータパネルのレイアウト（段階 8）

spec §5-2。**本刷新で作るのは静的なレイアウトまで。**
`read_image_metadata` / `write_image_metadata` / `grant_metadata_editing` は
次工程で追加される Tauri コマンドなので、この時点では
**撮影情報（`getExifInfo`、既存）だけが実データ**。
タイトル・コメント・★は編集できるが保存先が無い状態にする。
**保存ボタンは disabled、未保存ガードも配線しない。**

**Files:**
- Create: `gui-frontend/src/lib/panels/metadataDraft.svelte.ts`
- Create: `gui-frontend/src/lib/panels/MetadataPanel.svelte`
- Modify: `gui-frontend/src/App.svelte`

**Interfaces:**
- Produces: `createMetadataDraft()`
  - `readonly path: string | null` / `values: { title: string; comment: string; rating: number }`
  - `readonly isDirty: boolean`
  - `load(path: string | null): void` — 次工程で `read_image_metadata` を差し込む場所
  - `discard(): void`
  - **`isDirty` を読む側は本刷新では存在しない。** 3-4 の離脱経路（モード切替・
    フォルダー変更・`editingPath` の変更）へ繋ぐのはメタデータ編集の工程
- Produces: `MetadataPanel`
  `{ image: ImageEntry | null, draft: ReturnType<typeof createMetadataDraft>,
     thumbnailFor, onRequestThumbnail }`

- [ ] **Step 1: `metadataDraft.svelte.ts` を書く**

```ts
export interface MetadataValues {
  title: string;
  comment: string;
  /** 0〜5。0 は未設定 */
  rating: number;
}

function empty(): MetadataValues {
  return { title: "", comment: "", rating: 0 };
}

/**
 * メタデータの下書き。
 *
 * 本刷新ではレイアウトのためだけに存在する。値の読み込み（read_image_metadata）と
 * 保存（write_image_metadata）は次工程で追加される Tauri コマンドなので、
 * load() は「保存済みの値」を空にリセットするだけ（spec §5-2）。
 *
 * isDirty は最初から持たせる。3-4 の離脱経路（メタデータモード内のフォーカス移動 /
 * rail での別モードへの移動 / フォルダー変更）をここへ通すのは次工程だが、
 * 判定そのものをここに置いておけば、繋ぎ込みが分岐を増やさずに済む。
 */
export function createMetadataDraft() {
  let path = $state<string | null>(null);
  let values = $state<MetadataValues>(empty());
  let saved = $state<MetadataValues>(empty());

  return {
    get path() {
      return path;
    },
    get values() {
      return values;
    },
    get isDirty(): boolean {
      return (
        values.title !== saved.title ||
        values.comment !== saved.comment ||
        values.rating !== saved.rating
      );
    },

    /** 次工程で read_image_metadata の結果を saved に入れる */
    load(next: string | null) {
      path = next;
      saved = empty();
      values = empty();
    },

    discard() {
      values = { ...saved };
    },
  };
}
```

- [ ] **Step 2: `MetadataPanel.svelte` を書く**

spec §5-2 の並び（上から 7 つ）をそのまま作る。

```svelte
<script lang="ts">
  import Button from "../ui/Button.svelte";
  import Card from "../ui/Card.svelte";
  import Rating from "../ui/Rating.svelte";
  import TextField from "../ui/TextField.svelte";
  import { getExifInfo } from "../api";
  import type { RequestKind } from "../browser/requestQueue";
  import type { ExifInfo, ImageEntry } from "../types";
  import type { createMetadataDraft } from "./metadataDraft.svelte";

  interface Props {
    image: ImageEntry | null;
    draft: ReturnType<typeof createMetadataDraft>;
    thumbnailFor: (path: string, size: number) => string | undefined;
    onRequestThumbnail: (
      path: string,
      size: number,
      kind: RequestKind,
      index: number
    ) => void;
  }

  let { image, draft, thumbnailFor, onRequestThumbnail }: Props = $props();

  const THUMB = 160;

  let exif = $state<ExifInfo | null>(null);
  let exifToken = 0;

  $effect(() => {
    const path = image?.path ?? null;
    if (!path) {
      exif = null;
      return;
    }
    // パネル先頭のサムネイルはグリッドの index 範囲に入らないので pinned
    onRequestThumbnail(path, THUMB, "pinned", -1);

    const token = ++exifToken;
    getExifInfo(path)
      .then((info) => {
        if (token === exifToken) exif = info;
      })
      .catch(() => {
        // EXIF は無くても表示は成立するので通知しない
        if (token === exifToken) exif = null;
      });
  });

  let exifRows = $derived(
    exif === null
      ? []
      : [
          ["カメラ", [exif.camera_make, exif.camera_model].filter(Boolean).join(" ")],
          ["レンズ", exif.lens_model ?? ""],
          ["焦点距離", exif.focal_length ?? ""],
          ["F値", exif.f_number ?? ""],
          ["SS", exif.shutter_speed ?? ""],
          ["ISO", exif.iso === null ? "" : String(exif.iso)],
          ["撮影日時", exif.date_taken ?? ""],
        ].filter(([, value]) => value !== "")
  );
</script>

<div class="panel">
  <div class="scroll">
    {#if !image}
      <p class="empty">グリッドで写真を 1 枚選んでください。</p>
    {:else}
      <!-- 1. サムネイル + ファイル名 + 未保存表示 -->
      <Card level={1} padding="var(--space-3)">
        <div class="head">
          {#if thumbnailFor(image.path, THUMB)}
            <img class="thumb" src="data:image/jpeg;base64,{thumbnailFor(image.path, THUMB)}" alt="" />
          {:else}
            <div class="thumb placeholder" aria-hidden="true">📷</div>
          {/if}
          <div class="head-text">
            <p class="name">{image.name}</p>
            <p class="dims">{image.width}×{image.height}</p>
            {#if draft.isDirty}
              <p class="unsaved">未保存の変更があります</p>
            {/if}
          </div>
        </div>
      </Card>

      <!-- 2. タイトル / 3. コメント -->
      <Card level={1} title="タイトルとコメント">
        <TextField bind:value={draft.values.title} label="タイトル" placeholder="未設定" />
        <div class="sub">
          <TextField
            bind:value={draft.values.comment}
            label="コメント"
            multiline
            rows={4}
            placeholder="未設定"
          />
        </div>
        <!-- 食い違い警告の表示領域（次工程で XPToolkit / MWG の不一致を出す） -->
        <div class="mismatch" aria-live="polite"></div>
      </Card>

      <!-- 4. レーティング -->
      <Card level={1} title="レーティング">
        <Rating bind:value={draft.values.rating} />
      </Card>

      <!-- 5. 撮影情報（読み取り専用） -->
      <Card level={1} title="撮影情報">
        {#if exifRows.length === 0}
          <p class="empty">Exif がありません。</p>
        {:else}
          <dl>
            {#each exifRows as [key, value] (key)}
              <dt>{key}</dt>
              <dd>{value}</dd>
            {/each}
          </dl>
        {/if}
      </Card>

      <!-- 6. 書き込み承認の状態表示。
           本刷新では disabled のまま場所だけ確保する（spec §5-2）。
           ここを作っておかないと「刷新後に部品を継ぎ足す事故を避ける」という
           本 spec の目的に穴が開く -->
      <Card level={1} title="書き込みの許可">
        <p class="empty">このフォルダーへの書き込みはまだ許可されていません。</p>
        <Button variant="outlined" disabled>このフォルダーへの書き込みを許可...</Button>
      </Card>
    {/if}
  </div>

  <!-- 7. 連続して付けていく作業が主なので、次へ送りを主ボタンに置く（spec §5-2） -->
  <div class="action">
    <Button variant="filled" full disabled>保存して次の写真へ</Button>
    <Button variant="outlined" full disabled>保存</Button>
  </div>
</div>
```

スタイルは `ConvertPanel` の `.panel` / `.scroll` / `.sub` / `.action` を踏襲する
（モード切替の 150ms フェードもそこに入っている。spec §3-3。ただし `.action` は
ボタンを縦に 2 つ並べるので下の定義で上書きする）。追加分:

```css
  .head {
    display: flex;
    gap: var(--space-3);
  }

  .thumb {
    width: 64px;
    height: 80px;
    flex-shrink: 0;
    object-fit: cover;
    border-radius: var(--md-sys-shape-corner-sm);
    background: var(--md-sys-color-surface-container-high);
  }

  .placeholder {
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .head-text {
    min-width: 0;
  }

  .name {
    margin: 0;
    font: var(--md-sys-typescale-title-sm);
    overflow-wrap: anywhere;
  }

  .dims,
  .empty {
    margin: 0;
    font: var(--md-sys-typescale-body-sm);
    color: var(--md-sys-color-on-surface-variant);
  }

  .unsaved {
    margin: var(--space-1) 0 0;
    font: var(--md-sys-typescale-body-sm);
    color: var(--md-sys-color-error);
  }

  .mismatch:empty {
    display: none;
  }

  dl {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: var(--space-1) var(--space-3);
    margin: 0;
    font: var(--md-sys-typescale-body-sm);
  }

  dt {
    color: var(--md-sys-color-on-surface-variant);
  }

  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }

  .action {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
```

- [ ] **Step 3: `App.svelte` を配線する**

```ts
  const metadata = createMetadataDraft();

  let editingImage = $derived(
    editingPath === null ? null : (images.find((img) => img.path === editingPath) ?? null)
  );

  $effect(() => {
    metadata.load(editingPath);
  });
```

右カラムのメタデータ分岐を差し替える:

```svelte
    {:else if mode === "metadata"}
      <MetadataPanel
        image={editingImage}
        draft={metadata}
        thumbnailFor={thumbnails.get}
        onRequestThumbnail={thumbnails.request}
      />
```

- [ ] **Step 4: `App.svelte` の行数を確認する**

```bash
wc -l gui-frontend/src/App.svelte
```

期待: 180 行以下（spec §3-5 の「150 行程度」）。超えているなら、
残っている処理のうち「モード／フォルダー／選択／フォーカスの 4 状態と
パネルの差し替え」に当たらないものを探して切り出す。

- [ ] **Step 5: `test-integrity` スキルを起動し、検査を書く**

`gui-frontend/e2e/metadata.spec.ts`:

```ts
import { expect, test } from "@playwright/test";
import { clearStorageOnce, installTauriStub } from "./stub";

test.describe("メタデータモード（レイアウトのみ）", () => {
  test.beforeEach(async ({ page }) => {
    await installTauriStub(page, { imageCount: 12 });
    await clearStorageOnce(page);
    await page.goto("/");
    await page.getByRole("button", { name: "photos", exact: false }).first().click();
    await page.getByRole("option").first().click();
    await page.getByRole("navigation", { name: "モード" })
      .getByRole("button", { name: "情報" }).click();
  });

  test("spec §5-2 の 7 要素がすべて場所を持つ", async ({ page }) => {
    await expect(page.getByText("photo-0000.jpg")).toBeVisible();
    await expect(page.getByLabel("タイトル")).toBeVisible();
    await expect(page.getByLabel("コメント")).toBeVisible();
    await expect(page.getByRole("slider", { name: "レーティング" })).toBeVisible();
    await expect(page.getByText("撮影情報")).toBeVisible();
    await expect(page.getByText("書き込みの許可")).toBeVisible();
    await expect(page.getByRole("button", { name: "保存して次の写真へ" })).toBeVisible();
    await expect(page.getByRole("button", { name: "保存", exact: true })).toBeVisible();
  });

  test("保存ボタンは disabled（データ接続は次工程。spec §5-2）", async ({ page }) => {
    await expect(page.getByRole("button", { name: "保存して次の写真へ" })).toBeDisabled();
    await expect(page.getByRole("button", { name: "保存", exact: true })).toBeDisabled();
    await expect(
      page.getByRole("button", { name: "このフォルダーへの書き込みを許可..." })
    ).toBeDisabled();
  });

  test("撮影情報は実データ（getExifInfo）が出る", async ({ page }) => {
    await expect(page.getByText("ILCE-7M4", { exact: false })).toBeVisible();
    await expect(page.getByText("FE 35mm F1.4 GM")).toBeVisible();
  });

  test("メタデータモードのグリッドは単一フォーカス（spec §3-2）", async ({ page }) => {
    const grid = page.getByRole("listbox", { name: "写真" });
    await expect(grid).toHaveAttribute("aria-multiselectable", "false");

    await grid.getByRole("option").nth(2).click();
    await expect(grid.getByRole("option").nth(2)).toHaveAttribute("aria-selected", "true");
    await expect(grid.getByRole("option").nth(0)).toHaveAttribute("aria-selected", "false");
    await expect(page.getByText("photo-0002.jpg")).toBeVisible();
  });

  test("変換モードのクリックは editingPath を動かさない（spec §3-2）", async ({ page }) => {
    const rail = page.getByRole("navigation", { name: "モード" });

    // 前提条件: いまメタデータの対象は 1 枚目
    await expect(page.getByText("photo-0000.jpg")).toBeVisible();

    await rail.getByRole("button", { name: "変換" }).click();
    await page.getByRole("option").nth(5).click();
    await rail.getByRole("button", { name: "情報" }).click();

    // 変換モードでの選択は focusedPath しか動かさないので、編集対象は 1 枚目のまま
    await expect(page.getByText("photo-0000.jpg")).toBeVisible();
  });
});
```

最後の test は spec §3-2 の核心（`editingPath` と `focusedPath` を分ける理由）を
そのまま検査している。**ここが落ちるなら `handleFocus` の分岐が間違っている。**

- [ ] **Step 6: 走らせてコミット**

```bash
cd gui-frontend && bun run typecheck && bun test && bun run e2e
git add -A gui-frontend
git commit -m "feat(gui): メタデータパネルのレイアウトを追加（データ接続は次工程）"
```

---

## Task 18: 旧変数の削除と総点検（段階 9）

spec §6 の段階 9 / §7-3。**この完了が「デザイン刷新の完了」。**

**Files:**
- Modify: `gui-frontend/src/app.css`（旧変数の削除）
- Create: `gui-frontend/e2e/a11y.spec.ts`, `gui-frontend/e2e/errors.spec.ts`
- Modify: `docs/README.md`, `CLAUDE.md`, 本計画（実施メモ）

- [ ] **Step 1: 旧変数の参照が 0 件であることを確認する**

```bash
grep -rn -- "--bg-primary\|--bg-secondary\|--bg-hover\|--border-color\|--text-primary\|--text-secondary\|--text-muted\|--accent\|--accent-hover\|--accent-bg\|--danger\|--success\|--warning\|--radius" gui-frontend/src/ | grep -v "src/app.css"
```

期待: **0 件**。残っていたらそのファイルを先に直す。

- [ ] **Step 2: `app.css` を刈り込む**

`:root { … }` ブロックを丸ごと削除し、リセットと `body` と
スクロールバーだけを残してトークンに寄せる:

```css
/* リセットと最小限の地の設定だけを持つ。
   色・形・余白・文字のトークンは styles/tokens.css にある。 */
* {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
}

body {
  font: var(--md-sys-typescale-body-md);
  background: var(--md-sys-color-surface);
  color: var(--md-sys-color-on-surface);
  overflow: hidden;
  height: 100vh;
}

#app {
  height: 100vh;
  display: flex;
  flex-direction: column;
}

::-webkit-scrollbar {
  width: 8px;
  height: 8px;
}

::-webkit-scrollbar-track {
  background: transparent;
}

::-webkit-scrollbar-thumb {
  background: var(--md-sys-color-outline-variant);
  border-radius: var(--md-sys-shape-corner-full);
}

::-webkit-scrollbar-thumb:hover {
  background: var(--md-sys-color-outline);
}
```

再度 grep して、`app.css` からも旧変数が消えたことを確認する:

```bash
grep -rn -- "--bg-primary\|--accent\|--radius:" gui-frontend/src/
```

期待: 0 件。

- [ ] **Step 3: 生の色が残っていないことを確認する**

```bash
grep -rnE "#[0-9a-fA-F]{3,8}\b|rgba?\(" gui-frontend/src/ \
  | grep -v "src/styles/" | grep -v "\.test\.ts"
```

期待: 0 件。**残るのは `styles/` の 2 ファイルだけ**（spec §1 の再発防止線）。
`Toast.svelte` などに `rgba(0,0,0,0.4)` が残っていたら
`var(--md-sys-elevation-shadow-*)` に置き換える。

**px は grep しない。** Global Constraints のとおり、部品固有の内部寸法
（`Button` の `min-height: 40px`、`Switch` のトラック 52×32px 等）は
各コンポーネント内に直接書いてよい。機械的に検査するのは色だけである。
代わりに目で見るのは次の 2 点で、いずれも見つけたらトークンへ直す:

- 要素の**間**の余白（`gap` / `margin` / `padding`）に生の px が使われていないか
- `border-radius` / `font-size` / `transition` の時間に生の値が使われていないか

```bash
grep -rnE "(gap|margin|padding)[^:]*:\s*[0-9]+px" gui-frontend/src/ | grep -v "src/styles/"
grep -rnE "border-radius:\s*[0-9]" gui-frontend/src/ | grep -v "src/styles/"
```

期待: 0 件（`grid` の `padding` など spec §4-1 に数値が定義されているものを除く。
除外したものはこの Step のチェック時にコメントで理由が書かれていること）。

- [ ] **Step 4: `test-integrity` スキルを起動し、総点検の検査を書く**

`gui-frontend/e2e/a11y.spec.ts`:

```ts
import { expect, test } from "@playwright/test";
import { clearStorageOnce, installTauriStub } from "./stub";

test.describe("総点検（spec §7-3）", () => {
  test.beforeEach(async ({ page }) => {
    await installTauriStub(page, { imageCount: 40 });
    await clearStorageOnce(page);
  });

  test("prefers-reduced-motion で全トランジションが止まる", async ({ page }) => {
    await page.emulateMedia({ reducedMotion: "reduce" });
    await page.goto("/");
    await page.getByRole("button", { name: "photos", exact: false }).first().click();

    const durations = await page.evaluate(() =>
      Array.from(document.querySelectorAll("*"))
        .map((el) => getComputedStyle(el).transitionDuration)
        .flatMap((v) => v.split(",").map((s) => Number.parseFloat(s)))
        .filter((n) => Number.isFinite(n))
    );

    // 前提条件: そもそも transition を持つ要素があること。
    // 0 個なら「止まっている」は自明に成立してしまう
    const withMotion = await page.evaluate(() =>
      Array.from(document.querySelectorAll("*")).filter(
        (el) => getComputedStyle(el).transitionProperty !== "all"
      ).length
    );
    expect(withMotion).toBeGreaterThan(0);

    expect(Math.max(...durations)).toBeLessThan(0.01);
  });

  test("Tab 順が rail → 左カラム → グリッド → 右パネル", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "photos", exact: false }).first().click();

    const order: string[] = [];
    for (let i = 0; i < 12; i++) {
      await page.keyboard.press("Tab");
      order.push(
        await page.evaluate(
          () =>
            document.activeElement?.closest("[data-region]")?.getAttribute("data-region") ??
            document.activeElement?.tagName ??
            ""
        )
      );
    }
    // 前提条件: フォーカスが実際に動いていること
    expect(new Set(order).size).toBeGreaterThan(1);
  });

  test("ライトとダークの両方でスクリーンショットを撮る", async ({ page }) => {
    for (const scheme of ["light", "dark"] as const) {
      await page.emulateMedia({ colorScheme: scheme });
      await page.goto("/");
      await page.getByRole("button", { name: "photos", exact: false }).first().click();
      await expect(page.getByRole("option").first()).toBeVisible();

      for (const [modeLabel, file] of [
        ["変換", "convert"],
        ["情報", "metadata"],
        ["フレーム", "frame"],
      ] as const) {
        await page.getByRole("navigation", { name: "モード" })
          .getByRole("button", { name: modeLabel }).click();
        await page.screenshot({ path: `e2e/__screenshots__/${file}-${scheme}.png` });
      }
    }
  });
});
```

**Tab 順の検査には `data-region` 属性が要る。**
`AppShell.svelte` の `.column`（左）／`.center`／`.column.right` に
`data-region="left"` / `"center"` / `"right"`、`NavigationRail` の `nav` に
`data-region="rail"` を足すこと。

- [ ] **Step 5: エラー経路の検査を書く（spec §7-3 の Playwright 2 用途のうち片方）**

`gui-frontend/e2e/errors.spec.ts`。**スタブを注入しない。**
Tauri の外では `window.__TAURI_INTERNALS__` が無く、`invoke` が全部 reject するので、
エラー経路が一度に全部出る。

```ts
import { expect, test } from "@playwright/test";

/**
 * スタブ無しで開く。invoke が全部 reject する状態でも、
 *  - 画面が真っ白にならない
 *  - 握りつぶさずトーストで知らせる
 *  - 例外がコンソールへ漏れない
 * ことを確かめる（spec §7-3）。
 */
test.describe("エラー経路（スタブ無し）", () => {
  test("IPC が全部失敗しても画面は立ち上がり、トーストで知らせる", async ({ page }) => {
    const consoleErrors: string[] = [];
    page.on("pageerror", (e) => consoleErrors.push(String(e)));

    await page.goto("/");

    // シェルは出る
    await expect(page.getByRole("navigation", { name: "モード" })).toBeVisible();
    await expect(page.getByRole("listbox", { name: "写真" })).toBeVisible();

    // 握りつぶさずに知らせている（ドライブ一覧・お気に入りの取得失敗）
    await expect(page.getByRole("region", { name: "通知" }).getByRole("alert").first())
      .toBeVisible();

    // 捕まえ損ねた例外が無い
    expect(consoleErrors).toEqual([]);
  });

  test("3 モードすべてが IPC 失敗下でも描画される", async ({ page }) => {
    await page.goto("/");
    const rail = page.getByRole("navigation", { name: "モード" });
    for (const label of ["情報", "フレーム", "変換"]) {
      await rail.getByRole("button", { name: label }).click();
      await expect(rail.getByRole("button", { name: label })).toHaveAttribute(
        "aria-current",
        "page"
      );
    }
  });
});
```

**`pageerror` が 0 件であることが、この検査の要。**
どこかで `.catch` を付け忘れると unhandled rejection としてここに出る。

- [ ] **Step 6: 走らせる**

```bash
cd gui-frontend && bun run typecheck && bun test && bun run build && bun run e2e
make check
```

- [ ] **Step 7: 明暗のスクリーンショットを目視する**

`gui-frontend/e2e/__screenshots__/` の 6 枚（3 モード × 明暗）を並べて見る。
**ダークのままの島が 1 つも残っていないこと**（Task 6 の中間状態がここで解消される）。

- [ ] **Step 8: 実機で `rAF` 指標を 1 度取る（spec §7-2 の最後の項目）**

Chromium 上の測定は相対比較でしかない。デコードとスクロールの挙動が最も違う層を
またいでいるので、**実機（`make dev` で起動した Tauri）でも同じ指標を 1 度取り、
桁が違わないことを確認する。**

```bash
make dev
```

Tauri の webview には devtools コンソールが開ける（開発ビルド）。
`e2e/scrollPerf.ts` の `page.evaluate` に渡している関数の中身を、
そのままコンソールに貼って実行する。**セレクタは
`'[role="listbox"][aria-label="写真"]'`。**

得られた p95 と 32ms 超の割合を、Chromium での 3,000 枚の値と並べて
本計画の実施メモに記録する。**桁が違ったら、その事実を記録したうえで
spec §7-2 の「限界」の記述を更新する**（測定を捨てるのではなく、
何が分かって何が分からないかを書き残す）。

- [ ] **Step 9: キーボード操作を手で通す（spec §7-3）**

- Tab 順が rail → 左 → グリッド → 右 の順で回ること
- グリッドで **Space = 選択 / Enter = プレビュー**（現行から変わっている）
- グリッドで ← → ↑ ↓ がフォーカスを動かし、画面外へ出るとスクロールが追うこと
- プレビューで ← → が送り、Esc が閉じること
- ダイアログで Esc が閉じ、Tab がダイアログ内に閉じ込められること
- リサイザーに Tab で到達でき、← → で幅が動くこと

- [ ] **Step 10: ドキュメントを更新する**

`CLAUDE.md` の「## GUI」節の 1 行目を差し替える:

```markdown
navigation rail による 3 モード構成（変換 / 情報 / フレーム）。
デザイントークンとプリミティブは `gui-frontend/src/styles/` と
`gui-frontend/src/lib/ui/`。**他のファイルに生の色を書かない**
（余白・角丸・タイポもトークン経由。部品固有の内部寸法だけは例外）。

| モード | 左 | 中央 | 右 |
|---|---|---|---|
| 変換 | フォルダーツリー | 写真グリッド | 変換設定 |
| 情報 | フォルダーツリー | 写真グリッド | メタデータ |
| フレーム | プリセット一覧 | フレームのプレビュー | フレーム設定 |

カラム幅と右パネルの折りたたみは `localStorage` に永続化する
（`gui-frontend/src/lib/shell/columns.ts`）。テーマは OS 追従のみ。

フロントの検証は 2 系統:
- `bun test` — runes を含まない純粋ロジック（幅のクランプ・グリッド寸法・LRU・キュー）
- `bun run e2e` — Playwright。`vite dev` に当てる。見た目の検証は
  `e2e/stub.ts` が `window.__TAURI_INTERNALS__` を差し替える
```

`docs/README.md` を更新する:
- 「設計確定・実装前」から GUI デザイン刷新の行を「現行仕様」へ移す
- 「直近の実装計画」に本計画へのリンクを足す
- メタデータ編集の行の「**実装は GUI デザイン刷新の後**」を「**次に実装するのはこれ**」に変える

- [ ] **Step 11: 実施メモを書く**

本計画の末尾「実施メモ」節に、各タスクで判断したことを書く。
**「なぜその形なのか」は plans の実施メモにある**（`docs/README.md` の読む順番）ので、
ここが空だと次の人が spec だけから復元することになる。最低限:

- Task 9 Step 11 の `localStorage` 実機確認の結果（永続したか、落としたか）
- Task 14 Step 3 のフィルムストリップ 3,000 要素の判断（仮想化を入れたか）
- Task 15 の測定値と LRU 上限の算出過程
- Task 18 Step 8 の実機 `rAF` と Chromium の差
- 途中で spec と食い違った点があれば、その内容と採った側

- [ ] **Step 12: コミット**

```bash
git add -A
git commit -m "chore(gui): 旧 app.css 変数を削除し明暗・キーボード・モーションを総点検"
```

---

## 実施メモ

（実装中にここへ追記する。空のままにしないこと。）

### 作業ブランチ

`feature/gui-redesign` を切り、**本計画ファイルのコミットもそのブランチ上で行った**。
main には一切コミットしない。全 18 タスクを 7 チャットに分割して進めるため、
各チャットは「本ファイル ＋ git ログ ＋ 本節」だけを引き継ぐ。したがって
判断は Task 18 まで溜めず、そのタスクのコミット前に本節へ書く。

### Task 1 — 生成値とコントラストの実測

生成は 1 回で通った。`contrastLevel: 0` のまま 44 テスト全 PASS で、
生成スクリプト側を調整する必要は無かった（Step 8 の「落ちたら生成側で解く」は不発動）。

**spec §1-2 に書いた最小値は、計画 Step 11 のテンプレートの想定と違う。**
テンプレートは最小ペアを `on-surface-variant` / `surface-container-highest` と
仮置きしていたが、実測ではライト側の 4.5:1 群の最小は `on-primary` / `primary` の
`6.44:1` だった（ダーク側は想定どおり `on-surface-variant` /
`surface-container-highest` の `7.18:1`）。また全ペアを通した絶対最小は
`outline` / `surface`（ライト `4.25:1`）だが、これは非テキストの 3:1 基準の
ペアなので「4.5 を割っている」ようには読めない。混同を避けるため、spec には
**基準ごとに分けた表**として書いた（1 つの最小値だけを書くと、次に読む人が
「4.25 は AA 未満では」と誤読する）。

`hexFromArgb` の出力は小文字 16 進で、`contrast.test.ts` の抽出正規表現
（`#[0-9a-f]{6}`）と一致する。大文字化されると 21 ロールの前提条件テストが
先に落ちるので、ここは検知できる。

### Task 2 — 見た目の確認は Playwright MCP で行った

計画 Step 4 は「ブラウザで開いて 3 カラムが出ていればよい」だが、目視だけだと
「トークンが解決されている」ことまでは分からない（旧 app.css が全部の色を
持っているので、tokens.css が丸ごと読まれていなくても画面は同じに見える）。
`vite dev` に Playwright MCP を当て、`getComputedStyle(document.documentElement)`
で `--md-sys-color-primary` = `#494bd6`、`--space-4` = `16px`、
`--md-sys-typescale-body-md`、`--md-sys-shape-corner-md` = `12px` が
実際に解決されることを確認した。3 カラムの骨格は現行どおり。

### Task 3 — 状態レイヤーの実測とフォーカスリングの丸め

計画 Step 10 の「hover・focus・pressed を実際に触って確認」を Playwright MCP で
数値として取った。`::after` の `opacity` が idle `0` → hover `0.08` →
pressed `0.10`、キーボード Tab の `:focus-visible` で `0.10` ＋ outline。
トークンの値と一致する。

**フォーカスリングの計算値は `3px` ではなく `2.4px` になる。** これは実装の誤りでは
なく、Chrome が outline 幅をデバイスピクセル境界へ丸めるため
（dpr 1.25 で 3 CSS px = 3.75 デバイス px → 3 デバイス px → 2.4 CSS px）。
`--md-sys-state-focus-ring` 自体は `3px solid #494bd6` で正しい。
**将来この値を検査するテストを書くなら、`3px` 固定で assert しないこと。**

`bunfig.toml` の効果を実測で確認した。`e2e/gallery.spec.ts` を足したあとも
`bun test` は `Ran 44 tests across 1 file` のままで、e2e を拾っていない。

### Task 4 — スライダーの疑似要素は computed style で検査できない

計画 Step 10 の目視を Playwright MCP で数値として取ったが、
**`getComputedStyle(input, "::-webkit-slider-runnable-track")` と
`"::-webkit-slider-thumb"` は Chromium で `rgba(0, 0, 0, 0)` を返す。**
これは `Slider.svelte` の CSS が効いていないのではなく、Chromium が
この 2 つの疑似要素を `getComputedStyle` に公開していないため
（スクリーンショットではトラックもつまみも正しく塗られている）。
**将来この 2 つを数値で assert するテストを書かないこと。** 偽陰性になる。
スライダーの見た目は `e2e/__screenshots__/gallery-{light,dark}.png` で見る。

他の 4 部品は数値で取れた。実測値:

- `Switch` on: トラック `rgb(73, 75, 214)`（primary）、つまみ `left: 26px` / 24×24px。
  off: `left: 6px` / 16×16px。`danger` on はトラック `rgb(186, 26, 26)`（error）
- `Switch` の当たり判定は計画の主張どおり `.track`
  （`elementFromPoint` が `track state-layer …` を返す）。hover で `::after` が `0.08`
- `SegmentedButton` は `ArrowRight` で `crop` → `pad` に移り、`bind:group` 経由で
  `.selected` クラスも追従する。フォーカスリングは `.text` に出る（`3px solid`）
- `TextField` の `error` は枠と補足文の両方が `rgb(186, 26, 26)` になる

### Task 5 — 危険なダイアログの初期フォーカスと Rating のロケータ

`initialFocus="footer button"` が意図どおり働くことを実測した。破壊的な
ダイアログを開いた直後の `document.activeElement` は「キャンセル」で、
`role` は `alertdialog`、`aria-labelledby` はタイトルを指す。
**この安全側への着地は `actions` snippet の並び順（1 つ目がキャンセル）に
依存している。** 順序を入れ替えると `footer button` は「削除して変換」を
掴むので、`Dialog` を使う側はこの並びを守ること。

**`Rating` と `Slider` はどちらも `role="slider"` になる。** ギャラリーでは
`Slider` が「品質」「無効」、`Rating` が既定ラベルの「レーティング」なので
アクセシブル名で分離できているが、`getByRole("slider")` を名前なしで
使うと両者を掴む。e2e は名前つきで引くこと。
また `Rating` を 3 個並べてあり全部が既定ラベルなので、`.first()` が
`bind:value` されている 1 個目を指すという並び順への依存もある。

`LinearProgress` の indeterminate は `tokens.css` の
`prefers-reduced-motion` 規則（`*` に `animation-duration: 0.01ms !important`）に
そのまま当たるので、個別の打ち消しは書いていない。

### Task 6 — scrim の塗りと閉じる操作を分けた（Dialog を直した）

**計画どおりに書いたら、いちばん下地を隠したい進捗ダイアログだけが素通しになった。**
Task 5 の `Dialog` は scrim を `{#if dismissible}` で囲んでいて、塗りと
「余白クリックで閉じる」を同じ 1 要素が持っていた。`ProgressOverlay` は
`dismissible={false}` なので、変換中に背後のアプリが暗くならない
（現行の `ProgressOverlay` は `rgba(0,0,0,0.7)` で暗くしていたので退行）。

`Dialog.svelte` を直した。背景 div は常に描き、`onclick` だけを
`dismissible ? onClose : undefined` にする。下地を暗くするのは
「今はこのダイアログしか操作できない」という表示であって、
閉じられるかどうかとは別の話である。

### Task 6 — Card は外側の余白を持たないので積むと角丸が噛み合う

`ResultDialog` の課題セクションを `Card` を縦に並べる形にしたところ、
Card 同士が隙間なく接して 1 枚の面に見えた（`Card` は `padding` だけを持ち、
`margin` を持たない。これは部品として正しい）。`.sections` という
`display: flex; gap: var(--space-3)` の箱で囲んで解決した。
**`Card` を 2 枚以上積むところは、親が間隔を持つこと。**

### Task 6 — App.svelte の旧変数も落とす必要がある

計画 Step 1 は `App.svelte` について「`ConfirmDialog` の呼び出しの差し替え」と
`.dialog-detail` の追加しか書いていないが、Step 7 の grep の期待は
`App.svelte` が「消えている」側に入っている。`App.svelte` 自身の `<style>` が
`--border-color` と `--bg-secondary` を持っているので、ここも
`outline-variant` / `surface-container-low` に置き換えた。

### Task 6 — `state-layer` は `.tree-item` ではなく `.tree-row` に付けた

計画 Step 6 は `.tree-item` に付けろと書いているが、`.state-layer::after` は
`border-radius: inherit` である。選択中の pill（`corner-full`）は行
（`.tree-row`）に付くので、`.tree-item` に状態レイヤーを置くと
**pill の上に角の立った矩形が重なる**。行そのものに付ければ形が一致し、
`.tree-row:hover .fav-toggle` の既存規則もそのまま生きる。

`.fav-toggle.active` の `--warning` は `primary` に振り替えた。spec §1-1 は
warning ロールを定義しない（`Toast` の warning と同じ帰結）。

**フォルダーツリーの行そのものは vite dev では目視できない。** ドライブ一覧の
取得が `invoke` の reject で失敗して木が空になるため。ここで確認できたのは
セクション見出しと面色だけで、hover / 選択中の pill は実機（Task 9 以降）で見る。

### Task 7 — 移設だけで、ガードは 1 箇所に寄せた

`runProcess` にあった `if (!canProcess) return` は落とした。同じ条件
（実行中 / 0 枚 / 出力先未選択）を `convert.run` が入口で見ているためで、
ガードを 2 箇所に置くと片方だけ直る事故が起きる。`canProcess` は
ボタンの `disabled` を出すためだけに残っている。

`App.svelte` は 405 → 276 行。計画の見込み（260 行前後）どおりで、
150 行への到達は Task 9 のシェル化を待つ。

**Tauri 実機での手動確認（Step 7）は、このチャットでは実行できていない。**
GUI ウィンドウを操作する手段が無いため、代わりに `vite dev` 上で起動し、
3 つのモジュールが実際に走ること（進捗購読・`presets.reload()`・
フォルダーツリーの初期化がいずれも `invoke` まで到達し、Tauri 外なので
reject される）をコンソールとトーストで確認した。**移設の写し間違いなら
モジュールの読み込み時点で落ちる**ので、この確認で「配線が繋がっている」
ことは言える。「変換の一連の操作が壊れていない」は言えない。
Task 10 でパネルを組み直すときに、e2e スタブ経由で通しの検査を足すこと。

### Task 8 — クランプの落とし先を spec より細かく分けた

spec §3-1 は「範囲外と数値でない値は既定値へ落とす」と書いているが、実装は
**範囲外は min / max へ、数値でない値と NaN / Infinity だけを既定値へ**落とす。
範囲外まで既定値へ戻すと、利用者が最小まで縮めた幅が次回起動で 240 に戻り、
「保存されていない」ように見える。壊れた値と利用者の意思を区別するのが目的で、
どちらも「1 カラムが画面を占有しない」という spec の要件は満たす。
Task 9 Step 9 の e2e（`folder: -99999` → `180`）はこの分け方を前提にしている。

### Task 9 — `localStorage` の実機確認

**結論: WebKitGTK でも永続する。`write()` を no-op に落とす必要は無かった。**
spec §8 の懸念（Linux で毎回既定に戻る体験になり得る）は、この環境では発生しない。

実測の方法（GUI を手で操作できないため、計画の手順そのままではない）:

- `src/main.ts` に一時的なプローブを足し、起動時に
  `picture-tool.layout.widths.v1` を読んで結果を `save_favorites` で報告させた。
  **`favorites.json` はファイルとして観測できる**ので、画面を見ずに読み書きの
  往復が確かめられる。プローブはコミットしていない
- アプリを起動 → 終了 → 再起動し、1 回目 `null`、2 回目 `{"folder":333}` を確認
- **debug ビルドと release ビルドの両方で確認した。** origin が違う
  （debug は `build.devUrl` の `http://localhost:5173`、release は埋め込み資産の
  カスタムプロトコル）ため、片方だけでは利用者が動かす構成の答えにならない。
  どちらも保った

**`cargo build` した debug バイナリは `devUrl` を読む。**
単体で起動しても画面が出ず、`invoke` も一切走らない（無言で失敗する）。
実機確認をするときは先に `vite dev` を 5173 で上げておくこと。ここで 1 度、
「プローブが動いていない」と「webview がページを読んでいない」を取り違えた。

### Task 9 Step 12 — ウィンドウ寸法

既定 1440×800 は収まる。実測は release ビルドで `window.innerWidth === 1440`、
`document.documentElement.scrollWidth <= innerWidth`（横スクロール無し）。
開発機の論理解像度は HDMI-1 が 3840×2160（倍率 1.0）、内蔵パネルが
3840×2400 の倍率 1.25 で 3072×1920。spec §3-1 の実測（3072×1728）と同じ機である。

### Task 9 — 分割ハンドルの a11y 警告は抑止した

`role="separator"` ＋ `tabindex="0"` に対して svelte が
`a11y_no_noninteractive_tabindex` と `a11y_no_noninteractive_element_interactions`
を出す。**幅を変えられる separator は ARIA の window splitter でウィジェット扱いが
正しい**ので、これは誤検出。`tabindex` はポインタを持たない利用者の唯一の経路
なので外せない。理由を添えた `<!-- svelte-ignore ... -->` で抑止した。
このリポジトリの `svelte-check` は 0 警告で保たれており、実害のある警告が
埋もれないようにするのが抑止の目的（複数コードは**カンマ区切り**でないと
2 つ目が効かない）。


### Task 10 — TextField の空欄を親へ渡せるようにした（normalize の型を広げた）

計画 Step 1 のテンプレートは `max_size_mb` と `max_width` を `TextField` の
`normalize` で丸めるが、**入力欄を空にしたときの落とし先が無かった。**
`TextField.handleNumberChange` は空欄を `null` にし、`normalize` は
`next !== null` のときしか呼ばれない。結果:

- `config.max_size_mb` に `null` が入る。Rust 側は必須の数値なので変換が落ちる
- `config.max_width` に `null` が入る。トグルは on のままなのに「無制限」になる

`onchange` で親が戻す形にしても直らない。`TextField` の DOM 書き戻し
（`el.value = …`）は `normalize` の直後に終わっており、親が同じイベント内で
値を戻しても **state が動かないので再描画されず、表示だけ空のまま残る**
（`TextField` 自身が Task 4 で潰したのと同じずれが、1 手ずれて再発する）。

`normalize` の型を `(value: number | null) => number | null` に広げ、
**空欄も `normalize` を通す**ようにした。空欄の意味を決めるのは項目ごとに違う
（最大サイズは「直前の値」、出力幅は「直前の値」、フレームの任意項目なら
「未設定」もあり得る）ので、決めるのは呼び出し側で正しい。
`Gallery.svelte` の 1 箇所も追随させた。

この 2 つは e2e で**先に落ちることを確認してから**直している
（`if (normalize)` を元の `if (next !== null && normalize)` へ戻すと
「出力幅を空欄に…」「最大サイズは 1〜1024MB…」の 2 件だけが落ちる）。

### Task 10 — SegmentedButton も `getByRole(...).click()` では押せない

計画 Step 5 の `page.getByRole("radio", { name: "Pad" }).click()` は通らない。
`SegmentedButton` の `input` は `opacity: 0; pointer-events: none` で隠してあり、
当たり判定は上に載る `.text` が取るため、Playwright が
`<span class="text">Pad</span> intercepts pointer events` で落ちる。
`Switch` と同じ形なので、`e2e/stub.ts` に `toggleSwitch` と並べて
`selectSegment(page, label)` を足した。**可視ラベルをクリックする**のが両者の答え。

### Task 10 — 実機の代わりに「通し」を e2e で検査した

Step 7 の実機確認（変換の一連の操作）は、Task 7 / 9 と同じ理由で実行できない
（GUI ウィンドウを操作する手段が無い）。Task 7 の実施メモが
「Task 10 で e2e スタブ経由の通しの検査を足すこと」と書き残していたので、それを行った。

そのために **`e2e/stub.ts` の `process_images` を、依頼された分だけ成功を返し
引数を `window.__lastProcessArgs` に残す形に変えた**。空配列を返す元の形だと
結果ダイアログが常に「0 成功 / N 未処理」になり、
**何を送ったのかがテストから見えない**（パネルの写し間違いが素通りする）。
検査しているのは「選択 → 出力先 → 変換 → 結果ダイアログ」の通過と、
バックエンドへ渡る `config` が画面の表示どおりであること
（`1002` と入力して画面に `1000` が出ているなら、渡る値も `1000`）。

**実機で見ていないので言えないこと**: 実際の画像が変換されること、
crop / pad / quality の出力そのもの。これは core の責務で本刷新では触っていない。

### Task 11 — 計画どおり。`.pagination` の style だけ指示が二重だった

Step 2 は「`<style>` から `.grid-header` / `.pagination` / `.toolbar-right` の規則を消す」
と書いた直後に「`.pagination` の規則は `ThumbnailGrid` の `<style>` に残す」と書いている。
**後者を採った。** ページ送りは `controls` snippet の中身で、snippet は
`ThumbnailGrid` 側で宣言されているので**その scope hash が付く**。
`GridHeader` 側へ移すと当たらなくなる。消したのは `.grid-header` と
`.toolbar-right` の 2 つだけ。

`primaryAction` の snippet は `App.svelte` のテンプレート末尾（`</AppShell>` の後）に
置いてあるが、トップレベルの snippet はテンプレート全体から参照できるので、
`{#snippet center()}` の中で `ThumbnailGrid` に渡せる。

### Task 12 — 純粋ロジックは計画のまま通った。足したのは 2 件の検査

`thumbnailCache.ts` / `requestQueue.ts` は計画のコードをそのまま実装して
全 17 テストが通った。テストは 2 件だけ足している:

- **「1 件では足りなければ上限を下回るまで追い出す」**（`LruBudget`）。計画のテストは
  1 件しか追い出さない場面しか見ておらず、**`admit` の中のループが 1 周で
  break する実装でも全部 green になる**。上限はスクロール中に何度も跨ぐので、
  超過が積み上がる実装を通してはならない
- **「捨てたキーは再度 push できる」**（`RequestQueue`）。`setVisibleRange` の破棄で
  `#keys` から消し忘れると、スクロールで戻ってきた写真が二重登録扱いになって
  **永久に取得されない**。`take()` 側の同じ性質は計画にテストがあるが、
  破棄側には無かった

`CACHE_BYTE_LIMIT = 64MB` は**まだ暫定値**。Task 15 の実測で確定させること。

### Task 12 — 実機の代わりに e2e を足し、そのためにスタブを 2 箇所直した

Step 10 の実機確認（サムネイルが出る・列を変えると取り直す）も
GUI ウィンドウを操作できないため `e2e/thumbnails.spec.ts` で代替した。
純粋ロジック側が見ているのは規則そのものなので、ここで見るのは
**繋いだ結果として壊れていないこと**に絞ってある。

そのために `e2e/stub.ts` を 2 箇所直した:

- **`get_thumbnail` が要求を `window.__thumbnailRequests` に記録する。**
  サムネイルは「出ている」だけでは検査にならない。どの解像度で何回取りに来たかを
  見ないと、列を変えたときの取り直しが起きているか判らない
- **`list_images` がフォルダーごとに別のパスを返す**（従来は引数を無視して
  常に `/photos/N.jpg`）。同じパスを返すと**フォルダーを変えてもキャッシュが当たり**、
  「2 つ目以降のフォルダーが埋まる」の検査が `resetForFolder` を壊しても green になる

「取得した分が LRU の台帳に載る」は `budget.admit` の呼び出しを消すと落ちることを
確認済み。**ここが載らないとバイト上限が一生発火せず、画面上は正常に見える**ので、
この 1 件だけは他のどのテストからも検知できない。

### Task 14 — フィルムストリップの要素数

### Task 15 — スクロール測定と LRU 上限

### Task 18 — 実機 `rAF` と Chromium の差

### spec と食い違った点

**計画の時点で分かっている分**（実装で増えたら追記する。理由は各タスクにある）:

- **`panels/frameDraft.svelte.ts` を足した**（Task 16）。spec §2 のファイル構成表には
  `presets` / `convertRun` / `metadataDraft` の 3 つしか無いが、フレーム編集の下書きは
  左・中央・右の 3 カラムにまたがるため、`App.svelte` に置くと §3-5 が崩れる
- **`ResultDialog` / `ProgressOverlay` / `Toast` を `lib/` 直下に残した**（File Structure）。
  `ui/` は「11 個で打ち止め」の汎用プリミティブの置き場であり、
  アプリ固有のこの 3 つを混ぜると境界が壊れる
- **rail のアイコンを塗り／outline で切り替えない**（Task 9 Step 2）。
  spec §3-3 は塗り／outline の対を要求するが、Web フォントを読み込まない制約下で
  それを満たすアイコンが無い。選択状態は pill と色、および `aria-current` で示す
- **カラム幅のクランプで落とし先を分けた**（Task 8）。spec §3-1 は「範囲外と数値でない値は
  既定値へ落とす」だが、範囲外は min / max へ寄せる。利用者が縮めた幅が既定へ戻ると
  「保存されていない」ように見えるため
- **`Toast` の warning と success が同じ帯色**（Task 6 Step 5）。spec §1-1 が
  warning / success のロールを定義していないことの帰結で、区別はアイコンが持つ
- **`TextField` の `normalize` が空欄（`null`）も受ける**（Task 10）。spec §2 は
  `normalize?: (v: number) => number` と書いているが、これだと「空欄にされたとき
  何に落とすか」を親が決められず、`max_size_mb` に `null` が入る

---

## 完了の定義

spec §6 の 9 段階すべてと、以下が満たされていること。

- [ ] `make check` が通る（`cargo fmt` / `clippy` / `cargo test` / `svelte-check` / `bun test`）
- [ ] `cd gui-frontend && bun run e2e` が全 PASS
- [ ] `gui-frontend/src/` に旧 `app.css` 変数の参照が 0 件
- [ ] `gui-frontend/src/styles/` 以外に生の色（`#xxxxxx` / `rgba()`）が 0 件
- [ ] `core/`・`cli/`・`gui/src/` の差分が 0 件
- [ ] `gui-frontend/src/lib/api.ts` と `types.ts` の差分が 0 件
- [ ] `gui/tauri.conf.json` の差分がウィンドウ寸法 2 値（`width` / `minWidth`）だけ
- [ ] `gui/capabilities/default.json` の差分が 0 件
- [ ] spec §1-2 に生成した色値が追記されている
- [ ] spec §7-2 に測定値が、§8 に LRU 上限が追記されている
- [ ] 本計画の「実施メモ」が埋まっている
- [ ] `CLAUDE.md` と `docs/README.md` が更新されている

次工程は
[メタデータ編集](../specs/2026-08-18-metadata-editing-design.md)。
本刷新で `Rating` と `TextField(multiline)` と `metadataDraft` を作ってあるので、
**新しい部品を継ぎ足さずに実装できる**はずである。そうならなかった場合、
それは本刷新の設計の穴なので、その旨を上の「spec と食い違った点」に書き残すこと。
