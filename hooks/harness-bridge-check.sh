#!/usr/bin/env bash
# PreToolUse hook: check if the pending tool call matches a harness delegation rule
# in config/harness-harness-comms.md. If so, inject additionalContext suggesting
# delegation to the appropriate harness.
#
# Design: FAST (<100ms). No LLM calls. Regex match against routing_rules in config YAML.
# Only fires when a non-claude-code harness is active and a routing rule matches.

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

COMMS_CONFIG="${NLR_ROOT}/config/harness-harness-comms.md"

# Skip if config doesn't exist or bridge is disabled
if [[ ! -f "$COMMS_CONFIG" ]]; then
  exit 0
fi

# Read tool call from stdin JSON
input="$(cat)"

# Write python script to a temp file to avoid heredoc quoting issues
_PY_SCRIPT="$(mktemp)"
trap 'rm -f "$_PY_SCRIPT" "$_RESULT_FILE"' EXIT
_RESULT_FILE="$(mktemp)"

cat > "$_PY_SCRIPT" << 'PYEOF'
import json, sys, re, os

comms_config_path = sys.argv[1]
result_path = sys.argv[2]

try:
    data = json.load(sys.stdin)
except (json.JSONDecodeError, ValueError):
    sys.exit(0)

tool_name = data.get("tool_name", "")
tool_input = data.get("tool_input", {})

# Build a searchable string from tool context
search_text = tool_name.lower()
if isinstance(tool_input, dict):
    for v in tool_input.values():
        if isinstance(v, str):
            search_text += " " + v.lower()
elif isinstance(tool_input, str):
    search_text += " " + tool_input.lower()

# Parse config YAML frontmatter for routing_rules and harness status
try:
    with open(comms_config_path) as f:
        content = f.read()
except OSError:
    sys.exit(0)

# Extract frontmatter
fm_match = re.search(r"^---\n(.*?)\n---", content, re.DOTALL)
if not fm_match:
    sys.exit(0)

fm = fm_match.group(1)

# Check if bridge is enabled
enabled_match = re.search(r"enabled:\s*(true|false)", fm)
if enabled_match and enabled_match.group(1) == "false":
    sys.exit(0)

# Extract routing rules (simple YAML list parsing)
rules = []
in_rules = False
current_rule = {}
for line in fm.split("\n"):
    stripped = line.strip()
    if stripped == "routing_rules:":
        in_rules = True
        continue
    if in_rules:
        if stripped.startswith("- pattern:"):
            if current_rule:
                rules.append(current_rule)
            pattern = stripped.split(":", 1)[1].strip().strip('"').strip("'")
            current_rule = {"pattern": pattern}
        elif stripped.startswith("route_to:"):
            route = stripped.split(":", 1)[1].strip()
            current_rule["route_to"] = route
        elif not stripped.startswith("-") and not stripped.startswith("route_to") and stripped and not stripped.startswith("#"):
            if current_rule:
                rules.append(current_rule)
            break
if current_rule and current_rule not in rules:
    rules.append(current_rule)

if not rules:
    sys.exit(0)

# Extract active harnesses (skip claude-code)
active_harnesses = set()
in_harnesses = False
current_harness = None
for line in fm.split("\n"):
    stripped = line.strip()
    if stripped == "harnesses:":
        in_harnesses = True
        continue
    if in_harnesses:
        harness_match = re.match(r"^  (\S+):$", line)
        if harness_match:
            current_harness = harness_match.group(1)
            continue
        if current_harness and "status:" in stripped:
            status = stripped.split(":", 1)[1].strip()
            if status == "active" and current_harness != "claude-code":
                active_harnesses.add(current_harness)
        if stripped and not stripped.startswith("-") and not line.startswith("  ") and not line.startswith("    "):
            in_harnesses = False

if not active_harnesses:
    sys.exit(0)

# Match routing rules against search text
for rule in rules:
    pattern = rule.get("pattern", "")
    route_to = rule.get("route_to", "")
    if not pattern or not route_to:
        continue
    if route_to not in active_harnesses:
        continue
    try:
        if re.search(pattern, search_text):
            msg = (
                "NEURO-LINK-RECURSIVE HARNESS-BRIDGE: This task matches delegation rule "
                + repr(pattern) + " -> " + route_to + ". Consider using /harness-bridge dispatch to "
                "delegate this to the " + route_to + " harness, which specializes in this type of work."
            )
            with open(result_path, "w") as rf:
                rf.write(msg)
            sys.exit(0)
    except re.error:
        continue

sys.exit(0)
PYEOF

printf '%s' "$input" | python3 "$_PY_SCRIPT" "$COMMS_CONFIG" "$_RESULT_FILE" 2>/dev/null || true

if [[ -s "$_RESULT_FILE" ]]; then
  result="$(cat "$_RESULT_FILE")"

  # F1: attach env snapshot so the suggested harness dispatch has reproducible
  # context (neuro-link version, heartbeat, docker/ngrok/wiki state, MCP list).
  _ENV_SCRIPT="${NLR_ROOT}/skills/harness-bridge/env_snapshot.py"
  env_snap=""
  if [[ -x "$_ENV_SCRIPT" ]] || [[ -f "$_ENV_SCRIPT" ]]; then
    env_snap="$(python3 "$_ENV_SCRIPT" --root "$NLR_ROOT" 2>/dev/null || true)"
  fi
  if [[ -n "$env_snap" ]]; then
    result="${result}

env_snapshot: ${env_snap}"
  fi

  cat <<EOF
{
  "hookSpecificOutput": {
    "hookEventName": "PreToolUse",
    "additionalContext": "$(printf '%s' "$result" | python3 -c 'import sys,json; print(json.dumps(sys.stdin.read()))' | sed 's/^"//;s/"$//')"
  }
}
EOF
fi

exit 0
