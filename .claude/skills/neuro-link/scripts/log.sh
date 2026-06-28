#!/usr/bin/env bash
# Append an entry to 04-Agent-Memory/logs.md in the canonical JSONL-ish
# format. Used by every neuro-link skill that needs to record an action.
#
# Usage:
#   log.sh <action> <scope> <outcome> [--skill <name>] [--target <path>]
#
# Example:
#   log.sh "wiki-update" "02-KB-main/math/stochastic-calculus.md" "success" \
#       --skill neuro-surgery

set -euo pipefail

REPO_ROOT="${NLR_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)}"
LOG_FILE="$REPO_ROOT/04-Agent-Memory/logs.md"

ACTION="${1:?action required}"
SCOPE="${2:?scope required}"
OUTCOME="${3:?outcome required}"
shift 3

SKILL=""
TARGET=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --skill)  SKILL="$2"; shift 2 ;;
    --target) TARGET="$2"; shift 2 ;;
    *) echo "unknown flag: $1" >&2; exit 1 ;;
  esac
done

TS="$(date -Iseconds)"
mkdir -p "$(dirname "$LOG_FILE")"

# Append-only — never rewrite. Format is markdown with inline JSON-like
# metadata in a single line, easy to grep and easy to consolidate later.
cat >> "$LOG_FILE" <<EOF
- [$TS] action=$ACTION scope=$SCOPE outcome=$OUTCOME${SKILL:+ skill=$SKILL}${TARGET:+ target=$TARGET}
EOF
