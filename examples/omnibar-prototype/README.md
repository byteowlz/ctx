# Local Jev routing for ctx-bar

Loopback-only Bun service, not a browser interface or execution server. Synthetic
platform/context fixtures go to Jev through the local EAVS SystemOne route;
choosing an interface does not execute it. `catalog.json` is review data, not an
authoritative capability registry or proof of installed OS adapters.

From the ctx root, in separate terminals:

```sh
just omnibar-server
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

Configure **`$XDG_CONFIG_HOME/ctx/config.toml`**, fallback
**`~/.config/ctx/config.toml`**, alongside capture and native preferences:

```toml
[bar]
design = "dot-matrix"

[eavs]
endpoint = "http://127.0.0.1:3033/jev/v1/systemone"
model = "jev-latest"
profile = "your-existing-authorized-eavs-profile"
```

No second user configuration file is needed. Startup adds only a missing `[eavs]`
table, retaining other settings/comments and old files. The previous
`omnibar-prototype.toml` is a one-time read-only migration source (`eavs_profile`
becomes `profile`); existing `[eavs]` wins. Missing default macOS shared files also
import the old core platform config. New files are private0600; migration containing
credential values hardens the destination to0600 before appending.

Precedence: explicit `CTX_CONFIG` file > `CTX__EAVS__...` environment > local
`ctx.toml` > global shared file. Explicit legacy flat adapter files still load.
`CTX_EAVS__...`, `OMNIBAR_CONFIG`, and `OMNIBAR_EAVS_URL/MODEL/TOKEN/KEY/PROFILE`
remain compatibility aliases; canonical environment names win. An optional
`just omnibar-server PROFILE` supplies the legacy profile override, but the normal
command reads `[eavs].profile`. Shared explicit files override environment values.

Listener resolution is `[eavs].listener_token`, `EAVS_AUTH_TOKEN`, EAVS
`server.auth_token`, then `keys.master_key`. `[eavs].virtual_key` can supply a
private client key instead of the selected profile. `OMNIBAR_EAVS_CONFIG` and
`OMNIBAR_MODELS_FILE` select credential stores. Keep credentials private, never
in git. Native configuration selects only `[bar]` and never displays `[eavs]`.
The checked-in shared JSON schema covers both tables.

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
