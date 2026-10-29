# Focus and context signals (measured on the MacBook, 2026-10-29)

Read-only probes of a live MacBook (macOS 15.7, Ghostty 1.4.0, herdr 0.9.5,
pi). No focus changes, no window contents, no AX values read. This is the
evidence behind the dictation-vs-command gate and the "which command" context;
it is a measurement record, not an implemented feature.

## macOS accessibility: what the focused element reports

| context | AX role | value settable | notes |
|---|---|---|---|
| Ghostty terminal | `AXTextArea` | **false** | `AXHelp = "Terminal content area"` |
| Outlook | `AXTextField` | **true** (31 fields) | |
| ChatGPT app | `AXTextArea` | **true** | |
| Zen browser | `AXComboBox` | **true** (address bar) | page fields need AX mode |
| Finder | `AXGroup` | false | not a text target |
| Steam | `AXWindow` | false | not a text target |

So "is the caret in a text box" needs **role + settable**, not role alone: a
terminal is an `AXTextArea` whose value is not settable. That pair separates
real text targets from terminals without reading any text.

Caveats measured the same day:

- **Chromium/Electron apps expose nothing** until an accessibility client asks
  them to enable AX: Teams, WhatsApp, Telegram, Codex, Electron apps and
  Outlook's message list returned `-25212` (no focused element). Page fields in
  a browser behave the same. Treat "no focused element" as **unknown**, which
  must default to dictation, never to command.
- `AXFocusedUIElement` on the system-wide element returned `-25204` in this
  environment; asking the **frontmost application's** element (same PID as
  `NSWorkspace.frontmostApplication`) worked.
- A spot check while the screen was locked reported `loginwindow`; the OS
  reports that as the frontmost app, so lock state must be treated as "no
  input target".

## Terminals: which pane, and is it an agent?

Ghostty exposes no TTY or process for its terminal view (attributes:
`AXRole`, `AXSelectedTextRange`, `AXNumberOfCharacters`, `AXValue`,
`AXHelp`, frame/position, children; no identifier/document/process). The
focused window title does not identify the pane.

herdr does know, via `herdr api snapshot`:

`agents[]` and `panes[]`: `pane_id`, `tab_id`, `workspace_id`, `agent`,
`agent_session`, `agent_status`, `focused`, `cwd`, `foreground_cwd`,
`terminal_id`, `terminal_title`; plus `focused_pane_id`, `focused_tab_id`,
`focused_workspace_id`. A pane's `agent` empty means a plain shell.

**Missing bit:** herdr does not report whether its own terminal window is the
focused one. Two cheap fixes: herdr exposes client focus (terminals can report
focus-in/out to the program), or it sets a recognizable window title.

## Program status (OSC 7501)

- **pi emits it natively**: `program-status-reporter` sends `working` during
  runs and compaction, `blocked` while a dialog waits, then `done`/`error`, with
  feature detection.
- **Ghostty 1.4 implements it**: the binary contains the parser and the reply
  `states=idle,working,done,blocked,error:kinds=permission,question,auth`.
- **herdr 0.9.5 does not** (no trace in the binary); it still guesses agent
  status by watching the screen (`agent_status`, `screen_detection_skipped`).

This is the cleanest "what is the program waiting for" signal: `idle` and
`blocked/question` mean dictation is the right input; `blocked/permission` and
`blocked/auth` must never be auto-answered by voice; `working` means draft, not
submit.

## `ctx current` on this MacBook

The state file was empty (never written): no reporter is installed. ctx does not
poll; `examples/{herdr,tmux,aerospace,zellij}-current-context.*` are opt-in.
Two defects found: a missing file is reported with a **fresh `updated_at`**
(reads like a live empty report), and the config uses `sessions.json` while the
README documents `current-context.json`.

## Screen content as context

Measured separately (`command-gate-r1/image-context-r2.md`): OCR'd screen text
made the gate slightly worse and slower; images cannot be tested through EAVS
(it drops the TypeSafe danger-zone header) and d3-lite has no image support.
Structure (the signals above) is the context worth having, not OCR prose.

## What this means for the gate

| focus | decision |
|---|---|
| editable text target | dictation, typed immediately |
| terminal, herdr pane with an agent | dictation to the agent, never auto-submitted |
| terminal, shell or unknown | dictation, plus a "run as command?" chip; never auto-Enter |
| no text target | command, resolved by ctx |
| unresolvable (Electron, locked, unknown) | dictation |
