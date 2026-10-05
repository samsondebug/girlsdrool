# Kept task runner (ADR-0009). `just check` is the gate every milestone must pass.
# Needs bash on PATH (Git for Windows provides it), pnpm, and a Rust toolchain with rustfmt + clippy.

set shell := ["bash", "-euo", "pipefail", "-c"]
set windows-shell := ["bash", "-euo", "pipefail", "-c"]

default: check

# Everything. On Windows this includes the Playwright critical path against the real app; on any
# other host the E2E step cannot run (no WebView2) and the run ends with an explicit notice.
check: check-core
    @just {{ if os() == "windows" { "e2e" } else { "e2e-notice" } }}

# Every check that runs on any host.
check-core: fmt-check clippy test-rust typecheck lint format-check test-ts gates
    @echo "check-core: green"

fmt-check:
    cd src-tauri && cargo fmt --all -- --check

clippy:
    cd src-tauri && cargo clippy --all-targets --no-default-features -- -D warnings
    cd src-tauri && cargo clippy --all-targets --all-features -- -D warnings

test-rust:
    cd src-tauri && cargo test --no-default-features

typecheck:
    pnpm typecheck

lint:
    pnpm lint

format-check:
    pnpm format:check

test-ts:
    pnpm test

# Greppable quality gates: no TODO, no `any`, no unwrap/expect outside tests, no println!, no
# floats in the core, no network APIs or crates, no commented-out code.
gates:
    bash scripts/gates.sh

# Build the debug app without an installer (what the E2E drives).
build-debug:
    pnpm tauri build --debug --no-bundle

# Playwright critical path against the built app. Windows only: WebView2 exposes CDP.
e2e: build-debug
    pnpm exec playwright test

e2e-notice:
    @echo "E2E NOT RUN: requires Windows/WebView2 — this is not a full green (ADR-0009)"

# Unsigned Windows installer (signing is a manual step, docs/release.md).
build:
    pnpm tauri build

# Auto-fix formatting.
fmt:
    cd src-tauri && cargo fmt --all
    pnpm format

# Run the app against a scratch data folder.
dev:
    KEPT_DATA_DIR="${KEPT_DATA_DIR:-$PWD/.dev-data}" pnpm tauri dev
