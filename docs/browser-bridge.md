# Explicit browser context (development)

`ctx browser` is a **read-only** consumer of the standalone brwsr bridge:
`brwsr/browser-bridge/`. Install/start that bridge and explicitly consent to a
tab in its Chrome toolbar action first.

```bash
ctx browser tabs
ctx browser read --tab-id 7 --document-id DOCUMENT --max-chars 4096
ctx browser --socket /private/runtime/bridge.sock tabs
```

Always JSON on stdout for runtime outcomes; non-success exits nonzero.
Only `tabs` and `read` are exposed. No grants, input, navigation, generic
command relay or remote listener is implemented in ctx.

An explicit target is required: tab ID **and Chrome document ID** from a fresh
list. The bridge independently checks owner consent, actual origin, document,
exact URL, expiry and revocation. ctx bounds replies to 256 KiB and IO to two
seconds by default (`--timeout-ms` is bounded to 100–30000).

Socket default: `$XDG_STATE_HOME/brwsr/bridge/bridge.sock`, otherwise
`~/.local/state/brwsr/bridge/bridge.sock`. If brwsr uses a custom runtime directory,
pass its socket explicitly.

Outcomes include `ok`, `truncated`, `permission_denied`, `stale`,
`unavailable` and `uncertain`. No automatic retry, implicit grant or arbitrary
JS is possible through this CLI. Request IDs and versions are verified on replies.

This implements explicit browser reads under ADR-0001 / `trx-43f0`.
It **does not** claim global OS focus, auto-publish browser events, overwrite
`ctx current`, attach results to a capture sequence or integrate ctx-bar/MCP.
Those automatic provider/freshness seams remain separate work. A browser-active
window is not proof of desktop focus.

## Evidence

- `cargo test -p ctx-cli browser::`: protocol/denial/correlation tests.
- `just check-all`: existing ctx workspace gates plus these tests.
- brwsr's opt-in synthetic Chrome proof can use `BRWSR_TEST_CTX=/path/to/ctx`:
  actual ctx binary → private broker → native messaging → extension → fixture page.

The synthetic browser proof uses a disposable profile and fixture host permissions.
Actual user gestures, installed personal profiles and desktop-focus matching are
not proven by those tests.
