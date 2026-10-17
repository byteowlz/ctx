# Thud evaluation: access gate and benchmark protocol

Status: **access blocked; no Thud inference measured**. Public-source check on
2026-10-14 for `trx-xv4n.18`. The user authorized evaluation, not native integration,
a replacement decision backend, continuous microphone capture or action execution.

## What is established

Mira's [announcement](https://trymira.com/blog/meet-thud), dated 2026-10-13,
describes configurable audio classification without an intermediate transcript,
category probabilities, local phone operation and option-order independence. Its
comparison reports medians of 107.6ms for Thud, 383.8ms for Qwen ASR plus Jev and
985.1ms for Gemini Live. Elsewhere it gives 114.8ms classification and a sub-300ms
demo. These are vendor claims with different or unspecified timing boundaries,
not our measurements. The page does not establish evaluation hardware, dataset,
accuracy, calibration, speech-end detection or reproducible benchmark code.

Restricting output to offered categories does not establish correct selection,
refusal or absence of harmful false positives. Model output is not execution
permission.

## Access check

- The announcement links no model checkpoint, inference SDK/API, model license or
  runnable evaluation package.
- The linked [Mira SDK page](https://trymira.com/pages/mira-sdk) describes building
  HTML apps inside the Mira iPhone application. It does not document a standalone
  Thud inference contract.
- A bounded public GitHub search did not identify an official Thud release in the
  results inspected. That is not proof of global absence.
- Hugging Face model searches returned gateway errors or timed out, and web
  search engines reported availability/rate-limit errors. These checks are
  incomplete; do not report an exhaustive registry search.
- No weights downloaded, dependencies installed, private endpoints probed,
  microphone enabled, requests sent to a Thud inference service or actions run.

To proceed, obtain an official release link or authorized evaluation API/SDK,
its license/terms, exact model/version, supported hardware, resource footprint,
audio format/window limits and documented category/probability/refusal contract.
Confirm any download, paid API use or service changes before starting them. Do
not reverse-engineer access controls or treat glasses ownership as API access.

Public contact listed by Mira: `team@trymira.com`. No outreach has been sent.

## Proposed bounded trial

Use synthetic or explicitly licensed audio with no private conversations. No
always-listening mode. Predictions select inert fixture IDs and produce preview
records only; no app opening, navigation of real interfaces or message sending.

Freeze fixtures, gold labels, offered descriptions, model IDs, seeds and audio
hashes before the comparison. Use identical audio and offered choices for:

1. Thud direct audio-to-choice.
2. An explicitly named ASR model followed by the existing Jev route.

First establish both access paths; a baseline result alone is not a Thud
comparison. Keep capability/context/platform scope identical, with an explicit
no-action/refusal category where the contract allows it. Preserve all offered
choices, rather than prefiltering them through embedding scores.

The first fixture set should cover:

- Clear commands for the synthetic ctx capability catalog and branch navigation.
- Unsupported commands, silence, background speech and environmental sounds.
- Ambiguous commands and self-corrections, such as a command followed by cancel.
- Speech directed at another person, quoted commands and non-command conversation.
- Noise, accents and a small declared multilingual subset.
- Several candidate-order permutations, reported separately from other changes.

Record authored/TTS cases as synthetic. Hold out voices and paraphrases where
possible; repeated clips and variants from the same utterance are not independent
samples. Use a separate calibration split for thresholds; never tune on the final
test set. This small trial cannot qualify general dictation or always-on use.

## Measurement and acceptance evidence

Measure cold startup, warm inference and actual transport/postprocessing
separately. For the ASR path also report transcription latency and errors. Define
latency both from the end of a fixed complete audio clip and from streaming
speech endpoint detection if supported. Neither excludes the duration of speaking
without saying so. Publish sample counts and median/p95, device/runtime/precision,
input duration, candidate count, concurrency and idle/co-resident conditions.

Report correct-choice rate, unsupported/no-command false activations, refusal
coverage, ambiguity/cancellation handling and order disagreement. If comparable
probabilities exist, report their normalization, Brier score/calibration and
threshold tradeoffs; otherwise say that calibration is unmeasured. Check finite
outputs, unknown IDs, malformed audio and window overflow explicitly. Limited
choices alone do not make incorrect confident answers safe.

Include cold/warm memory and idle load; sampled process RSS is not total device
memory or a guaranteed peak. Avoid claiming on-device privacy when audio is sent
to a remote endpoint, even on a trusted network. Honor existing resource
admission and service ownership; do not stop unrelated processes to improve a
benchmark without coordination.

A successful trial produces a reproducible source/version-bound report and
preview-only predictions. It does not authorize native integration, automatic
execution, continuous listening or replacement of Jev. Negative findings and
unavailable access are valid evaluation outcomes.

## Relationship to ctx

Thud is a potential audio decision backend, not a content embedding engine.
[vqtrs/EmbeddingGemma 2](https://github.com/byteowlz/vqtrs/blob/573c2f53cb3cc4b2c185f76b9150519ff0d921ef/docs/embeddinggemma2-parity.md)
remains the reuse-first candidate for optional multimodal content retrieval.
Embedding similarity is not a substitute Thud benchmark. Existing opt-in
[Evoke capability recall](local-retrieval.md), Jev refusal/routing and preview-only
behavior remain unchanged. Both classic list and proposed moving-branch views
can eventually present a validated voice selection; neither depends on adopting
Thud.

## Sources

- [Original announcement](https://x.com/caineardayfio/status/2110035727753126298).
- [Official Thud blog](https://trymira.com/blog/meet-thud).
- [Official Mira SDK page](https://trymira.com/pages/mira-sdk).
- [ctx local retrieval contract](local-retrieval.md).

Source inspection is not model execution or independent benchmark verification.
