.PHONY: build build-cli build-gui build-frontend dev test test-core lint fmt typecheck check clean install release

# ローカルに固定インストールした Tauri CLI（gui-frontend の devDependency）
TAURI := $(CURDIR)/gui-frontend/node_modules/.bin/tauri

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
	cd gui && $(TAURI) dev

# 全テスト実行（core / cli / gui）
test:
	cargo test --workspace

# coreライブラリのテストのみ
test-core:
	cargo test -p picture-tool-core -- --nocapture

# Rust の lint（CI と同じ条件）
lint:
	cargo fmt --all -- --check
	cargo clippy --workspace --all-targets -- -D warnings

# フォーマット適用
fmt:
	cargo fmt --all

# フロントエンドの型検査
typecheck:
	cd gui-frontend && bun run typecheck

# CI と同等の検証を一括実行
check: lint test typecheck

# フロントエンド依存インストール
install:
	cd gui-frontend && bun install

# リリースビルド（フロントエンド埋め込み済みバイナリ）
release: build-frontend
	cargo build --release -p picture-tool
	cargo build --release -p picture-tool-gui --features tauri/custom-protocol

# クリーンアップ
clean:
	cargo clean
	rm -rf gui-frontend/dist gui-frontend/node_modules
