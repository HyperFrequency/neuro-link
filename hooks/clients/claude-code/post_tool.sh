#!/usr/bin/env bash
# Claude Code PostToolUse hook — forward tool result metadata to neuro-link.
# stdin: JSON (tool_name, tool_input, tool_response, session_id, ...)
set -u
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../_lib/common.sh
. "${SCRIPT_DIR}/../_lib/common.sh"

input="$(cat)"
session_id="$(printf '%s' "$input" | python3 -c 'import sys,json
try: d=json.load(sys.stdin)
except: d={}
print(d.get("session_id","") or "")' 2>/dev/null)"

(printf '%s' "$input" | nlr_post_event "post_tool" "claude-code" "$session_id" >/dev/null 2>&1) &
disown 2>/dev/null || true
exit 0
