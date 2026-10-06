# Capture latency

Full `ctx` capture is not the omnibar's per-keystroke context source. Use the
lightweight current-state snapshot for routing; obtain pixels/accessibility only
when a selected capability needs them. The GPUI prototype currently uses synthetic
context, not a live desktop reporter.

## Measured bottleneck and fix

On one macOS 26 Retina desktop, content-free stage timing of a release build found:

| Screenshot stage | Before | Buffered output |
| --- | ---: | ---: |
| Display enumeration | 0.02 ms | 0.02 ms |
| Native acquisition | 60–63 ms | 64–90 ms |
| RGBA to RGB conversion | 11–14 ms | 12–14 ms |
| JPEG encoding and writing | 726–768 ms | 68–71 ms |

`image`'s JPEG encoder emits byte-sized writes. Sending those directly to
`File` made system calls dominate. `ctx-core/src/jpeg.rs` now buffers both
ordinary screen captures and bundle screenshots and explicitly flushes errors.
Image dimensions, JPEG quality clamping and capture defaults are unchanged. No
new image dependency or reduced-resolution default was introduced.

Three-run cold CLI samples, including startup/config/output, measured:

| Workload | Before median | After median |
| --- | ---: | ---: |
| Full capture | 1,911 ms | 202–267 ms |
| Screenshots, no accessibility | 1,347 ms | 188–308 ms |
| No screenshots, accessibility requested | 52 ms | 47–63 ms |
| No screenshots or accessibility | 45 ms | 47–62 ms |
| No-op provider | 7 ms | 7 ms |
| `ctx current`, populated synthetic snapshot | Not initially measured | 7 ms |

After values span repeated three-run group medians. Individual full/screenshot
samples still reached 610/962 ms: pixel capture is not a latency guarantee.
These are local samples, not an SLA. Host load and visible screen content vary.
Accessibility failed in these samples; its timings do not establish the cost of
successful traversal. Before/after binaries were optimized release builds. The
installed executable is not automatically replaced when building the fix. Another
process replaced the installed binary during this session; a later invocation
of that path is not the original baseline. The harness fingerprints the executable
before/after and refuses a run if it changes while benchmarking.

## Reproduce

```bash
just benchmark-capture
# Compare an existing installed binary; optional second argument is runs (1–30).
bun scripts/benchmark_capture.ts ~/.cargo/bin/ctx 3
cargo test -p ctx-core jpeg_
```

The benchmark emits timings, success counts and whether accessibility succeeded,
never app/window titles, clipboard contents, AX values or screenshot paths.
Screenshots are written into a private temporary directory and deleted on exit.
The current-file workload reads an explicitly populated synthetic snapshot in
that directory, not an empty fallback or the user's desktop state. The initially
measured default `ctx current` returned unavailable state (sequence zero); it was
not evidence of live focus or an active reporter.
The regression first failed with 20,955 underlying writes for 21,551 encoded
bytes; buffered output must use at most four writes on the same synthetic image.
Tests also check decoding, quality clamping and write/flush error propagation.
Temporary profiling instrumentation is removed from production code.

## Routing latency and freshness

- `ctx --no-screenshots --no-accessibility` is a live metadata-only capture, not
  a complete context capture. Missing focus/window data remains missing.
- `ctx current --json` reads reporter-maintained state. A fast read is **not**
  proof of fresh focus, selected text or browser context. Use its sequence/source
  and timestamp; explicitly handle unavailable or stale state.
- A persistent native client can read current state without spawning the CLI and
  refresh it from event reporters. That avoids cold startup and pixel encoding
  on the selection path. Automatic cross-platform focus/reporting is separate
  work; this fix does not implement it.
- Screenshot capture/encoding remains asynchronous and on demand. A selected
  action must revalidate its target before execution; model ranking and a timer
  do not authorize a capability.

Tracked under `trx-xv4n.1`; native omnibar work remains under `trx-xv4n`.
