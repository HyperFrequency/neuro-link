#!/usr/bin/env bash
# Claude Code UserPromptSubmit hook — forward prompt, optionally inject RAG context.
# stdin: JSON (prompt, session_id, ...)
# stdout: JSON with hookSpecificOutput.additionalContext if server returns one.
set -u
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../_lib/common.sh
. "${SCRIPT_DIR}/../_lib/common.sh"

input="$(cat)"
session_id="$(printf '%s' "$input" | python3 -c 'import sys,json
try: d=json.load(sys.stdin)
except: d={}
print(d.get("session_id","") or "")' 2>/dev/null)"

# Synchronous POST here because we want the server's additionalContext back.
response="$(printf '%s' "$input" | nlr_post_event "user_prompt" "claude-code" "$session_id" 2>/dev/null || echo "")"

if [[ -n "$response" ]]; then
  python3 - <<PY "$response"
import json, sys
raw = sys.argv[1] if len(sys.argv) > 1 else ""
try:
    data = json.loads(raw)
except Exception:
    sys.exit(0)
ctx = data.get("additionalContext")
if not ctx:
    sys.exit(0)
out = {
    "hookSpecificOutput": {
        "hookEventName": "UserPromptSubmit",
        "additionalContext": ctx,
    }
}
print(json.dumps(out))
PY
fi
exit 0
