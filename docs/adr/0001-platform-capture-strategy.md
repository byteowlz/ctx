---
status: accepted
---

# Best-effort, per-platform capture with a shared normalized model

ctx must collect context (screenshots, selected text, page text, window
metadata, narration) across macOS and Linux, where the "magical" capture paths
rest on incompatible OS facilities with different permission models and very
different reliability. This ADR fixes how we absorb that reality without
fragmenting the codebase or the user's trust.

## Decision

Capture is **best-effort and cleanly separated by platform**, behind the
existing provider seam (`ctx-core::platform`). Concretely:

1. **One normalized capture model, many platform adapters.** Selection, Page
   Text, and window metadata each have a single cross-platform result type with
   explicit outcome variants (captured / none / unsupported / permission-denied
   / timeout). Adapters are macOS-first and Linux-first as separate
   implementations; there is no shared half-native abstraction that leaks one
   platform's model onto another.

2. **No intra-platform fragmentation.** Within a platform we commit to one
   capture path per capability (e.g. macOS Selection via AX, Linux via AT-SPI)
   rather than maintaining several partial ones. A capability is either
   supported on a platform or reports `unsupported`.

3. **Minimize permission surface.** We aim for as few OS permission prompts as
   possible per platform — ideally one consent moment that unlocks the platform's
   capture set — and we never cripple the rest of ctx to achieve it. Capture
   paths that require a permission the user has not granted report
   `permission-denied` and the session continues.

4. **Browser context via our own extension (or CDP).** Page Text and browser
   URL come from a ctx-authored browser extension / native-messaging host, or
   Chrome DevTools Protocol where cleaner. This is fair game to build and is not
   gated on OS accessibility APIs.

5. **Vision as the universal fallback.** Where native text extraction is absent
   (Wayland selection, non-instrumented browsers, arbitrary apps), a VLM/OCR
   pass over a Screenshot Capture is the portable fallback for recovering text.
   Native paths are preferred when available; vision fills the gaps rather than
   being the primary source.

6. **Best-Effort Capture is a hard contract.** No capture path may fail the
   surrounding capture, block past a bounded timeout, leak sensitive captured
   text into logs, or attach stale data to a newer context. Freshness is
   correlated to the current-context sequence / focused app+window.

## Context

- Primary platform now is macOS (foxline voice runtime and AX selection work
  there today); Linux is a committed first-class target, not an afterthought,
  and the author works primarily on Arch/Wayland.
- Selection capture is the least portable capability: macOS AX works, Windows
  UIA differs, Linux/Wayland AT-SPI is unreliable and often absent. See
  trx-grzk.1, which deliberately chose the clean AX path over the portable
  clipboard-synthesis hack.
- "Everything fits together perfectly" and "full cross-platform" are in tension.
  The resolution is to let the *magic* run end-to-end on the primary platform
  and let other platforms degrade to Screenshot + Transcript + vision-recovered
  text, never to a broken or crippled ctx.

## Consequences

- Each capability ships as `capture_<capability>` with a macOS impl and a Linux
  impl gated by `cfg(target_os)`, plus a `#[cfg(test)]` fake provider for the
  normalized outcomes.
- The current-context schema and full-capture schema share one representation
  per capability (no incompatible selected-text formats between `ctx --json`
  and `ctx current --json`).
- A VLM/OCR text-recovery pass is a first-class pipeline stage keyed off
  Screenshot Captures, reusing the OCR fallback work (trx-80md) and AI pipeline
  (trx-b5ak).
- Feature availability is queryable per platform so the Overlay can show what is
  and is not supported here, rather than presenting dead controls.
- Sensitive captures (Selection, Page Text) obey opt-in configuration and the
  privacy controls tracked in trx-qbcn.

## Considered options

- **Lowest-common-denominator cross-platform capture** (clipboard-synthesis for
  selection everywhere, screenshots only): rejected — portable but lossy and
  crippling on the primary platform where AX works cleanly.
- **One grand cross-platform accessibility abstraction**: rejected — leaks each
  OS's model onto the others and produces intra-platform fragmentation, the
  exact thing we are avoiding.
- **Vision-only capture** (screenshot + VLM for everything, skip native a11y):
  rejected as the default — great as a universal fallback, but throws away exact
  native text (Unicode, whitespace, URLs) when it is freely available, and costs
  latency/compute on the hot path.
