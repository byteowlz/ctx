# Scoped ctx native-host patch

Vendored `gpui-pre-macos` **0.3.6**, Apache-2.0 (adjacent LICENSE-APACHE).
Source: crates.io published package, Zed snapshot
`bcf6582ce3500df93a8a39366640173e6786cea6`. This standalone omnibar workspace
uses a local `[patch.crates-io]`; no global Cargo/cache/framework files change.

## Delta

- `src/window.rs`: only `WindowKind::PopUp` **and** `titlebar: None` select the
  true AppKit borderless style **at creation**. Existing popup construction then
  adds its nonactivating-panel flag. Other window/titlebar modes are unchanged.
- Disable the host's native shadow for those popups. Bar designs may retain their
  own intentional painted material/radius; no native container radius is requested.
- Assert actual native properties after creation: raw style mask is `128`
  (nonactivating only; no titled/full-size/resize/close/minimize flags) and native
  `hasShadow` is false. Cocoa's convenience getter truncates unknown mask bits,
  so the regression reads the raw native property via the existing platform FFI.
- `src/gpui_macos.rs`: allow only preexisting upstream Cocoa deprecation warnings.
  Published dependency builds cap these; local path builds would otherwise emit
  1,213 inherited warnings. Application warnings-denied gates stay unchanged.

No new application unsafe code; its `unsafe_code = forbid` remains intact.
Do not mutate the mask during rendering or design switching: that can resign
key status/first responder. Existing NSPanel subclass/input/resize handling stays
in place, and initial first-responder setup follows native construction as before.

## Proof

A real hidden native popup failed the titled-mask regression before the delta.
Afterward its debug record is `borderless popup style=128, shadow=false`.
All-ten scene/Enter review and 200 native input edits pass; Dot median about
5.43 ms and Unicode/composition/selection fallback remains intact. Checks,
65 native test invocations and warnings-denied application Clippy pass.

These properties verify the native host, unlike app-scene alpha alone. User
on-screen visual/physical-keyboard acceptance remains separate. Never commit
private desktop captures. The exact registry/source hashes and changed files
are recorded in `provenance.json`.

Keep this patch bounded. Re-evaluate against an upstream non-fullscreen borderless
API when available; do not silently broaden behavior to all native windows.
