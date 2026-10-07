# Native pixel visibility and input latency

The Dot overlay needs both an explicit position and batched paint ordering.
A canvas appended after native Input with `.absolute().size_full()` retained its
static flow position: one bar below the input. Expanded rows covered it. Mask
success and submitted-quad counters therefore **did not prove visible pixels**.

The corrected overlay uses `.top_0().left_0()`. Non-overlapping cells are painted
inside GPUI's `paint_layer` batch; the covering black quad stays outside, earlier
in draw order. There is still exactly one grid per validated native mask. This
avoids thousands of independent visible primitive-bound ordering insertions.
Native editing, glyph/font/geometry checks and IME/Unicode fallback remain.

## Reproduce

From `examples/omnibar-gpui`:

```sh
just benchmark-input --assert-expanded-grid
just benchmark-input --append-only
just benchmark-input --local-menu
just benchmark-input --width 1200 --expect-plain
```

Hidden native GPUI windows and synthetic fixtures only; no model/config/desktop
capture. Expanded-grid checks capture synthetic native scenes for empty, `ctx`,
`ctx theme`, selection, Unicode and marked composition, light/dark palettes and
supported/oversized widths. They inspect separated cell centers/gaps and lit text
pixels, **not just counters**. Actual InputState insertion/deletion and selection
replacement are exercised; native input values and idle observations are checked.
Opt-in `native-review` diagnostics record no real input values or ordinary clocks.

## Measured CPU edit + draw

macOS debug build, 200 edits; no GPU/compositor completion fence. Compilation is
excluded. Representative runs:

| Workload | Median / p95 |
| --- | ---: |
| Visible anchored Dot, independent quad submission | 89.65 / 96.78 ms |
| Visible anchored Dot, batched cells | 5.70 / 7.09 ms |
| Plain insert/delete | 2.41 / 3.27 ms |
| Expanded local-menu Dot, owner reproduction | 8.18 / 10.55 ms |

The earlier ~94 ms to ~5.45 ms result removed an expensive pre-native grid but
left the post-native overlay misplaced/clipped. That result is historical,
**not evidence of a fast visible matrix**. Anchoring alone reproduced ~90 ms
again. The real improvement is verified with both visible pixels and the batch.

A conservative 8,192-cell paint ceiling remains. At width 1200 the native input
falls back visibly rather than covering ink; this is intentional bounded geometry,
not proof of a pixel treatment at every configured width. Future full-surface
styling must budget the whole frame, not increase raw quads without measurement.
Unsupported live Unicode/composition stays visibly native on black.

65 native test invocations, default/all-feature all-target checks and application
warnings-denied Clippy pass. User physical keyboard-to-present, active caret/IME,
dictation/accessibility and other platforms remain unaccepted. Native borderless
host properties are verified separately; synthetic scene pixels are not a
private desktop/compositor capture. Expanded rows still need the separately
tracked whole-surface theme extension.
