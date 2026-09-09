# ドキュメント索引

`docs/superpowers/` 配下は時系列にフラットで並んでいるため、**どれが現行仕様か**が
ファイル名からは分からない。ここが唯一の入口。

- **specs/** — 設計仕様（何をどう作るか）
- **plans/** — 実装計画と実施メモ（どう進めたか、何を判断したか）

## 現行仕様

| ドキュメント | 内容 |
|---|---|
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
