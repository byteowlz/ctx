# Development checks

The root `justfile` is the local command surface. `just check-all` runs workspace
compilation (all targets/features), stable formatting checks, Clippy with warnings
denied (all targets/features), workspace tests with all features including
doctests, and local schema/example validation. Tests use `cargo test`; nextest is
not required. `just fmt` uses stable rustfmt without nightly-only import options.

Prerequisites: Rust with rustfmt and Clippy, `just`, and `uv`. Schema validation
uses a Python >=3.11 script with a pinned `jsonschema` dependency; the first `uv`
run may download Python/packages. Validation itself reads only local schemas and
examples, not the central schema registry. Linux capture-backend builds also
require PipeWire development headers and libclang.

Capture benchmarks and the routing/capture latency boundary are documented in
[capture-performance.md](capture-performance.md). `just benchmark-capture`
requires Bun and builds the release CLI; it does not install it. Native GPUI
prototype sources are an excluded standalone workspace under
`examples/omnibar-gpui`, so the root gate does not prove that UI builds.

## Configuration and schema checks

- `just validate-config`: validate `examples/config.toml` against its schema and
  run the Rust check that the example's parsed values equal first-run defaults.
- `just validate-examples`: additionally check all `examples/*.schema.json`
  documents against their declared JSON Schema dialect and validate every
  `examples/bundles/*.json` against the bundle schema.
- `cargo test -p ctx-core save_config_uses_schema_comment_and_round_trips_values`:
  save into isolated temporary directories, assert a `#:schema` comment and no
  parsed `$schema` key, preserve non-default capture/provider/OCR values, and
  reload output paths through the application parser.

TOML editor metadata is now a `#:schema` comment both on first run and when
saving configuration. The config schema no longer advertises `$schema` as an
application setting. Old TOML `$schema` settings remain tolerated by Serde's
unknown-field behavior but are omitted on the next save. Saving rewrites the
file; it does not preserve user comments. JSON Schema's own `$schema` remains.

Schemas are hand-maintained; these checks catch invalid documents/examples and
example/default drift, not every possible typed-model/schema divergence. There
is no `generate-config` recipe or full schema-generation drift gate. Publishing
is separate: `just schema` invokes `scripts/sync_schemas.sh`, which modifies,
commits, and pushes a sibling repository. Never use it for local validation.

## Paths and bounded baseline

Config precedence is CLI overrides, explicit CLI config, `CTX__...` environment
variables, local `./ctx.toml`, then global config. XDG variables are honored;
without them config/data follow the `dirs` platform defaults (Linux:
`~/.config` / `~/.local/share`, macOS: `~/Library/Application Support`). Unix
state defaults to `~/.local/state`. Windows config uses `APPDATA`; data/state use
XDG variables or `LOCALAPPDATA`/platform defaults. Changing platform fallbacks to
an all-platform `~/.config` contract is separate work, not part of this baseline.

The root package version is under Cargo's supported `[workspace.package]` table;
member crates still declare their versions explicitly. No version inheritance
or release-helper redesign is included. The only platform lint cleanup removes
a needless final `return` from the macOS accessibility branch; behavior is unchanged.

YAML tooling, ast-grep configuration, CI, release migration, central schema
publication, and adoption of the full template Rust lint preset are excluded.
Track deferred gaps and verification evidence in `trx-yy8h`, not Markdown TODOs.
A passing local gate does not prove Linux/Windows builds or live capture with
host permissions, nor does it turn the `ctx-api` placeholder into a working API.
