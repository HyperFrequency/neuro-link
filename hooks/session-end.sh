#!/usr/bin/env bash
# session-end.sh — Claude Code Stop / SessionEnd hook for neuro-link-recursive.
#
# Reads the JSON payload on stdin emitted by Claude Code at Stop or SessionEnd
# and appends one JSON line to $NLR_ROOT/state/session_log.jsonl containing:
#   { ts, session_id, duration_ms, tool_call_count }
#
# Fields pulled (best-effort) from the payload:
#   session_id, duration_ms, tool_call_count (or computed from turns).
#
# Registration (add to ~/.claude/settings.json):
#   "hooks": {
#     "Stop": [
#       { "hooks": [
#         { "type": "command",
#           "command": "$NLR_ROOT/hooks/session-end.sh" }
#       ] }
#     ]
#   }
#
# The hook never fails: missing fields, missing NLR_ROOT, or malformed JSON
# all degrade to a single log line with whatever data could be parsed.
set -u

# Resolve NLR_ROOT (env > ~/.claude/state/nlr_root > cwd).
root="${NLR_ROOT:-}"
if [[ -z "$root" ]]; then
  if [[ -f "${HOME}/.claude/state/nlr_root" ]]; then
    root="$(tr -d '[:space:]' < "${HOME}/.claude/state/nlr_root")"
  fi
fi
if [[ -z "$root" || ! -d "$root" ]]; then
  # No sensible root: silently no-op rather than blocking the Stop event.
  exit 0
fi

state_dir="${root}/state"
mkdir -p "$state_dir" 2>/dev/null || exit 0
log_file="${state_dir}/session_log.jsonl"

input="$(cat || true)"

INPUT_JSON="$input" python3 - "$log_file" <<'PY' 2>/dev/null || true
import os, sys, json, time

log_file = sys.argv[1]
raw = os.environ.get("INPUT_JSON", "")

try:
    payload = json.loads(raw) if raw.strip() else {}
    if not isinstance(payload, dict):
        payload = {}
except Exception:
    payload = {}

def pick(*keys):
    for k in keys:
        v = payload.get(k)
        if v is not None:
            return v
    return None

session_id = pick("session_id", "sessionId", "id") or ""
duration_ms = pick("duration_ms", "durationMs")
tool_call_count = pick("tool_call_count", "toolCallCount", "num_tool_calls")

# Fallbacks: derive from nested fields if present.
if tool_call_count is None:
    tools = payload.get("tool_calls") or payload.get("toolCalls")
    if isinstance(tools, list):
        tool_call_count = len(tools)
if duration_ms is None:
    start = pick("started_at_ms", "start_ts_ms")
    end = pick("ended_at_ms", "end_ts_ms")
    if isinstance(start, (int, float)) and isinstance(end, (int, float)):
        duration_ms = int(end - start)

entry = {
    "ts": int(time.time() * 1000),
    "session_id": session_id,
    "duration_ms": duration_ms,
    "tool_call_count": tool_call_count,
}

try:
    with open(log_file, "a") as f:
        f.write(json.dumps(entry, separators=(",", ":")) + "\n")
except Exception:
    pass
PY

exit 0
