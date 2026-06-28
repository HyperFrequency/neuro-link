#!/usr/bin/env bash
# PostToolUse hook: log tool invocation metadata to neuro-link-recursive session log.
# Fire-and-forget background append — never blocks the conversation.
#
# Logs ONLY safe metadata: timestamp, tool name, exit code, success/failure.
# NO tool input content is logged — avoids persisting secrets, tokens, or sensitive payloads.

set -euo pipefail

# Resolve NLR_ROOT: env var > persisted root file > exit
_NLR_ROOT_FILE="${HOME}/.claude/state/nlr_root"
if [[ -n "${NLR_ROOT:-}" ]]; then
  : # use env var
elif [[ -f "$_NLR_ROOT_FILE" ]]; then
  NLR_ROOT="$(cat "$_NLR_ROOT_FILE")"
else
  exit 0  # not configured — skip silently
fi

LOG_FILE="${NLR_ROOT}/state/session_log.jsonl"

# Ensure log directory and file exist
mkdir -p "$(dirname "$LOG_FILE")"

# Read stdin once, then pipe to Python in the background.
# Python script loaded via fd 3 so stdin stays free for JSON data.
# No shell interpolation of untrusted data.
input="$(cat)"

(
  printf '%s' "$input" | python3 /dev/fd/3 "$LOG_FILE" 3<<'PYEOF'
import json, sys, time

log_path = sys.argv[1]

try:
    data = json.load(sys.stdin)
except (json.JSONDecodeError, ValueError):
    data = {}

tool_name = data.get('tool_name', 'unknown')
tool_response = data.get('tool_response', {})
exit_code = tool_response.get('exit_code', None) if isinstance(tool_response, dict) else None

entry = {
    'timestamp': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime()),
    'tool': tool_name,
    'exit_code': exit_code,
    'success': exit_code == 0 or exit_code is None,
    'session': 'claude-code'
}

with open(log_path, 'a') as f:
    f.write(json.dumps(entry) + '\n')
PYEOF
) 2>/dev/null &

# Don't output anything — this is a silent logger
exit 0
