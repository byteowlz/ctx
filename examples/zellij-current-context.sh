#!/usr/bin/env bash
set -euo pipefail

# Mirror the focused zellij tab/pane into ctx's current-context state.
#
# zellij has no user-facing focus hooks (that would need a WASM plugin), so
# this script polls `zellij action dump-layout` and only reports when the
# focused context actually changes. Run it inside the zellij session you want
# mirrored (for example from a small background pane or via `zellij run`):
#
#   ./examples/zellij-current-context.sh &
#
# Verify the mirrored state with:
#   ctx current --json
#
# The session name is reported as --project, the focused tab name as --window,
# and the focused pane's command as --app. When the focused pane runs a nested
# multiplexer (herdr, tmux, zellij) the report is skipped so the inner
# reporter owns the state; see examples/herdr-current-context.sh.
#
# Environment:
#   CTX_ZELLIJ_POLL_INTERVAL  poll interval in seconds (default 2)
#   CTX_CURRENT_DEPTH         override the computed nesting depth

command -v zellij >/dev/null || { echo "zellij not found" >&2; exit 1; }
[[ -n "${ZELLIJ:-}" ]] || { echo "not running inside a zellij session" >&2; exit 1; }

interval="${CTX_ZELLIJ_POLL_INTERVAL:-2}"
session="${ZELLIJ_SESSION_NAME:-}"

depth="${CTX_CURRENT_DEPTH:-}"
if [[ -z "$depth" ]]; then
  # zellij itself is one level; add one if zellij runs inside tmux.
  depth=1
  [[ -n "${TMUX:-}" ]] && depth=$((depth + 1))
fi

# Prints three lines from the dump-layout KDL: focused tab name, focused pane
# cwd (joined with the layout-level cwd when relative), focused pane command.
parse_layout() {
  awk '
    function attr(line, name,    rest) {
      if (match(line, name "=\"[^\"]*\"") == 0) return ""
      rest = substr(line, RSTART, RLENGTH)
      sub(name "=\"", "", rest)
      sub("\"$", "", rest)
      return rest
    }
    /^[[:space:]]*cwd[[:space:]]+"/ && !in_tab {
      global_cwd = $0
      sub(/^[[:space:]]*cwd[[:space:]]+"/, "", global_cwd)
      sub(/".*$/, "", global_cwd)
    }
    /^[[:space:]]*tab / {
      in_tab = ($0 ~ /focus=true/)
      if (in_tab) tab_name = attr($0, "name")
    }
    in_tab && /^[[:space:]]*pane/ && /focus=true/ && !found {
      found = 1
      pane_cwd = attr($0, "cwd")
      pane_cmd = attr($0, "command")
    }
    END {
      if (pane_cwd == "") pane_cwd = global_cwd
      else if (pane_cwd !~ /^\// && global_cwd != "") pane_cwd = global_cwd "/" pane_cwd
      print tab_name; print pane_cwd; print pane_cmd
    }
  '
}

last=""
while :; do
  if layout="$(zellij action dump-layout 2>/dev/null)"; then
    { IFS= read -r window; IFS= read -r cwd; IFS= read -r app; } < <(parse_layout <<<"$layout")

    case "$app" in
    herdr | tmux | zellij) ;;
    *)
      current="$cwd|$window|$app"
      if [[ -n "$cwd" && "$current" != "$last" ]]; then
        args=(ctx current report --source zellij --kind terminal --depth "$depth"
          --cwd "$cwd")
        [[ -n "$session" ]] && args+=(--project "$session")
        [[ -n "$window" ]] && args+=(--window "$window")
        [[ -n "$app" ]] && args+=(--app "$app")
        if "${args[@]}" >/dev/null; then
          last="$current"
        fi
      fi
      ;;
    esac
  fi
  sleep "$interval"
done
