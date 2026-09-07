# picture-tool

Instagram投稿用の画像一括変換ツール。Cargo Workspace（`core` / `cli` / `gui`）＋ Svelte 5 フロントエンド。

**構成・使い方・CLI仕様は [`README.md`](./README.md) にある。ここには書き写さない**
（同じ表を2箇所に置くと片方だけ古くなる）。このファイルが持つのは
**コードを読んでも分からない不変条件**と、権威ある記述の在り処だけ。

## 検証

```bash
make check   # CI と同等（lint / cargo test / typecheck / bun test）。変更したら必ず通す
```

`make check` に入らないもの:

| 目的 | コマンド |
|---|---|
| GUI の見た目・操作（Playwright。`vite dev` に当てる） | `cd gui-frontend && bun run e2e` |
| 上記でポート 5174 が塞がっている環境 | `E2E_PORT=5199 bun run e2e` |
| Rust ⇄ TS の DTO 契約の再生成 | `UPDATE_DTO_CONTRACT=1 cargo test -p picture-tool-gui contract` |

## 守る不変条件

**破ってもコンパイルとテストが通ってしまう**ものだけを挙げる。理由と詳細はリンク先にある。

- **webview から来たパス・フォント・プリセット名は `gui/src/security.rs` の検証関数を通す。**
  Tauri v2 の capabilities/ACL は自作コマンドに効かないので、自作コマンド自身が唯一の信頼境界。
  **コマンドを追加するときは必ずここを経由させること** → `gui/src/security.rs` のモジュールコメント
- **不可逆な操作（元ファイル削除、メタデータ書き込み先の承認）は OS ネイティブのダイアログで確認する。**
  webview 内の確認ダイアログは、乗っ取られた状態では素通りする
- **webview にプラグイン権限を与えない**（`gui/capabilities/default.json` は `core:default` のみ）。
  フォルダー選択もお気に入りの保存も、パスを Rust 側に固定したコマンドで行う → 同ファイルの `description`
- **設定ディレクトリのパスを組み立てるのは `core::exif_frame::AssetDirs` の1箇所だけ。** 直書きしない
  → `core/src/exif_frame/mod.rs` の `user_config_dir`
- **生の色を書いてよいのは `gui-frontend/src/styles/` の2ファイルだけ。**
  余白・角丸・タイポもトークン経由（部品固有の内部寸法だけは例外）→ `styles/tokens.css` 冒頭
- **`gui-frontend/src/lib/dto.contract.ts` は自動生成物。手で編集しない。**
  ヘッダも生成元（`gui/src/types.rs` の `mod contract`）側にある → 同ファイル冒頭
- **`data-theme` は常に明示的に書く。** メディアクエリ任せにしない
  （Linux では WebKitGTK の `prefers-color-scheme` が OS の設定を映さないため）→ `gui/src/theme.rs` 冒頭
- **`core` は `eprintln!` しない。** 伝えるべき事象は `warnings` に積み、出すか捨てるかは
  境界（CLI は stderr、GUI は `gui/src/commands.rs`）が決める → `core/src/lib.rs` 冒頭
- **Svelte 5 の runes 構文（`$state` / `$derived` / `$effect`）を使う。** 旧構文は使わない

## どこに何が書いてあるか

| 知りたいこと | 読む場所 |
|---|---|
| 使い方・CLIオプション・終了コード・ロゴやフォントの差し替え | [`README.md`](./README.md) |
| ビルド／検証コマンドの実体 | `Makefile`（各ターゲットにコメントあり） |
| `core` の公開 API と呼び出し側が守ること | `core/src/lib.rs` の crate ドキュメント |
| 信頼境界の設計と脅威モデル | `gui/src/security.rs` のモジュールコメント |
| Rust ⇄ TypeScript 境界の仕組みと、検知できない食い違い | `gui-frontend/src/lib/dto.contract.ts` 冒頭 |
| 設計仕様と実装計画（**なぜその形なのか**は plans の実施メモ） | [`docs/README.md`](./docs/README.md) が唯一の入口 |

## リポジトリに含めているもの

- `.claude/agents/`, `.claude/skills/` — **意図的にコミットしている**。このリポジトリで
  作業するときの共通のエージェント定義／スキルであり、チームと将来の自分が同じ手順で
  作業できるようにするため。内容を更新したらコミットすること
  （個人設定は `.claude/settings.local.json` 側に置く）
- `docs/` — 設計仕様と実装計画。入口は [`docs/README.md`](./docs/README.md)
