# Native surface scope

- Product/surface: `ctx/omnibar`; variant: modern glass; revision: 1.
- Approval: exploratory implementation only, not visually approved.
- Mode: Operate. One anchored input and a compact list, no chat, sidebar,
  dashboard, expanded tree or multipanel composition.
- Studio record/source: `design-studio-ctx-omnibar/tokens.toml`, Lumen dark/light
  schemes. Snapshot hash and template revision: design crate
  `schemes/PROVENANCE.md`. SystemUIFont is a native platform font exception to
  the Studio SF Pro Text family; other-platform font fallback needs review.
- Material: role-derived translucent background and controls plus native
  `WindowBackgroundAppearance::Blurred`. This is not a claim of true Apple
  Liquid Glass. OS blur appearance/support remains unverified without launch.
- Tinycast supplied principles only: compact panel, input anchor, gentle
  highlight, blur/tint main surface and glass controls. No AGPL source copied.
- Input, stale/loading/error/no-match behavior and permissions take precedence
  over material. Probabilities rank suggestions, never grant authority.
- The Flat/Branches selector is an A/B experiment, not production approval.
- Greyrot shader treatment is deferred and not implemented.

## Direction contract

THESIS: Text to bounded tool/interface choices, never conversation or automatic
execution. OWN-WORLD: Studio Lumen role/base24 seam with native translucent tint.
STORY: Type or system-dictate, inspect highlighted interfaces, choose a preview.
FIRST VIEWPORT: Input at the top, small platform/fixture/A/B controls, then a
scrollable list and a quiet timeout/keyboard footer. FORM: User-pinned compact
native panel, not a concept tournament. FINISH: Build and pure contract tests
only in this task; real native screenshots, focus/input/dictation/accessibility
checks and Studio actual-render comparison remain required before visual
acceptance. No screenshot or blur-fidelity evidence is claimed.
