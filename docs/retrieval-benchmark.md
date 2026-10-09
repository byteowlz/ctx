# Local sparse retrieval experiment

Bounded headless probe, **not integrated into ctx-bar or Jev**. No desktop capture,
private documents, native windows, execution or grants. Retrieval only proposes
candidate IDs; it cannot refuse reliably or establish current context authority.

```sh
just validate-retrieval-fixtures             # fast, no model/download
just benchmark-retrieval /tmp/ctx-retrieval  # macOS, Swift/Bun/git/curl
```

The launcher downloads one ~58 MB Core ML L64 package into
`$XDG_CACHE_HOME/ctx/retrieval-probe` (`~/.cache` fallback), SHA-256 verifies every
asset, and compiles an optimized Swift CLI. No weights or third-party Swift source
are vendored. Cached modified source and unverified precompiled models are refused.
Normal app configuration remains the single `ctx/config.toml`; this is an explicit
experiment command, not another product configuration or service.

## Frozen inputs and correctness

- 28 existing synthetic capabilities, plus 972 template-derived synthetic
  workspace capabilities for a 1,000-candidate corpus.
- 84 labeled requests fixed before scoring: 28 exact labels, 56 manually written
  paraphrases. Eight deliberately unsupported requests; 14 keystroke prefixes.
- Same declared platform/context filtering for every retrieval method. This is
  fixture eligibility, not live focus or installed-capability verification.
- BM25 k1=1.2/b=0.75, no stemming/aliases; learned sparse dot via a real inverted
  index; fixed equal-weight reciprocal rank fusion with k=60. No result tuning.
- Native FluidUse tokenizer/manager at
  `fda0e480329150d47e60735ad80ad90c41f01b09` ([open PR28](https://github.com/FluidInference/FluidUse/pull/28)).
  Model snapshot `4ef29e090fd5a8480cc0830132d8011ac68dd71b`,
  [Granite-Embedding-30M-Sparse Core ML](https://huggingface.co/FluidInference/granite-embedding-30m-sparse-coreml).
  IBM authored the model; Fluid Inference converted it; pooling constants come
  from Intelligent Internet's Evoke P2.2. All Apache-2.0.
- 14 independently captured HuggingFace tokenizer cases match exactly. 52 shipped
  ONNX sparse reference cases pass predeclared overlap >=0.90/relative-L2 <=0.05
  gates: CPU+ANE minimum overlap0.9583/max error0.0265; CPU0.9310/0.0298.
- Primitive checks cover empty/missing lexical terms, sparse scoring/filtering,
  hybrid deduplication, selection eligibility and published pooling. Non-finite
  embeddings fail the backend rather than being silently ranked or sanitized.

The manager/model store shim disables upstream downloads; the checked-in launcher
alone installs pinned assets. Original inference/transform/tokenizer source is
compiled unchanged from the external clone. Tests do not require the rest of
FluidUse, its UI, Accessibility driver or form-filling dependencies.

## Measured result

Apple **M2 Ultra**, macOS26.6.2, optimized Swift. Requested `.cpuAndNeuralEngine`
compute units, **not physical per-operation placement proof**. Archived raw data,
fixture/runner hashes and failure records: [benchmarks/evoke-r1/](benchmarks/evoke-r1/).

| Corpus | Method | Recall@1 | Recall@5 | MRR |
|---|---|---:|---:|---:|
| 28 | BM25 | 70.2% | 85.7% | .757 |
| 28 | Evoke | 92.9% | 100% | .962 |
| 1,000 | BM25 | 70.2% | 77.4% | .741 |
| 1,000 | Evoke | 89.3% | 97.6% | .933 |
| 1,000 | untuned RRF60 hybrid | 77.4% | 83.3% | .814 |

All literal-label queries are easy and rank first for every method. On the harder
56 paraphrases in the 1,000 corpus, Recall@5 is **66.1% BM25 vs96.4% Evoke**.
The fixed hybrid hurts this fixture set; do not present it as an improvement.

Warm end-to-end sparse retrieval includes static eligibility filtering,
tokenization, prediction, weight conversion and actual index lookup/sort:
**3.149ms median /3.475ms p95** over1,000 candidates (276 sequential queries).
The Core ML prediction alone is2.694/2.981ms; BM25 end-to-end0.087/0.169ms.
CPU-only sparse is3.460/3.955ms and achieves the same97.6% Recall@5.

CPU+ANE first encoding9.9ms, encoding after six-second idle26.1ms; compiling/loading
1.25s, encoding1,000 documents3.11s. No document or query truncation in this suite.
These measurements are not M5 Pro results or keyboard-to-present latency. Index,
download and compilation are excluded from warm queries, not hidden as free work.

## Failures and decision

- Every unsupported request still produces sparse candidates (**8/8**); BM25 also
  produces nonempty lists for7/8 in the large corpus. Neither supplies a reliable
  refusal/intent decision. Jev and authorization remain separate.
- Sparse misses “remind me to buy groceries” (confuses purchase orders) and
  “find an existing feature ticket” (support-ticket distractors). BM25 rescues the
  latter. Fixed hybrid does not reliably combine their strengths.
- **CPU+GPU failed** with infinite embedding weights, at different synthetic
  documents across repeats. No GPU quality/latency claim; tracked in
  `trx-xv4n.14`. CPU and CPU+ANE passed validation. Do not silently enable GPU.

Conclusion: useful evidence for an **opt-in local shortlist experiment**, not a
production routing decision or permission to replace Jev. Next gate: test the
user's MacBook, a stronger lexical baseline and representative multilingual
capabilities; then background inference/cancellation, real typing/idle behavior,
and preservation of current native editing/selection. Do not run encoding on the
GPUI input/render critical path.

Limits: one device, synthetic English labels, repeated distractor templates,
no calibrated refusal, no async stale-query test, no live registry or NFCorpus
retrieval replication. The 52 reference cases check conversion numerics, not a
full dataset-quality claim. Prefix traces are sequential and not a UI benchmark.
