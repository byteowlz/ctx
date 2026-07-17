#!/usr/bin/env bash
set -euo pipefail

# Example AeroSpace hook script for reporting the focused macOS workspace/window
# to ctx. Configure AeroSpace to call this from an on-focused-monitor-changed,
# on-focus-changed, or workspace-change callback as appropriate for your setup.
#
# This example intentionally reports an application context and omits cwd/project.
# Terminal-specific cwd should only be reported by a hook that knows the focused
# surface is actually a terminal/shell context.
#
# If your AeroSpace version exposes focused app/window data directly, replace the
# osascript block with those commands.

workspace="${AEROSPACE_FOCUSED_WORKSPACE:-${1:-}}"

read -r app window <<EOF
$(osascript <<'APPLESCRIPT'
tell application "System Events"
  set frontApp to first application process whose frontmost is true
  set appName to name of frontApp
  set winTitle to ""
  try
    set winTitle to name of front window of frontApp
  end try
  return appName & linefeed & winTitle
end tell
APPLESCRIPT
)
EOF

args=(ctx current report --source aerospace --kind application --app "$app" --window "$window")
if [[ -n "$workspace" ]]; then
  args+=(--workspace "$workspace")
fi

"${args[@]}"
