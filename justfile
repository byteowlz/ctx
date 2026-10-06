set positional-arguments

# Display help
help:
    just -l

# === Build ===

# Debug build
build:
    cargo build

# Release build
build-release:
    cargo build --release

# Fast compile check
check:
    cargo check --workspace --all-targets --all-features

# Clean build artifacts
clean:
    cargo clean

# === Code Quality ===

# Format code with stable rustfmt (no nightly-only settings)
fmt:
    cargo fmt --all

# Check formatting without changing files
fmt-check:
    cargo fmt --all -- --check

# Deny warnings across the workspace, including tests and examples
clippy:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

# Alias for clippy
lint: clippy

# Auto-fix lint warnings
fix *args:
    cargo clippy --fix --all-features --tests --allow-dirty "$@"

# === Testing ===

# Run workspace tests, including doctests; nextest is not required
test:
    cargo test --workspace --no-fail-fast

# Measure release capture stages without exposing captured content
benchmark-capture:
    cargo build --release -p ctx-cli
    bun scripts/benchmark_capture.ts target/release/ctx

# Run tests with all features
test-all:
    cargo test --workspace --all-features --no-fail-fast

# Comprehensive local baseline (YAML tooling/CI are outside this check)
check-all: check fmt-check clippy test-all validate-examples check-repo-hygiene

# Archive clone-local legacy Beads integration; preserve trx and unrelated hooks
purge-beads:
    uv run --script scripts/purge_beads.py

# Verify legacy hook cleanup against isolated Git repositories
check-repo-hygiene:
    uv run --script scripts/test_purge_beads.py

# === Install ===

# Fetch dependencies (run before install tasks)
fetch:
    rustup show active-toolchain
    cargo fetch

# Install CLI
install: fetch
    cargo install --path ctx-cli

# Install all components (CLI, API, MCP)
install-all: fetch
    cargo install --path ctx-cli
    cargo install --path ctx-api
    cargo install --path ctx-mcp

# === Run ===

# Run ctx CLI
run *args:
    cargo run -p ctx-cli -- {{args}}

# === Native omnibar prototype (separate workspace; no action execution) ===

# Verify native build/tests and local adapter without launching a window
omnibar-check:
    bun test examples/omnibar-prototype/prototype.test.ts
    just --working-directory examples/omnibar-gpui --justfile examples/omnibar-gpui/justfile check-all

# Run the loopback adapter with an explicitly selected existing EAVS profile
omnibar-server profile="":
    OMNIBAR_EAVS_PROFILE="$1" bun examples/omnibar-prototype/server.ts

# Human launch: opens a focus-taking GPUI prototype window
omnibar-run:
    just --working-directory examples/omnibar-gpui --justfile examples/omnibar-gpui/justfile run

# === Dependencies ===

# Update dependencies
update:
    cargo update

# === Documentation ===

# Generate documentation
docs:
    cargo doc --no-deps --open

# === Schema ===

# Validate local config TOML against its JSON Schema (uv + Python >=3.11)
validate-config:
    uv run --script scripts/validate_examples.py --config-only
    cargo test -p ctx-core config_example_matches_default_values

# Validate every local schema and the config/bundle examples
validate-examples:
    uv run --script scripts/validate_examples.py
    cargo test -p ctx-core config_example_matches_default_values

# Sync config + bundle schemas to shared schemas repo (commits and pushes there)
schema:
    ./scripts/sync_schemas.sh

# === Release ===

# Bump version: just bump patch|minor|major
bump level:
    #!/usr/bin/env bash
    set -euo pipefail
    ROOT="$(git rev-parse --show-toplevel)"
    current=$(grep -m1 '^version = ' "$ROOT/Cargo.toml" | sed 's/.*"\(.*\)"/\1/')
    IFS='.' read -r major minor patch <<< "$current"
    case "{{level}}" in
        patch) new="$major.$minor.$((patch + 1))" ;;
        minor) new="$major.$((minor + 1)).0" ;;
        major) new="$((major + 1)).0.0" ;;
        *) echo "Usage: just bump patch|minor|major"; exit 1 ;;
    esac
    echo "Bumping $current -> $new"
    sed -i '' "s/^version = \"${current}\"/version = \"${new}\"/" "$ROOT/Cargo.toml"
    for crate in ctx-cli ctx-core ctx-api ctx-mcp; do
        sed -i '' "s/^version = \"${current}\"/version = \"${new}\"/" "$ROOT/$crate/Cargo.toml"
    done
    cargo check --workspace 2>/dev/null
    git add -A
    git commit -m "chore: bump to ${new}"
    echo "Bumped ${current} -> ${new}"
