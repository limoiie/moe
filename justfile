# Moe task runner. Toolchain versions live in mise.toml.
# Every line runs through `mise exec`, so recipes use the pinned rust/node/pnpm
# even when mise isn't activated in your shell.
set shell := ["mise", "exec", "--", "bash", "-cu"]

# List available recipes
default:
	@just --list

# One-time setup: pinned toolchain + UI dependencies
setup:
	mise install
	pnpm -C ui install --frozen-lockfile

# Build the Rust workspace
build:
	cargo build --workspace

# Run the Rust test suites (contract and platform tests)
test:
	cargo test --workspace

# CI lint gate: rustfmt check + clippy with warnings denied
lint:
	cargo fmt --all --check
	cargo clippy --workspace --all-targets -- -D warnings

# Format the Rust workspace
fmt:
	cargo fmt --all

# Type-check the UI (tsc --noEmit)
ui-check:
	pnpm -C ui typecheck

# Build the UI bundle (tsc + vite build, output ui/dist)
ui-build:
	pnpm -C ui build

# Vite dev server only (for the two-terminal workflow)
ui-dev:
	pnpm -C ui dev

# Run the panel in development (Vite + hot reload)
dev:
	cd crates/moe-app && cargo tauri dev

# Production bundle (.app/.dmg with ui/dist embedded)
package:
	cd crates/moe-app && cargo tauri build

# Everything CI runs
ci: lint test ui-build