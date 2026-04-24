#!/usr/bin/env bash
# PostToolUse(Bash) hook: when a git push to deep-tool-wiki is detected,
# inject a reminder to run /doc-sync-embed-verify for verification.

set -euo pipefail

input="$(cat)"
cmd="$(printf '%s' "$input" | python3 -c 'import sys,json; d=json.load(sys.stdin); print(d.get("tool_input",{}).get("command",""))' 2>/dev/null || echo "")"
exit_code="$(printf '%s' "$input" | python3 -c 'import sys,json; d=json.load(sys.stdin); print(d.get("tool_response",{}).get("exit_code",0))' 2>/dev/null || echo "0")"

if [[ "$exit_code" != "0" ]]; then
  exit 0
fi

if [[ "$cmd" != *"git push"*"deep-tool-wiki"* ]] && [[ "$cmd" != *"git push"*"origin main"* ]]; then
  # Also check if CWD contains deep-tool-wiki
  if [[ "$cmd" != *"git push"* ]]; then
    exit 0
  fi
  cwd="$(printf '%s' "$input" | python3 -c 'import sys,json; d=json.load(sys.stdin); print(d.get("cwd",""))' 2>/dev/null || echo "")"
  if [[ "$cwd" != *"deep-tool-wiki"* ]]; then
    exit 0
  fi
fi

cat <<'EOF'
{
  "hookSpecificOutput": {
    "hookEventName": "PostToolUse",
    "additionalContext": "DOC-SYNC: A push to HyperFrequency/deep-tool-wiki was detected. Run /doc-sync-embed-verify to verify consistency: check that Obsidian pages are in sync, InfraNodus graphs match, and Devin DeepWiki is re-indexed. Also add any changed tools to Augment Code's workspace index."
  }
}
EOF
