# Voice-driven tree traversal (design intent, not implemented)

User direction, 2026-10-21. Captured so the pieces built now stay compatible;
nothing here is implemented yet. Tracker: see the voice epic under `trx-xv4n`.

## Intent

1. **Hands-free submit.** eaRS voice activity detection ends an utterance and
   submits it, optionally gated by a wake word and live speaker detection.
2. **Live traversal.** The final transcript goes to the decision model, which
   walks the moving tree level by level with per-choice probabilities.
3. **Spoken disambiguation.** When the top choice is below a confidence
   threshold, the bar shows the two most likely branches, each tagged with an
   easy-to-pronounce adjective-noun code (for example "copper whale" versus
   "silent tulip"). The user reads one aloud; eaRS transcribes it and ctx picks
   that branch.

## Constraints already recorded (AGENTS.md, voice section)

- Speaker evidence for the same completed utterance is required for voice
  *instructions* (eaRS `trx-2stp.4`); unknown, overlapping, stale or
  unavailable evidence dispatches nothing, with no ungated fallback.
- Interim/provisional transcripts never drive traversal; only finals do.
- Identity comes from voqx, never a ctx-local speaker registry.
- Navigation (choosing branches, reaching a preview) is not execution. Running
  a whitelisted action stays an explicit gesture unless the user decides
  otherwise; see open question 1.

## Design notes

- **Codes are matched deterministically first.** Compare the transcript with
  the two on-screen codes (normalized, with phonetic similarity) before asking
  the decision model. That is cheaper and auditable; the model is the fallback
  when the user says something else, which is then treated as a new request.
- **Codes must be acoustically distinct, not just different words:** different
  first phonemes, different stress and syllable counts, no near-homophones,
  drawn from a fixed, reviewed vocabulary per language (EN/DE first, matching
  eaRS's proven models). Codes are bound to the generation and node they were
  shown for; a late or stale reading selects nothing.
- **Threshold policy is per level**, recorded with each decision, and never
  turns low confidence into a guess: below threshold the bar always asks.
- **The visible tree is the state.** Every spoken step moves the same aperture
  the keyboard does, so voice and keys share Back, stale-safety and receipts.

## Open questions for the user

1. When voice traversal reaches a runnable leaf, does a spoken confirmation
   (with speaker evidence) run it, or does running stay a key/click gesture?
2. Wake word, push-to-talk, or both?
3. Threshold: one global value, or per catalog family?
