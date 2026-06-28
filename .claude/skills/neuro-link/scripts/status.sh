#!/usr/bin/env bash
# Fast health probe for the neuro-link system. Prints a single-screen status
# table: heartbeat, Qdrant collections, Neo4j, MCP servers, pending tasks.
# Exits 0 if everything is healthy, 1 if anything is red.

set -uo pipefail

REPO_ROOT="${NLR_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)}"
HEARTBEAT="$REPO_ROOT/state/heartbeat.json"

red()    { printf "\033[31m%s\033[0m" "$1"; }
green()  { printf "\033[32m%s\033[0m" "$1"; }
yellow() { printf "\033[33m%s\033[0m" "$1"; }

fail=0
row() { printf "  %-24s %-10s %s\n" "$1" "$2" "$3"; }

echo "neuro-link status"
echo

# Heartbeat
if [[ -f "$HEARTBEAT" ]]; then
  status="$(jq -r '.status // "unknown"' "$HEARTBEAT" 2>/dev/null)"
  case "$status" in
    ready)       row "heartbeat" "$(green OK)" "status=$status" ;;
    initialized) row "heartbeat" "$(yellow WARN)" "status=$status (setup not complete)"; fail=1 ;;
    *)           row "heartbeat" "$(red FAIL)" "status=$status"; fail=1 ;;
  esac
else
  row "heartbeat" "$(red FAIL)" "missing $HEARTBEAT"
  fail=1
fi

# Qdrant
QDRANT_URL="${QDRANT_URL:-http://localhost:6333}"
if curl -s -f "$QDRANT_URL/collections" >/dev/null 2>&1; then
  n_cols=$(curl -s "$QDRANT_URL/collections" | jq -r '.result.collections | length' 2>/dev/null || echo "?")
  row "qdrant" "$(green OK)" "$n_cols collections at $QDRANT_URL"
else
  row "qdrant" "$(red FAIL)" "unreachable at $QDRANT_URL"
  fail=1
fi

# Neo4j
NEO4J_URL="${NEO4J_URL:-bolt://localhost:7687}"
# Crude check: is port open?
host=$(echo "$NEO4J_URL" | sed -E 's#^[^:]+://([^:]+):.*#\1#')
port=$(echo "$NEO4J_URL" | sed -E 's#.*:([0-9]+)$#\1#')
if (echo > "/dev/tcp/$host/$port") 2>/dev/null; then
  row "neo4j" "$(green OK)" "$NEO4J_URL"
else
  row "neo4j" "$(yellow WARN)" "port $port not reachable"
fi

# llama-server (Octen)
if curl -s -f http://127.0.0.1:8400/health >/dev/null 2>&1; then
  row "llama-server (Octen)" "$(green OK)" "http://127.0.0.1:8400"
else
  row "llama-server (Octen)" "$(yellow WARN)" "not serving at 8400"
fi

# neuro-link Rust server (http variant)
if curl -s -f http://127.0.0.1:8787/healthz >/dev/null 2>&1; then
  row "nlr http" "$(green OK)" "http://127.0.0.1:8787"
else
  row "nlr http" "$(yellow WARN)" "not serving at 8787"
fi

# TurboVault
if curl -s -f http://127.0.0.1:3001/healthz >/dev/null 2>&1; then
  row "turbovault" "$(green OK)" "http://127.0.0.1:3001"
else
  row "turbovault" "$(yellow WARN)" "not serving at 3001"
fi

# Pending tasks
if [[ -d "$REPO_ROOT/00-neuro-link/tasks" ]]; then
  pending=$(find "$REPO_ROOT/00-neuro-link/tasks" -maxdepth 1 -name '*.md' \
    -exec grep -l '^status: pending' {} + 2>/dev/null | wc -l | tr -d ' ')
  if [[ "$pending" -gt 0 ]]; then
    row "tasks" "$(yellow INFO)" "$pending pending"
  else
    row "tasks" "$(green OK)" "0 pending"
  fi
fi

# Skills installed
n_skills=$(ls -d "$HOME/.claude/skills"/*/ 2>/dev/null | wc -l | tr -d ' ')
row "skills" "$(green INFO)" "$n_skills installed at ~/.claude/skills/"

echo
if [[ $fail -eq 0 ]]; then
  echo "$(green all checks passed)"
  exit 0
else
  echo "$(red one or more critical checks failed)"
  exit 1
fi
