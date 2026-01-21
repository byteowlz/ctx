# ctx

Rust workspace for the ctx context-capture tool.

- Default config is created at `$XDG_CONFIG_HOME/ctx/config.toml` (or `~/.config/ctx/config.toml`), with data in `$XDG_DATA_HOME/ctx` and state in `$XDG_STATE_HOME/ctx`.
- A commented example config lives in `examples/config.toml`.
- Run `cargo run -p ctx-cli` to exercise the CLI; it emits structured JSON (metadata + system, displays, windows, clipboard, screenshots, accessibility) or a human summary (`--json` for machine output). Use `--save` to write the JSON into the configured capture directory. Clipboard and primary-screen screenshots are captured when enabled; use `--provider noop` to disable host interactions.
- Tooling commands are listed in AGENTS.md.
