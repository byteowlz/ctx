---
status: proposed
---

# Live boundary signal as the spine, drag as correction, AI as assistant

A Capture Session produces a *continuous* narration stream and *event-based*
Screenshot Captures, but the reviewable output is *discrete* Sections. Something
must decide which narration and which screenshots belong to which Section. This
is the load-bearing decision of the whole product: it determines whether the
hard, least-portable pieces (Substantial Change Detection, AI segmentation) are
required for a usable v1 or are optional enhancements.

## Decision (PROPOSED — to be confirmed by Tommy)

**The spine is a live Section Boundary Signal. Drag-and-drop is correction. AI
is an advisory assistant that proposes but never owns.**

1. **Live default.** Every Capture is stamped with the *current Section* at the
   moment it is captured. The user advances the current Section with a Section
   Boundary Signal — a hotkey, or a recognized narration phrase ("next slide").
   Narration flowing in after the signal forms the next Section's Transcript
   Span. This keeps the human in the driver seat with near-zero friction,
   because narrating a deck naturally includes "okay, next slide."

2. **Drag as correction, not sorting.** Because every Capture is also
   timestamped, the Overlay lets the user drag any Capture between Sections and
   reorder Sections after the fact. The live signal exists so the user is
   *correcting a good default*, never sorting an undifferentiated pile.

3. **AI as assistant.** A "propose sections" action can re-bucket a session by
   clustering narration + screenshots (optionally VLM-assisted). It is never the
   default, never blocking, and always lands as an editable proposal the user
   accepts or discards.

## Context

- Design north star: "human in the driver seat, AI taking care of the work."
- The three candidate mechanisms are: (1) live boundary signal, (2) purely
  post-hoc manual drag, (3) AI segmentation. "All three as modes" is a
  non-decision; one must be the spine.
- Pure post-hoc (2) makes the Overlay a manual sorting UI and discards the
  session's temporal structure. Pure AI (3) puts a fallible model in the hot
  loop and fights the driver-seat principle. The live signal (1) is
  deterministic, dependency-free, and matches how the author already talks
  through a deck.

## Consequences

- **v1 does not require the two hardest pieces.** With a live signal + drag, a
  session is useful using only on-demand Screenshot Captures and eaRS narration.
  Substantial Change Detection (auto-clip) becomes an enhancement to on-demand
  capture, and AI segmentation becomes a later assist. Both can be deferred
  without blocking a shippable Overlay.
- The Section Boundary Signal needs two triggers: a global hotkey (always
  available) and a narration-phrase recognizer (reads the eaRS transcript for a
  configurable phrase). The hotkey ships first; phrase recognition is additive.
- Captures carry `{ section_id, captured_at }`; Sections carry an explicit
  order. The data model must let a Capture be reassigned to any Section without
  re-capture (already required for re-cropping Screenshot Captures).
- Export lowers Sections → Bundle structure in the user-corrected order, not the
  raw capture order.

## Considered options

- **Post-hoc manual drag as the spine**: rejected — throws away temporal
  structure the session already has, and makes the common case (things arrive in
  roughly slide order) as much work as the worst case.
- **AI segmentation as the spine**: rejected as the default — violates the
  driver-seat principle, adds a hot-loop model dependency, and makes v1 depend
  on getting clustering right. Kept as an opt-in assistant.
- **No sections, one flat timeline exported whole**: rejected — the slide-deck
  use case is inherently sectioned; a flat dump pushes all organizing work onto
  the agent and the user's later editing.

## Open questions for confirmation

- Is the hotkey-first Section Boundary Signal acceptable, or must the voice
  phrase ship in the same v1 (implies eaRS transcript is in the hot loop from
  day one)?
- Does a Capture ever legitimately belong to *no* Section (a session-level
  "unsorted" tray), or is there always a current Section?
- Should the live signal also *start a new Screenshot Capture* (i.e. is "next
  slide" also a capture trigger), or purely a boundary?
