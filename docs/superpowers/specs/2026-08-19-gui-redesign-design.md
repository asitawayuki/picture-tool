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
- **`gui/src/` の Rust コードも触らない**。新規 Tauri コマンドを 1 つも追加しない
- **`gui-frontend/src/lib/api.ts` と `types.ts` を変更しない**（Tauri 境界は現状維持）
- プリセット JSON のスキーマ変更（フォントはプリセットに残す。5-3 参照）
- 国際化（日本語のまま）

**この制約が成立する根拠**（設計上、意識して外した機能がある）:

| もし入れると | 必要になるもの | 本刷新での扱い |
|---|---|---|
| テーマの手動切替 | 選択を永続化する新規コマンド | **入れない。OS 追従のみ**（1-7） |
| グリッドへの★表示 | `read_image_metadata`（次工程のコマンド）。`ExifInfo` に `rating` は無い | **メタデータ工程へ移す**（4-3） |
| 未保存マークの表示 | 同上 | 同上 |
| 終了時の未保存ガード | `set_unsaved_state` と `on_window_event` | **インターフェースだけ決める**（3-4） |

カラム幅の永続化だけは本刷新に含めるが、**`localStorage` を使う**ため Rust 側の追加は要らない。
パスもファイル名も webview に渡らないので `gui/src/security.rs` の境界に触れない。
`localStorage` が消えた場合は既定幅に戻るだけで、失って困る情報ではない。

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
| 面（6 段階） | `surface` / `surface-container-lowest` / `-low` / `surface-container` / `-high` / `-highest` |
| 面の文字 | `on-surface` / `on-surface-variant` |
| 線 | `outline` / `outline-variant` |
| 状態色 | `error` / `on-error` / `error-container` / `on-error-container` |
| 補助 | `inverse-surface` / `inverse-on-surface`（トースト）/ `scrim`（モーダル背面） |

無印の `surface-container` を含める（1-5 の elevation がこれに依存する。
Material Theme Builder の出力とも一致させる）。

`secondary` / `tertiary` は**定義しない**。現行の画面に「2 番目に強いアクション」は存在せず、
用意すると使い道を探し始めて一貫性が崩れる。必要になった時点で追加する。

### 1-2. 配色方針: 面は無彩色、アクセントだけ色を持つ

M3 の標準スキーム（tonal spot）は surface にも source color の色相が薄く乗る。
**写真ツールでこれをやると背景の色被りが写真の色判断を狂わせる**
（Lightroom / Capture One の UI が無彩色なのはこの理由）。

> **neutral palette の chroma を最小（≈0）、primary palette は chroma を保つ**
> カスタム `DynamicScheme` を使う。面は無彩色グレー、色を持つのはボタン・
> フォーカスリング・選択インジケータのみ。

source color は現行の `--accent-hover`（`#6366F1`）を採る。
現行の `--accent` は同色相で明るい `#818cf8` であり、生成後は primary の明るいトーンとして
再現される。したがって見慣れた印象の連続性は保たれる。

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
M3 どおり **surface-container 系の明度差を主、`box-shadow` を従**として併用する。
`backdrop-filter` は使わない。

### 1-6. モーション

```
--md-sys-motion-duration-short:  150ms
--md-sys-motion-duration-medium: 250ms
```

easing は standard `cubic-bezier(0.2, 0, 0, 1)` と emphasized-decelerate の 2 種のみ。
**すべてのトランジションを `prefers-reduced-motion: reduce` で無効化する。**

### 1-7. ライト／ダークは OS 追従のみ

- `:root` にダーク、`@media (prefers-color-scheme: light)` でライトを定義
- **手動オーバーライドは本刷新に入れない**

手動切替を入れると選択の永続化に新規 Tauri コマンドが必要になり、
「Rust を触らない」というスコープ制約が崩れる。OS のテーマ設定に従うだけで
不満 1（見た目）への回答としては足りる、と判断した。

将来入れる場合に備え、**トークンはライト／ダークの 2 セットを完全に定義しておく**
（`:root[data-theme="light"]` の形で `data-theme` による上書きが効く構造にする）。
必要になったときは属性を立てるコードと永続化コマンドを足すだけで済む。

## 2. コンポーネントプリミティブ

`gui-frontend/src/lib/ui/` に配置。すべて Svelte 5 runes、`$props()` と snippet のみ。
**どれも状態を持たず、値は `bind:value`（または `bind:checked`）で親が持つ。**

**11 個で打ち止めとする。** これ以外は各パネルのローカル実装。

| 部品 | 主な props | 使う場所 |
|---|---|---|
| `Button` | `variant: filled\|tonal\|outlined\|text`, `danger`, `disabled`, `icon?` | 変換実行、保存、フォルダー選択、ダイアログ |
| `IconButton` | `variant: standard\|filled`, `toggle?`, `pressed?`, `label`（必須, a11y） | 閉じる、削除、プレビュー送り |
| `TextField` | `value`, `type: text\|number`, `multiline?`, `suffix?`, `error?`, `min/max` | 最大サイズ、出力幅、タイトル、コメント |
| `Switch` | `checked`, `label` | 元ファイル削除、出力幅の有効化、Exif フレーム |
| `Slider` | `value`, `min/max/step`, `suffix?` | 品質、サムネイルサイズ、フレーム文字サイズ |
| `SegmentedButton` | `value`, `options: {value,label,icon?}[]` | crop/pad/quality、背景色、帯の位置 |
| `Select` | `value`, `options` | ロゴ選択、フォント選択 |
| `Rating` | `value: 0-5`, `readonly?` | メタデータ（★再クリックで解除） |
| `Card` | `level: 0-3`, `padding?` | パネル内のグループ、結果ダイアログの行 |
| `Dialog` | `title`, `danger?`, snippet で本文／アクション | 削除確認、結果、未保存警告 |
| `LinearProgress` | `value?`（未指定で indeterminate） | 変換進捗、保存中 |

`Rating` も他と同じく **`bind:value`** で扱う（★の再クリックで 0 に戻す挙動は
コンポーネント内部のクリック処理で完結し、外向きの API は他部品と揃える）。

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
├── App.svelte                 # 状態の保持とパネルの差し替えのみ（150 行程度）
└── gallery.html + gallery.ts  # 部品確認用の別エントリ（6 段階 2 参照）
```

## 3. アプリシェル

### 3-1. レイアウト

rail 幅は 80px 固定（アイコン + 日本語ラベル、選択インジケータは pill 形）。
**それ以外の縦カラムはすべてドラッグで幅を変えられ、幅は `localStorage` に永続化する** —
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

### 3-2. モード間で共有する状態

`App.svelte` が保持する:

| 状態 | 範囲と寿命 |
|---|---|
| 現在フォルダー / 写真一覧 / サムネイルキャッシュ | 全モードで共有。**rail の切替では破棄しない**（スクロール位置も保つ） |
| `selectedPaths: SvelteSet<string>`（変換対象・複数） | 変換モード専用。**フォルダーを変えたらクリアする**（後述） |
| `focusedPath: string \| null`（最後にクリックした 1 枚） | 全モードで共有。メタデータの編集対象であり、フレームの見本写真でもある |

**選択の概念を 2 つに分ける。** 変換は「複数チェック」、メタデータは「単一フォーカス」。
同じ `PhotoGrid` に `selectionMode: "multi" | "single"` を渡して切り替える。
両者が同時に見えることはないので UI 上の混乱は起きない。

**`focusedPath` はどちらのモードのクリックでも更新する。**
変換モードのクリックは「選択のトグル ＋ フォーカスの移動」を同時に行う。
これにより、フレームモードに切り替えたときの見本写真が常に
「最後に触った写真」になる（現行は `selectedImages[0]` を使っており、
選択順によっては意図しない写真が見本になっていた）。

**フォルダーを変えたら `selectedPaths` をクリアする**（現行は保持する）。
現行は `SelectionList` が「画面外の選択を可視化し、個別に解除する」唯一の窓口として
機能していたため保持が成立していた。5-1 でこれを廃止するので、
選択は常に画面内で完結させる。

（メタデータ編集の操作単位を「1 枚ずつ」と確定させたことによる帰結。
複数選択して一括適用する案は、値が異なる写真を選んだときの「混在」表示が複雑になる割に、
タイトルとコメントは写真ごとに違う文が入るのが自然という理由で退けた。）

### 3-3. rail の挙動

- destination: **変換 / 情報（メタデータ）/ フレーム**（アイコン + 日本語ラベル）
- 選択中は pill 型インジケータ ＋ 塗りアイコン、非選択は outline アイコン
- 切替アニメーションは **150ms のフェードのみ**。スライドは重く、
  写真グリッドの再レイアウトを誘発するため使わない

（テーマ切替ボタンは 1-7 の判断により置かない。rail に bottom section は作らない。）

### 3-4. 未保存ガード

`lib/panels/metadataDraft.svelte.ts` が `isDirty` を持ち、離脱経路をすべてここに通す。

| 経路 | 扱い |
|---|---|
| グリッドで別の写真をフォーカス | webview 内の `Dialog`（破棄して移動 / 保存して移動） |
| rail で別モードへ | 同上 |
| フォルダーを変える | 同上 |
| ウィンドウを閉じる | **Rust 側で処理**（下記） |

ウィンドウ終了だけは webview に任せない。

`core:default` には `window:allow-destroy` が含まれず（`gui/gen/schemas/acl-manifests.json`
で確認済み）、フロントで `onCloseRequested` を使うには webview に追加権限が必要になる。
これは「webview にプラグイン権限を与えない」という既存の前提（`gui/src/security.rs`）に反する。

> フロントは dirty 状態が変わるたびに `set_unsaved_state(bool)` コマンドで Rust に通知する。
> Rust の `on_window_event` が `CloseRequested` を捕まえ、未保存があれば
> **常に `api.prevent_close()` を呼んでから、別スレッドで** OS ネイティブのダイアログを出し、
> 承認された場合にのみ `window.destroy()` を呼ぶ。

**`on_window_event` の中で `blocking_show()` を直接呼んではならない。**
既存の `confirm_delete_originals`（`commands.rs`）は `blocking_show()` を使っているが、
あれは `async` コマンドの中＝tokio のワーカースレッド上で走るので成立している。
`on_window_event` はメインスレッドで走るため、同じ書き方をするとイベントループを
塞いでダイアログが出ないまま固まる。

実装時の前提として、**この経路は非同期であり、ダイアログ表示中に
再度 `CloseRequested` が来うる**（多重呼び出し）。表示中フラグで抑止すること。

これは「不可逆な操作は OS ネイティブのダイアログで確認する」という既存方針
（元ファイル削除と同じ扱い）と一致し、`capabilities` を `core:default` のまま保てる。

**この Rust 側の変更は本刷新では実装しない**（メタデータ編集の工程で行う）。
インターフェースと上記の制約だけをここで確定させる。

### 3-5. `App.svelte` の縮小

現行 405 行 → 150 行程度。切り出し先:

- `lib/browser/thumbnailQueue.svelte.ts` — サムネイル取得キュー（4-2 で仕様を変更する）
- `lib/panels/presets.svelte.ts` — プリセット一覧の保持と再読込
- `lib/panels/convertRun.svelte.ts` — 変換実行・進捗・キャンセル・結果

`App.svelte` に残るのは「モード」「フォルダー」「選択」「フォーカス」の 4 状態と、
パネルの差し替えのみ。

## 4. 写真グリッド

### 4-1. 密度と操作

- **列数は JS で算出する**: `cols = floor((W + gap) / (N + gap))`、
  出力は `grid-template-columns: repeat(cols, 1fr)`
- **N の既定値は 200px**（大きめ）。スライダーで N を変える。
  現行はサムネイルが小さいという不満があった

`auto-fill minmax(N, 1fr)` を CSS に任せる案は退けた。
仮想スクロールの行高計算には列数が必須だが、`auto-fill` の列数を決めるのは CSS 側であり、
JS で gap 込みの折り返し規則を再現することになる。1px でもずれれば行位置がずれて
スクロールが飛ぶ。列数の決定を JS の単一のソースに寄せる。
既存の「サムネイル要求サイズを 64px 刻みに丸める」ロジック（`SIZE_STEP`）とも噛み合う。

**キーボードとマウスの割り当て**（現行からの**変更**であることを明記する）:

| 操作 | 動作 |
|---|---|
| クリック | multi: 選択トグル ＋ フォーカス移動 / single: フォーカス移動 |
| ダブルクリック | 全画面プレビュー |
| **Space** | クリックと同じ |
| **Enter** | 全画面プレビュー |
| ← → ↑ ↓ | フォーカス移動 |

現行のタイルは `<button>` で `onclick` が選択トグル、プレビューは `ondblclick` のみ。
つまり **現行の Enter は選択トグルであり、プレビューではない**。
`<button>` のまま Enter をプレビューに割り当てると選択のキーボード操作が消えるため、
**タイルを `role="option"`（親は `role="listbox"`、multi では `aria-multiselectable="true"`）に
変更**し、Space と Enter を上表のとおり分ける。

全画面プレビューには**フィルムストリップを追加**する。現行は ← → で送れるが
「いま何枚目か」が見えない。

### 4-2. 仮想スクロールとサムネイル取得

**仮想スクロールのライブラリは入れず、自前で実装する。**
全タイルが同じ高さ（4:5 固定 ＋ 一定 gap）で、列数は 4-1 のとおり JS が持っているため、
行高が計算で出る。`scrollTop` から可視行を求め、前後 2 行分をパディングする
60 行程度の実装で足りる。

現行のページネーション（50 枚固定）は廃止する。
**ただしページネーションは、意図せず 2 つの上限として機能していた。**
廃止するなら、代わりの上限を明示的に入れる必要がある。

| 現行が偶然抑えていたもの | 廃止後に起きること | 対策 |
|---|---|---|
| 取得キューの長さ | `pendingQueue` は FIFO で、スクロールアウトした要求を捨てない。3,000 枚を高速スクロールすると、可視分の要求が過去の要求の後ろで待つ | **LIFO に変更し、可視範囲を外れた未処理要求は破棄する** |
| キャッシュの総量 | `thumbnailCache` に eviction が無い。サイズスライダーで解像度別のキーが増える（200px の base64 が 1 枚あたり十数 KB、3,000 枚 × 複数サイズ） | **LRU で上限を設ける**（上限値は段階 5 で実測して決める） |

この 2 点は `lib/browser/thumbnailQueue.svelte.ts` の仕様として実装する。

### 4-3. メタデータモードでのグリッド表示

写真の★（設定済みレーティング）と未保存マークをタイル上に出す案は、
**本刷新では実装しない**。表示に必要な `read_image_metadata` が次工程で追加される
コマンドであり（`ExifInfo` に `rating` は無い）、「Rust も `api.ts` も触らない」という
本刷新の制約と両立しないため。

**メタデータ編集の工程で実装する**（そのとき、3,000 枚分のメタデータ読み取りコストを
実測し、重ければ可視範囲のみの遅延読み込みに切り替える）。

本刷新のメタデータモードでは、フォーカス中の 1 枚を太いアウトラインで示すところまで。

## 5. 各モードのパネル

### 5-1. 変換パネル

**`SelectionList.svelte`（168 行）を廃止する。**
3-2 でフォルダー変更時に選択をクリアすると決めたため、選択は常に画面内の写真だけになり、
サムネイル上の ✓ で全容が分かる。右カラムに一覧を持つ理由が無くなった。

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
`write_image_metadata` / `grant_metadata_editing` は次工程で追加される Tauri コマンドなので、
この時点では **撮影情報（`getExifInfo`、既存）だけが実データ**で、
タイトル・コメント・★は編集できるが保存先が無い状態にする。
保存ボタンは disabled、未保存ガードも配線しない。
rail の destination としては最初から存在させる。

上から:

1. フォーカス中の写真のサムネイル + ファイル名 + 未保存表示
2. タイトル `TextField`
3. コメント `TextField(multiline)` + 食い違い警告の表示領域
4. `Rating`
5. 撮影情報（読み取り専用）
6. **書き込み承認の状態表示**
7. 「保存して次の写真へ」（filled）と「保存」（outlined）

**6 の承認状態表示の場所を必ず確保する。**
メタデータ編集 spec は、書き込み前に OS ネイティブの確認で `WritableRoots` に
grant する設計（`grant_metadata_editing`）になっている。承認前後の状態と承認ボタンは
必ず UI に要るため、ここで場所を作っておかないと「刷新後に部品を継ぎ足す事故を避ける」
という本 spec の目的に穴が開く。本刷新では disabled のまま置く。

**撮影情報（カメラ・レンズ・焦点距離・F 値・SS・ISO・撮影日時）を読み取り専用で常設する。**
現行は全画面プレビューでしか見えないが、メタデータを書く場面では手元にある方が自然。
`getExifInfo` は既にあるので追加の Tauri コマンドは不要。

最下部の 2 ボタンは、連続して付けていく作業が主なので次へ送りを主ボタンに置く。

### 5-3. フレームパネル

現行 `ExifFrameSettings.svelte`（585 行、モーダル）を解体する。

| 変更 | 内容 |
|---|---|
| プレビューが主役になる | 中央の最も広い場所を占め、実際の余白込みで見える |
| プリセットが左に一覧で並ぶ | 切り替えるたびにプレビューが更新され、見比べられる（現行はドロップダウン） |
| 「プリセット名」入力欄を廃止 | 一覧の項目をダブルクリックで改名 |
| 表示項目 10 個をチップに | チェックボックス 10 個 → チップ 10 個。入／切の集合はチップの方が状態を読みやすい |
| **背景色をパネル内に持つ** | 下記 |
| フォント選択 | **プリセットに残す**（下記） |

**背景色の `SegmentedButton`（white / black）をフレームパネルに置く。**
`renderExifFramePreview(path, config, bgColor)` の `bgColor` は必須引数で、
現行は変換設定の `config.bg_color` を流用している。フレームを独立モードにすると
供給元が消えるため、パネル自身が持つ。
（このパネルの背景色はプレビュー専用で、変換時に使われるのは従来どおり変換設定側の値。）

**見本写真は `focusedPath`（最後にクリックした 1 枚）を使う。**
選び直しはパネル内のボタンから。

**パネル上部に「Exif フレームは pad モードでのみ出力されます」の注記を出す。**
rail の destination として常時見えるようになるため、crop / quality しか使わない利用者が
プリセットを作り込んだのに出力に出ない、という経路が新しく生まれる。

**フォント選択はプリセットに残す。**
「アプリ設定に移す」案（フォントは環境の設定であってプリセットごとに変えるものではない、
という理屈）も検討したが、「明朝のプリセット／ゴシックのプリセット」を作り分けたい
という要求があるため退けた。**プリセット JSON のスキーマは変更しない。**

#### 警告の扱い（現行の挙動を変えない）

2 種類あり、扱いが違う。**まとめて「GUI に出さない」と書くと退行する。**

| 警告 | 扱い |
|---|---|
| フレーム描画由来（`preview.warnings`） | **Rust 側（`commands.rs`）で捨てる。変更なし。** プレビューは長辺 400px 固定で、実出力ではフレームが出る写真でも `skip_exif` に落ちるため偽陽性になる |
| アセット由来（`assets.warnings`、カスタム `model_map` の不備など） | **返ってくるので、従来どおり toast する。** 以前は Rust が `eprintln!` するだけで GUI から見えなかった（S6-M15 の修正）。同じ重複抑止（一度出した警告は再表示しない）も維持する |

## 6. 実装の順序

各段階の終わりで**必ずビルドが通り、アプリが動く**こと。

| # | やること | 完了の目印 |
|---|---|---|
| 1 | `material-color-utilities` で色値を生成 → `tokens.css` 確定（**生成値を本 spec に追記**） | 7-1 のコントラスト検査が通る |
| 2 | `ui/` プリミティブ 11 個 ＋ 部品確認用エントリ | ギャラリーで全部品 × 全 state × 明暗を目視できる |
| 3 | `shell/`（AppShell・NavigationRail）と `App.svelte` の分解 | 3 モードが切り替わる（中身は現行コンポーネントのまま） |
| 4 | 変換パネル再構築、`SelectionList` 廃止、フォルダー変更時の選択クリア | 変換が最後まで通る |
| 5 | グリッド刷新（列数算出・仮想スクロール・キュー仕様変更・プレビュー＋フィルムストリップ） | 7-2 のスクロール検査が通る |
| 6 | フレームモード（モーダル解体・プリセット一覧・背景色・警告の出し分け） | プリセットの作成／編集／削除が通り、`assets.warnings` が toast される |
| 7 | メタデータパネルのレイアウト（5-2、データ接続なし） | rail の 3 モードすべてが実体を持つ |
| 8 | 明暗の総点検、`prefers-reduced-motion`、キーボード操作 | 7-3 が全部通る |

段階 8 の完了が「デザイン刷新の完了」。その後にメタデータ編集の plan へ移る。
段階 2 で `Rating` と `TextField(multiline)` を作っておくため、メタデータ側は
新しい部品を継ぎ足さずに実装できる。

**部品確認用エントリについて**: このプロジェクトは素の Vite + Svelte で、
ルーターが無い（SvelteKit ではない）。`/dev/gallery` のようなパスは作れないため、
**`gallery.html` + `gallery.ts` の別エントリ**を `vite.config` の
`build.rollupOptions.input` に追加する。Tauri の `frontendDist` は `index.html` を
起点にするので、配布物には影響しない。

## 7. 検証

現状の `gui-frontend` には**テスト基盤が無い**（`svelte-check` のみ）。刷新で埋める。

### 7-1. コントラスト検査（段階 1）

生成した色値に対してスクリプトで検査する。**全ペアを検査する方式は成立しない** —
`outline-variant` や `scrim` は意図的に低コントラストで、M3 のトーン設計上
AA を満たさないため、全ペア検査は必ず赤になる。

検査対象を**対になるロールに限定する**:

| ペア | 基準 |
|---|---|
| `on-surface` vs `surface` および全 `surface-container-*` | 4.5:1（AA・本文） |
| `on-surface-variant` vs 同上 | 4.5:1 |
| `on-primary` vs `primary` | 4.5:1 |
| `on-primary-container` vs `primary-container` | 4.5:1 |
| `on-error` vs `error` / `on-error-container` vs `error-container` | 4.5:1 |
| `inverse-on-surface` vs `inverse-surface` | 4.5:1 |
| `outline` vs `surface`（境界線） | 3:1（AA・非テキスト） |

**検査から除外するもの（意図的な低コントラスト）**: `outline-variant`、`scrim`、
`surface-container-*` 同士の隣接。

ライト／ダークの両セットで実行する。

### 7-2. スクロール検査（段階 5）

3,000 枚のフォルダーを Playwright で一定速度スクロールし、以下を記録する:

- **long task（50ms 超）の件数**
- サムネイルキャッシュの実サイズ（保持している base64 の合計バイト数）

**現行のページネーション実装を同条件で測ったものをベースラインとし、
long task 件数がそれを上回らないこと**を合格条件とする。
キャッシュの LRU 上限値は、この測定で得た 1 枚あたりの実バイト数から決めて
本 spec に追記する。

（絶対値の閾値を先に決めない。手元のマシンと画像で測った値でしか意味が無く、
推測で書いた数値は判定に使えないため。）

### 7-3. その他

- **`svelte-check`** — 各段階で通す
- **Playwright を devDependency に追加**する。用途は 2 つあり、**両方必要**:

  | 用途 | やり方 |
  |---|---|
  | エラー経路の検証 | `vite dev` にそのまま当てる。Tauri 外では `invoke` が全部 reject するので、エラー経路が一度に全部出る |
  | 見た目の検証 | **`window.__TAURI_INTERNALS__.invoke` を差し替えるスタブ**を注入する。固定の画像一覧と base64 サムネイル数枚を返す |

  スタブ無しでは、フォルダーツリーも写真グリッドもプレビューもフレームも空のままになる。
  刷新の主目的である見た目の検証対象がちょうど全部撮れない。

- キーボード操作: Tab 順、**Space = 選択 / Enter = プレビュー**（4-1 の表）、
  Esc で閉じる、← → で送り
- `prefers-reduced-motion: reduce` で全トランジションが止まること
- ライト／ダーク両方でのスクリーンショット比較

テストコードを書く段階で **`test-integrity` スキルを起動する**。

## 8. 未確定・実装時に潰すこと

| 項目 | 内容 |
|---|---|
| 色の具体値 | 段階 1 で生成し、本 spec に追記して確定させる |
| サムネイルキャッシュの LRU 上限 | 段階 5 の実測（7-2）で決めて本 spec に追記する |
| rail 幅 80px | M3 の標準値を採ったが、日本語ラベル（「メタデータ」等）が収まるかは実装時に確認。収まらなければラベルを短くする（「情報」等）か幅を広げる |
| `localStorage` の可用性 | Tauri の webview では origin が安定するため永続する想定。消えた場合はカラム幅が既定に戻るだけで、実害は無い |
| `set_unsaved_state` の実装 | 本刷新ではインターフェースのみ。実装はメタデータ編集の工程（3-4 の非同期・多重呼び出しの制約を守ること） |
| ★・未保存マークのグリッド表示 | 本刷新では実装しない。メタデータ編集の工程で、読み取りコストの実測とセットで行う（4-3） |

## 参照

- [メタデータ編集 設計](./2026-08-18-metadata-editing-design.md) — 本刷新の後に実装する機能
- `gui/src/security.rs` モジュールコメント — 信頼境界の設計と脅威モデル
- `docs/superpowers/plans/2026-08-04-full-codebase-review-fixes.md` S6 節 — capabilities を
  `core:default` に留める判断の経緯（S6-H8 / S6-M15 / S6-M16）
- [Material Web roadmap](https://github.com/material-components/material-web/blob/main/docs/roadmap.md) —
  maintenance mode、navigation rail 未実装の根拠
- [Material Web theming/color](https://github.com/material-components/material-web/blob/main/docs/theming/color.md) —
  トークン名と `material-color-utilities` による生成
