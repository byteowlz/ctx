ctx

Objective

- Build a cross-platform (macOS → Wayland → Windows) Rust-based context-capture and automation tool that assembles maximum situational context for agent queries: system/app info, visible windows, screenshots, accessibility trees, clipboard (with consent), and optional actions.

Target users

- Developers and operators running local agents needing rich, up-to-date desktop context.
- Security/privacy-conscious users who need explicit control over captured scope and storage.

Priorities and rollout

- P0 macOS (richest AX; first-class permissions flow).
- P1 Wayland (portal-first: GNOME/KDE; Hyprland via compositor socket where available).
- P2 Windows (UIAutomation, Desktop Duplication/GDI).

Functional requirements

- Context capture CLI:
  - Frontmost app/window metadata; enumerate visible windows/apps.
  - System info: OS name/version, kernel, display layout, input locale.
  - Clipboard capture gated by opt-in and per-run flag; redactable modes (text only by default, image optional).
  - Screenshots: full display; optionally active window; encode for AI upload; warn on failures (permissions or compositor limits).
  - Accessibility trees: focused element path and selected attributes (role, label, value, enabled, frame); degrade gracefully if unavailable.
  - Action layer (opt-in): click/type/invoke via AX (macOS), UIA (Windows), RemoteDesktop/portal if enabled (Wayland); otherwise report unsupported.
- Configuration:
  - Use `config` crate; default config at `$XDG_CONFIG_HOME/<app>/config.toml` or `~/.config/<app>/config.toml`; Windows `%APPDATA%/<app>/config.toml`.
  - Example config shipped under `examples/config.toml` with commented defaults.
  - Data dirs: `$XDG_DATA_HOME` or `~/.local/share`; state dirs: `$XDG_STATE_HOME` or `~/.local/state`; Windows `%LOCALAPPDATA%`.
  - Settings include: provider keys, capture scopes (clipboard, accessibility depth), redaction rules, image quality, timeouts.
- Output:
  - Structured JSON blob for agent consumption (context sections: system, displays, apps/windows, accessibility, clipboard, screenshots references).
  - Human-readable summary for CLI logs.
- AI integration:
  - Pluggable providers (OpenAI/Anthropic/etc.); file upload or base64; streaming responses where supported.
  - Optional local redaction/blur before upload.
- Observability:
  - Structured logging; debug mode to emit platform checks (permissions, portal availability).
  - Telemetry optional and off by default.

Platform notes

- macOS: AX for frontmost/trees/actions; screenshots via CoreGraphics or `screencapture` fallback; permissions guidance (Screen Recording, Accessibility); clipboard via NSPasteboard.
- Wayland: prefer xdg-desktop-portal for screenshot/screencast and RemoteDesktop; compositor-specific IPC for window metadata (Hyprland socket); expect limited or no accessibility—document gaps; clipboard via portal or wl-clipboard where allowed.
- Windows: foreground/window enum via Win32; screenshots via GDI or Desktop Duplication; UIAutomation for trees/actions; clipboard via Win32 clipboard APIs.

Non-functional requirements

- Performance: single capture round-trip under 750 ms without screenshots; under 2 s with screenshots on modern hardware.
- Resilience: feature-flagged capabilities; clear error messages instead of panics when permissions/APIs missing.
- Security/privacy: minimal default scope (no clipboard/actions unless enabled); explicit consent prompts; avoid persistent secrets beyond config; no silent network calls.

Milestones

- M1 Foundations: workspace setup, config/state directories, logging, JSON output schema, stubbed platform trait (`PlatformContext`, `Screenshotter`, `AccessibilityAdapter`, `ClipboardProvider`).
- M2 macOS read-only: system/app/window info, clipboard opt-in, screenshots, focused accessibility snapshot; permission UX; `cargo check` baseline.
- M3 macOS actions: AX click/type/invoke guarded by flags; safety prompts.
- M4 Wayland read-only: portal-based screenshot/clipboard; compositor metadata (Hyprland IPC); graceful degradation messaging.
- M5 Windows read-only: foreground/window info, clipboard, screenshot via GDI; initial UIA element read.
- M6 Actions where possible: UIA invoke/value (Windows); Wayland RemoteDesktop if available; fallback notices elsewhere.
- M7 AI pipeline: provider abstraction, image/text upload, redaction/blur options; end-to-end CLI command (`peek context capture`).
- M8 Hardening: tests/mocks per platform module, benchmarks, docs and troubleshooting guides.

Risks and mitigations

- Wayland variability: mitigate with portal-first strategy and compositor-specific fallbacks; document unsupported paths.
- Permission friction (macOS/Windows): provide preflight checks and clear prompts; cache permission states in state dir.
- Accessibility fragility: narrow requested attributes; timeouts and depth limits; retries with trimmed scope.
- Privacy concerns: default to minimal capture; explicit flags for clipboard/screenshots/actions; local redaction pipeline.

Success criteria

- macOS capture (apps/windows/accessibility/clipboard/screenshot) works after granting permissions; JSON output consumable by agents.
- Wayland capture works on portal-enabled desktops; clear errors on unsupported compositors; clipboard and screenshots succeed where portals allow.
- Windows capture returns foreground/window info, clipboard, screenshot; UIA read of focused control on common apps.
- CLI command completes within target latencies and degrades gracefully without crashes.
