# ctx

Rust workspace for the ctx context-capture tool.

- Default config is created at `$XDG_CONFIG_HOME/ctx/config.toml` (or `~/.config/ctx/config.toml`), with data in `$XDG_DATA_HOME/ctx` and state in `$XDG_STATE_HOME/ctx`.
- Config precedence (highest first): CLI flags, CLI `--config`, environment variables (`CTX__...`), local `./ctx.toml`, global config file.
- A commented example config lives in `examples/config.toml` with a `$schema` reference (schema in `examples/config.schema.json`).
- Run `cargo run -p ctx-cli` (binary name: `ctx`) to exercise the CLI; it emits structured JSON (metadata + system, displays, apps, windows, clipboard, screenshots, accessibility, actions) or a human summary (`--json` for machine output). Use `--save` to write the JSON into the configured capture directory. Clipboard and screenshots are captured when enabled; use `--provider noop` to disable host interactions. Use `scripts/copy_config_schema.sh` to publish schema updates to the shared schemas repo.
- Tooling commands are listed in AGENTS.md.
