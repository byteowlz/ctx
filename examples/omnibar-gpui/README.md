# ctx native omnibar trial

Standalone `ctx-bar` + GPUI-free `ctx-bar-design`, based on the first-party
`templates/rust-gpui` revision `222d3b4`, with `gpui-kit = 0.6.6`.
Owner: `trx-xv4n.3`. All selections remain previews, never execution.

## Use

```sh
# From the ctx root; opens a focus-taking native window:
just omnibar-run
```

The popup hides the titlebar/window controls, requests client-side decorations
without app chrome, and disables user resize handles. The scoped macOS
[vendor patch](vendor/gpui-pre-macos/CTX-PATCH.md) now creates a genuinely
borderless popup and disables its native shadow. Actual native style/shadow
properties are asserted at creation; user on-screen confirmation remains open.
See [frameless research](docs/frameless-research.md).
The GPUI root wrapper also has its border/shadow disabled, a transparent
background and no outer gutter; window options alone do not remove that wrapper.
Programmatic progressive expansion remains enabled. Compositors can override
Linux decoration requests; actual OS border/shadow appearance remains an
on-screen acceptance check.

Idle is only the input, in a 56–64-logical-pixel-high window. The default trial
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

## List and tree views

Two first-class views share one catalog, routing, retrieval and preview
contract. `view = "list"` (default) keeps compact rows. `view = "tree"` uses
branch routing with a stationary centered aperture: nodes and connectors move
on one coordinate layer, the current choice stays centered, earlier steps sit
to the left and the final receipt is inert (`Nothing executed`). `tree_variant`
picks the path treatment: `faint` (earlier choices dimmed), `trail` (only the
chosen horizontal path) or `crumbs` (contextual breadcrumbs). All three are
implemented for review; none is a selected winner.

- **F3** switches view without clearing the request; it invalidates the old
  scope, pending requests and timers. A final preview keeps its receipt.
- **F4** (or Left in the tree) goes Back to the previous offered scope and
  selection without a new inference. **F2** browses the full offered scope in
  either view; the tree shows five nodes around the selection, never filtering.
- Up/Down follow the tree's visual order. Enter or a second click on the settled
  selection chooses; nothing is chosen while the layer is still moving or while
  IME text is marked.
- In-input commands `ctx view list|tree`, `ctx tree faint|trail|crumbs` and
  `ctx dwell on|off` apply in memory only, like `ctx theme`.
- Tree dwell is opt-in (`ctx dwell on` / `auto_select_first`). It arms only after
  motion settles, for fresh `Jev via EAVS` results with finite probabilities.
  Edits, navigation, focus loss, view changes and Escape pause it; Space or the
  Resume control restarts it explicitly. It stops at the final preview. Local
  recall, F2 browsing, refusals and errors never arm it.
- Motion is a 220 ms ease-out retargeted from the rendered position. It snaps
  when `reduce_motion = true` or the platform reports reduced motion.

In the classic list, rows outside the scrolled viewport (and away from the
selection) render as equal-height spacers, so a 94-entry F2 scope stays fully
scrollable without exhausting the per-frame Dot budget.

Optional first-row timeout defaults off, independent of confidence. Edits,
navigation, selection changes, focus loss and local commands cancel it.
Generation/timer tickets reject stale completions. Config precedence is CLI >
`--config` file > `CTX__BAR__...` environment (old `CTX_BAR__...` alias retained)
> local `ctx.toml` > global `$XDG_CONFIG_HOME/ctx/config.toml` (`~/.config` fallback).
Native settings live in `[bar]`; `[eavs]` in the same file configures the backend.
Optional `[retrieval]` enables local CPU Evoke candidates while Jev is pending;
F2 browses the full offered scope, Enter previews only. No local probability or
timer, no filtering of Jev's choices. See [setup and proof limits](../../docs/local-retrieval.md).
First launch adds only a missing bar table/schema, preserving other settings and
comments. A prior `omnibar-gpui.toml` is a one-time read-only migration source;
existing shared `[bar]` wins. Explicit legacy flat native files still load. `height` is maximum
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

`just review-tree /tmp/ctx-tree-proof` renders 384 hidden scenes: all ten
designs x dark/light x 680/480 px x three variants x root/child/preview/back/
panned, plus 1200 px and 480 px catalog/loading/error/no-match/Unicode/idle,
an in-flight pan (aperture fixed, Enter refused until settled), marked-text
Enter, and deep F2 list browsing to index 60. It runs 240 native keyboard
checks and writes `timing.json`, which holds synthetic CPU edit+draw time for
list and tree. That timing is not GPU completion, compositor or physical input
latency. On the M2 Ultra review host, medians/p95 are 7.4/8.1 ms (Dot list, 3
rows), 9.2/10.0 ms (Dot tree, 5 nodes), 2.9/3.4 ms (Unframed list) and
3.6/4.0 ms (Unframed tree). It overrides reduced motion inside the app only, restores it
afterwards, and never changes OS preferences.

Dot matrix uses the actual OFL font at 11px sampling/3x display with separated
rounded cells. A complete current native-layout mask is required before covering
native ink. Non-ASCII values, IME composition or unsupported geometry leave
visible native text. No shared bottom-left focus line is painted on any design; native caret and
selection remain authoritative. The pixel caret is steady during this trial. The grid is submitted once per
successful mask, with an 8,192-cell paint ceiling and visible native fallback
above it. [Pixel visibility and input latency](docs/input-latency.md) records
explicit overlay anchoring, batched visible-cell painting (~90 ms to ~5.7 ms
synthetic edit/draw), expanded-menu pixel checks and the corrected historical
clipped-overlay measurement. The [whole-surface trial](docs/full-surface-trial.md)
now carries the chosen design through rows, status, navigation and previews,
including actual Departure cell text for supported Dot labels. Native fallback,
icons/controls and physical/accessibility acceptance remain explicit exceptions.

On-screen focus/keyboard, selection/scroll alignment, real IME, dictation,
VoiceOver, OS blur and other-platform runtime acceptance remain open. Transparent
surfaces need real-backdrop review. `DESIGN.md` records native exceptions; shared
Studio `artifacts/ctx/omnibar/native-designs/` owns source/render evidence.
