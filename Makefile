.PHONY: dev build run check test e2e clean

# Build frontend and run Tauri app in dev mode (hot reload)
dev:
	cargo tauri dev --manifest-path crates/lgtm-app/Cargo.toml

# Build everything for production
build: build-web build-app build-cli

build-web:
	cd packages/web && npm run build

build-app: build-web
	cargo build -p lgtm-app --release

build-cli:
	cargo build -p lgtm --release

# Run the Tauri app (builds frontend first)
run: build-web
	cargo run -p lgtm-app

# Type-check and lint
check:
	cargo check --workspace
	cd packages/web && npm run check

# Run all tests
test:
	cargo test --workspace

# Run e2e tests
e2e: build-web
	cargo build -p lgtm-app
	cd packages/web && npx playwright test

# Remove build artifacts
clean:
	cargo clean
	rm -rf packages/web/dist
