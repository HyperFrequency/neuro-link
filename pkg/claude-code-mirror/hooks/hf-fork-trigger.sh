#!/usr/bin/env bash
# PreToolUse(Bash) hook: when a `gh repo fork` command is about to run,
# inject a reminder to first run docs-dual-lookup on the upstream library.

set -euo pipefail

input="$(cat)"
cmd="$(printf '%s' "$input" | python3 -c 'import sys,json; d=json.load(sys.stdin); print(d.get("tool_input",{}).get("command",""))' 2>/dev/null || echo "")"

if [[ -z "$cmd" ]]; then
  exit 0
fi

if [[ "$cmd" != *"gh repo fork"* ]] && [[ "$cmd" != *"git clone"*"hyperfrequency"* ]]; then
  exit 0
fi

cat <<'EOF'
{
  "hookSpecificOutput": {
    "hookEventName": "PreToolUse",
    "additionalContext": "AUTO-TRIGGER (fork detected): A fork/clone command is about to run. Before working with the forked code, invoke the docs-dual-lookup skill to query Context7 and Auggie in parallel for the upstream library's current API surface, breaking changes, and recent release notes. This prevents forking against stale assumptions."
  }
}
EOF
