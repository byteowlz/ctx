# Whole-surface native styling trial

Scope: Studio `ctx/omnibar` full-surface-tree-r1 (`6428621`), extending the
selected native ten-way design to disclosed content. This is not a new winner
or production visual/accessibility approval. Idle remains input-only.

`app::surfaces::Frame` applies material, ink, text and enclosure vocabulary to
local theme choices, tool rows, loading/error/no-match/preview status, breadcrumb
and Back. Underline/Corners/Slot edge ink follows row backgrounds; Notch's filled
silhouette precedes text. Lens alone intentionally rounds its group/rows. The
native host remains genuinely borderless with shadow off.

## Dot treatment and exceptions

Static labels use the actual embedded Departure Mono 11px raster: labels/status/
preview/navigation at 2x (pitch 2), metadata at 1.5x (pitch 1.5), Back at 1x
(pitch 1); gap .25, corner .34. Only lit cells are submitted in non-overlapping paint batches, not
an unbounded ambient full-window grid. The input retains its existing 3x/pitch3
validated native mask and editing guards unchanged.

Monochrome extension: black groups, white primary/error copy, `#CCCCCC` metadata,
`#252525` selected/hover rows. Explicit error copy remains; it does not rely on a
red-only state cue. These are the recorded Dot-stage exceptions, not canonical
palette mutations. Other designs use existing canonical roles, including the
light Signal paper and light metadata foreground contrast corrections. Normal
error text uses readable foreground plus an explicit `Error:` marker instead of
low-contrast destructive ink. Back uses original native control font/size/weight
rather than larger variant body metrics, preserving its hitbox without clipping.

Native static strings remain in layout/semantics. Their ink becomes transparent
only after a complete supported raster plan and paint-budget reservation; the
replacement cells draw in an explicitly anchored canvas. This does **not** disable
or make the live native Input transparent. Unsupported Unicode, oversized text,
font failure or exhausted budget retain ordinary visible native static text.
Native controls/icons/focus markers and Back's original hitbox remain explicit
platform exceptions. Actual VoiceOver/keyboard/material approval is still open.

Plans: max256 bytes and4096 lit cells per text; max24000 reserved static cells per
frame; bounded64-entry in-memory cache. Input's separate8192-cell ceiling remains.
Truncation uses actual Departure ellipsis. No new dependency/font or permissions.

## Repeatable evidence

From `examples/omnibar-gpui`:

```sh
just review-surfaces /tmp/ctx-surface-review
just benchmark-input --local-menu
just benchmark-input --assert-expanded-grid
```

The feature-gated example renders 226 hidden, synthetic native scenes: ten
variants, both palettes, eleven states (empty/local/tools/branch/child/loading/
error/no-match/preview/long/Unicode). No foreground window, model request,
private screenshot or desktop/compositor capture. Six additional native Down/
scroll scenes verify that Corners/Slot/Underline enclosure paint bounds stay
fixed while selection moves to row8. Its manifest records actual
static label canvas bounds. Pixel checks use those bounds—not assumed rem/padding:
260 lit title-cell centers, zero lit gaps in both palettes. Native fallback and
root transparency checks remain. The existing expanded-input pixel test passes.

Owner checks caught/fixed a late worker defect: GPUI permits one hover refinement;
two `.hover()` calls panic when building a row. All-ten selected/unselected row
builder coverage now guards that contract, alongside raster/font, clipping,
budget, palette and preview-only fixture tests.

Representative debug CPU edit+draw for200 expanded local-menu edits:

| Surface | Before median/p95 | Styled median/p95 |
| --- | ---: | ---: |
| Dot | 8.018/8.486 ms | 11.453/12.001 ms |
| Unframed | 4.818/5.389 ms | 5.265/5.785 ms |

Whole-frame styling adds work. This is not the earlier5.7ms short-input result,
not a GPU/compositor fence, and not proof of120Hz physical typing. Actual active
caret/scroll/focus/IME, dictation, VoiceOver and blur/backdrop remain separate.
The centered moving tree is a separate exploration, not implemented by this port.
