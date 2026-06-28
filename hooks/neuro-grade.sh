#!/usr/bin/env bash
# PostToolUse hook: score tool effectiveness and write to state/score_history.jsonl.
# Grades based on: did the tool succeed? was there an error? was a correction needed?
#
# Design: Fire-and-forget background append — never blocks the conversation.
# Tracks per-tool and per-session effectiveness metrics for self-improve-hitl analysis.
# NO tool input content is logged — only metadata and scores.

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

SCORE_FILE="${NLR_ROOT}/state/score_history.jsonl"
SESSION_LOG="${NLR_ROOT}/state/session_log.jsonl"

# Ensure files exist
mkdir -p "$(dirname "$SCORE_FILE")"

# Read stdin once
input="$(cat)"

# Background processing — never block the conversation
(
  printf '%s' "$input" | python3 /dev/fd/3 "$SCORE_FILE" "$SESSION_LOG" 3<<'PYEOF'
import json, sys, time, os

score_path = sys.argv[1]
session_log_path = sys.argv[2]

try:
    data = json.load(sys.stdin)
except (json.JSONDecodeError, ValueError):
    sys.exit(0)

tool_name = data.get('tool_name', 'unknown')
tool_response = data.get('tool_response', {})

# --- Scoring logic ---

# Base score: did the tool return successfully?
exit_code = None
stderr_present = False
output_empty = False

if isinstance(tool_response, dict):
    exit_code = tool_response.get('exit_code', None)
    stderr = tool_response.get('stderr', '')
    stdout = tool_response.get('stdout', '')
    stderr_present = bool(stderr and stderr.strip())
    output_empty = not bool(stdout and stdout.strip()) and not bool(tool_response.get('content', ''))
elif isinstance(tool_response, str):
    output_empty = not bool(tool_response.strip())

# Score components (0.0 to 1.0)
scores = {}

# 1. Execution success (0.0 = failed, 1.0 = success)
if exit_code is not None:
    scores['execution'] = 1.0 if exit_code == 0 else 0.0
else:
    # Tools without exit codes (Read, Edit, etc.) — assume success unless error indicators
    error_indicators = ['error', 'Error', 'ERROR', 'failed', 'Failed', 'FAILED', 'not found', 'permission denied']
    resp_str = str(tool_response)[:500]
    has_error = any(ind in resp_str for ind in error_indicators)
    scores['execution'] = 0.3 if has_error else 1.0

# 2. Output quality (0.0 = empty, 0.5 = has stderr, 1.0 = clean output)
if output_empty:
    scores['output'] = 0.0
elif stderr_present:
    scores['output'] = 0.5
else:
    scores['output'] = 1.0

# 3. Tool-specific adjustments
tool_weight = 1.0
# NLR skills get tracked with higher weight (their scores matter more)
nlr_skills = [
    'wiki-curate', 'crawl-ingest', 'neuro-scan', 'neuro-surgery',
    'auto-rag', 'job-scanner', 'reasoning-ontology', 'knowledge-gap',
    'code-docs', 'harness-bridge', 'progress-report', 'hyper-sleep',
    'self-improve-hitl', 'self-improve-recursive'
]
if any(skill in tool_name.lower() for skill in nlr_skills):
    tool_weight = 1.5

# 4. Check for recent correction pattern
# If the same tool was called within the last 3 entries in session_log with different
# inputs, it suggests a retry/correction (lower score)
correction_penalty = 0.0
try:
    if os.path.exists(session_log_path):
        with open(session_log_path) as f:
            lines = f.readlines()
        # Check last 5 entries
        recent = []
        for line in lines[-5:]:
            try:
                entry = json.loads(line.strip())
                recent.append(entry)
            except (json.JSONDecodeError, ValueError):
                continue
        # Count how many of the last 5 entries are the same tool
        same_tool_count = sum(1 for e in recent if e.get('tool') == tool_name)
        if same_tool_count >= 2:
            # Repeated calls suggest correction/retry
            correction_penalty = 0.15 * (same_tool_count - 1)
    else:
        correction_penalty = 0.0
except Exception:
    correction_penalty = 0.0

# Composite score
composite = (
    scores.get('execution', 0.5) * 0.5 +
    scores.get('output', 0.5) * 0.3 +
    (1.0 - min(correction_penalty, 0.5)) * 0.2
)

# Clamp to [0.0, 1.0]
composite = max(0.0, min(1.0, composite))

entry = {
    'timestamp': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime()),
    'tool': tool_name,
    'scores': scores,
    'correction_penalty': round(correction_penalty, 3),
    'composite': round(composite, 3),
    'tool_weight': tool_weight,
    'session': 'claude-code'
}

with open(score_path, 'a') as f:
    f.write(json.dumps(entry) + '\n')
PYEOF
) 2>/dev/null &

# Don't output anything — this is a silent background scorer
exit 0
