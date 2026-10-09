# Synthetic ctx retrieval probe

No production integration or execution. Same frozen 84 positive queries (28 literal, 56 paraphrase), eight unsupported requests and fixtures across requested backends. Larger corpus adds 972 synthetic workspace capabilities. Baseline is BM25 k1=1.2 b=0.75, no stemming/aliases; hybrid is untuned equal-weight RRF60.

Hardware: Apple M2 Ultra, macOS 26.6.2. Compute units are requested, not verified physical operation placement.

## Quality

| Requested backend | Candidates | Method | R@1 | R@5 | MRR | Unsupported nonempty shortlists |
|---|---:|---|---:|---:|---:|---:|
| ane | 28 | bm25 | 0.702 | 0.857 | 0.757 | 6/8 |
| ane | 28 | sparse | 0.929 | 1.000 | 0.962 | 8/8 |
| ane | 28 | hybrid_rrf60 | 0.833 | 0.940 | 0.883 | 8/8 |
| ane | 1000 | bm25 | 0.702 | 0.774 | 0.741 | 7/8 |
| ane | 1000 | sparse | 0.893 | 0.976 | 0.933 | 8/8 |
| ane | 1000 | hybrid_rrf60 | 0.774 | 0.833 | 0.814 | 8/8 |
| cpu | 28 | bm25 | 0.702 | 0.857 | 0.757 | 6/8 |
| cpu | 28 | sparse | 0.917 | 1.000 | 0.956 | 8/8 |
| cpu | 28 | hybrid_rrf60 | 0.833 | 0.940 | 0.883 | 8/8 |
| cpu | 1000 | bm25 | 0.702 | 0.774 | 0.741 | 7/8 |
| cpu | 1000 | sparse | 0.881 | 0.976 | 0.927 | 8/8 |
| cpu | 1000 | hybrid_rrf60 | 0.774 | 0.833 | 0.814 | 8/8 |

**gpu FAILED:** Invalid Evoke asset: Invalid sparse embedding for fixture.quartz.0: [3119: inf, 3116: inf, 3123: inf, 3120: inf, 3121: inf, 3122: inf, 3117: inf, 3118: inf]. No quality or latency claim is made for this backend.

## Warm query latency

Includes static eligibility filtering, tokenizer, input/output/weight transform and actual inverted-index retrieval (276 sequential queries per corpus). Compilation/download/index build excluded.

| Requested backend | Candidates | ML prediction median/p95 ms | Sparse end-to-end median/p95 ms | Hybrid end-to-end median/p95 ms | BM25 end-to-end median/p95 ms |
|---|---:|---:|---:|---:|---:|
| ane | 28 | 2.588 / 2.893 | 2.884 / 3.193 | 2.901 / 3.209 | 0.012 / 0.018 |
| ane | 1000 | 2.694 / 2.981 | 3.149 / 3.475 | 3.289 / 3.658 | 0.087 / 0.169 |
| cpu | 28 | 2.965 / 3.440 | 3.254 / 3.754 | 3.267 / 3.773 | 0.015 / 0.022 |
| cpu | 1000 | 2.987 / 3.503 | 3.460 / 3.955 | 3.606 / 4.097 | 0.090 / 0.172 |

## Startup, indexing and six-second idle

| Backend | Model compile/load ms | First encoding ms | After-idle encoding ms | Encode 1,000 documents ms | Truncated docs | Tokenizer parity cases |
|---|---:|---:|---:|---:|---:|---:|
| ane | 1252.8 | 9.879 | 26.111 | 3110.1 | 0 | 14 |
| cpu | 370.3 | 10.412 | 3.260 | 3510.2 | 0 | 14 |

## Independent shipped ONNX reference

Predeclared gates: term-set overlap >=0.90 and relative L2 weight error <=0.05. Tokenizer also matched all14 pinned HuggingFace cases.

- ane: 52 cases, minimum term overlap 0.9583, maximum relative L2 error 0.0265; 0 cases skipped for L64.
- cpu: 52 cases, minimum term overlap 0.9310, maximum relative L2 error 0.0298; 0 cases skipped for L64.

## Failure cases (CPU+Neural Engine request)

### 28 candidates

### 1000 candidates

- "remind me to buy groceries": target tasks.add; sparse fixture.birch.3, fixture.coral.3, fixture.hazel.3, fixture.slate.3, fixture.orchid.3; BM25 fixture.harbor.37, fixture.quartz.37, fixture.cedar.37, fixture.maple.37, fixture.orchid.37
- "find an existing feature ticket": target issues.search; sparse fixture.flint.30, fixture.slate.35, fixture.amber.30, fixture.larch.30, fixture.flint.35; BM25 issues.search, fixture.harbor.35, fixture.quartz.35, fixture.cedar.35, fixture.maple.35

## Limits

Synthetic English queries, one device and one run; not NFCorpus replication, user preference, keyboard-to-present or native UI proof. Literal labels are intentionally easy; paraphrase labels were fixed before scoring. Workspace distractors repeat templates, not 1,000 independent real capabilities. No tuning, reranker, strong lexical stemming/alias baseline, refusal calibration, Jev decision or grants. Prefix traces are sequential; async cancellation/stale-result handling remains untested. BM25 IDF uses whole corpus before identical eligibility filtering for all methods. Keep execution authorization and context freshness separate.
