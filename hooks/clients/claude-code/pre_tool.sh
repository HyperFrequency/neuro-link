#!/usr/bin/env bash
# Claude Code PreToolUse hook — forward tool invocation metadata to neuro-link.
# stdin: JSON payload from Claude Code (tool_name, tool_input, session_id, ...)
# stdout: JSON (optional) — empty on no injection
set -u
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../_lib/common.sh
. "${SCRIPT_DIR}/../_lib/common.sh"

input="$(cat)"
session_id="$(printf '%s' "$input" | python3 -c 'import sys,json
try: d=json.load(sys.stdin)
except: d={}
print(d.get("session_id","") or "")' 2>/dev/null)"

# Fire-and-forget POST so PreToolUse never blocks the tool call.
(printf '%s' "$input" | nlr_post_event "pre_tool" "claude-code" "$session_id" >/dev/null 2>&1) &
disown 2>/dev/null || true
exit 0
