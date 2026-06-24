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
    cargo check --all-targets

# Clean build artifacts
clean:
    cargo clean

# === Code Quality ===

# Format code
fmt:
    cargo fmt -- --config imports_granularity=Item

# Run linter
clippy:
    cargo clippy --all-features --tests

# Alias for clippy
lint: clippy

# Auto-fix lint warnings
fix *args:
    cargo clippy --fix --all-features --tests --allow-dirty "$@"

# === Testing ===

# Run tests
test:
    cargo nextest run --no-fail-fast

# Run tests with all features
test-all:
    cargo nextest run --no-fail-fast --all-features

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

# === Dependencies ===

# Update dependencies
update:
    cargo update

# === Documentation ===

# Generate documentation
docs:
    cargo doc --no-deps --open

# === Schema ===

# Sync config + bundle schemas to shared schemas repo
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
