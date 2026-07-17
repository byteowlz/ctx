#!/usr/bin/env bash
set -euo pipefail

# Mirror herdr's focused workspace/tab/pane into ctx's current-context state.
#
# herdr exposes a socket API but no focus hooks, so this script polls and only
# reports when the focused context actually changes. Run it inside herdr (for
# example in a small background pane) so the computed nesting depth reflects
# the terminal stack herdr itself runs in:
#
#   ./examples/herdr-current-context.sh &
#
# Verify the mirrored state with:
#   ctx current --json
#
# The report includes the focused pane's foreground command as --app (falling
# back to herdr's registered agent label) to distinguish agents from plain
# shells, the workspace label as --project, and the tab label as --window.
#
# Requires: herdr, jq
#
# Environment:
#   CTX_HERDR_POLL_INTERVAL  poll interval in seconds (default 2)
#   CTX_CURRENT_DEPTH        override the computed nesting depth

command -v herdr >/dev/null || { echo "herdr not found" >&2; exit 1; }
command -v jq >/dev/null || { echo "jq not found" >&2; exit 1; }

interval="${CTX_HERDR_POLL_INTERVAL:-2}"

depth="${CTX_CURRENT_DEPTH:-}"
if [[ -z "$depth" ]]; then
  # herdr itself is one level; add one per outer multiplexer visible in the
  # environment this script inherited.
  depth=1
  [[ -n "${TMUX:-}" ]] && depth=$((depth + 1))
  [[ -n "${ZELLIJ:-}" ]] && depth=$((depth + 1))
fi

last=""
while :; do
  if pane="$(herdr pane list 2>/dev/null \
    | jq -ce '[.result.panes[] | select(.focused)][0] // empty')"; then
    pane_id="$(jq -r '.pane_id' <<<"$pane")"
    cwd="$(jq -r '.foreground_cwd // .cwd // empty' <<<"$pane")"
    workspace_id="$(jq -r '.workspace_id // empty' <<<"$pane")"
    tab_id="$(jq -r '.tab_id // empty' <<<"$pane")"
    agent="$(jq -r '.agent // empty' <<<"$pane")"

    project="$(herdr workspace list 2>/dev/null \
      | jq -r --arg id "$workspace_id" \
        '.result.workspaces[] | select(.workspace_id == $id) | .label // empty')" || project=""
    window="$(herdr tab list 2>/dev/null \
      | jq -r --arg id "$tab_id" \
        '.result.tabs[] | select(.tab_id == $id) | .label // empty')" || window=""
    app="$(herdr pane process-info --pane "$pane_id" 2>/dev/null \
      | jq -r '.result.process_info.foreground_processes[0].name // empty')" || app=""
    [[ -z "$app" ]] && app="$agent"

    current="$cwd|$project|$window|$app"
    if [[ -n "$cwd" && "$current" != "$last" ]]; then
      args=(ctx current report --source herdr --kind terminal --depth "$depth"
        --cwd "$cwd")
      [[ -n "$project" ]] && args+=(--project "$project")
      [[ -n "$window" ]] && args+=(--window "$window")
      [[ -n "$app" ]] && args+=(--app "$app")
      if "${args[@]}" >/dev/null; then
        last="$current"
      fi
    fi
  fi
  sleep "$interval"
done
