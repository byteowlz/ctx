# Image/OCR context for the command gate, r2 (2026-10-29)

Question: does showing the model the screen (OCR text or the image itself)
change the dictation-vs-command decision for the better?

## Setup

- Two real screenshots of the user's MacBook (both terminal sessions with pi at
  its prompt: ctx and sldr), OCR'd locally with macOS Vision. Not stored here.
- 13 synthetic labelled utterances plausible in both sessions (7 commands,
  6 dictation, incl. "it is too bright in here" and "restart caddy").
- Arms: utterance only; + OCR text of screenshot A / B (1500 chars into
  `state.screen_text`); + the image itself (top-level `assets`, base64 jpg,
  `TypeSafe-Danger-Zone-Images: true`).
- Routes: Jev (hosted, via EAVS) and d3-lite (0.85B, 4090, via EAVS).

## Results

| arm | Jev | p50 ms | d3-lite | p50 ms |
|---|---|---|---|---|
| utterance only | **13/13** | 178 | 10/13 | 112 |
| + OCR text A | 12/13 | 205 | 7/13 | 221 |
| + OCR text B | 10/13 | 177 | 6/13 | 241 |
| + image A | HTTP 400 | - | HTTP 400 | - |
| + image B | HTTP 400 | - | HTTP 400 | - |

Image failures are not model verdicts:
- **d3-lite**: `unknown wire field assets`; it has no image support.
- **Jev**: `Images are experimental. Send the header TypeSafe-Danger-Zone-Images: true`
  even though that header was sent, so EAVS does not forward the experimental
  header to TypeSafe. Images are untestable through the gateway today.

## Findings

1. **Screen text did not help this decision; it hurt.** Jev stayed perfect
   without context and slipped to 12/13 and 10/13 with the sessions' OCR text,
   pushing dictation probabilities up ("I will retry with the password prompt"
   0.42 -> 0.69, "please send me the report by Friday" 0.38 -> 0.63). Agent
   session text reads like prose, which biases toward "dictation".
2. **Context cost latency**: d3-lite 112 -> 221/241 ms with 1500 characters of
   screen text, Jev 178 -> 205 ms.
3. **The useful context is structure, not prose**: editable focus, herdr pane
   agent/shell kind, OSC 7501 status. Those come from accessibility/herdr/pane
   state in milliseconds, not from OCR or a vision model.
4. Images remain worth testing for *routing* (which action/branch), not for
   dictation-vs-command; that needs the gateway to forward the header, or a
   local multimodal decision model (d3 27B) instead of d3-lite.

n=13, two contexts, one sitting; directional only.
