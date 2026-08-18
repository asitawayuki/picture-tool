# ドキュメント索引

`docs/superpowers/` 配下は時系列にフラットで並んでいるため、**どれが現行仕様か**が
ファイル名からは分からない。ここが唯一の入口。

- **specs/** — 設計仕様（何をどう作るか）
- **plans/** — 実装計画と実施メモ（どう進めたか、何を判断したか）

## 現行仕様

| ドキュメント | 内容 |
|---|---|
| [specs/2026-08-12-output-width-limit-design.md](superpowers/specs/2026-08-12-output-width-limit-design.md) | **出力幅の上限指定**（`--max-width`）。pad/crop の 4:5 キャンバスを指定 px 以下に縮小 |
| [specs/2026-03-29-exif-frame-v2-design.md](superpowers/specs/2026-03-29-exif-frame-v2-design.md) | **Exifフレーム v2**（現行）。padモード限定・2段表示・ロゴ配置 |
| [specs/2026-03-23-gui-improvements-v2-design.md](superpowers/specs/2026-03-23-gui-improvements-v2-design.md) | GUI 改善 v2 |
| [specs/2026-03-22-picture-tool-gui-design.md](superpowers/specs/2026-03-22-picture-tool-gui-design.md) | GUI の初期設計（3カラム構成の出典） |

## 直近の実装計画

| ドキュメント | 内容 |
|---|---|
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

1. リポジトリ直下の `CLAUDE.md` — 構成・CLI仕様・設計方針・GUIバックエンドの前提
2. `gui/src/security.rs` のモジュールコメント — 信頼境界の設計と脅威モデル
3. 触る機能の spec → 対応する plan の実施メモ（**なぜその形なのか**は plan 側にある）
