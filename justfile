set positional-arguments

# Display help
help:
    just -l

# format code
fmt:
    cargo fmt -- --config imports_granularity=Item

fix *args:
    cargo clippy --fix --all-features --tests --allow-dirty "$@"

clippy:
    cargo clippy --all-features --tests "$@"

# Fetch dependencies (run before install tasks)
fetch:
    rustup show active-toolchain
    cargo fetch

# Install all components (CLI, syncd, MCP, server, desktop)
install-all: fetch
    cargo install --path ctx-cli
    cargo install --path ctx-api


# Install minimal CLI only (no Tauri/GUI features)
install: fetch
    cargo install --path ctx-cli

# Run `cargo nextest` since it's faster than `cargo test`, though including
# --no-fail-fast is important to ensure all tests are run.
#
# Run `cargo install cargo-nextest` if you don't have it installed.
test:
    cargo nextest run --no-fail-fast
