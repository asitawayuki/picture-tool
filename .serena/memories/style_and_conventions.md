# Code Style & Conventions

## General
- 単一ファイル構成 (`src/main.rs`) — シンプルなCLIツールのため
- clap derive マクロでCLI引数を定義
- anyhow::Result でエラーハンドリング
- rayon の par_iter で並列処理

## Naming
- Rust標準の snake_case (関数・変数)
- PascalCase (構造体・列挙型)
- 列挙型バリアントもPascalCase

## Patterns
- 画像処理失敗時はスキップして次へ（パニックしない）
- 元ファイルは上書きしない（出力先を分離）
- 重複ファイル名は連番で回避
