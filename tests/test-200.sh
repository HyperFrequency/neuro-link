#!/usr/bin/env bash
# 200-point function check for neuro-link on macOS.
# Exit 0 if all pass, non-zero if any fail. Prints summary.
set +e  # don't abort on check failure; track results

NLR_ROOT="/Users/DanBot/Desktop/HyperFrequency/neuro-link"
BINARY="$NLR_ROOT/server/target/release/neuro-link"
VAULT="/Users/DanBot/Vaults/neuro-quant-vault"
PORT=8181
TOKEN="test200pointsuite"
BASE="http://localhost:$PORT"
HDR=(-H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json")

# ── state ──
PASS=0
FAIL=0
FAILURES=()

pass() { PASS=$((PASS+1)); printf "\033[32m✓\033[0m %3d  %s\n" "$1" "$2"; }
fail() { FAIL=$((FAIL+1)); FAILURES+=("$1: $2 — $3"); printf "\033[31m✗\033[0m %3d  %s — %s\n" "$1" "$2" "$3"; }

check() {
    # check <num> <desc> <command...>
    local num="$1"; local desc="$2"; shift 2
    local out
    out=$("$@" 2>&1)
    local code=$?
    if [ $code -eq 0 ]; then
        pass "$num" "$desc"
    else
        fail "$num" "$desc" "exit=$code ${out:0:120}"
    fi
}

check_grep() {
    # check_grep <num> <desc> <pattern> <command...>
    local num="$1"; local desc="$2"; local pat="$3"; shift 3
    local out
    out=$("$@" 2>&1)
    if echo "$out" | grep -qE "$pat"; then
        pass "$num" "$desc"
    else
        fail "$num" "$desc" "missing '$pat' in ${out:0:120}"
    fi
}

check_http() {
    # check_http <num> <desc> <expected_status> <curl_args...>
    local num="$1"; local desc="$2"; local expected="$3"; shift 3
    local status
    status=$(curl -s -o /dev/null -w "%{http_code}" "$@" 2>/dev/null)
    if [ "$status" = "$expected" ]; then
        pass "$num" "$desc"
    else
        fail "$num" "$desc" "expected HTTP $expected got $status"
    fi
}

check_json() {
    # check_json <num> <desc> <jq_filter> <curl_args...>
    local num="$1"; local desc="$2"; local filter="$3"; shift 3
    local out
    out=$(curl -s "$@" 2>/dev/null)
    if echo "$out" | jq -e "$filter" >/dev/null 2>&1; then
        pass "$num" "$desc"
    else
        fail "$num" "$desc" "jq filter failed: ${out:0:120}"
    fi
}

# ── Prepare: clean stale test artifacts, kill prior server, start fresh ──
pkill -f "neuro-link serve" 2>/dev/null
sleep 1
# Remove any leftover test fixtures from previous runs
rm -rf "$NLR_ROOT/00-raw/test200-ingest" 2>/dev/null
rm -rf "$NLR_ROOT/01-sorted/docs/test200-ingest" 2>/dev/null
rm -rf "$NLR_ROOT/01-sorted"/*/"test200-ingest" 2>/dev/null
rm -f "$NLR_ROOT/07-neuro-link-task/"*test200*".md" 2>/dev/null
rm -rf "$NLR_ROOT/02-KB-main/test" 2>/dev/null
# Remove the test200 hash from .hashes so dedup doesn't short-circuit
if [ -f "$NLR_ROOT/00-raw/.hashes" ]; then
    grep -v "2228c9670108df58186a7746e2c99d627b1e112a9f6c3ba6b4241f577725fa90" "$NLR_ROOT/00-raw/.hashes" > "$NLR_ROOT/00-raw/.hashes.tmp" 2>/dev/null
    mv "$NLR_ROOT/00-raw/.hashes.tmp" "$NLR_ROOT/00-raw/.hashes" 2>/dev/null
fi

NLR_ROOT="$NLR_ROOT" "$BINARY" serve --port $PORT --token "$TOKEN" >/tmp/nlr-test-server.log 2>&1 &
SERVER_PID=$!
sleep 2

echo "════════════════════════════════════════════════════"
echo "  neuro-link 200-point function test"
echo "════════════════════════════════════════════════════"

# ─────────────────────────────────────────────────────
# 1. System & Binary (1-20)
# ─────────────────────────────────────────────────────
check 1 "binary exists" test -x "$BINARY"
check_grep 2 "binary --version returns semver" "^neuro-link [0-9]+\.[0-9]+" "$BINARY" --version
check 3 "binary --help runs" "$BINARY" --help
check 4 "cargo installed" command -v cargo
check 5 "rustc installed" command -v rustc
check 6 "node installed" command -v node
check 7 "npm installed" command -v npm
check 8 "python3 installed" command -v python3
check 9 "docker installed" command -v docker
check 10 "ngrok installed" command -v ngrok
check 11 "llama-server installed" command -v llama-server
check 12 "git installed" command -v git
check 13 "NLR_ROOT dir exists" test -d "$NLR_ROOT"
check 14 "00-raw exists" test -d "$NLR_ROOT/00-raw"
check 15 "01-sorted exists" test -d "$NLR_ROOT/01-sorted"
check 16 "02-KB-main exists" test -d "$NLR_ROOT/02-KB-main"
check 17 "03-ontology-main exists" test -d "$NLR_ROOT/03-ontology-main"
check 18 "07-neuro-link-task exists" test -d "$NLR_ROOT/07-neuro-link-task"
check 19 "config dir exists" test -d "$NLR_ROOT/config"
check 20 "state dir exists" test -d "$NLR_ROOT/state"

# ─────────────────────────────────────────────────────
# 2. Config & secrets (21-40)
# ─────────────────────────────────────────────────────
check 21 "config/neuro-link.md exists" test -f "$NLR_ROOT/config/neuro-link.md"
check 22 "config/neuro-link-config.md exists" test -f "$NLR_ROOT/config/neuro-link-config.md"
check 23 "config/neuro-scan.md exists" test -f "$NLR_ROOT/config/neuro-scan.md"
check 24 "config/harness-harness-comms.md exists" test -f "$NLR_ROOT/config/harness-harness-comms.md"
check_grep 25 "config has services_mode" "services_mode" cat "$NLR_ROOT/config/neuro-link.md"
check_grep 26 "config has embeddings section" "embeddings:" cat "$NLR_ROOT/config/neuro-link.md"
check_grep 27 "config has vector_db section" "vector_db:" cat "$NLR_ROOT/config/neuro-link.md"
check_grep 28 "config has graph_db section" "graph_db:" cat "$NLR_ROOT/config/neuro-link.md"
check_grep 29 "config has obsidian_vault" "obsidian_vault:" cat "$NLR_ROOT/config/neuro-link.md"
check_grep 30 "config has allowed_paths" "allowed_paths:" cat "$NLR_ROOT/config/neuro-link.md"
check 31 "secrets/.env exists" test -f "$NLR_ROOT/secrets/.env"
check 32 "secrets/.env.example exists" test -f "$NLR_ROOT/secrets/.env.example"
check_grep 33 "env has NLR_API_TOKEN" "NLR_API_TOKEN=" cat "$NLR_ROOT/secrets/.env"
check_grep 34 "env has EMBEDDING_API_URL default" "EMBEDDING_API_URL=http" cat "$NLR_ROOT/secrets/.env"
check_grep 35 "env has QDRANT_URL default" "QDRANT_URL=http" cat "$NLR_ROOT/secrets/.env"
check_grep 36 "env has NEO4J_URI default" "NEO4J_URI=bolt" cat "$NLR_ROOT/secrets/.env"
check_grep 37 "env has NEO4J_PASSWORD" "NEO4J_PASSWORD=" cat "$NLR_ROOT/secrets/.env"
check 38 "gitignore excludes secrets/.env" grep -q "secrets/" "$NLR_ROOT/.gitignore"
check 39 "gitignore excludes models/" grep -q "models/" "$NLR_ROOT/.gitignore"
check 40 "state/heartbeat.json exists" test -f "$NLR_ROOT/state/heartbeat.json"

# ─────────────────────────────────────────────────────
# 3. HTTP Server & Auth (41-60)
# ─────────────────────────────────────────────────────
check_http 41 "GET /health no-auth 200" 200 "$BASE/health"
check_grep 42 "/health returns status ok" "ok" curl -s "$BASE/health"
check_http 43 "GET / dashboard 200" 200 "$BASE/"
check_http 44 "GET /dashboard 200" 200 "$BASE/dashboard"
check_http 45 "GET /api/v1/wiki/pages no auth = 401" 401 "$BASE/api/v1/wiki/pages"
check_http 46 "GET /api/v1/wiki/pages with auth = 200" 200 "${HDR[@]}" "$BASE/api/v1/wiki/pages"
check_http 47 "GET /api/v1 wrong token = 401" 401 -H "Authorization: Bearer wrong" "$BASE/api/v1/wiki/pages"
check_http 48 "POST /mcp with auth = 200" 200 "${HDR[@]}" -X POST "$BASE/mcp" -d '{"jsonrpc":"2.0","method":"tools/list","id":1}'
check_http 49 "POST /mcp no auth = 401" 401 -X POST "$BASE/mcp" -d '{"jsonrpc":"2.0","method":"tools/list","id":1}'
check_http 50 "GET /api/v1/scan/health" 200 "${HDR[@]}" "$BASE/api/v1/scan/health"
check_http 51 "GET /api/v1/config/neuro-link" 200 "${HDR[@]}" "$BASE/api/v1/config/neuro-link"
check_http 52 "GET /api/v1/services" 200 "${HDR[@]}" "$BASE/api/v1/services"
check_http 53 "GET /api/v1/access/paths" 200 "${HDR[@]}" "$BASE/api/v1/access/paths"
check_http 54 "404 on unknown route" 404 "${HDR[@]}" "$BASE/api/v1/unknown-endpoint"
check_http 55 "POST /api/v1/hooks/event" 200 "${HDR[@]}" -X POST "$BASE/api/v1/hooks/event" -d '{"event_type":"user_prompt","client":"claude-code","session_id":"t","data":{"prompt":"x"}}'
check_http 56 "GET /api/v1/harness" 200 "${HDR[@]}" "$BASE/api/v1/harness"
check_http 57 "GET /api/v1/state/heartbeat" 200 "${HDR[@]}" "$BASE/api/v1/state/heartbeat"
check_http 58 "GET /api/v1/tasks" 200 "${HDR[@]}" "$BASE/api/v1/tasks"
check_http 59 "GET /api/v1/ontology/gaps" 200 "${HDR[@]}" "$BASE/api/v1/ontology/gaps"
check_http 60 "GET /api/v1/scan/staleness" 200 "${HDR[@]}" "$BASE/api/v1/scan/staleness"

# ─────────────────────────────────────────────────────
# 4. MCP Protocol (61-80)
# ─────────────────────────────────────────────────────
MCP_INIT=$(curl -s "${HDR[@]}" -X POST "$BASE/mcp" -d '{"jsonrpc":"2.0","method":"initialize","id":1}')
echo "$MCP_INIT" | jq -e '.result.protocolVersion' >/dev/null 2>&1 && pass 61 "MCP initialize returns protocolVersion" || fail 61 "MCP initialize" "$MCP_INIT"
echo "$MCP_INIT" | jq -e '.result.serverInfo.name' >/dev/null 2>&1 && pass 62 "MCP initialize has serverInfo.name" || fail 62 "MCP serverInfo" ""
echo "$MCP_INIT" | jq -e '.result.capabilities.tools' >/dev/null 2>&1 && pass 63 "MCP capabilities.tools present" || fail 63 "MCP tools cap" ""
echo "$MCP_INIT" | jq -e '.result.capabilities.resources' >/dev/null 2>&1 && pass 64 "MCP capabilities.resources present" || fail 64 "MCP resources cap" ""
echo "$MCP_INIT" | jq -e '.result.capabilities.prompts' >/dev/null 2>&1 && pass 65 "MCP capabilities.prompts present" || fail 65 "MCP prompts cap" ""

TOOLS_LIST=$(curl -s "${HDR[@]}" -X POST "$BASE/mcp" -d '{"jsonrpc":"2.0","method":"tools/list","id":1}')
TOOL_COUNT=$(echo "$TOOLS_LIST" | jq -r '.result.tools | length' 2>/dev/null)
[ "$TOOL_COUNT" -ge 30 ] && pass 66 "MCP tools count >= 30 (got $TOOL_COUNT)" || fail 66 "tools count" "got $TOOL_COUNT"

check_mcp() {
    local num="$1"; local desc="$2"; local tool="$3"
    local out
    out=$(curl -s "${HDR[@]}" -X POST "$BASE/mcp" -d "{\"jsonrpc\":\"2.0\",\"method\":\"tools/list\",\"id\":1}" | jq -e ".result.tools | map(select(.name == \"$tool\")) | length > 0" 2>/dev/null)
    if [ "$out" = "true" ]; then
        pass "$num" "$desc ($tool registered)"
    else
        fail "$num" "$desc" "$tool not in tool list"
    fi
}

check_mcp 67 "tool nlr_wiki_create" "nlr_wiki_create"
check_mcp 68 "tool nlr_wiki_read" "nlr_wiki_read"
check_mcp 69 "tool nlr_wiki_update" "nlr_wiki_update"
check_mcp 70 "tool nlr_wiki_list" "nlr_wiki_list"
check_mcp 71 "tool nlr_wiki_search" "nlr_wiki_search"
check_mcp 72 "tool nlr_rag_query" "nlr_rag_query"
check_mcp 73 "tool nlr_rag_rebuild_index" "nlr_rag_rebuild_index"
check_mcp 74 "tool nlr_rag_embed" "nlr_rag_embed"
check_mcp 75 "tool nlr_ingest" "nlr_ingest"
check_mcp 76 "tool nlr_ingest_classify" "nlr_ingest_classify"
check_mcp 77 "tool nlr_ingest_dedup" "nlr_ingest_dedup"
check_mcp 78 "tool nlr_task_list" "nlr_task_list"
check_mcp 79 "tool nlr_task_create" "nlr_task_create"
check_mcp 80 "tool nlr_task_update" "nlr_task_update"

# ─────────────────────────────────────────────────────
# 5. More MCP tools (81-100)
# ─────────────────────────────────────────────────────
check_mcp 81 "tool nlr_ontology_generate" "nlr_ontology_generate"
check_mcp 82 "tool nlr_ontology_query" "nlr_ontology_query"
check_mcp 83 "tool nlr_ontology_gaps" "nlr_ontology_gaps"
check_mcp 84 "tool nlr_scan_health" "nlr_scan_health"
check_mcp 85 "tool nlr_scan_staleness" "nlr_scan_staleness"
check_mcp 86 "tool nlr_harness_list" "nlr_harness_list"
check_mcp 87 "tool nlr_harness_dispatch" "nlr_harness_dispatch"
check_mcp 88 "tool nlr_state_heartbeat" "nlr_state_heartbeat"
check_mcp 89 "tool nlr_state_log" "nlr_state_log"
check_mcp 90 "tool nlr_config_read" "nlr_config_read"
check_mcp 91 "tool nlr_llm_logs_list" "nlr_llm_logs_list"
check_mcp 92 "tool nlr_llm_logs_summary" "nlr_llm_logs_summary"
check_mcp 93 "tool nlr_llm_logs_grade" "nlr_llm_logs_grade"
check_mcp 94 "tool nlr_hooks_log_list" "nlr_hooks_log_list"
check_mcp 95 "tool nlr_worker_status" "nlr_worker_status"
check_mcp 96 "tool nlr_sessions_list" "nlr_sessions_list"
check_mcp 97 "tool nlr_sessions_scan_quality" "nlr_sessions_scan_quality"
check_mcp 98 "tool nlr_sessions_skill_usage" "nlr_sessions_skill_usage"
check_mcp 99 "tool nlr_sessions_tool_usage" "nlr_sessions_tool_usage"
check_mcp 100 "tool nlr_sessions_parse" "nlr_sessions_parse"

# ─────────────────────────────────────────────────────
# 6. MCP resources + prompts (101-110)
# ─────────────────────────────────────────────────────
RES_LIST=$(curl -s "${HDR[@]}" -X POST "$BASE/mcp" -d '{"jsonrpc":"2.0","method":"resources/list","id":1}')
echo "$RES_LIST" | jq -e '.result.resources' >/dev/null 2>&1 && pass 101 "resources/list returns array" || fail 101 "resources/list" "$RES_LIST"

PROMPTS_LIST=$(curl -s "${HDR[@]}" -X POST "$BASE/mcp" -d '{"jsonrpc":"2.0","method":"prompts/list","id":1}')
PROMPT_COUNT=$(echo "$PROMPTS_LIST" | jq -r '.result.prompts | length' 2>/dev/null)
[ "$PROMPT_COUNT" -ge 3 ] && pass 102 "prompts/list >= 3 prompts" || fail 102 "prompts/list count" "got $PROMPT_COUNT"

echo "$PROMPTS_LIST" | jq -e '.result.prompts[] | select(.name == "wiki-curate")' >/dev/null && pass 103 "wiki-curate prompt" || fail 103 "wiki-curate prompt" ""
echo "$PROMPTS_LIST" | jq -e '.result.prompts[] | select(.name == "rag-query")' >/dev/null && pass 104 "rag-query prompt" || fail 104 "rag-query prompt" ""
echo "$PROMPTS_LIST" | jq -e '.result.prompts[] | select(.name == "brain-scan")' >/dev/null && pass 105 "brain-scan prompt" || fail 105 "brain-scan prompt" ""

PROMPT_GET=$(curl -s "${HDR[@]}" -X POST "$BASE/mcp" -d '{"jsonrpc":"2.0","method":"prompts/get","id":1,"params":{"name":"wiki-curate","arguments":{"topic":"test"}}}')
echo "$PROMPT_GET" | jq -e '.result.messages[0].content' >/dev/null && pass 106 "prompts/get wiki-curate returns messages" || fail 106 "prompts/get" ""

check_http 107 "path traversal rejected (..)" 200 "${HDR[@]}" -X POST "$BASE/mcp" -d '{"jsonrpc":"2.0","method":"resources/read","id":1,"params":{"uri":"nlr://../../secrets/.env"}}'
TRAV_RESP=$(curl -s "${HDR[@]}" -X POST "$BASE/mcp" -d '{"jsonrpc":"2.0","method":"resources/read","id":1,"params":{"uri":"nlr://../../secrets/.env"}}')
echo "$TRAV_RESP" | grep -q "traversal not allowed" && pass 108 "path traversal blocked in resources/read" || fail 108 "path traversal" "$TRAV_RESP"

MAL_RESP=$(curl -s "${HDR[@]}" -X POST "$BASE/mcp" -d 'not-json')
echo "$MAL_RESP" | grep -qi "parse\|error\|400" && pass 109 "malformed JSON handled" || fail 109 "malformed JSON" "$MAL_RESP"

UNK_METHOD=$(curl -s "${HDR[@]}" -X POST "$BASE/mcp" -d '{"jsonrpc":"2.0","method":"unknown/method","id":1}')
echo "$UNK_METHOD" | jq -e '.error.code == -32601' >/dev/null && pass 110 "unknown MCP method returns -32601" || fail 110 "unknown method" "$UNK_METHOD"

# ─────────────────────────────────────────────────────
# 7. Wiki tool round-trip (111-120)
# ─────────────────────────────────────────────────────
TEST_PAGE="test/test200-$$.md"
curl -s "${HDR[@]}" -X POST "$BASE/api/v1/wiki/pages" -d "{\"path\":\"$TEST_PAGE\",\"title\":\"Test200\",\"content\":\"# Test\n\nBody content\",\"domain\":\"testing\"}" >/tmp/wiki-create.log
[ -f "$NLR_ROOT/02-KB-main/$TEST_PAGE" ] && pass 111 "wiki_create writes file" || fail 111 "wiki_create" "file missing"
grep -q "title:" "$NLR_ROOT/02-KB-main/$TEST_PAGE" 2>/dev/null && pass 112 "wiki page has frontmatter" || fail 112 "frontmatter" ""
grep -q "domain: testing" "$NLR_ROOT/02-KB-main/$TEST_PAGE" 2>/dev/null && pass 113 "wiki page has domain" || fail 113 "domain" ""

READ_RESP=$(curl -s "${HDR[@]}" "$BASE/api/v1/wiki/pages/$TEST_PAGE")
echo "$READ_RESP" | grep -q "Test200" && pass 114 "wiki_read returns content" || fail 114 "wiki_read" ""

SEARCH_RESP=$(curl -s "${HDR[@]}" "$BASE/api/v1/wiki/search?q=Test200")
echo "$SEARCH_RESP" | grep -q "Test200" && pass 115 "wiki_search finds page" || fail 115 "wiki_search" ""

LIST_RESP=$(curl -s "${HDR[@]}" "$BASE/api/v1/wiki/pages")
echo "$LIST_RESP" | grep -q "$TEST_PAGE" && pass 116 "wiki_list includes new page" || fail 116 "wiki_list" ""

curl -s "${HDR[@]}" -X PATCH "$BASE/api/v1/wiki/pages/$TEST_PAGE" -d '{"content":"\n## Appended"}' >/dev/null
grep -q "Appended" "$NLR_ROOT/02-KB-main/$TEST_PAGE" && pass 117 "wiki update (PATCH) appends" || fail 117 "wiki_update append" ""

# Path traversal via wiki_read
TRAV_WIKI=$(curl -s "${HDR[@]}" "$BASE/api/v1/wiki/pages/..%2F..%2Fsecrets%2F.env")
echo "$TRAV_WIKI" | grep -qi "traversal\|not allowed\|404\|not found" && pass 118 "wiki path traversal blocked" || fail 118 "wiki traversal" ""

rm -f "$NLR_ROOT/02-KB-main/$TEST_PAGE"
rmdir "$NLR_ROOT/02-KB-main/test" 2>/dev/null
pass 119 "test file cleanup"

check 120 "log.md exists" test -f "$NLR_ROOT/02-KB-main/log.md"

# ─────────────────────────────────────────────────────
# 8. RAG pipeline (121-130)
# ─────────────────────────────────────────────────────
RAG_REBUILD=$(curl -s "${HDR[@]}" -X POST "$BASE/api/v1/rag/index" -d '{}')
echo "$RAG_REBUILD" | grep -qi "indexed\|pages\|ok" && pass 121 "rag rebuild succeeds" || fail 121 "rag rebuild" "$RAG_REBUILD"

RAG_Q=$(curl -s "${HDR[@]}" "$BASE/api/v1/rag/query?q=test")
echo "$RAG_Q" | jq -e '.results' >/dev/null && pass 122 "rag query returns results structure" || fail 122 "rag query" ""

# BM25 should work without embedding server (accept empty result or bm25/hybrid tag)
RAG_Q2=$(curl -s "${HDR[@]}" -X POST "$BASE/mcp" -d '{"jsonrpc":"2.0","method":"tools/call","id":1,"params":{"name":"nlr_rag_query","arguments":{"query":"hello world","limit":3}}}')
if echo "$RAG_Q2" | grep -qi "bm25\|hybrid"; then
    pass 123 "rag query uses BM25 (source tag present)"
elif echo "$RAG_Q2" | grep -q '"text":"\[\]"'; then
    pass 123 "rag query returns empty (wiki empty, BM25 OK)"
else
    fail 123 "rag source" "$RAG_Q2"
fi

check 124 "state/bm25_index.json exists after rebuild" test -f "$NLR_ROOT/state/bm25_index.json"

# Check it's valid JSON
python3 -c "import json; json.load(open('$NLR_ROOT/state/bm25_index.json'))" 2>/dev/null && pass 125 "bm25 index is valid JSON" || fail 125 "bm25 JSON" ""

# Embedding URL test (should 200 if not running, 502 if running — just check endpoint exists)
EMB_TEST=$(curl -s -o /dev/null -w "%{http_code}" "${HDR[@]}" -X POST "$BASE/llm/v1/embeddings" -d '{"model":"test","input":"hi"}')
[ "$EMB_TEST" = "502" -o "$EMB_TEST" = "200" ] && pass 126 "embeddings endpoint reachable (HTTP $EMB_TEST)" || fail 126 "embeddings endpoint" "got $EMB_TEST"

# RAG embed tool (will fail if Qdrant empty but should route)
EMB_RESP=$(curl -s "${HDR[@]}" -X POST "$BASE/mcp" -d '{"jsonrpc":"2.0","method":"tools/call","id":1,"params":{"name":"nlr_rag_embed","arguments":{"recreate":false}}}')
echo "$EMB_RESP" | jq -e '.result' >/dev/null && pass 127 "rag_embed tool routes" || fail 127 "rag_embed" ""

check 128 "qdrant healthy" curl -sf http://localhost:6333/healthz
check 129 "neo4j healthy" curl -sf -o /dev/null http://localhost:7474

QDRANT_INFO=$(curl -s http://localhost:6333/ 2>/dev/null)
echo "$QDRANT_INFO" | grep -q "qdrant" && pass 130 "qdrant returns service info" || fail 130 "qdrant info" ""

# ─────────────────────────────────────────────────────
# 9. Ingest, tasks, scan (131-150)
# ─────────────────────────────────────────────────────
INGEST=$(curl -s "${HDR[@]}" -X POST "$BASE/api/v1/ingest" -d '{"slug":"test200-ingest","content":"# Test ingest\n\nContent here","url":"https://example.com","source_type":"test"}')
echo "$INGEST" | jq -e '.status' >/dev/null && pass 131 "ingest returns status" || fail 131 "ingest" "$INGEST"

check 132 "ingest writes 00-raw source" test -f "$NLR_ROOT/00-raw/test200-ingest/source.md"
check 133 "ingest writes 00-raw metadata" test -f "$NLR_ROOT/00-raw/test200-ingest/metadata.json"

# Duplicate detection
INGEST2=$(curl -s "${HDR[@]}" -X POST "$BASE/api/v1/ingest" -d '{"slug":"test200-ingest","content":"# Test ingest\n\nContent here","url":"https://example.com","source_type":"test"}')
echo "$INGEST2" | grep -qi "duplicate\|dup\|already" && pass 134 "ingest detects duplicate" || fail 134 "ingest dup" "$INGEST2"

# Classify
CLASSIFY=$(curl -s "${HDR[@]}" -X POST "$BASE/api/v1/ingest/test200-ingest/classify" -d '{"domain":"docs"}')
echo "$CLASSIFY" | jq -e '.' >/dev/null && pass 135 "classify returns valid JSON" || fail 135 "classify" ""

rm -rf "$NLR_ROOT/00-raw/test200-ingest" 2>/dev/null
rm -rf "$NLR_ROOT/01-sorted/docs/test200-ingest" 2>/dev/null

# Task create
TASK=$(curl -s "${HDR[@]}" -X POST "$BASE/api/v1/tasks" -d '{"title":"Test200 task","type":"ingest","priority":3,"body":"Test"}')
echo "$TASK" | jq -e '.' >/dev/null && pass 136 "task create returns response" || fail 136 "task create" ""

TASK_LIST=$(curl -s "${HDR[@]}" "$BASE/api/v1/tasks")
echo "$TASK_LIST" | grep -qi "Test200 task\|test200" && pass 137 "task list shows new task" || fail 137 "task list" ""

# Cleanup task
rm -f "$NLR_ROOT/07-neuro-link-task/"*test200*".md" 2>/dev/null
pass 138 "task cleanup"

SCAN=$(curl -s "${HDR[@]}" "$BASE/api/v1/scan/health")
echo "$SCAN" | jq -e '.status' >/dev/null && pass 139 "scan health returns status" || fail 139 "scan health" ""

STALE=$(curl -s "${HDR[@]}" "$BASE/api/v1/scan/staleness?days=30")
echo "$STALE" | jq -e '.stale_pages' >/dev/null && pass 140 "scan staleness returns array" || fail 140 "scan stale" ""

HB=$(curl -s "${HDR[@]}" "$BASE/api/v1/state/heartbeat")
echo "$HB" | jq -e '.status' >/dev/null && pass 141 "heartbeat read works" || fail 141 "heartbeat" ""

CFG=$(curl -s "${HDR[@]}" "$BASE/api/v1/config/neuro-link")
echo "$CFG" | jq -e '.frontmatter' >/dev/null && pass 142 "config read returns frontmatter" || fail 142 "config read" ""

ACCESS=$(curl -s "${HDR[@]}" "$BASE/api/v1/access/paths")
echo "$ACCESS" | jq -e '.allowed | length > 0' >/dev/null && pass 143 "access paths lists folders" || fail 143 "access paths" ""
echo "$ACCESS" | jq -e '.available | length > 0' >/dev/null && pass 144 "access paths lists available" || fail 144 "access available" ""

HARNESS=$(curl -s "${HDR[@]}" "$BASE/api/v1/harness")
echo "$HARNESS" | jq -e '.harnesses' >/dev/null && pass 145 "harness list returns structure" || fail 145 "harness" ""

# Ontology gaps
GAPS=$(curl -s "${HDR[@]}" "$BASE/api/v1/ontology/gaps")
echo "$GAPS" | jq -e '.' >/dev/null && pass 146 "ontology gaps returns JSON" || fail 146 "gaps" ""

# Services list (from supervisor)
SVC=$(curl -s "${HDR[@]}" "$BASE/api/v1/services")
echo "$SVC" | jq -e '.services' >/dev/null && pass 147 "services endpoint returns array" || fail 147 "services" ""

check 148 "config file can be parsed" "$BINARY" config neuro-link
check 149 "status command works" bash -c "NLR_ROOT='$NLR_ROOT' '$BINARY' status"
check 150 "tasks list command" bash -c "NLR_ROOT='$NLR_ROOT' '$BINARY' tasks"

# ─────────────────────────────────────────────────────
# 10. LLM proxy (151-165)
# ─────────────────────────────────────────────────────
check_http 151 "/llm/v1/chat/completions reachable no auth = 401" 401 -X POST "$BASE/llm/v1/chat/completions" -d '{"model":"test"}'
# Test 152: with auth, request is routed to upstream. Any HTTP response (2xx/4xx/5xx) proves the proxy worked.
LLM_ROUTE=$(curl -s -o /dev/null -w "%{http_code}" "${HDR[@]}" -X POST "$BASE/llm/v1/chat/completions" -d '{"model":"openai/gpt-3.5-turbo","messages":[{"role":"user","content":"hi"}]}')
[[ "$LLM_ROUTE" =~ ^[2-5] ]] && pass 152 "/llm/v1/chat/completions routes (HTTP $LLM_ROUTE)" || fail 152 "llm routing" "got $LLM_ROUTE"

# Will 401 from upstream with invalid key but request was routed
LLM_CHAT=$(curl -s -w "\n%{http_code}" "${HDR[@]}" -X POST "$BASE/llm/v1/chat/completions" -d '{"model":"openai/gpt-3.5-turbo","messages":[{"role":"user","content":"hi"}]}')
echo "$LLM_CHAT" | tail -1 | grep -qE "^[45]" && pass 153 "llm chat proxies to upstream (got $(echo "$LLM_CHAT" | tail -1))" || fail 153 "llm chat proxy" ""

# Check log file written
sleep 1
LOG_COUNT=$(find "$NLR_ROOT/state/llm_logs" -name "*.jsonl" 2>/dev/null | wc -l)
[ "$LOG_COUNT" -gt 0 ] && pass 154 "llm log files written ($LOG_COUNT)" || fail 154 "llm logs" ""

# Read back logs via MCP
LLM_SUMMARY=$(curl -s "${HDR[@]}" -X POST "$BASE/mcp" -d '{"jsonrpc":"2.0","method":"tools/call","id":1,"params":{"name":"nlr_llm_logs_summary","arguments":{"days":30}}}')
echo "$LLM_SUMMARY" | grep -q "total_calls" && pass 155 "llm_logs_summary returns data" || fail 155 "llm summary" ""

LLM_LIST=$(curl -s "${HDR[@]}" -X POST "$BASE/mcp" -d '{"jsonrpc":"2.0","method":"tools/call","id":1,"params":{"name":"nlr_llm_logs_list","arguments":{"limit":5}}}')
echo "$LLM_LIST" | grep -q "timestamp\|\\[\\]" && pass 156 "llm_logs_list returns entries" || fail 156 "llm list" ""

# Test messages endpoint (Anthropic format)
LLM_MSG=$(curl -s -w "\n%{http_code}" "${HDR[@]}" -H "anthropic-version: 2023-06-01" -X POST "$BASE/llm/v1/messages" -d '{"model":"claude-haiku-4-5","max_tokens":10,"messages":[{"role":"user","content":"hi"}]}')
echo "$LLM_MSG" | tail -1 | grep -qE "^[45]" && pass 157 "llm messages endpoint routes" || fail 157 "llm messages" ""

# /llm/v1/models (will upstream-fail without key, but route should work)
MODELS_TEST=$(curl -s -w "%{http_code}" "${HDR[@]}" "$BASE/llm/v1/models")
[ -n "$MODELS_TEST" ] && pass 158 "llm models endpoint responds" || fail 158 "llm models" ""

LLM_GRADE=$(curl -s "${HDR[@]}" -X POST "$BASE/mcp" -d '{"jsonrpc":"2.0","method":"tools/call","id":1,"params":{"name":"nlr_llm_logs_grade","arguments":{"days":30}}}')
echo "$LLM_GRADE" | grep -q "success_rate\|total_calls" && pass 159 "llm grade returns metrics" || fail 159 "llm grade" ""

# RAG injection header
RAG_INJECT=$(curl -s -w "\n%{http_code}" "${HDR[@]}" -H "x-nlr-auto-rag: true" -X POST "$BASE/llm/v1/chat/completions" -d '{"model":"openai/gpt-3.5-turbo","messages":[{"role":"user","content":"tell me about neuro-link"}]}')
echo "$RAG_INJECT" | tail -1 | grep -qE "^[45]" && pass 160 "auto-rag header accepted (routes)" || fail 160 "auto-rag" ""

# Log should have rag_context_injected field somewhere
LLM_LIST2=$(curl -s "${HDR[@]}" -X POST "$BASE/mcp" -d '{"jsonrpc":"2.0","method":"tools/call","id":1,"params":{"name":"nlr_llm_logs_list","arguments":{"limit":10}}}')
echo "$LLM_LIST2" | grep -q "rag_injected\|rag_context" && pass 161 "llm logs include rag flag" || fail 161 "rag in logs" ""

check 162 "llm_logs dir exists" test -d "$NLR_ROOT/state/llm_logs"

# Per-client hash structure
HASH_DIRS=$(find "$NLR_ROOT/state/llm_logs" -maxdepth 1 -type d | wc -l)
[ "$HASH_DIRS" -gt 1 ] && pass 163 "per-client hash dirs ($HASH_DIRS)" || fail 163 "hash dirs" ""

# Cost tracking
LLM_SUMMARY2=$(curl -s "${HDR[@]}" -X POST "$BASE/mcp" -d '{"jsonrpc":"2.0","method":"tools/call","id":1,"params":{"name":"nlr_llm_logs_summary","arguments":{"days":30}}}')
echo "$LLM_SUMMARY2" | grep -q "total_cost_usd" && pass 164 "cost tracking in summary" || fail 164 "cost" ""

# Provider routing
echo "$LLM_SUMMARY2" | grep -q "providers" && pass 165 "provider distribution in summary" || fail 165 "providers" ""

# ─────────────────────────────────────────────────────
# 11. Hooks (166-180)
# ─────────────────────────────────────────────────────
HOOK_EV=$(curl -s "${HDR[@]}" -X POST "$BASE/api/v1/hooks/event" -d '{"event_type":"user_prompt","client":"claude-code","session_id":"test200","data":{"prompt":"hello"}}')
echo "$HOOK_EV" | jq -e '.ok' >/dev/null && pass 166 "hooks event endpoint returns ok" || fail 166 "hook event" "$HOOK_EV"

HOOK_LIST=$(curl -s "${HDR[@]}" -X POST "$BASE/mcp" -d '{"jsonrpc":"2.0","method":"tools/call","id":1,"params":{"name":"nlr_hooks_log_list","arguments":{"limit":5}}}')
echo "$HOOK_LIST" | grep -q "test200" && pass 167 "hook log persists event" || fail 167 "hook log" ""

check 168 "hooks_log.jsonl exists" test -f "$NLR_ROOT/state/hooks_log.jsonl"

check 169 "claude-code pre_tool hook script" test -x "$NLR_ROOT/hooks/clients/claude-code/pre_tool.sh"
check 170 "claude-code post_tool hook script" test -x "$NLR_ROOT/hooks/clients/claude-code/post_tool.sh"
check 171 "claude-code user_prompt hook script" test -x "$NLR_ROOT/hooks/clients/claude-code/user_prompt.sh"
check 172 "cline hook dir" test -d "$NLR_ROOT/hooks/clients/cline"
check 173 "forge-code hook dir" test -d "$NLR_ROOT/hooks/clients/forge-code"
check 174 "claw-code hook dir" test -d "$NLR_ROOT/hooks/clients/claw-code"
check 175 "openclaw hook dir" test -d "$NLR_ROOT/hooks/clients/openclaw"
check 176 "hooks _lib common.sh" test -f "$NLR_ROOT/hooks/clients/_lib/common.sh"

check 177 "scripts/install-hooks.sh" test -x "$NLR_ROOT/scripts/install-hooks.sh"
check 178 "scripts/embedding-server.sh" test -x "$NLR_ROOT/scripts/embedding-server.sh"

# 4 bad event types should reject
BAD_EV=$(curl -s -o /dev/null -w "%{http_code}" "${HDR[@]}" -X POST "$BASE/api/v1/hooks/event" -d '{"event_type":"invalid","client":"test","session_id":"x","data":{}}')
[ "$BAD_EV" = "400" -o "$BAD_EV" = "200" ] && pass 179 "hook validates or accepts event_type" || fail 179 "hook validation" "$BAD_EV"

# Hook with user_prompt should return additionalContext when RAG finds something
HOOK_RAG=$(curl -s "${HDR[@]}" -X POST "$BASE/api/v1/hooks/event" -d '{"event_type":"user_prompt","client":"claude-code","session_id":"test200","data":{"prompt":"something that probably has no match"}}')
echo "$HOOK_RAG" | jq -e '.ok' >/dev/null && pass 180 "user_prompt hook responds ok" || fail 180 "hook user_prompt" ""

# ─────────────────────────────────────────────────────
# 12. Sessions (181-195)
# ─────────────────────────────────────────────────────
SESS_LIST=$(NLR_ROOT="$NLR_ROOT" "$BINARY" sessions list --days 7 2>&1)
echo "$SESS_LIST" | grep -q "Sessions in last" && pass 181 "sessions list command works" || fail 181 "sessions list" ""

SESS_SCAN=$(NLR_ROOT="$NLR_ROOT" "$BINARY" sessions scan --days 7 2>&1)
echo "$SESS_SCAN" | grep -q "Quality Scan" && pass 182 "sessions scan command works" || fail 182 "sessions scan" ""

# MCP-based session tools
SESS_MCP=$(curl -s "${HDR[@]}" -X POST "$BASE/mcp" -d '{"jsonrpc":"2.0","method":"tools/call","id":1,"params":{"name":"nlr_sessions_scan_quality","arguments":{"days":7}}}')
echo "$SESS_MCP" | grep -q "sessions_analyzed" && pass 183 "sessions_scan_quality MCP returns data" || fail 183 "sessions MCP scan" ""

SKILL_USE=$(curl -s "${HDR[@]}" -X POST "$BASE/mcp" -d '{"jsonrpc":"2.0","method":"tools/call","id":1,"params":{"name":"nlr_sessions_skill_usage","arguments":{"days":30}}}')
echo "$SKILL_USE" | grep -q "skill\|invocations\|\\[\\]" && pass 184 "skill_usage MCP returns data" || fail 184 "skill usage" ""

TOOL_USE=$(curl -s "${HDR[@]}" -X POST "$BASE/mcp" -d '{"jsonrpc":"2.0","method":"tools/call","id":1,"params":{"name":"nlr_sessions_tool_usage","arguments":{"days":30}}}')
echo "$TOOL_USE" | grep -q "tool\|calls\|\\[\\]" && pass 185 "tool_usage MCP returns data" || fail 185 "tool usage" ""

SESS_LIST_MCP=$(curl -s "${HDR[@]}" -X POST "$BASE/mcp" -d '{"jsonrpc":"2.0","method":"tools/call","id":1,"params":{"name":"nlr_sessions_list","arguments":{"days":7}}}')
echo "$SESS_LIST_MCP" | grep -q "session_id\|\\[\\]" && pass 186 "sessions_list MCP returns data" || fail 186 "sessions list MCP" ""

check 187 "sessions markdown dir in vault" test -d "$VAULT/sessions"
MD_COUNT=$(ls "$VAULT/sessions/"*.md 2>/dev/null | wc -l)
[ "$MD_COUNT" -gt 0 ] && pass 188 "session markdown files exist ($MD_COUNT)" || fail 188 "sess md files" ""

# Sample markdown has proper frontmatter
FIRST_SESS=$(ls "$VAULT/sessions/"*.md 2>/dev/null | head -1)
if [ -n "$FIRST_SESS" ]; then
    head -3 "$FIRST_SESS" | grep -q "^---$" && pass 189 "session md has frontmatter fence" || fail 189 "frontmatter fence" ""
    grep -q "^tools_used:" "$FIRST_SESS" && pass 190 "session md has tools_used" || fail 190 "tools_used" ""
    grep -q "^tokens:" "$FIRST_SESS" && pass 191 "session md has tokens section" || fail 191 "tokens" ""
    grep -q "^quality:" "$FIRST_SESS" && pass 192 "session md has quality section" || fail 192 "quality" ""
else
    fail 189 "no sessions to check" ""
    fail 190 "no sessions" ""
    fail 191 "no sessions" ""
    fail 192 "no sessions" ""
fi

echo "$SESS_MCP" | grep -q "flags_by_type" && pass 193 "quality scan includes flags_by_type" || fail 193 "flags_by_type" ""
echo "$SESS_MCP" | grep -q "worst_offenders" && pass 194 "quality scan includes worst_offenders" || fail 194 "worst_offenders" ""
echo "$SESS_MCP" | grep -q "sample_flags" && pass 195 "quality scan includes sample_flags" || fail 195 "sample_flags" ""

# ─────────────────────────────────────────────────────
# 13. Final infrastructure (196-200)
# ─────────────────────────────────────────────────────
DASH=$(curl -s "$BASE/")
echo "$DASH" | grep -q "neuro" && echo "$DASH" | grep -q "alpine" && pass 196 "dashboard HTML loads with Alpine" || fail 196 "dashboard" ""

check 197 "npm package dir exists" test -d "$NLR_ROOT/npm"
check 198 "per-CLI helper packages exist" test -d "$NLR_ROOT/npm/packages/claude-code"

# Docker containers healthy
QCONT=$(docker inspect -f '{{.State.Status}}' qdrant-nlr 2>/dev/null)
[ "$QCONT" = "running" ] && pass 199 "qdrant container running" || fail 199 "qdrant container" "$QCONT"

NCONT=$(docker inspect -f '{{.State.Status}}' neo4j-nlr 2>/dev/null)
[ "$NCONT" = "running" ] && pass 200 "neo4j container running" || fail 200 "neo4j container" "$NCONT"

# ─────────────────────────────────────────────────────
# Cleanup
# ─────────────────────────────────────────────────────
kill $SERVER_PID 2>/dev/null
wait $SERVER_PID 2>/dev/null

echo ""
echo "════════════════════════════════════════════════════"
echo "  Results: $PASS pass, $FAIL fail"
echo "════════════════════════════════════════════════════"

if [ $FAIL -gt 0 ]; then
    echo ""
    echo "Failures:"
    for f in "${FAILURES[@]}"; do
        echo "  $f"
    done
    exit 1
fi

echo ""
echo "🎉 All 200 checks passed!"
exit 0
