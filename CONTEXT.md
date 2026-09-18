# ctx

ctx captures the context a person is working in — screenshots, selected text,
URLs, page text, active window/app, and narration — and organizes it into
reviewable, sendable bundles for handing off to an agent.

This document is the shared vocabulary. When a term here has an `_Avoid_` list,
do not use those alternatives in code, docs, commits, or issues.

## Language

**Capture Session**:
One continuous run in which the user narrates and collects context. A session
owns an ordered set of Sections and an always-on narration transcript. Starting
a session begins narration + (optionally) screen watching; ending it produces a
reviewable collection ready to export as a Bundle.
_Avoid_: recording, capture run, session (unqualified)

**Section**:
An ordered bucket inside a Capture Session that groups Captures the user wants
kept together. In the slide-deck workflow a Section maps 1:1 to a slide, but the
concept is workflow-agnostic (a section can be a quote line item, a bug report,
a research theme). Sections are what the user reorders in the Overlay.
_Avoid_: slide (in code/schema — slide is one presentation of a Section), group,
bucket, lane

**Capture**:
An atomic collected artifact that belongs to exactly one Section. Every Capture
is timestamped and provenance-stamped. Kinds: Screenshot, Transcript Span, URL,
Page Text, Selection.
_Avoid_: item (reserved for the existing Bundle `Item`), clip, snippet, artifact

**Screenshot Capture**:
A still image Capture, taken on-demand by the user or auto-clipped when the
screen changes substantially. The user can re-crop a Screenshot Capture before
export without re-taking it.
_Avoid_: snap, grab

**Transcript Span**:
A time-bounded slice of the session narration assigned to a Section. The
narration itself is one continuous stream (via eaRS); a Span is the portion of
it that belongs to a given Section.
_Avoid_: transcript chunk, segment (segment is reserved for auto-segmentation),
utterance

**Selection Capture**:
Text the user had deliberately selected at capture time, read via the
platform's accessibility stack (macOS AX, Windows UIA, Linux AT-SPI). Privacy-
sensitive and best-effort: it degrades to unavailable rather than failing the
rest of a capture. See trx-grzk.1.
_Avoid_: highlighted text, clipboard text (clipboard is a distinct, lossy path)

**Page Text**:
The readable text content of the web page the user is looking at, pulled by a
ctx browser extension (or CDP), independent of what is visually on screen.
_Avoid_: DOM dump, scrape, article text

**Overlay**:
The always-available, cross-platform GUI surface the user drives to run a
Capture Session: start/stop, take a Screenshot, mark section boundaries, and
review/reorder Sections and Captures via drag-and-drop before export.
_Avoid_: HUD, widget, popup, Overlayz (working spike name; not the product term)

**Bundle**:
The existing ctx export unit (`ctx-core::store::BundleStore`, manifest of
`Item`s). A Capture Session exports *into* a Bundle: Sections and Captures are
lowered to manifest Items with provenance. The Bundle is what goes to the agent.
_Avoid_: export, package, payload

**Section Boundary Signal**:
The live event that advances the "current Section" during a session, so
incoming Captures default into the right Section without post-hoc sorting.
Concretely a hotkey or a recognized voice phrase ("next slide"). See ADR-0002.
_Avoid_: slide change, cut, marker

**Substantial Change Detection**:
The heuristic that decides the on-screen content changed enough to warrant an
auto Screenshot Capture (perceptual hashing / frame diffing). An enhancement to
on-demand capture, never a prerequisite for a usable session. See ADR-0002.
_Avoid_: scene detection, keyframe, motion detection

**Best-Effort Capture**:
The contract that every platform-specific capture path (Selection, Page Text,
window metadata) either returns a structured result or a structured
unavailable/denied outcome — it never fails the surrounding capture and never
silently returns stale data. See ADR-0001.
_Avoid_: graceful degradation (say Best-Effort Capture), fallback
