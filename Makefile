.PHONY: build build-cli build-gui build-frontend dev test test-core clean install release

# デフォルト: 全ビルド
build: build-cli build-gui

# CLIバイナリのビルド
build-cli:
	cargo build -p picture-tool

# GUIアプリのビルド（フロントエンド含む）
build-gui: build-frontend
	cargo build -p picture-tool-gui

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

# リリースビルド（フロントエンド埋め込み済みバイナリ）
release: build-frontend
	cargo build --release -p picture-tool
	cargo build --release -p picture-tool-gui

# クリーンアップ
clean:
	cargo clean
	rm -rf gui-frontend/dist gui-frontend/node_modules
