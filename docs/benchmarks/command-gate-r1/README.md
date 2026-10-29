# Command-vs-dictation gate, r1 (2026-10-29)

Question: can a decision model tell a voice command from dictated text, fast
enough to sit between utterance completion and text insertion?

## Setup

- 102 synthetic utterances (`cases.py`): 45 commands, 57 dictation; English 58,
  German 44; groups `short` (1-4 words), `long`, `hard` (dictated imperatives
  such as "Please send me the report by Friday", or commands phrased as prose
  such as "it is too bright in here"), `quoted` (reported commands, dictation).
- Routes via the public EAVS ingress `https://eavs.has.skillissues.net`:
  `jev-latest` (TypeSafe hosted API, external) and `vllm-sr/d3-lite`
  (0.85B, on the 4090). Kev was unavailable (upstream 502).
- One binary choice question; each utterance with no context and with
  `focus = "email body"`. Sequential calls after 3 warm-ups; latency is end to
  end from the Mac Studio, including TLS and the gateway (~19 ms floor).
- Reproduce: `python3 -I bench.py OUTDIR && python3 -I report.py OUTDIR/results.jsonl`
  (reads the `eavs-fhgenie` virtual key from `~/.pi/agent/models.json`; never
  printed). Full numbers: `report.txt`.

## Results

| method | all | short | long | hard | quoted | p50 / p90 ms |
|---|---|---|---|---|---|---|
| always "dictation" | 55.9% | | | | | 0 |
| length <= 4 words | 58.8% | 52.2% | 50.0% | 71.4% | 100% | 0 |
| first-word verb list* | 87.3% | 100% | 78.6% | 66.7% | 100% | 0 |
| Jev, no context | **92.2%** | 95.7% | 92.9% | 81.0% | 100% | 168 / 245 |
| Jev, email focus | 69.6% | | | | | |
| d3-lite, no context | 80.4% | 89.1% | 92.9% | 52.4% | 57.1% | 119 / 189 |

\* The verb list was written while looking at this set, so it is optimistic.

Selective use (decide only when confident, otherwise treat as dictation):

- Jev at p >= 0.9 / <= 0.1: decided 69.6% of utterances with 0 errors here.
- d3-lite at p >= 0.97: decided 42.2% and still took 3 dictated sentences as
  commands ("Restart the server before the demo...", "Open the window if...",
  "Starte den Server vor der Demo neu..."). Overconfident on the hard group.

## Findings

1. **Length is not a signal** (58.8%, barely above always-dictation).
2. **Context dominates content.** With an email focus, Jev called almost every
   command dictation (p around 0.01). The same sentence is dictation in a
   message and a command in the bar; mode/focus decides more than wording.
3. **Jev is accurate and well calibrated but hosted**: as an always-on gate it
   would send every dictated sentence to an external provider, and it adds
   ~170 ms before each insertion.
4. **d3-lite is local and faster but unsafe as a gate**: confident mistakes on
   dictated imperatives. Not usable without a larger model or fine-tuning.
5. Small set (n=102), synthetic, one sitting, no audio/ASR errors. Treat as a
   direction, not a production threshold.

## Alias fast path (r1, same 102 utterances)

`ctx command` first tries the fixed catalog's EN/DE aliases, with a
command-shape check: at most 10 words, only catalog words plus filler, and no
conditional or reported-speech markers ("when", "if", "that", "wenn", "dass",
"gesagt", ...). Otherwise the utterance is prose and only the model may route it.

| rule | commands matched | dictation wrongly matched |
|---|---|---|
| contains-a-keyword (first attempt) | 17/45 (38%) | **6/57 (11%)** |
| command-shaped (shipped) | 7/45 (16%) | **0/57 (0%)** |

The false commands were conditional and quoted prose ("Mute your microphone
*when* you are not speaking", "She said turn on dark mode"). Recall is low
because the catalog is still small (no lock screen, screenshot, tree or agent
actions); growing the catalog, not loosening the shape rule, is the lever.
