# AGENTS.md

## Architecture and domain

Rust 2024 workspace:

- `ctx-core`: configuration/XDG directories, host capture and platform adapters,
  current-context state, bundle manifests/store, ingest/export, and handoff logic.
- `ctx-cli`: the `ctx` CLI (`src/main.rs`, `bundle.rs`, `destinations.rs`).
- `ctx-mcp`: read-only stdio JSON-RPC server and resource tests; no network listener.
- `ctx-api`: placeholder binary, not a shipped HTTP API.

Read [CONTEXT.md](CONTEXT.md) for domain vocabulary and [docs/adr/](docs/adr/)
for platform-capture and section-assignment decisions. Do not equate domain plans
with implemented features or mocked tests with live host verification.

## Required checks

Use `just` as the stable command surface; `just help` lists recipes.

```bash
just build                 # Debug workspace build
just check                 # Workspace, all targets/features
just test                  # cargo test, including doctests (no nextest required)
just test-all              # Workspace tests with all features
just fmt                   # Stable cargo fmt; changes files
just fmt-check             # Formatting check, no writes
just clippy                # All targets/features, -D warnings
just validate-config       # Local TOML/schema + typed-default drift check
just validate-examples     # All local schemas + config and bundle examples
just check-all             # check, fmt-check, clippy, test-all, validate-examples
cargo test -p ctx-core save_config_uses_schema_comment_and_round_trips_values
```

Run `cargo check --workspace` after Rust changes and `just check-all` before a
significant commit. Report existing blockers rather than claiming a green gate
or sweeping unrelated formatting/lint changes. Linux builds require PipeWire
headers and libclang for `xcap`; see [README.md](README.md).

Validation requires `uv` and Python >=3.11; `uv` resolves the script's pinned
JSON Schema validator. See [docs/development.md](docs/development.md) for scope
and prerequisites. Never run `just schema` as a local validation check: it writes,
commits, and pushes to the separate shared schemas repository.

## Code and configuration

- Imports: std, external crates, then local modules; match the local style.
- Use `thiserror` for library errors, `anyhow` for applications. Prefer `Result`
  over panics; degrade gracefully when host permissions/APIs are unavailable.
- No emojis in code, comments, or commit messages.
- Config uses the `config` crate. Precedence: CLI flags, explicit CLI config file,
  `CTX__...` environment variables, local `./ctx.toml`, global config file.
- ctx, ctx-bar `[bar]`, and Jev routing `[eavs]` share
  `$XDG_CONFIG_HOME/ctx/config.toml`, with `~/.config/ctx/config.toml` fallback on
  Unix including macOS. Do not introduce a separate bar/prototype config file.
  Startup migrates missing tables from old files without deleting them; existing
  shared tables win. Core config saves must retain other consumers' tables.
- Optional `[retrieval]` belongs to the adapter in the same ctx configuration.
  Install its pinned assets explicitly with `just install-retrieval`; never
  infer/download on the GPUI input/render thread. Local sparse candidates are
  uncalibrated preview-only recall; they must not hard-filter Jev, arm timers or
  replace refusal/authorization. F2 browses the full offered scope explicitly.
  Keep model/derived index replaceable and out of ctx-core. See docs/local-retrieval.md.
- Data/state paths retain their existing XDG/platform defaults. Windows core
  config currently uses APPDATA; native alignment there remains a separate gate.
  See `ctx-core/src/directories.rs`. First load creates a commented config.
- Editor metadata in TOML is a `#:schema` comment, never an application-visible
  `$schema` setting. `save_config` rewrites values/comments rather than editing
  the original file in place. JSON Schema documents still use `$schema`.
- Schemas in `examples/` are hand-maintained. Validate examples and update schema
  constraints alongside typed config changes; there is no typed schema generator.

## Issue tracking

Use **trx** for all substantial work; use `--json` for programmatic calls.

```bash
trx ready --json
trx create "Issue title" -t task -p 2 --json
trx update trx-42 --status in_progress --json
trx create "Discovered gap" -t bug -p 2 --parent trx-42 --json
trx verify add trx-42 --status passed --command "just check-all" --summary "Checks passed" --json
trx close trx-42 --reason "Implemented and verified" --json
```

Tracker state is centralized: `.trx/central` selects this repository's entry in
the per-user central store, so the old repo-local `.trx/*.jsonl` ledgers are no
longer tracked here and must not be recreated by hand. trx updates the central
ledger without committing or pushing this repository's code, so a change set
carries only its own files.
Do not install bd/Beads hooks, filters or configuration. Use `just purge-beads`
on existing clones to archive legacy integration outside git; it never invokes bd
or alters `.trx`. Mixed/unrecognized or external shared hooks require manual review.
Do not use Beads paths, Markdown TODO lists, or external issue trackers.
Record deferred gaps and honest verification evidence in the issue.

## Scoped exceptions and artifacts

- `just check-all` excludes YAML tooling, ast-grep, and CI work. Do not introduce
  or modify YAML or ast-grep configuration as part of the baseline task.
- Strict Clippy means the existing lint set with warnings denied, not adoption of
  the entire template lint preset. Platform FFI and existing environment-mutating
  tests use unsafe code; enabling `forbid(unsafe_code)` needs a separate audit.
- Release/version workflow changes and central schema publication need separate
  approval; existing release helpers are not a claim of standard compliance.
- Put ephemeral planning documents under `history/`; only inspect existing
  history when explicitly requested. Use repo docs/ADRs for durable design and
  trx for work status. Keep private host data and credentials out of git.
