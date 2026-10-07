# Native input latency regression

Dot matrix painted two full cell grids per successful native mask: an unlit
backdrop before Input, then a complete validated overlay that covered it. The
first grid was discarded visually but still submitted thousands of overlapping
quads. Removing it preserves the actual pixel appearance and native input.
Unsupported/IME/oversized text stays visible as ordinary native ink on black;
there is no expensive decorative fallback grid.

## Reproduce

From `examples/omnibar-gpui`:

```sh
cargo run -p ctx-bar --features native-review --example input_lag -- --assert-single-grid
cargo run -p ctx-bar --features native-review --example input_lag -- --assert-single-grid --append-only
cargo run -p ctx-bar --features native-review --example input_lag -- --assert-single-grid --width 1200 --expect-plain
```

Hidden native GPUI windows, synthetic input, no model/config/desktop capture.
The harness inserts/deletes through the actual native InputState handler and
forces draws. It checks the actual paint submission seam: one grid per successful
mask, not a source-string counter. Selection replacement, Unicode/composition
fallback, native values, long-input geometry and idle notifications are checked.
Instrumentation is opt-in under `native-review`; ordinary builds record no clocks
or values.

## Measurements

macOS, debug profile, 200 synthetic edits. Timing is native edit + CPU draw;
compilation excluded, GPU/compositor presentation not fenced.

| Workload | Before median / p95 | Fixed median / p95 |
| --- | ---: | ---: |
| Dot, 200 consecutive insertions | 94.41 / 95.36 ms | 5.45 / 5.92 ms |
| Plain, same workload | 2.58 / 3.13 ms | 2.55 / 3.07 ms |
| Dot, insert/delete | 94.24 / 95.03 ms | 5.31 / 5.78 ms |

Owner independently reproduced the fixed append measurement: 200 successful
masks, 200 grid passes, 949,200 quads; before was 400 passes/1,898,400 quads. The
red submission gate failed after restoring only the redundant grid. Mask creation
cost about 0.20 ms/edit; painting dominated. A notification feedback loop did
not reproduce: 200 changes/renders/observations, no extra observations during ten
forced idle draws. Existing observer/editor/IME handling remains unchanged.

An 8,192-cell conservative paint ceiling prevents large-grid workloads from
covering native ink. The boundary has a unit regression. At width 1200 the owner
reproduced visible native fallback: zero grid submissions, median 2.23 ms.
This ceiling is headroom policy, not an adaptive device-performance guarantee.

65 native test invocations, all-target default/all-feature checks and Clippy
pass. Physical keyboard-to-present latency, real visible-window focus/selection,
IME/dictation/accessibility and other platforms remain unaccepted. No claim that
all lag on every design is fixed. Native border/compositor work is separate.
