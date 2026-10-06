# ctx native omnibar trial

Standalone `ctx-bar` + GPUI-free `ctx-bar-design`, based on the first-party
`templates/rust-gpui` revision `222d3b4`, with `gpui-kit = 0.6.6`.
Owner: `trx-xv4n.3`. All selections remain previews, never execution.

## Use

```sh
# From the ctx root; opens a focus-taking native window:
just omnibar-run
```

Idle is only the input, in a 72–80-logical-pixel-high window. The default trial
is **Dot matrix with embedded Departure Mono**. Suggestions/status expand the
window; up to five rows are visible before scrolling.

Type in the bar:

- `ctx theme` — list all ten designs.
- `ctx theme dot` — filter to Dot matrix; Enter applies it and clears the input.
- `ctx theme lens` — likewise choose Lens.

Underline, Monolith, Lens, Signal, Prompt, Dot matrix, Corners, Slot, Unframed and
Notch are available. Arrows/Enter or a click select; Escape cancels. Design changes
are in-memory; TOML/`--design` sets the next-launch default. Unknown design filters
remain local. These commands never go to Jev and invalidate pending responses and
timers. Paused command prefixes are also withheld; the exact standalone queries
`c` and `ct` therefore do not route. Other ordinary queries retain real routing.
`ctx theme` changes the design; `--theme` selects the Lumen light/dark palette.

For ordinary suggestions, start the existing adapter in another terminal:

```sh
just omnibar-server your-existing-authorized-eavs-profile
```

EAVS must already run. Context/catalog/platform remain synthetic; the adapter's
Jev transport is real. The browser gallery is separate and uses no model.
Enter previews a suggested interface. Flat is the default; `presentation =
"branches"` permits root plus one lazily requested child level. No speculative
tree, tool invocation, grants, host capture or microphone recording is added.
Native explicit editing remains available; no periodic clipboard reads occur.

Optional first-row timeout defaults off, independent of confidence. Edits,
navigation, selection changes, focus loss and local commands cancel it.
Generation/timer tickets reject stale completions. Config precedence is CLI >
`--config` file > `CTX_BAR__...` environment > local `ctx-bar.toml` > global
`$XDG_CONFIG_HOME/ctx/omnibar-gpui.toml` (`~/.config` fallback). First launch creates
missing defaults/schema without rewriting existing files. `height` is maximum
expansion, not idle geometry. See `examples/` and `ctx-bar --help`.

## Runtime boundary

The client reads only the owner-private loopback descriptor
`$XDG_STATE_HOME/ctx/omnibar-prototype.json` (`~/.local/state` fallback), containing
`origin` and `reviewKey`, not provider credentials. Only
`http://127.0.0.1:4784` is accepted; redirects/environment proxies are disabled.
Catalog/response validation and transport bounds remain enforced. Startup has no
catalog examples or permanent controls.

## Checks and evidence

```sh
cd examples/omnibar-gpui
just check-all
cargo clippy --workspace --all-targets --all-features -- -D warnings
# Explicit, hidden synthetic native scene capture; no desktop capture/model:
cargo run -p ctx-bar --features native-review -- --review-dir /tmp/ctx-native-proof
```

Use a dedicated review directory: named synthetic PNGs/manifest are overwritten.
Review requires the opt-in `native-review` feature; failure exits nonzero.
Evidence includes actual GPUI scene renders for all ten empty/suggestion states
at dark 680px and light 480px widths, and GPUI-dispatched Enter switching checks.
These are **hidden native scenes**, not browser mocks or OS-compositor captures.

Dot matrix uses the actual OFL font at 11px sampling/3x display with separated
rounded cells. A complete current native-layout mask is required before covering
native ink. Non-ASCII values, IME composition or unsupported geometry leave
visible native text. The pixel caret is steady during this trial.

On-screen focus/keyboard, selection/scroll alignment, real IME, dictation,
VoiceOver, OS blur and other-platform runtime acceptance remain open. Transparent
surfaces need real-backdrop review. `DESIGN.md` records native exceptions; shared
Studio `artifacts/ctx/omnibar/native-designs/` owns source/render evidence.
