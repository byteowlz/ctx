# Optional local retrieval

ctx-bar can show local Evoke matches while Jev is pending. This is an opt-in
macOS Apple Silicon trial, not a replacement for Jev or an execution engine.
The current catalog/context remain synthetic; no private corpus is indexed.

## Enable

```bash
just install-retrieval
```

Then add to the existing `$XDG_CONFIG_HOME/ctx/config.toml` (fallback
`~/.config/ctx/config.toml`), without replacing its other tables:

```toml
[retrieval]
enabled = true
backend = "cpu"
```

Restart the adapter (`just omnibar-server`) and bar (`just omnibar-run`).
Installation needs Bun, Git, curl and Swift/Xcode command-line tools. The model
requires macOS 14+. Startup and typing never download assets. Disabled is the
shipped default; CPU-only is the initial recommended backend. `ane` is an
explicit experimental option; GPU is rejected because the benchmark produced
non-finite weights. Requested compute units do not prove hardware placement.

The installer uses the pinned Apache-2.0 FluidUse source and L64 fp16 CoreML
snapshot/checksums in `scripts/retrieval/assets.json`. Source, weights, installed
worker and licence stay in XDG cache, not git. The model/tokenizer are verified
before loading; unverified `.mlmodelc` overrides are refused. Run installation
again after updating the runtime/source pin. The index is an in-memory,
rebuildable cache of the current offered declarations, never authoritative data.

Optional absolute `worker_path` and `model_dir` override the normal
`ctx/retrieval-runtime/worker` and `ctx/retrieval-probe/model` cache paths.
Configuration is trusted local executable/asset selection, not an interface for
untrusted callers. Precedence: explicit `CTX_CONFIG` > canonical
`CTX__RETRIEVAL__*` environment > local `ctx.toml` > global shared file > defaults.
Unknown keys, GPU, wrong types and relative paths fail startup validation.
Core saves preserve this table; native preferences still select only `[bar]`.

## Behavior and boundaries

- The existing five-row viewport displays uncalibrated local matches. No
  probability or timer is attached; Enter/click only previews or navigates.
- Status distinguishes local/pending, unavailable, Jev and catalog browsing.
  Jev replaces provisional rows, including clearing them on refusal/error.
- **F2** explicitly browses every offered capability in the current
  platform/context/branch scope; Up/Down scroll, Enter previews. This invalidates
  pending results and never starts a timeout. On macOS, function-key preferences
  may require Fn+F2. Browsing is not an inferred answer or a grant.
- Jev receives its original complete offered set. Five local matches never
  hard-filter its choices; unsupported requests can still retrieve neighbors.
- Edits, scope changes, Back, Escape and previews reject stale results. Pointer
  selection binds generation and identity, not merely a changing row index.
  Navigation cancels timers; later Jev responses retain a selected identity when
  present and do not rearm after intervention. A redundant native Change emitted
  after Enter does not reopen the completed preview.
- Empty input and `ctx theme` commands bypass both paths; idle remains only the
  input. Existing native editing, selection, IME/fallback and framing are intact.
- Queries/documents beyond the installed 64-token model budget fail explicitly
  rather than silently returning results from truncated declarations. Jev is
  unaffected. Native 512-character and 128-candidate protocol bounds remain.

The adapter owns a persistent Swift process with a single in-flight prediction,
no unbounded queue, bounded NDJSON input/output, 10-second startup and 3-second
prediction deadlines. It excludes provider credentials from the helper's
environment, ignores private helper diagnostics and terminates it on shutdown.
The same private descriptor/Origin/key guards protect `/api/retrieve`; local
requests have a separate two-request/600-per-minute cap from Jev. Native requests
coalesce at 35 ms, independently of the existing Jev debounce/busy flag, on
background executors. Old CoreML calls are not preemptively canceled on each
keystroke: completion is discarded, then only the latest pending generation is
submitted. A hard deadline terminates a stuck helper; restart the adapter to
recover. Model startup/indexing can lose the race to Jev without delaying it.

## Verification

```bash
just omnibar-check
just test-retrieval-runtime       # installed CPU helper; no download
just check-all
# Explicit hidden native real-transport proof; fixed synthetic request only:
cd examples/omnibar-gpui
cargo run -p ctx-bar --features native-review --example retrieval_live -- /tmp/ctx-local-proof
just review-surfaces /tmp/ctx-local-surfaces
```

Owner tests cover local/remote races, refusal clearing, timer suppression,
identity-bound clicks, expansion beyond five, branch/context/platform isolation,
configuration precedence, disabled/unavailable installation and token budgets.
Real CPU worker tests retrieved light mode/brightness and reused the process.
The hidden native proof observed local rows, actual Jev, F2 browsing 25 eligible
entries and native Down/Enter preview beyond the first five. It caught and fixed
an unchanged-text Change event reopening the preview after native Enter.
306 synthetic dark/light all-ten surface captures passed the incumbent visible
matrix/enclosure/focus-line checks. Post-change expanded local-theme CPU
edit+draw was 11.599 ms median/12.030 ms p95 for Dot, 5.472/5.936 ms for Unframed;
this control bypasses retrieval and is not the local retrieval latency.
A warmed hidden native local-result observation was about 58 ms including
35 ms debounce/polling, versus about 774 ms for real Jev in that run. These are
single synthetic observations, not physical keyboard-to-present or GPU timings.

See [benchmark evidence](retrieval-benchmark.md) for quality limits: synthetic
English catalog/basic BM25/one device; real catalog, stronger lexical baseline,
multilingual, MacBook, physical typing, VoiceOver and other platforms remain
unverified. No execution, grants or live-context reporters were added.
