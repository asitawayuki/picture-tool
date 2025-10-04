# Picture Tool - 画像バッチ処理ツール

画像ファイルを一括で4:5のアスペクト比に変換し、指定サイズ以下に圧縮するRust製のコマンドラインツールです。

## 特徴

- **4:5アスペクト比変換**: Instagram等のSNSに最適なアスペクト比に自動変換
- **自動サイズ圧縮**: 8MB（デフォルト）を超える画像を自動的に圧縮
- **高速並列処理**: rayonによるマルチスレッド処理で大量の画像を高速処理
- **柔軟な変換モード**: クロップまたはパディングを選択可能
- **対応フォーマット**: JPEG, PNG, WebP

## インストール

### ビルド要件
- Rust 1.70以降

### ビルド方法

```bash
cargo build --release
```

実行ファイルは `target/release/picture-tool` に生成されます。

## 使い方

### 基本的な使い方

```bash
# 指定フォルダー内の全画像を処理（クロップモード）
./target/release/picture-tool --input ./photos

# 短縮形
./target/release/picture-tool -i ./photos
```

### 変換モード

#### クロップモード（デフォルト）
元の画像を中央から4:5にクロップします。

```bash
./target/release/picture-tool -i ./photos -m crop
```

#### パディングモード
元の画像を保持し、余白を追加して4:5にします。

```bash
# 背景色: 白（デフォルト）
./target/release/picture-tool -i ./photos -m pad

# 背景色: 黒
./target/release/picture-tool -i ./photos -m pad -b black
```

### 品質・サイズ設定

```bash
# 初期JPEG品質を95%に設定
./target/release/picture-tool -i ./photos -q 95

# 最大ファイルサイズを10MBに設定
./target/release/picture-tool -i ./photos --max-size 10

# 組み合わせ
./target/release/picture-tool -i ./photos -q 95 --max-size 10 -m pad -b white
```

## オプション一覧

| オプション | 短縮 | 説明 | デフォルト |
|----------|------|------|-----------|
| `--input` | `-i` | 入力フォルダーパス（必須） | - |
| `--mode` | `-m` | 変換モード (`crop` または `pad`) | `crop` |
| `--bg-color` | `-b` | パディング時の背景色 (`white` または `black`) | `white` |
| `--quality` | `-q` | 初期JPEG品質 (1-100) | `90` |
| `--max-size` | - | 最大ファイルサイズ (MB) | `8` |

## 出力形式

- 処理済み画像は元のフォルダーに保存されます
- ファイル名: `元のファイル名_processed.jpg`
- フォーマット: JPEG

### 出力例

```
Processing images in: ./photos
Found 150 images

[1/150] cat.jpg → cat_processed.jpg (3.2 MB) ✓
[2/150] dog.png → dog_processed.jpg (7.8 MB) ✓
[3/150] landscape.jpg → landscape_processed.jpg (7.9 MB, quality: 85%) ✓
...

Completed: 148 successful, 2 failed
Total time: 12.3s
```

## 動作仕様

### アスペクト比変換

**クロップモード:**
- 横長画像: 幅を削って4:5に
- 縦長画像: 高さを削って4:5に
- 常に中央を基準に切り取り

**パディングモード:**
- 横長画像: 上下に余白を追加
- 縦長画像: 左右に余白を追加
- 元の画像は完全に保持

### サイズ圧縮

1. 初期品質で保存を試行
2. サイズが制限を超える場合、品質を5%ずつ下げて再試行
3. 最低品質60%まで下げても制限を超える場合、その状態で保存

## エラーハンドリング

- 読み込みに失敗した画像はスキップして処理を継続
- エラーメッセージには失敗した画像のファイル名と理由を表示
- 最終的な成功/失敗件数をサマリーで表示

## 注意事項

- 元の画像ファイルは上書きされません
- 処理済みファイルが既に存在する場合は上書きされます
- サブフォルダー内の画像も再帰的に処理されます
