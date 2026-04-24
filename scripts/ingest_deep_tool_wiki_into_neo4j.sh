#!/usr/bin/env bash
# ingest_deep_tool_wiki_into_neo4j.sh — parse deep-tool-wiki/ into Neo4j
#
# Walks $DEEP_TOOL_WIKI/<tool>/{wiki.md,pitfalls.md,code.md} and
# assets/refgraph-*.mmd, parses YAML frontmatter + mermaid graph bodies,
# and inserts Tool + Symbol nodes with edges via Cypher POSTs to
# http://$NEO4J_HOST:$NEO4J_HTTP_PORT/db/neo4j/tx/commit.
#
# Schema:
#   (:Tool {name, upstream, fork, ingested, refreshed, languages})
#   (:Symbol {name, tool, kind})       # kind: py | rust | pyo3
#   (:Tool)-[:HAS_SYMBOL]->(:Symbol)
#   (:Symbol)-[:REFERS_TO]->(:Symbol)  # from mermaid edges
#   (:Tool)-[:HAS_PITFALL {anchor}]->(:Tool)  # self-loop with body in anchor
#   (:Tool)-[:HAS_CODE]->(:Tool)              # ditto
#
# Env:
#   NEO4J_HOST       — default: localhost
#   NEO4J_HTTP_PORT  — default: 7474
#   NEO4J_USER       — default: neo4j
#   NEO4J_PASS       — REQUIRED (read from secrets/.env if present)
#   DEEP_TOOL_WIKI   — path to deep-tool-wiki submodule
#                      (default: auto-detect $NEURO_QUANT_ROOT/deep-tool-wiki)
#   DRY_RUN=1        — print Cypher statements without sending them

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
NLR_ROOT="${NLR_ROOT:-$(cd "$SCRIPT_DIR/.." && pwd)}"
NEURO_QUANT_ROOT="${NEURO_QUANT_ROOT:-$(cd "$NLR_ROOT/.." && pwd)}"
DEEP_TOOL_WIKI="${DEEP_TOOL_WIKI:-$NEURO_QUANT_ROOT/deep-tool-wiki}"
NEO4J_HOST="${NEO4J_HOST:-localhost}"
NEO4J_HTTP_PORT="${NEO4J_HTTP_PORT:-7474}"
NEO4J_USER="${NEO4J_USER:-neo4j}"
DRY_RUN="${DRY_RUN:-0}"

# Load NEO4J_PASS from secrets/.env if present (but never echo it).
if [[ -z "${NEO4J_PASS:-}" ]] && [[ -f "$NLR_ROOT/secrets/.env" ]]; then
  # shellcheck disable=SC1090
  set -a; source "$NLR_ROOT/secrets/.env"; set +a
fi

log() { printf "[neo4j-ingest] %s\n" "$*" >&2; }

if [[ ! -d "$DEEP_TOOL_WIKI" ]]; then
  log "ERROR: deep-tool-wiki not found at $DEEP_TOOL_WIKI"
  log "       set DEEP_TOOL_WIKI or run: git submodule update --init deep-tool-wiki"
  exit 1
fi

if [[ "$DRY_RUN" != "1" && -z "${NEO4J_PASS:-}" ]]; then
  log "ERROR: NEO4J_PASS not set. Export it or set it in $NLR_ROOT/secrets/.env"
  exit 1
fi

NEO4J_URL="http://$NEO4J_HOST:$NEO4J_HTTP_PORT/db/neo4j/tx/commit"

# Counters
TOOLS=0
SYMBOLS=0
EDGES=0

# --- Helper: send Cypher. statement is passed as $1 with optional params
#     already substituted (we keep each statement self-contained to avoid
#     parameter-map escaping hell over the transactional endpoint).
send_cypher() {
  local stmt="$1"
  if [[ "$DRY_RUN" == "1" ]]; then
    printf "CYPHER: %s\n" "$stmt"
    return 0
  fi
  local body
  body=$(jq -n --arg s "$stmt" '{statements: [{statement: $s}]}')
  local response
  response=$(curl -sS -X POST \
    -u "$NEO4J_USER:$NEO4J_PASS" \
    -H "Content-Type: application/json" \
    -H "Accept: application/json" \
    -d "$body" \
    "$NEO4J_URL" 2>&1) || {
    log "  curl error: $response"
    return 1
  }
  # Neo4j returns { results: [...], errors: [...] }. Fail loudly on errors.
  local errs
  errs=$(echo "$response" | jq -r '.errors // [] | length' 2>/dev/null || echo "0")
  if [[ "$errs" != "0" && "$errs" != "" ]]; then
    local msg
    msg=$(echo "$response" | jq -r '.errors[0].message // "unknown"' 2>/dev/null)
    log "  Neo4j error: $msg"
    log "  stmt: ${stmt:0:200}..."
    return 1
  fi
}

# --- Helper: escape a string for Cypher single quotes.
cq() {
  printf '%s' "$1" | sed -e "s/'/\\\\'/g" -e 's/$/ /' | sed 's/ $//'
}

# --- Parse YAML frontmatter from a wiki.md file.
#     Emits KEY=VALUE lines for the keys we care about.
parse_frontmatter() {
  local md="$1"
  # Read lines between the first "---" and the next "---".
  awk '
    BEGIN { in_fm=0; count=0 }
    /^---$/ { count++; if (count==1) { in_fm=1; next } else { exit } }
    in_fm {
      # Split on first ":"
      idx = index($0, ":")
      if (idx > 0) {
        key = substr($0, 1, idx-1)
        val = substr($0, idx+1)
        # Strip leading/trailing whitespace
        gsub(/^[ \t]+|[ \t]+$/, "", key)
        gsub(/^[ \t]+|[ \t]+$/, "", val)
        # Strip surrounding [ ] and quotes for simplicity
        gsub(/^["\[\]]|["\[\]]$/, "", val)
        print key "=" val
      }
    }
  ' "$md"
}

# --- Parse mermaid refgraph edges from a .mmd file.
#     Mermaid flowchart edges look like:  A --> B   or   A[label] --> B[label]
#     We only extract node ids (not labels) and the --> connector.
parse_mermaid_edges() {
  local mmd="$1"
  grep -E '^\s*[A-Za-z_][A-Za-z0-9_]*' "$mmd" 2>/dev/null \
    | grep -E -- '-->' \
    | sed -E 's/\[[^]]*\]//g; s/:::[a-zA-Z]+//g; s/\{[^}]*\}//g' \
    | awk -F '-->' '{
        src=$1; dst=$2;
        gsub(/^[ \t]+|[ \t]+$/, "", src);
        gsub(/^[ \t]+|[ \t]+$/, "", dst);
        # strip semicolons, pipes
        gsub(/[;|].*$/, "", dst);
        gsub(/[ \t].*$/, "", src);
        gsub(/[ \t].*$/, "", dst);
        if (src != "" && dst != "" && src !~ /^(graph|classDef|subgraph|end|style)/ && dst !~ /^(graph|classDef|subgraph|end|style)/) {
          print src "|" dst
        }
      }'
}

# --- Walk each tool dir ---
for tool_dir in "$DEEP_TOOL_WIKI"/*/; do
  [[ -d "$tool_dir" ]] || continue
  tool=$(basename "$tool_dir")
  wiki="$tool_dir/wiki.md"
  if [[ ! -f "$wiki" ]]; then
    log "SKIP $tool: no wiki.md"
    continue
  fi
  log "=== $tool ==="

  # Parse frontmatter
  upstream=""
  fork=""
  ingested=""
  refreshed=""
  languages=""
  while IFS='=' read -r k v; do
    case "$k" in
      upstream)   upstream="$v" ;;
      fork)       fork="$v" ;;
      ingested)   ingested="$v" ;;
      refreshed)  refreshed="$v" ;;
      languages)  languages="$v" ;;
    esac
  done < <(parse_frontmatter "$wiki")

  # MERGE Tool node
  stmt=$(printf "MERGE (t:Tool {name: '%s'}) SET t.upstream='%s', t.fork='%s', t.ingested='%s', t.refreshed='%s', t.languages='%s'" \
    "$(cq "$tool")" "$(cq "$upstream")" "$(cq "$fork")" "$(cq "$ingested")" "$(cq "$refreshed")" "$(cq "$languages")")
  if send_cypher "$stmt"; then
    TOOLS=$((TOOLS+1))
  fi

  # HAS_PITFALL / HAS_CODE relationships (self-loops with body anchors)
  for aux in pitfalls code; do
    aux_file="$tool_dir/$aux.md"
    if [[ -f "$aux_file" ]]; then
      # Record only the fact that the file exists; don't embed its content
      # in Neo4j (that lives in qdrant via embed_wiki).
      stmt=$(printf "MATCH (t:Tool {name: '%s'}) MERGE (t)-[r:HAS_%s {path: '%s'}]->(t)" \
        "$(cq "$tool")" "$(echo "$aux" | tr '[:lower:]' '[:upper:]')" "$(cq "$aux.md")")
      send_cypher "$stmt" || true
    fi
  done

  # Symbols + REFERS_TO edges from mermaid refgraphs
  if [[ -d "$tool_dir/assets" ]]; then
    for mmd in "$tool_dir"/assets/refgraph-*.mmd; do
      [[ -f "$mmd" ]] || continue
      while IFS='|' read -r src dst; do
        [[ -z "$src" || -z "$dst" ]] && continue
        # MERGE both symbols and the edge in a single statement
        stmt=$(printf "MERGE (s:Symbol {name: '%s', tool: '%s'}) MERGE (d:Symbol {name: '%s', tool: '%s'}) MERGE (t:Tool {name: '%s'}) MERGE (t)-[:HAS_SYMBOL]->(s) MERGE (t)-[:HAS_SYMBOL]->(d) MERGE (s)-[:REFERS_TO]->(d)" \
          "$(cq "$src")" "$(cq "$tool")" "$(cq "$dst")" "$(cq "$tool")" "$(cq "$tool")")
        if send_cypher "$stmt"; then
          SYMBOLS=$((SYMBOLS+2))
          EDGES=$((EDGES+1))
        fi
      done < <(parse_mermaid_edges "$mmd")
    done
  fi
done

log "----------------------------------------"
log "SUMMARY:"
log "  Tools:   $TOOLS"
log "  Symbols: ~$SYMBOLS (counted per MERGE; MERGE is idempotent so actual node count in Neo4j may be lower)"
log "  Edges:   $EDGES"
log ""
log "Verify: curl -u $NEO4J_USER:\$NEO4J_PASS $NEO4J_URL -H 'Content-Type: application/json' \\"
log "  -d '{\"statements\":[{\"statement\":\"MATCH (t:Tool) RETURN count(t)\"}]}'"
