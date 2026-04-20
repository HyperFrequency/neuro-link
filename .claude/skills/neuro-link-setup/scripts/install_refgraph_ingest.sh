#!/usr/bin/env bash
# install_refgraph_ingest.sh — step 16 of NLR install.
#
# Populates Qdrant + Neo4j from the deep-tool-wiki + llm-wiki content
# on disk. Idempotent: re-running after new wiki pages land re-indexes
# only the changed files.
#
# Preconditions:
#   - Qdrant running on 127.0.0.1:6333 (install.sh step 7)
#   - Neo4j running on 127.0.0.1:7474 / 7687 (install.sh step 6)
#   - llama-server running on 127.0.0.1:8400 hosting Octen-Embedding-8B
#     (install.sh step 9)
#   - Python 3.11+ with `qdrant-client`, `neo4j`, `httpx` packages
#   - deep-tool-wiki checked out at $DEEP_TOOL_WIKI_DIR
#   - llm-wiki vault at $LLM_WIKI_DIR
#
# Env vars (all optional):
#   DEEP_TOOL_WIKI_DIR   default: /Users/DanBot/hyperfrequency/docs/deep-tool-wiki
#   LLM_WIKI_DIR         default: /Users/DanBot/hyperfrequency/neuro-link/02-KB-main
#   QDRANT_URL           default: http://127.0.0.1:6333
#   NEO4J_URL            default: bolt://127.0.0.1:7687
#   NEO4J_USER           default: neo4j
#   NEO4J_PASSWORD_FILE  default: $HOME/.neuro-link/neo4j.password
#   LLAMA_SERVER_URL     default: http://127.0.0.1:8400
#   DRY_RUN              0|1, respects parent install.sh
#   ONLY_TOOL            if set, ingest only this tool (e.g. "vectorbtpro")

set -euo pipefail

: "${DRY_RUN:=0}"
: "${DEEP_TOOL_WIKI_DIR:=/Users/DanBot/hyperfrequency/docs/deep-tool-wiki}"
: "${LLM_WIKI_DIR:=/Users/DanBot/hyperfrequency/neuro-link/02-KB-main}"
: "${QDRANT_URL:=http://127.0.0.1:6333}"
: "${NEO4J_URL:=bolt://127.0.0.1:7687}"
: "${NEO4J_USER:=neo4j}"
: "${LLAMA_SERVER_URL:=http://127.0.0.1:8400}"
: "${ONLY_TOOL:=}"

log()  { printf '[refgraph-ingest] %s\n' "$*"; }
warn() { printf '[refgraph-ingest][warn] %s\n' "$*" >&2; }
run()  { if [[ "$DRY_RUN" = "1" ]]; then printf '[dry] %s\n' "$*"; else eval "$@"; fi; }

# --- preflight checks ---
preflight_ok=1
curl -fsS --max-time 5 "$QDRANT_URL/collections" >/dev/null 2>&1 || { warn "Qdrant unreachable at $QDRANT_URL"; preflight_ok=0; }
curl -fsS --max-time 5 "$NEO4J_URL/../../" >/dev/null 2>&1 || curl -fsS --max-time 5 "http://127.0.0.1:7474" >/dev/null 2>&1 || { warn "Neo4j HTTP unreachable at 127.0.0.1:7474"; preflight_ok=0; }
curl -fsS --max-time 5 "$LLAMA_SERVER_URL/health" >/dev/null 2>&1 || { warn "llama-server unreachable at $LLAMA_SERVER_URL (embeddings will fail)"; preflight_ok=0; }

if [[ -d "$DEEP_TOOL_WIKI_DIR" ]]; then
  log "deep-tool-wiki: $DEEP_TOOL_WIKI_DIR ($(find "$DEEP_TOOL_WIKI_DIR" -maxdepth 1 -type d | tail -n +2 | wc -l | awk '{print $1}') tools)"
else
  warn "DEEP_TOOL_WIKI_DIR missing: $DEEP_TOOL_WIKI_DIR"
  preflight_ok=0
fi

if [[ -d "$LLM_WIKI_DIR" ]]; then
  log "llm-wiki:       $LLM_WIKI_DIR ($(find "$LLM_WIKI_DIR" -maxdepth 1 -type d | tail -n +2 | wc -l | awk '{print $1}') tools)"
else
  warn "LLM_WIKI_DIR missing: $LLM_WIKI_DIR"
  preflight_ok=0
fi

if [[ "$preflight_ok" != "1" ]]; then
  warn "preflight failed — skipping ingest (re-run once services are up)"
  exit 0   # WARN, not FAIL — installer should not block on this optional step
fi

# --- delegate the real work to Python ---
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PY_SCRIPT="$SCRIPT_DIR/ingest_refgraph.py"
if [[ ! -f "$PY_SCRIPT" ]]; then
  warn "ingest_refgraph.py not found at $PY_SCRIPT — skipping"
  exit 0
fi

log "running ingest_refgraph.py..."
export DEEP_TOOL_WIKI_DIR LLM_WIKI_DIR QDRANT_URL NEO4J_URL NEO4J_USER LLAMA_SERVER_URL ONLY_TOOL

if [[ "$DRY_RUN" = "1" ]]; then
  run "python3 '$PY_SCRIPT' --dry-run"
else
  python3 "$PY_SCRIPT" || warn "ingest_refgraph.py reported errors (run 'bash $0' manually to retry)"
fi

log "done."
