# ドキュメント索引

`docs/superpowers/` 配下は時系列にフラットで並んでいるため、**どれが現行仕様か**が
ファイル名からは分からない。ここが唯一の入口。

- **specs/** — 設計仕様（何をどう作るか）
- **plans/** — 実装計画と実施メモ（どう進めたか、何を判断したか）

## 現行仕様

| ドキュメント | 内容 |
|---|---|
| [specs/2026-10-28-memory-budget-design.md](superpowers/specs/2026-10-28-memory-budget-design.md) | **メモリ予算**。画像を開く処理はすべて、開く前に 1 枚のピーク見積もりをプロセス共通の予算（物理メモリの 1/2）から借りる。予算を超える 1 枚は単独で、上限（3/4）を超える 1 枚は失敗に。到着順の待ち・一括変換の専用プール・JPEG XL の作業領域の上限を見積もりから導くこと。各形式のピーク RSS の実測値を持つ |
| [specs/2026-10-20-output-quality-design.md](superpowers/specs/2026-10-20-output-quality-design.md) | **書き出し品質の改善**。JPEG エンコーダを `jpeg-encoder` に替え、品質の目盛りを LrC に似せる（品質 20 = LrC 20% のテーブル）。入力 ICC の sRGB 変換・出力への sRGB ICC と Exif（位置情報・個体識別タグを除く）の埋め込み・透過の合成と背景色の全モード表示・Exif フレームのロゴと文字の合成修正・サイズ制限の二分探索。LrC の量子化テーブルの実測値を持つ |
| [specs/2026-10-14-avif-decoder-replacement-design.md](superpowers/specs/2026-10-14-avif-decoder-replacement-design.md) | **AVIF デコーダの差し替え**。壊れた AVIF 1 枚でプロセスが abort する問題を、rav1d（avif-decode）から C 版 libdav1d（`dav1d` クレート）への差し替えで解く。捨てた案（image の avif-native・子プロセス隔離）の実測、OS ごとの libdav1d の入手方法を持つ |
| [specs/2026-09-29-metadata-write-verification-design.md](superpowers/specs/2026-09-29-metadata-write-verification-design.md) | **メタデータ書き込みの検証強化**。保存時に編集対象 5 タグ以外の Exif・サムネイル・他セグメントが消えた／変わったら置き換えずに失敗させる。`little_exif` の書き込みが何を変えるかの実測値を持つ |
| [specs/2026-09-08-grid-filtering-design.md](superpowers/specs/2026-09-08-grid-filtering-design.md) | **写真グリッドの絞り込み**（現行）。ファイル名検索・並び替え（Phase 1）と★レーティング絞り込み（Phase 2）。**一覧に何を出すかを決める規則**と、フォルダー全件の★を得る軽量読み取り経路。走査コストの実測値と、その計測の罠を持つ |
| [specs/2026-08-19-gui-redesign-design.md](superpowers/specs/2026-08-19-gui-redesign-design.md) | **GUI デザイン刷新**（現行）。Material 3 系のトークン層 ＋ プリミティブ 11 個、navigation rail による 3 モード構成、写真グリッドの仮想スクロール化。3 カラム構成の出典はこちらへ移った |
| [specs/2026-08-18-metadata-editing-design.md](superpowers/specs/2026-08-18-metadata-editing-design.md) | **メタデータ編集**（タイトル・コメント・レーティング）。元写真の JPEG に Windows / MWG 系の両方から読める形で書き込む。**レイアウトの出典は GUI デザイン刷新 spec**（本 spec は書き込むタグの仕様・信頼境界・読み取りの優先順位を持つ） |
| [specs/2026-08-12-output-width-limit-design.md](superpowers/specs/2026-08-12-output-width-limit-design.md) | **出力幅の上限指定**（`--max-width`）。pad/crop の 4:5 キャンバスを指定 px 以下に縮小 |
| [specs/2026-03-29-exif-frame-v2-design.md](superpowers/specs/2026-03-29-exif-frame-v2-design.md) | **Exifフレーム v2**（現行）。padモード限定・2段表示・ロゴ配置 |
| [specs/2026-03-23-gui-improvements-v2-design.md](superpowers/specs/2026-03-23-gui-improvements-v2-design.md) | GUI 改善 v2 |
| [specs/2026-03-22-picture-tool-gui-design.md](superpowers/specs/2026-03-22-picture-tool-gui-design.md) | GUI の初期設計。**レイアウトは GUI デザイン刷新が後継**。フォルダーツリー・お気に入り・変換の流れの出典としてのみ有効 |

## 直近の実装計画

| ドキュメント | 内容 |
|---|---|
| [plans/2026-10-28-memory-budget.md](superpowers/plans/2026-10-28-memory-budget.md) | **メモリ予算**の実装計画（全 9 タスク）と実施メモ。spec を実装者の目で読んで決めたこと（D-1〜D-10）・係数を決めた計測（JPEG XL の最小 `max_alloc`、全形式 × モードのピーク RSS、`resize` の中間バッファ）・見積もりと実測の比較（29 件）・予算 3.75GiB での 4 ケース（直列・単独処理・上限超え）・ガードを外したときに落ちたテスト・専用プールのコスト |
| [plans/2026-10-20-output-quality.md](superpowers/plans/2026-10-20-output-quality.md) | **書き出し品質の改善**の実装計画（全 10 タスク）と実施メモ。spec を実装者の目で読んで決めたこと（エンコーダの関数の形・sRGB の判定・ロゴを箱に収める読み込みなど）・目盛りの初期形・計画時の試作（Pillow で目盛りの候補 11 通りを実写真 4 枚で測った品質 90 のサイズと PSNR。単純な倍率では性質 5 を大きく外れること）・目盛りの調整結果（式と定数・性質 5 の許容幅）・サイズ制限の下限（品質 28）・各検証の実測値（ICC の許容差・Exif の残ったタグ・ロゴ／文字を直す前に戻したときの失敗・LrC 20% との DQT とサイズの比較・ICC と Exif を載せた後の品質 90 の測り直し）を持つ |
| [plans/2026-10-19-avif-decoder-replacement.md](superpowers/plans/2026-10-19-avif-decoder-replacement.md) | **AVIF デコーダの差し替え**の実装計画（全 8 タスク）と実施メモ。spec を実装者の目で読んで決めたこと（A-1〜A-10）・ガードを外したときの実測（フレームサイズ上限・周回の上限・アルファの 512MB 判定・移植の罠 4 件）・差し替え前後の画素照合（SIMD 20/20 一致、スカラー 13/20 差）・ファズ（13 枚 × 130 件 = 1690 件で abort・signal・timeout 0 件）。**Windows での確認（Task 1 と完了条件）は未実施** |
| [plans/2026-09-30-metadata-write-verification.md](superpowers/plans/2026-09-30-metadata-write-verification.md) | **メタデータ書き込みの検証強化**の実装計画（全 8 タスク）と実施メモ。spec を実装者の目で読んで決めたこと（A-1〜A-7）・比較規則での 3 枚の再実測・検査を 1 つずつ外したときに落ちたテスト（M1〜M8、生き残りなし）・実写真（Lightroom 書き出し）への保存の確認 |
| [plans/2026-09-09-grid-rating-filter.md](superpowers/plans/2026-09-09-grid-rating-filter.md) | **★レーティング絞り込み**（Phase 2）の実装計画（全 8 タスク）と実施メモ。軽量読み取り経路のフォールバック率・`list_images` の並列化の実機確認・★の `<select>` の実幅とヘッダーの段の閾値・コンテナクエリが WebKitGTK で効いたか |
| [plans/2026-08-26-metadata-editing.md](superpowers/plans/2026-08-26-metadata-editing.md) | **メタデータ編集**の実装計画（全 18 タスク）と実施メモ。round-trip 実測・既存 XMP 保持・終了時未保存ガードの実機確認・spec と食い違った点 |
| [plans/2026-08-19-gui-redesign.md](superpowers/plans/2026-08-19-gui-redesign.md) | **GUI デザイン刷新**の実装計画（全 18 タスク）と実施メモ。トークン層・プリミティブ・仮想スクロール・LRU 上限の実測と、spec と食い違った点 |
| [plans/2026-08-12-output-width-limit.md](superpowers/plans/2026-08-12-output-width-limit.md) | **出力幅の上限指定**（`--max-width`）の実装計画と実施メモ |
| [plans/2026-08-04-full-codebase-review-fixes.md](superpowers/plans/2026-08-04-full-codebase-review-fixes.md) | **全体レビュー修正（S1〜S7）**。セッションごとの判断・受け入れた残リスク・検証結果 |
| [plans/2026-03-29-exif-frame-v2.md](superpowers/plans/2026-03-29-exif-frame-v2.md) | Exifフレーム v2 の実装計画 |
| [plans/2026-03-24-full-codebase-review-fixes.md](superpowers/plans/2026-03-24-full-codebase-review-fixes.md) | 前回の全体レビュー修正 |

## 過去の仕様（現行ではない）

| ドキュメント | 状態 |
|---|---|
| [specs/2026-03-25-exif-frame-design.md](superpowers/specs/2026-03-25-exif-frame-design.md) | **Superseded by v2**。レイアウト選択・型番の表示名変換など、v2 で廃止された概念を含む |
| [specs/2026-03-22-gui-improvements-design.md](superpowers/specs/2026-03-22-gui-improvements-design.md) | v2 が後継 |

## 読む順番

1. リポジトリ直下の `README.md` — 使い方・CLI仕様・動作仕様
2. リポジトリ直下の `CLAUDE.md` — 守る不変条件と、権威ある記述の在り処
3. `gui/src/security.rs` のモジュールコメント — 信頼境界の設計と脅威モデル
4. 触る機能の spec → 対応する plan の実施メモ（**なぜその形なのか**は plan 側にある）
