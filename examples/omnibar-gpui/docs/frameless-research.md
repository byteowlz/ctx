# GPUI frameless window research

Checked 2026-10-07 against the installed `gpui-kit = 0.6.6`,
`gpui-pre-macos = 0.3.6` snapshot. This is source research, not proof that the
user's visible frame is fixed. User screenshots remain private/local.

## Finding

Three separate layers must be controlled:

1. App layout: avoid an outer painted gutter.
2. GPUI Component Root: `bordered(false)`, zero wrapper shadow, transparent
   background override; keep Root for input/overlay management.
3. Platform window/compositor: the previous two do not remove native chrome.

The macOS backend's `titlebar: None` branch explicitly creates
`NSTitledWindowMask | NSFullSizeContentViewWindowMask`. Popup adds the
nonactivating-panel flag; it does not remove the titled flag. It then hides the
native title and makes the titlebar transparent. Thus titlebar-none is hidden
chrome/full-size content, **not an AppKit borderless window**.

The snapshot's published metadata pins Zed revision
`bcf6582ce3500df93a8a39366640173e6786cea6`. The immutable implementation is:

- [macOS window creation, lines 992–1025](https://github.com/zed-industries/zed/blob/bcf6582ce3500df93a8a39366640173e6786cea6/crates/gpui_macos/src/window.rs#L992-L1025)
- [title hiding, lines 1177–1180](https://github.com/zed-industries/zed/blob/bcf6582ce3500df93a8a39366640173e6786cea6/crates/gpui_macos/src/window.rs#L1177-L1180)
- [existing borderless-fullscreen handling/focus restoration](https://github.com/zed-industries/zed/blob/bcf6582ce3500df93a8a39366640173e6786cea6/crates/gpui_macos/src/window.rs#L630-L654)

The current upstream main file fetched during research has the same creation
branch; do not assume a dependency upgrade alone fixes this.

Apple documents:

- [borderless](https://developer.apple.com/documentation/appkit/nswindow/stylemask-swift.struct/borderless): displays none of the usual peripheral elements.
- [fullSizeContentView](https://developer.apple.com/documentation/appkit/nswindow/stylemask-swift.struct/fullsizecontentview): respected only for windows with a titlebar; not a borderless style.
- [hasShadow](https://developer.apple.com/documentation/appkit/nswindow/hasshadow): separate native shadow property.

## Correct implementation route

Prefer a narrowly scoped GPUI platform option/patch for a **non-fullscreen
borderless window**, applied at creation. Use AppKit's borderless style without
`titled`/`fullSizeContentView`, retaining appropriate popup/nonactivating-panel
semantics. Expose native shadow policy separately and disable the host shadow
for this bar; the selected design may still paint its own intentional material.

Keep text input/key-window/first-responder handling, resize callbacks and explicit
programmatic expansion intact. GPUI's own fullscreen code warns that changing
style masks after creation can resign key status and first responder; blindly
changing masks at runtime may introduce an input regression. Fullscreen is not a
valid workaround for a compact omnibar.

The current `WindowOptions` surface has no explicit non-fullscreen borderless
mask/native-shadow switch for macOS. A small platform patch is preferable to
loosening the application's `unsafe_code = forbid` or sprinkling raw AppKit
handle calls through rendering. Do not silently change the default behavior of
all titlebar-none GPUI windows; use an explicit bounded opt-in.

## Implemented bounded native correction

The standalone workspace now patches only the published `gpui-pre-macos` 0.3.6
package locally. [Patch provenance](../vendor/gpui-pre-macos/CTX-PATCH.md) records
license, registry archive/source hashes and the exact two changed upstream files.
No global Cargo files or application unsafe policy are changed.

Only titlebar-none `PopUp` hosts get borderless creation and native shadow off;
normal/fullscreen/titlebar windows remain upstream behavior. The actual native
mask regression failed before correction and now reads raw mask **128**
(nonactivating-panel only), **hasShadow false**. Raw inspection is important:
Cocoa's convenience getter truncates the nonactivating bit. All-ten native scene/
Enter checks and 200 native input edits pass afterward; physical keyboard and
user/compositor screenshot acceptance remain open. This corrects the native
layer rather than hiding it behind a transparent root.

## Other platforms

- Linux: request `WindowDecorations::Client` and disable Root's CSD wrapper.
  The compositor may reject that request; native/compositor inspection is needed.
- Windows: the pinned backend's `WindowKind::PopUp` creates zero normal frame
  style flags, with tool-window/topmost extended styles. Native corner/shadow
  policy still needs Windows testing. `is_resizable = false` alone is not a
  universal definition of frameless.

Do not infer the user's OS from a screenshot. Confirm the failing platform and
executable revision before attributing the remaining visible border.

## Acceptance proof

Existing root-alpha regression only proves absence of app-painted surround.
Actual acceptance needs an app-owned OS/compositor window capture or observed
user screenshot with synthetic input: no native outline/titlebar/buttons/shadow,
correct focus and typed text, arrows/Enter/Escape, selection and IME, and working
progressive resize. Test both initial opening and design switching. No desktop
capture or private screenshot belongs in git. The separate text-input lag report
requires measured render/event timing, not a borderless assumption.
