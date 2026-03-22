# Suggested Commands

## Build & Run
```bash
# ビルド
cargo build
cargo build --release

# 実行
cargo run -- --input ./photos --output ./out --mode crop --quality 90 --max-size 8

# CLIオプション
#   -i, --input <PATH>      入力フォルダーパス (必須)
#   -o, --output <PATH>     出力フォルダー (デフォルト: ./)
#   -m, --mode <MODE>       変換モード: crop, pad, quality (デフォルト: crop)
#   -b, --bg-color <COLOR>  パディング背景色: white, black (デフォルト: white)
#   -q, --quality <N>       初期JPEG品質 1-100 (デフォルト: 90)
#       --max-size <N>      最大ファイルサイズ MB (デフォルト: 8)
```

## Testing & Quality
```bash
cargo test
cargo clippy
cargo fmt -- --check
```

## Utility
```bash
git status
git log --oneline -10
```
