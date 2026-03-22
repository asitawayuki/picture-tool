.PHONY: build build-cli build-gui build-frontend dev test test-core clean install release

# デフォルト: 全ビルド
build: build-cli build-gui

# CLIバイナリのビルド
build-cli:
	cargo build -p picture-tool

# GUIアプリのビルド（フロントエンド埋め込み）
build-gui:
	cd gui && bunx @tauri-apps/cli build

# フロントエンドのビルド
build-frontend:
	cd gui-frontend && bun run build

# GUI開発サーバー起動
dev:
	cd gui && bunx @tauri-apps/cli dev

# 全テスト実行
test: test-core

# coreライブラリのテスト
test-core:
	cargo test -p picture-tool-core -- --nocapture

# フロントエンド依存インストール
install:
	cd gui-frontend && bun install

# リリースビルド
release:
	cargo build --release -p picture-tool
	cd gui && bunx @tauri-apps/cli build

# クリーンアップ
clean:
	cargo clean
	rm -rf gui-frontend/dist gui-frontend/node_modules
