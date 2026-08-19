# GUI デザイン刷新 設計

**状態**: 設計確定・実装前
**位置づけ**: [メタデータ編集 spec](./2026-08-18-metadata-editing-design.md) の「実装順序」の 2 番目。
本刷新の完了後にメタデータ編集を実装する

## 背景と目的

現行 GUI（Svelte 5 / 3 カラム固定 / 計 3,642 行）に対して、以下 4 つの不満が挙がっている:

1. 見た目・質感が安っぽい
2. レイアウトが窮屈／固定的
3. 操作の流れが分かりにくい
4. 一貫性が無い／場当たり的

同時に、メタデータ編集（タイトル・コメント・レーティング）が新機能として控えており、
**星評価・複数行テキスト・未保存状態といったコンポーネントが現行 GUI に一切存在しない**。

先にデザイン刷新を行い、メタデータ編集が必要とする部品を含んだデザインシステムを
一度だけ作る。刷新後に部品を継ぎ足す事故と、メタデータ編集 UI を 2 度作る無駄を避ける。

### 目指す質感

Material 3 系。**丸みと控えめな影を使い、無駄のない構成**。
iOS 的なガラス（`backdrop-filter` によるすりガラス）は**採用しない** — 見た目の要望であると
同時に、描画コストが写真グリッドのスクロールに直接効くため。

## スコープ

### やること

| 項目 | 内容 |
|---|---|
| デザイントークン層 | M3 のロール名に揃えた CSS custom properties。ライト／ダーク 2 セット |
| コンポーネントプリミティブ | 11 個を新規作成（後述） |
| アプリシェル | navigation rail による 3 モード構成 |
| 写真グリッド | 仮想スクロール化、サイズ可変、全画面プレビュー刷新 |
| 変換パネル | プリミティブで再構築。選択リストを廃止 |
| フレームパネル | 585 行のモーダルを解体し、独立したモードへ |
| メタデータパネル | **レイアウトだけ用意する**（データ接続は次工程。5-2 参照） |

### やらないこと

- **`core/` と `cli/` は一切触らない**
- **`gui/src/` の Rust コードも触らない**。終了ガード用の `set_unsaved_state` コマンドは
  インターフェースだけ本 spec で決め、実装はメタデータ編集の工程で行う
- `gui-frontend/src/lib/api.ts` と `types.ts` を変更しない（Tauri 境界は現状維持）
- プリセット JSON のスキーマ変更（フォントはプリセットに残す。後述）
- 国際化（日本語のまま）

## 採用した手段と、退けた手段

### デザインシステムの実装手段: M3 トークンを自前定義する

`@material/web`（Material Web Components）は**採用しない**。理由:

- 公式 roadmap 上、プロジェクトは **maintenance mode**。新機能開発は停止している
- **navigation rail が未実装**。「作る予定だったが着手前に停止した」コンポーネント群に含まれる。
  本設計の骨格に据える部品がライブラリ側に存在しない
- Lit ランタイムを丸ごと抱えることになり、「軽さ優先」の要件に反する
- Web Components と Svelte 5 の `bind:` 連携に回避コードが要る

一方、M3 の**トークン体系そのもの**（`--md-sys-color-*` / `--md-ref-typeface-*` 等の
CSS custom properties）は仕様として確立しており、コンポーネントを導入せずに
トークン層だけ採用できる。これを採る。

「トークンだけ整えて部品は各コンポーネント内 CSS のまま」という案も退けた。
不満 4（一貫性が無い）に応えられず、同じボタンが 7 ファイルに散ったまま残るため。

## 1. デザイントークン層

`gui-frontend/src/styles/tokens.css` に集約する。
**他のファイルは生の色・生の px を書かない。** これが「場当たり的」の再発防止線である。

### 1-1. 色ロール

命名は `--md-sys-color-*` に揃える（Figma の Material Theme Builder の出力をそのまま
貼れるという実利がある）。**使うロールを以下に限定し、これ以外は定義しない**:

| 分類 | ロール |
|---|---|
| 主アクション | `primary` / `on-primary` / `primary-container` / `on-primary-container` |
| 面（5 段階） | `surface` / `surface-container-lowest` / `-low` / `-high` / `-highest` |
| 面の文字 | `on-surface` / `on-surface-variant` |
| 線 | `outline` / `outline-variant` |
| 状態色 | `error` / `on-error` / `error-container` / `on-error-container` |
| 補助 | `inverse-surface` / `inverse-on-surface`（トースト）/ `scrim`（モーダル背面） |

`secondary` / `tertiary` は**定義しない**。現行の画面に「2 番目に強いアクション」は存在せず、
用意すると使い道を探し始めて一貫性が崩れる。必要になった時点で追加する。

### 1-2. 配色方針: 面は無彩色、アクセントだけ色を持つ

M3 の標準スキーム（tonal spot）は surface にも source color の色相が薄く乗る。
**写真ツールでこれをやると背景の色被りが写真の色判断を狂わせる**
（Lightroom / Capture One の UI が無彩色なのはこの理由）。

> **neutral palette の chroma を最小（≈0）、primary palette は chroma を保つ**
> カスタム `DynamicScheme` を使う。面は無彩色グレー、色を持つのはボタン・
> フォーカスリング・選択インジケータのみ。

source color は現行アクセントの `#6366F1` を引き継ぐ（見慣れた印象の連続性）。

**具体的な 16 進値は本 spec に書かない。** 実装フェーズの段階 1 で
`material-color-utilities` から生成し、生成結果を本 spec に追記して確定させる。
手で書いた推測値は tonal palette の階調を壊すため。

### 1-3. 形状

```
--md-sys-shape-corner-xs:    4px   /* チップ、バッジ */
--md-sys-shape-corner-sm:    8px   /* テキストフィールド、サムネイル */
--md-sys-shape-corner-md:   12px   /* カード、パネル */
--md-sys-shape-corner-lg:   16px   /* ダイアログ */
--md-sys-shape-corner-full: 999px  /* ボタン、セグメント、選択インジケータ */
```

### 1-4. 余白とタイポグラフィ

余白は 4px グリッドの 6 段（`--space-1: 4px` 〜 `--space-6: 32px`）。
現行はここが無く、間隔がバラついている。

タイポは M3 の 15 段は使わず、**実際に使う 5 段だけ**定義:
`title-md` / `title-sm` / `body-md` / `body-sm` / `label-lg`。

フォントは `system-ui` を先頭に、日本語は OS 同梱フォントへフォールバック
（Windows: Yu Gothic UI、Linux: Noto Sans JP）。**Web フォントは読み込まない**（起動の軽さ）。

### 1-5. 状態レイヤーと elevation

「安っぽさ」の実体はここにある。仕様値で固定する:

```
--md-sys-state-hover-opacity:   0.08
--md-sys-state-focus-opacity:   0.10
--md-sys-state-pressed-opacity: 0.10
```

適用は **`::after` による全面オーバーレイの 1 パターンに統一**する。
`background` を直接書き換える実装は禁止（hover と選択状態が混ざって破綻するため）。

Elevation は **level 0 / 1 / 2 / 3 の 4 段のみ**。ダークテーマでは影がほぼ見えないため、
M3 どおり **surface-container の明度差を主、`box-shadow` を従**として併用する。
`backdrop-filter` は使わない。

### 1-6. モーション

```
--md-sys-motion-duration-short:  150ms
--md-sys-motion-duration-medium: 250ms
```

easing は standard `cubic-bezier(0.2, 0, 0, 1)` と emphasized-decelerate の 2 種のみ。
**すべてのトランジションを `prefers-reduced-motion: reduce` で無効化する。**

### 1-7. ライト／ダークの切替

- `:root` にダーク、`:root[data-theme="light"]` にライトを定義
- `data-theme` 未指定時は `@media (prefers-color-scheme: light)` で OS 追従
- 初期値は OS 追従。手動オーバーライドは**レール最下部のアイコンボタン**
- 選択は既存の store プラグイン経由（Rust 側のコマンド）で永続化

## 2. コンポーネントプリミティブ

`gui-frontend/src/lib/ui/` に配置。すべて Svelte 5 runes、`$props()` と snippet のみ。
**どれも状態を持たない**（`bind:value` で親が持つ）。

**11 個で打ち止めとする。** これ以外は各パネルのローカル実装。

| 部品 | 主な props | 使う場所 |
|---|---|---|
| `Button` | `variant: filled\|tonal\|outlined\|text`, `danger`, `disabled`, `icon?` | 変換実行、保存、フォルダー選択、ダイアログ |
| `IconButton` | `variant: standard\|filled`, `toggle?`, `pressed?`, `label`（必須, a11y） | 閉じる、削除、テーマ切替、プレビュー送り |
| `TextField` | `value`, `type: text\|number`, `multiline?`, `suffix?`, `error?`, `min/max` | 最大サイズ、出力幅、タイトル、コメント |
| `Switch` | `checked`, `label` | 元ファイル削除、出力幅の有効化、Exif フレーム |
| `Slider` | `value`, `min/max/step`, `suffix?` | 品質、サムネイルサイズ、フレーム文字サイズ |
| `SegmentedButton` | `value`, `options: {value,label,icon?}[]` | crop/pad/quality、背景色、帯の位置 |
| `Select` | `value`, `options` | ロゴ選択、フォント選択 |
| `Rating` | `value: 0-5`, `onChange`, `readonly?` | メタデータ（★再クリックで解除） |
| `Card` | `level: 0-3`, `padding?` | パネル内のグループ、結果ダイアログの行 |
| `Dialog` | `title`, `danger?`, snippet で本文／アクション | 削除確認、結果、未保存警告 |
| `LinearProgress` | `value?`（未指定で indeterminate） | 変換進捗、保存中 |

**`Checkbox` は作らない** — 真偽値は `Switch` に統一する。
サムネイルの選択チェックは見た目が専用（写真の上に乗る円形マーク）なので
`PhotoGrid` のローカル実装とする。

`NavigationRail` はプリミティブではなく `lib/shell/` に置く。アプリ内に 1 つしか存在せず、
汎用化する意味がないため。

### 既存資産の扱い

- `focusTrap.ts`（68 行）→ `Dialog` の内部に取り込んで流用
- `toasts.svelte.ts`（48 行）→ ロジックは無変更。`Toast.svelte` の見た目だけ
  `inverse-surface` ロールで書き直し
- `api.ts` / `types.ts` → 変更なし

### ファイル構成

```
gui-frontend/src/
├── styles/tokens.css
├── lib/
│   ├── ui/                    # 上記 11 部品
│   ├── shell/                 # AppShell.svelte, NavigationRail.svelte
│   ├── panels/                # ConvertPanel, MetadataPanel, FramePanel
│   │                          #   + presets.svelte.ts, convertRun.svelte.ts,
│   │                          #     metadataDraft.svelte.ts
│   ├── browser/               # FolderTree, PhotoGrid, PhotoViewer,
│   │                          #   thumbnailQueue.svelte.ts
│   ├── api.ts, types.ts       # 変更なし
│   └── toasts.svelte.ts       # 変更なし
└── App.svelte                 # 状態の保持とパネルの差し替えのみ（150 行程度）
```

## 3. アプリシェル

### 3-1. レイアウト

rail 幅は 80px 固定（アイコン + 日本語ラベル、選択インジケータは pill 形）。
**それ以外の縦カラムはすべてドラッグで幅を変えられ、幅は永続化する** —
不満 2（固定的で窮屈）への直接の答え。

```
変換モード:     [rail 80] [フォルダー 240*] [写真グリッド flex] [変換設定   320*]
メタデータ:     [rail 80] [フォルダー 240*] [写真グリッド flex] [メタデータ 360*]
フレーム:       [rail 80] [プリセット 220*] [プレビュー   flex] [フレーム設定 360*]
                                                                  * = 可変・既定値
```

右パネルの既定幅は現行 240px → **320〜360px**。現行が窮屈な最大の原因はここ。

**フレームモードだけ左 2 カラムが変わる。**
メタデータ編集 spec は「左 2 カラムは全モード共通」と書いているが、フレーム編集を
rail の destination に昇格させた時点でこの前提は成り立たない。プリセット編集に
フォルダーツリーは不要で、必要なのは「見本の写真 1 枚」だけである。
見本写真は**変換／メタデータモードで最後にフォーカスした 1 枚**を引き継ぐ
（選び直しは設定パネル内のボタンから）。

### 3-2. モード間で共有する状態

`App.svelte` が保持し、rail の切替で**破棄しない**:

| 状態 | 共有範囲 |
|---|---|
| 現在フォルダー / 写真一覧 / サムネイルキャッシュ | 変換・メタデータで共有（フレームでも見本写真の解決に使用） |
| `selectedPaths: SvelteSet<string>`（変換対象・複数） | 変換モード専用。モードを離れても保持 |
| `focusedPath: string \| null`（編集対象・単一） | メタデータ／フレームで共有 |

**選択の概念を 2 つに分ける。** 変換は「複数チェック」、メタデータは「単一フォーカス」。
同じ `PhotoGrid` に `selectionMode: "multi" | "single"` を渡して切り替える。
両者が同時に見えることはないので UI 上の混乱は起きない。

（メタデータ編集の操作単位を「1 枚ずつ」と確定させたことによる帰結。
複数選択して一括適用する案は、値が異なる写真を選んだときの「混在」表示が複雑になる割に、
タイトルとコメントは写真ごとに違う文が入るのが自然という理由で退けた。）

### 3-3. rail の挙動

- destination: **変換 / 情報（メタデータ）/ フレーム**（アイコン + 日本語ラベル）
- 選択中は pill 型インジケータ ＋ 塗りアイコン、非選択は outline アイコン
- 最下部にテーマ切替の `IconButton`（3 モードのどれにも属さないため bottom section）
- 切替アニメーションは **150ms のフェードのみ**。スライドは重く、
  写真グリッドの再レイアウトを誘発するため使わない

### 3-4. 未保存ガード

`lib/panels/metadataDraft.svelte.ts` が `isDirty` を持ち、離脱経路をすべてここに通す。

| 経路 | 扱い |
|---|---|
| グリッドで別の写真をフォーカス | webview 内の `Dialog`（破棄して移動 / 保存して移動） |
| rail で別モードへ | 同上 |
| フォルダーを変える | 同上 |
| ウィンドウを閉じる | **Rust 側で処理**（下記） |

ウィンドウ終了だけは webview に任せない。

`core:default` には `window:allow-destroy` が含まれず、フロントで
`onCloseRequested` を使うには webview に追加権限が必要になる。これは
「webview にプラグイン権限を与えない」という既存の前提（`gui/src/security.rs`）に反する。

> フロントは dirty 状態が変わるたびに `set_unsaved_state(bool)` コマンドで Rust に通知する。
> Rust の `on_window_event` が `CloseRequested` を捕まえ、未保存があれば
> **OS ネイティブのダイアログ**を出し、キャンセルされたら閉じるのを止める。

これは「不可逆な操作は OS ネイティブのダイアログで確認する」という既存方針
（元ファイル削除と同じ扱い）と一致し、`capabilities` を `core:default` のまま保てる。

**この Rust 側の変更は本刷新では実装しない**（メタデータ編集の工程で行う）。
インターフェースだけをここで確定させる。

### 3-5. `App.svelte` の縮小

現行 405 行 → 150 行程度。切り出し先:

- `lib/browser/thumbnailQueue.svelte.ts` — 並列 3 本のキュー、失敗記録、キャッシュ
- `lib/panels/presets.svelte.ts` — プリセット一覧の保持と再読込
- `lib/panels/convertRun.svelte.ts` — 変換実行・進捗・キャンセル・結果

`App.svelte` に残るのは「モード」「フォルダー」「選択」「フォーカス」の 4 状態と、
パネルの差し替えのみ。

## 4. 写真グリッド

### 4-1. 密度と拡大の導線

- `grid-template-columns: repeat(auto-fill, minmax(N, 1fr))` とし、**スライダーが N を動かす**。
  列数固定ではないので、ウィンドウ幅に応じて列が増減する
- **N の既定値は 200px**（大きめ）。現行はサムネイルが小さいという不満があった
- クリック = 選択（multi）／フォーカス（single）
- **ダブルクリック or Enter = 全画面プレビュー**（現行の挙動を踏襲）
- 全画面プレビューに**フィルムストリップを追加**する。現行は ← → で送れるが
  「いま何枚目か」が見えない

### 4-2. 仮想スクロール

**ライブラリは入れず、自前で実装する。**

全タイルが同じ高さ（4:5 固定 ＋ 一定 gap）なので行高が計算で出る。
`scrollTop` から可視行を求め、前後 2 行分をパディングする 60 行程度の実装で足りる。
依存を足す理由が無く、「軽さ優先」の要件にも合う。

現行のページネーション（50 枚固定）は廃止する。

### 4-3. メタデータモードでのグリッド表示

- 写真の左下に **★**（設定済みのレーティング）
- 右上に **未保存マーク**（赤い点）
- フォーカス中の 1 枚は太いアウトライン

★の描画に必要なメタデータは、サムネイル取得と同時に読む。

## 5. 各モードのパネル

### 5-1. 変換パネル

**`SelectionList.svelte`（168 行）を廃止する。**
選択状態はサムネイル上の ✓ で分かるため、右カラムに一覧を持つ必要がない。
空いた幅を設定に回す。

設定は意味のまとまりごとに `Card` で区切る:

| Card | 内容 |
|---|---|
| 変換モード | `SegmentedButton`（crop / pad / quality）、pad のとき背景色の `SegmentedButton` |
| 出力 | 品質 `Slider`、最大サイズ `TextField`、出力幅制限 `Switch` + `TextField` |
| Exif フレーム | 有効化 `Switch`、プリセット `Select`（編集はフレームモードへの導線） |
| 出力先 | パス表示 + フォルダー選択 `Button` |
| 元ファイル削除 | `Switch`（danger 色） |

主ボタンは **「N 枚を変換」**（枚数を持つ）。パネル最下部に固定。

### 5-2. メタデータパネル（レイアウトのみ。データ接続は次工程）

本刷新で作るのは**静的なレイアウトまで**。`read_image_metadata` /
`write_image_metadata` は次工程で追加される Tauri コマンドなので、この時点では
**撮影情報（`getExifInfo`、既存）だけが実データで、タイトル・コメント・★は
編集できるが保存先が無い状態**にする。保存ボタンは disabled、未保存ガードも
配線しない。rail の destination としては最初から存在させる。

上から: フォーカス中の写真のサムネイル + ファイル名 + 未保存表示 →
タイトル `TextField` → コメント `TextField(multiline)` + 食い違い警告 →
`Rating` → 撮影情報（読み取り専用）。

最下部に **「保存して次の写真へ」（filled）** と **「保存」（outlined）** の 2 ボタン。
連続して付けていく作業が主なので、次へ送りを主ボタンに置く。

**撮影情報（カメラ・レンズ・焦点距離・F 値・SS・ISO・撮影日時）を読み取り専用で常設する。**
現行は全画面プレビューでしか見えないが、メタデータを書く場面では手元にある方が自然。
`getExifInfo` は既にあるので追加の Tauri コマンドは不要。

### 5-3. フレームパネル

現行 `ExifFrameSettings.svelte`（585 行、モーダル）を解体する。

| 変更 | 内容 |
|---|---|
| プレビューが主役になる | 中央の最も広い場所を占め、実際の余白込みで見える |
| プリセットが左に一覧で並ぶ | 切り替えるたびにプレビューが更新され、見比べられる（現行はドロップダウン） |
| 「プリセット名」入力欄を廃止 | 一覧の項目をダブルクリックで改名 |
| 表示項目 10 個をチップに | チェックボックス 10 個 → チップ 10 個。入／切の集合はチップの方が状態を読みやすい |
| フォント選択 | **プリセットに残す**（下記） |

**フォント選択はプリセットに残す。**
「アプリ設定に移す」案（フォントは環境の設定であってプリセットごとに変えるものではない、
という理屈）も検討したが、「明朝のプリセット／ゴシックのプリセット」を作り分けたい
という要求があるため退けた。**プリセット JSON のスキーマは変更しない。**

プレビューの警告（写真が小さすぎてフレームを描けない等）は、現行どおり
**GUI では利用者に出さない**。プレビューは長辺 400px 固定で、実出力ではフレームが出る
写真でも `skip_exif` に落ちるため。判断の場所は `gui/src/commands.rs`（変更なし）。

## 6. 実装の順序

各段階の終わりで**必ずビルドが通り、アプリが動く**こと。

| # | やること | 完了の目印 |
|---|---|---|
| 1 | `material-color-utilities` で色値を生成 → `tokens.css` 確定（**生成値を本 spec に追記**） | ライト／ダーク 2 セットの全ロールでコントラスト比 AA 以上 |
| 2 | `ui/` プリミティブ 11 個 ＋ 確認用ページ `/dev/gallery` | ギャラリーで全部品 × 全 state × 明暗を目視できる |
| 3 | `shell/`（AppShell・NavigationRail）と `App.svelte` の分解 | 3 モードが切り替わる（中身は現行コンポーネントのまま） |
| 4 | 変換パネル再構築、`SelectionList` 廃止 | 変換が最後まで通る |
| 5 | グリッド刷新（仮想スクロール・サイズスライダー・全画面プレビュー＋フィルムストリップ） | 3,000 枚のフォルダーでスクロールが滑らか |
| 6 | フレームモード（モーダル解体・プリセット一覧） | プリセットの作成／編集／削除が通る |
| 7 | メタデータパネルのレイアウト（5-2、データ接続なし） | rail の 3 モードすべてが実体を持つ |
| 8 | 明暗の総点検、`prefers-reduced-motion`、キーボード操作 | 下記の検証が全部通る |

段階 8 の完了が「デザイン刷新の完了」。その後にメタデータ編集の plan へ移る。
段階 2 で `Rating` と `TextField(multiline)` を作っておくため、メタデータ側は
新しい部品を継ぎ足さずに実装できる。

## 7. 検証

現状の `gui-frontend` には**テスト基盤が無い**（`svelte-check` のみ）。刷新で埋める。

- **`svelte-check`** — 各段階で通す
- **Playwright を devDependency に追加**し、`vite dev`（Tauri 外）に対して
  スクリーンショットと操作を回す。Tauri 外では `invoke` が即 reject するため、
  **エラー経路が一度に全部出る**という利点がある
- **コントラスト比は計算で検証する**。生成した色値の全ペアについて比を出すスクリプトを
  1 本置き、AA を下回ったら落とす
- キーボード操作: Tab 順、Enter でプレビュー、Esc で閉じる、← → で送り
- `prefers-reduced-motion: reduce` で全トランジションが止まること

テストコードを書く段階で **`test-integrity` スキルを起動する**。

## 8. 未確定・実装時に潰すこと

| 項目 | 内容 |
|---|---|
| 色の具体値 | 段階 1 で生成し、本 spec に追記して確定させる |
| rail 幅 80px | M3 の標準値を採ったが、日本語ラベル（「メタデータ」等）が収まるかは実装時に確認。収まらなければラベルを短くする（「情報」等）か幅を広げる |
| カラム幅の永続化先 | 既存の store プラグイン（Rust 側コマンド経由）に相乗りする想定。キーの設計は実装時 |
| ★のメタデータ読み取りコスト | グリッド表示で全写真のメタデータを読むことになる。3,000 枚で許容範囲かは段階 5 で実測する。重い場合は可視範囲のみ遅延読み込みに切り替える |
| `set_unsaved_state` の実装 | 本刷新ではインターフェースのみ。実装はメタデータ編集の工程 |

## 参照

- [メタデータ編集 設計](./2026-08-18-metadata-editing-design.md) — 本刷新の後に実装する機能
- `gui/src/security.rs` モジュールコメント — 信頼境界の設計と脅威モデル
- `docs/superpowers/plans/2026-08-04-full-codebase-review-fixes.md` S6 節 — capabilities を
  `core:default` に留める判断の経緯
- [Material Web roadmap](https://github.com/material-components/material-web/blob/main/docs/roadmap.md) —
  maintenance mode、navigation rail 未実装の根拠
- [Material Web theming/color](https://github.com/material-components/material-web/blob/main/docs/theming/color.md) —
  トークン名と `material-color-utilities` による生成
