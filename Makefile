.PHONY: build build-cli build-gui build-frontend dev test test-core lint fmt typecheck test-frontend check clean install release

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

# フロントエンドの単体テスト（純粋ロジックのみ。runes / DOM は Playwright 側）。
# 走査範囲は bunfig.toml の [test] root = "src" で src/ に限定してある
test-frontend:
	cd gui-frontend && bun test

# CI と同等の検証を一括実行
check: lint test typecheck test-frontend

# フロントエンド依存インストール
install:
	cd gui-frontend && bun install

# リリースビルド（CLI バイナリ + GUI のバンドル/インストーラ）
# GUI は `tauri build` を通す。cargo build だけだとバンドルが作られず、
# 「release」という名前と実態が食い違っていた（S6-L16）。
release: build-frontend
	cargo build --release -p picture-tool
	cd gui && $(TAURI) build

# クリーンアップ
clean:
	cargo clean
	rm -rf gui-frontend/dist gui-frontend/node_modules
