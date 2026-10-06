# ctx native omnibar prototype

Standalone nested workspace: `ctx-bar` + GPUI-free `ctx-bar-design`, scaffolded
from `templates/rust-gpui` at `222d3b4`. Only `ctx-bar` depends on GPUI, through
`gpui-kit = 0.6.6`. Owner issue: `trx-xv4n`.

## Interaction

One native text input, then highlighted tool/interface suggestions. System
dictation insertion is intended through platform text input but remains unverified. Up/down selects, Enter previews,
Escape cancels (or returns from a branch). Mouse selection also previews.
No actions, commands, capture, clipboard reads or microphone recording execute.
Selections produce an in-memory preview receipt, not an execution record.

Flat is the default. The exploratory Branches toggle permits only root plus one
child level. Selecting a declared branch requests its children lazily: one new
Jev request with the same query/platform/fixture. Back invalidates old child
results; edits and option changes reset root. Branch metadata must be exposed
by the proxy catalog as branch items (`kind = "branch"`, `node = <declared ID>`),
in `items` or `branches`. Missing metadata is an explicit error, not invented
branches or a silently substituted flat result.

Optional first-row timeout is **off by default**, independent of probability.
A fresh response arms it when enabled; edits, navigation, selectors, focus loss,
Escape, back, manual choice and timeout-option changes cancel it. Enabling the
option does not arm an already visible list. Debounced edits coalesce behind at
most one in-flight request. Generation/timer tickets discard stale completions.

## Runtime boundary

Start the separately owned loopback Bun proxy first. The native client reads
`$XDG_STATE_HOME/ctx/omnibar-prototype.json` (fallback
`~/.local/state/ctx/omnibar-prototype.json`) containing only `origin` and
`reviewKey`. Unix permissions must be owner-only. The accepted origin is exactly
`http://127.0.0.1:4784`; redirects and environment HTTP proxies are disabled.

`GET /api/catalog` loads prototype metadata; `POST /api/suggest` sends
`{query, platform, fixture, presentation, node}` with same-origin `Origin` and
`X-Prototype-Key` headers. No EAVS/upstream credential is read by this app.
Responses are bounded and validated; raw error bodies are withheld. The bundled
catalog provides labeled startup examples, **never fake inference**.

Context is synthetic: Desktop, Selection, Audio or Unavailable. Platforms are
macOS, Windows, Linux and Omarchy. No full `ctx capture` runs on input/decisions.
A future live adapter should consume cached/event-updated lightweight current
state rather than placing full capture on the keystroke path.

Config: `$XDG_CONFIG_HOME/ctx/omnibar-gpui.toml`, fallback
`~/.config/ctx/omnibar-gpui.toml`; defaults and sibling JSON schema are created on
first launch without rewriting existing files. Precedence: CLI > `--config`
file > `CTX_BAR__...` environment > local `ctx-bar.toml` > global file. See
`examples/` and `ctx-bar --help`.

## Checks and launch

```sh
cd examples/omnibar-gpui
just check-all
# From the ctx root, start the adapter in a separate terminal first:
# just omnibar-server your-existing-authorized-eavs-profile
# Human launch only: opens a focus-taking native window.
just run
```

Build/test evidence is not native render, dictation, accessibility or
cross-platform runtime proof. No window was launched during implementation.
See `DESIGN.md` for the exploratory visual scope and deferred rendering proof.
