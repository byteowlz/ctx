# Local Jev adapter for the ctx GPUI prototype

Loopback-only Bun service, not a browser interface or execution server. Synthetic
platform/context fixtures go to Jev through the local EAVS SystemOne route;
choosing an interface does not execute it. `catalog.json` is review data, not an
authoritative capability registry or proof of installed OS adapters.

From the ctx root, in separate terminals:

```sh
just omnibar-server your-existing-authorized-eavs-profile
just omnibar-run
```

The native command opens a focus-taking window; agent verification did not launch
one. Use the profile name already present in the local agent models file, not an
upstream provider secret. The profile name is only a credential selector: it does
not choose or start that profile's model/provider. All decisions still use Jev;
no fhgenie server is started or required. EAVS must already be running. This
separate ctx adapter is temporary prototype plumbing, not a required extra service
for the final native app. The backend reads listener auth from EAVS and the
explicitly selected virtual key. Nothing is printed or sent to the native client
except an ephemeral review capability in an owner-only state descriptor.

Config: `$XDG_CONFIG_HOME/ctx/omnibar-prototype.toml`, fallback
`~/.config/ctx/omnibar-prototype.toml`; defaults are created on first run.
`OMNIBAR_CONFIG` selects another file. Environment overrides file values:
`OMNIBAR_EAVS_URL`, `OMNIBAR_EAVS_MODEL`, `OMNIBAR_EAVS_TOKEN`,
`OMNIBAR_EAVS_KEY`, `OMNIBAR_EAVS_PROFILE`. Existing listener resolution is
`EAVS_AUTH_TOKEN`, EAVS `server.auth_token`, then `keys.master_key`.
`OMNIBAR_EAVS_CONFIG` and `OMNIBAR_MODELS_FILE` select local credential sources.
Keep all credentials in private local stores, never this directory.

The native prototype expects `http://127.0.0.1:4784`. Upstream must be the local
IPv4 EAVS listener, normally `http://127.0.0.1:3033/jev/v1/systemone` with model
`jev-latest`. `jev` is the provider path, not part of the model ID.
The adapter writes `$XDG_STATE_HOME/ctx/omnibar-prototype.json` (fallback
`~/.local/state/ctx/omnibar-prototype.json`) atomically with permissions 0600 and
removes its own descriptor on shutdown. EAVS credentials never enter this file.

## Decision contract

- `GET /api/catalog`: synthetic item/branch metadata only.
- `POST /api/suggest`: `{query,platform,fixture,presentation,node}`; defaults are
  `presentation = "flat"`, `node = "root"`. Origin and private review key required.
- Platforms: `macos`, `windows`, `linux`, `omarchy`; context fixtures: `desktop`,
  `selection`, `audio`, `unavailable`. Typed input is explicitly sent to Jev;
  no screenshot, AX, clipboard or live focused-app state is collected.
- Flat returns offered leaves. Branches root groups appearance/sound/display/
  network; selecting a declared group makes **one new decision**, with only its
  children. No speculative child decisions or precomputed tree; maximum two levels.
- Response: `{items,source,latencyMs,fixtureOnly}`. Unknown IDs, invalid/omitted
  probabilities, wrong branch scope and invalid JSON become errors, not fabricated
  fallback results. REFUSE becomes no-match.

Input is limited to 4 KB / 512 Unicode scalar values; upstream responses to 128 KB.
Two requests can be active; the adapter accepts at most 12 requests/minute. Reads
and upstream calls have deadlines. No CORS, arbitrary file serving, proxy redirects,
OS execution, approval escalation or model-controlled commands are provided.
Timeout in the native list selects its first item only; it grants no permission.

## Verification

```sh
just omnibar-check
```

Native tests cover selection, generation/timer cancellation and transport/catalog
validation. Adapter tests cover context gating, actual choice envelopes, branch
scope, lazy hops, malformed/oversize bodies, concurrency and credential isolation.
A live synthetic `light mode` probe returned HTTP 200 for flat selection, branch
root and child selection. This is transport proof, not native visual, dictation,
accessibility, OS-adapter or cross-platform runtime proof. See the native
prototype's `DESIGN.md` and `README.md` for the remaining verification boundary.
