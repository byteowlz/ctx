import { createHash } from "node:crypto";
import { join } from "node:path";
const dir = Bun.argv[2];
if (!dir) throw new Error("Result directory required");
const fixture = await Bun.file(join(dir, "fixtures.json")).json();
const { fixtureSha256, ...fixtureData } = fixture;
if (createHash("sha256").update(JSON.stringify(fixtureData)).digest("hex") !== fixtureSha256) throw new Error("Frozen fixture contents changed");
const attempts = await Promise.all(["ane", "gpu", "cpu"].map(async backend => ({ backend, ...await Bun.file(join(dir, `${backend}.json`)).json() })));
const results = attempts.filter(row => row.status !== "failed");
const failures = attempts.filter(row => row.status === "failed");
if (!results.length) throw new Error("No usable encoder backend");
if (results.some(row => row.fixture_sha256 !== fixture.fixtureSha256)) throw new Error("Fixture identity mismatch");
const provenance = {
  fluiduse_revision: "fda0e480329150d47e60735ad80ad90c41f01b09", fluiduse_pr: "https://github.com/FluidInference/FluidUse/pull/28",
  model_revision: "4ef29e090fd5a8480cc0830132d8011ac68dd71b", model_repository: "FluidInference/granite-embedding-30m-sparse-coreml", model_license: "Apache-2.0",
  fixture_sha256: fixture.fixtureSha256,
  runner_sha256: createHash("sha256").update(await Bun.file(new URL("Probe.swift", import.meta.url)).text()).digest("hex"),
  corpus_sha256: createHash("sha256").update(await Bun.file(new URL("../../examples/omnibar-prototype/catalog.json", import.meta.url)).text()).digest("hex"),
  host: { architecture: process.arch, platform: process.platform, hardware: Bun.spawnSync(["sysctl", "-n", "machdep.cpu.brand_string"]).stdout.toString().trim(), os: Bun.spawnSync(["sw_vers", "-productVersion"]).stdout.toString().trim() },
};
let text = "# Synthetic ctx retrieval probe\n\nNo production integration or execution. Same frozen 84 positive queries (28 literal, 56 paraphrase), eight unsupported requests and fixtures across requested backends. Larger corpus adds 972 synthetic workspace capabilities. Baseline is BM25 k1=1.2 b=0.75, no stemming/aliases; hybrid is untuned equal-weight RRF60.\n\n";
text += `Hardware: ${provenance.host.hardware}, macOS ${provenance.host.os}. Compute units are requested, not verified physical operation placement.\n\n`;
text += "## Quality\n\n| Requested backend | Candidates | Method | R@1 | R@5 | MRR | Unsupported nonempty shortlists |\n|---|---:|---|---:|---:|---:|---:|\n";
for (const r of results) for (const corpus of r.corpora) for (const method of ["bm25", "sparse", "hybrid_rrf60"]) {
  const q = corpus.quality[method];
  text += `| ${r.backend} | ${corpus.size} | ${method} | ${q.recall_at_1.toFixed(3)} | ${q.recall_at_5.toFixed(3)} | ${q.mrr.toFixed(3)} | ${q.no_match_nonempty_shortlists}/${q.no_match_queries} |\n`;
}
for (const failure of failures) text += `\n**${failure.backend} FAILED:** ${failure.error}. No quality or latency claim is made for this backend.\n`;
text += "\n## Warm query latency\n\nIncludes static eligibility filtering, tokenizer, input/output/weight transform and actual inverted-index retrieval (276 sequential queries per corpus). Compilation/download/index build excluded.\n\n| Requested backend | Candidates | ML prediction median/p95 ms | Sparse end-to-end median/p95 ms | Hybrid end-to-end median/p95 ms | BM25 end-to-end median/p95 ms |\n|---|---:|---:|---:|---:|---:|\n";
const pair = (v: any) => `${v.median.toFixed(3)} / ${v.p95.toFixed(3)}`;
for (const r of results) for (const c of r.corpora) text += `| ${r.backend} | ${c.size} | ${pair(c.latency_ms.prediction)} | ${pair(c.latency_ms.sparse_end_to_end)} | ${pair(c.latency_ms.hybrid_end_to_end)} | ${pair(c.latency_ms.bm25_end_to_end)} |\n`;
text += "\n## Startup, indexing and six-second idle\n\n| Backend | Model compile/load ms | First encoding ms | After-idle encoding ms | Encode 1,000 documents ms | Truncated docs | Tokenizer parity cases |\n|---|---:|---:|---:|---:|---:|---:|\n";
for (const r of results) text += `| ${r.backend} | ${r.model_load_ms.toFixed(1)} | ${r.first_encoding_ms.toFixed(3)} | ${r.idle_encoding_ms.toFixed(3)} | ${r.document_encoding_1000_ms.toFixed(1)} | ${r.documents_truncated.length} | ${r.tokenizer_parity_cases} |\n`;
text += "\n## Independent shipped ONNX reference\n\nPredeclared gates: term-set overlap >=0.90 and relative L2 weight error <=0.05. Tokenizer also matched all14 pinned HuggingFace cases.\n\n";
for (const r of results) text += `- ${r.backend}: ${r.onnx_reference_parity.cases} cases, minimum term overlap ${r.onnx_reference_parity.min_term_overlap.toFixed(4)}, maximum relative L2 error ${r.onnx_reference_parity.max_relative_l2_error.toFixed(4)}; ${r.onnx_reference_parity.skipped_over64} cases skipped for L64.\n`;
text += "\n## Failure cases (CPU+Neural Engine request)\n\n";
for (const c of (results.find(row => row.backend === "ane") ?? results[0]).corpora) {
  text += `### ${c.size} candidates\n\n`;
  for (const q of c.queries.filter((q: any) => q.relevant.length && !q.results.sparse.some((id: string) => q.relevant.includes(id))).slice(0, 12)) text += `- ${JSON.stringify(q.query)}: target ${q.relevant.join(", ")}; sparse ${q.results.sparse.join(", ")}; BM25 ${q.results.bm25.join(", ")}\n`;
}
text += "\n## Limits\n\nSynthetic English queries, one device and one run; not NFCorpus replication, user preference, keyboard-to-present or native UI proof. Literal labels are intentionally easy; paraphrase labels were fixed before scoring. Workspace distractors repeat templates, not 1,000 independent real capabilities. No tuning, reranker, strong lexical stemming/alias baseline, refusal calibration, Jev decision or grants. Prefix traces are sequential; async cancellation/stale-result handling remains untested. BM25 IDF uses whole corpus before identical eligibility filtering for all methods. Keep execution authorization and context freshness separate.\n";
await Bun.write(join(dir, "provenance.json"), JSON.stringify(provenance, null, 2) + "\n");
await Bun.write(join(dir, "report.md"), text);
console.log(text);
