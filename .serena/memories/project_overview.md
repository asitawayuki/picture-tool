# Project Overview: picture-tool-rust

## Purpose
画像ファイルを一括で4:5アスペクト比に変換し、ファイルサイズを制限するRust製CLIツール。

## Tech Stack
- **Language**: Rust (edition 2021)
- **image** (0.24) - 画像の読み込み・操作・JPEGエンコード
- **rayon** (1.10) - 並列処理
- **walkdir** (2.5) - ディレクトリ走査
- **clap** (4.5, derive) - コマンドライン引数パース
- **anyhow** (1.0) - エラーハンドリング

## Codebase Structure
- 単一ファイル構成: `src/main.rs`
- Structs: `Args` (CLI引数), `ProcessResult` (処理結果)
- Enums: `ConversionMode` (crop/pad/quality), `BackgroundColor` (white/black)
- Functions: `main`, `collect_image_files`, `is_supported_image`, `process_image`, `convert_aspect_ratio_crop`, `convert_aspect_ratio_pad`, `generate_output_path`, `save_with_size_limit`, `save_jpeg`

## Processing Flow
1. 入力フォルダー内の画像ファイル（jpg/jpeg/png/webp）を再帰的に収集
2. rayonで並列処理
3. モードに応じて4:5アスペクト比に変換（qualityモードはスキップ）
4. JPEG保存 → 8MB超の場合は品質を5%ずつ下げて再保存（最低60%）
5. 出力ファイル名: `{元のファイル名}_processed.jpg`
