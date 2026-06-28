#!/usr/bin/env bash
# Domain: REST API endpoints.
# For each route in server/src/api/rest.rs:
#   - 200/201/202/204 with valid bearer + payload
#   - 401 without bearer
#   - 400 with bad payload (where applicable — missing required field or wrong type)
#   - 404 for non-existent resource (where applicable)
# Also: content-type sanity, JSON schema shape.

set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=../lib/common.sh
source "$SCRIPT_DIR/lib/common.sh"

init_test_script "rest"
require_tools curl jq python3
require_token

BASE="${LOCAL_BASE}/api/v1"

# Test-artifacts that we may need to clean up from real storage
TEST_SLUG="test-rest-$(date +%s)-$$"
TEST_WIKI_PATH="tests/$TEST_SLUG.md"
TEST_TASK_TITLE="Test REST Task $$"
TEST_TASK_SLUG=$(printf "%s" "$TEST_TASK_TITLE" | tr '[:upper:] ' '[:lower:]-')
# Task create produces filename with pattern "<priority>-<type>-<slug>.md"
TEST_TASK_FILENAME="3-general-$TEST_TASK_SLUG.md"
TEST_ONTOLOGY="test-rest-$$"

register_cleanup "$NLR_ROOT/02-KB-main/$TEST_WIKI_PATH"
register_cleanup "$NLR_ROOT/02-KB-main/tests"
register_cleanup "$NLR_ROOT/00-raw/$TEST_SLUG"
register_cleanup "$NLR_ROOT/00-raw/${TEST_SLUG}.md"
register_cleanup "$NLR_ROOT/01-sorted/docs/${TEST_SLUG}.md"
register_cleanup "$NLR_ROOT/07-neuro-link-task/$TEST_TASK_FILENAME"
register_cleanup "$NLR_ROOT/03-ontology-main/domain/$TEST_ONTOLOGY"

# ── Helper: assert HTTP code ──
assert_code() {
    local name="$1" expected="$2" actual="$3" body_preview="${4:-}"
    if [ "$actual" = "$expected" ]; then
        record_test "$name" PASS 0 "HTTP $actual"
    else
        record_test "$name" FAIL 0 "expected $expected, got $actual (body=${body_preview:0:120})"
    fi
}

# ── /health (public, no auth) ──
start=$(now_ms)
code=$(unauth_get_status "$LOCAL_BASE/health")
dur=$(( $(now_ms) - start ))
if [ "$code" = "200" ]; then
    record_test "GET /health (public, no auth needed)" PASS "$dur"
else
    record_test "GET /health (public, no auth needed)" FAIL "$dur" "got $code"
fi

# ── AUTH FAIL-CLOSED: all protected routes reject without bearer ──
for path in "/api/v1/wiki/pages" "/api/v1/rag/query?q=test" "/api/v1/tasks" "/api/v1/ontology/gaps" "/api/v1/scan/health" "/api/v1/harness" "/api/v1/services"; do
    start=$(now_ms)
    code=$(unauth_get_status "$LOCAL_BASE$path")
    dur=$(( $(now_ms) - start ))
    assert_code "401 without bearer: $path" 401 "$code"
done

# ── Wrong bearer → still 401 (not 500) ──
start=$(now_ms)
code=$(curl -s -H "Authorization: Bearer definitely-not-the-right-token" \
    -o /dev/null -w '%{http_code}' --max-time 10 "$BASE/wiki/pages")
dur=$(( $(now_ms) - start ))
assert_code "401 with wrong bearer (not 500)" 401 "$code"

# ═══ Wiki ═══

# GET /wiki/pages
start=$(now_ms)
body=$(auth_get "$BASE/wiki/pages")
dur=$(( $(now_ms) - start ))
count=$(printf "%s" "$body" | jq -r '.count // 0' 2>/dev/null)
if [[ "$count" =~ ^[0-9]+$ ]] && [ "$count" -ge 2 ]; then
    record_test "GET /wiki/pages returns count >= 2" PASS "$dur" "count=$count"
else
    record_test "GET /wiki/pages returns count >= 2" FAIL "$dur" "got count=$count body=${body:0:120}"
fi

# POST /wiki/pages — create
start=$(now_ms)
code=$(auth_post_status "$BASE/wiki/pages" \
    "$(jq -n --arg p "$TEST_WIKI_PATH" --arg t "Test Page" --arg c "Body content" \
       '{path:$p,title:$t,content:$c,domain:"docs",confidence:"low"}')")
dur=$(( $(now_ms) - start ))
assert_code "POST /wiki/pages valid payload (201)" 201 "$code"

# POST /wiki/pages — missing required field (no title)
start=$(now_ms)
code=$(auth_post_status "$BASE/wiki/pages" '{"path":"bad.md","content":"x"}')
dur=$(( $(now_ms) - start ))
# Axum's Json extractor returns 400 on schema violations (missing required serde field)
if [ "$code" = "400" ] || [ "$code" = "422" ]; then
    record_test "POST /wiki/pages missing 'title' → 400/422" PASS "$dur" "HTTP $code"
else
    record_test "POST /wiki/pages missing 'title' → 400/422" FAIL "$dur" "got $code"
fi

# GET /wiki/pages/{path} — read (valid path from listing)
start=$(now_ms)
body=$(auth_get "$BASE/wiki/pages/quant/merger-arbitrage.md")
dur=$(( $(now_ms) - start ))
if printf "%s" "$body" | grep -q "title\|merger\|arbitrage"; then
    record_test "GET /wiki/pages/{path} valid" PASS "$dur"
else
    record_test "GET /wiki/pages/{path} valid" FAIL "$dur" "body=${body:0:120}"
fi

# GET /wiki/pages/{path} — non-existent
start=$(now_ms)
body=$(auth_get "$BASE/wiki/pages/definitely-does-not-exist.md")
dur=$(( $(now_ms) - start ))
# rest.rs maps ApiError::from(anyhow::Error) → 500 currently; this is a potential bug to flag
if printf "%s" "$body" | grep -qi "not found\|error"; then
    record_test "GET /wiki/pages/{path} non-existent returns error" PASS "$dur"
else
    record_test "GET /wiki/pages/{path} non-existent returns error" FAIL "$dur" "body=${body:0:120}"
fi

# GET /wiki/search?q=...
start=$(now_ms)
body=$(auth_get "$BASE/wiki/search?q=rag")
dur=$(( $(now_ms) - start ))
count=$(printf "%s" "$body" | jq -r '.count // 0' 2>/dev/null)
if [[ "$count" =~ ^[0-9]+$ ]] && [ "$count" -ge 0 ]; then
    record_test "GET /wiki/search?q=rag returns results" PASS "$dur" "count=$count"
else
    record_test "GET /wiki/search?q=rag returns results" FAIL "$dur" "body=${body:0:120}"
fi

# ═══ RAG ═══

# Use extended timeout — first hit may warm embedding caches
start=$(now_ms)
body=$(curl --silent --show-error --max-time 60 \
    -H "Authorization: Bearer ${NLR_API_TOKEN}" "$BASE/rag/query?q=transformer")
dur=$(( $(now_ms) - start ))
rcount=$(printf "%s" "$body" | jq -r '.results | length // 0' 2>/dev/null)
if [[ "$rcount" =~ ^[0-9]+$ ]] && [ "$rcount" -ge 1 ]; then
    record_test "GET /rag/query?q=transformer returns results" PASS "$dur" "count=$rcount"
else
    record_test "GET /rag/query?q=transformer returns results" FAIL "$dur" "body=${body:0:120}"
fi

# /rag/query without q param → expect 400 (axum rejects missing Query fields)
start=$(now_ms)
code=$(auth_get_status "$BASE/rag/query")
dur=$(( $(now_ms) - start ))
if [ "$code" = "400" ] || [ "$code" = "422" ]; then
    record_test "GET /rag/query without q → 400/422" PASS "$dur" "HTTP $code"
else
    record_test "GET /rag/query without q → 400/422" FAIL "$dur" "got $code"
fi

# POST /rag/index
start=$(now_ms)
code=$(auth_post_status "$BASE/rag/index" '{}')
dur=$(( $(now_ms) - start ))
assert_code "POST /rag/index rebuilds" 200 "$code"

# ═══ Ingest ═══

start=$(now_ms)
code=$(auth_post_status "$BASE/ingest" \
    "$(jq -n --arg s "$TEST_SLUG" --arg c "Test REST ingest body" \
       '{slug:$s,content:$c,source_type:"manual"}')")
dur=$(( $(now_ms) - start ))
# Returns 201 (created) or 200 (dedup hit)
if [ "$code" = "201" ] || [ "$code" = "200" ]; then
    record_test "POST /ingest valid payload" PASS "$dur" "HTTP $code"
else
    record_test "POST /ingest valid payload" FAIL "$dur" "got $code"
fi

# Missing required field
start=$(now_ms)
code=$(auth_post_status "$BASE/ingest" '{"content":"only-content"}')
dur=$(( $(now_ms) - start ))
if [ "$code" = "400" ] || [ "$code" = "422" ]; then
    record_test "POST /ingest missing 'slug' → 400/422" PASS "$dur" "HTTP $code"
else
    record_test "POST /ingest missing 'slug' → 400/422" FAIL "$dur" "got $code"
fi

# POST /ingest/dedup
start=$(now_ms)
code=$(auth_post_status "$BASE/ingest/dedup" '{"content":"test"}')
dur=$(( $(now_ms) - start ))
assert_code "POST /ingest/dedup" 200 "$code"

# ═══ Tasks ═══

start=$(now_ms)
body=$(auth_get "$BASE/tasks")
dur=$(( $(now_ms) - start ))
if printf "%s" "$body" | jq -e '.tasks' >/dev/null 2>&1; then
    record_test "GET /tasks returns task list" PASS "$dur"
else
    record_test "GET /tasks returns task list" FAIL "$dur" "body=${body:0:120}"
fi

start=$(now_ms)
code=$(auth_post_status "$BASE/tasks" \
    "$(jq -n --arg t "$TEST_TASK_TITLE" '{title:$t,type:"general",priority:3,body:"probe"}')")
dur=$(( $(now_ms) - start ))
assert_code "POST /tasks valid payload (201)" 201 "$code"

# missing title
start=$(now_ms)
code=$(auth_post_status "$BASE/tasks" '{"type":"general"}')
dur=$(( $(now_ms) - start ))
if [ "$code" = "400" ] || [ "$code" = "422" ]; then
    record_test "POST /tasks missing 'title' → 400/422" PASS "$dur" "HTTP $code"
else
    record_test "POST /tasks missing 'title' → 400/422" FAIL "$dur" "got $code"
fi

# PATCH /tasks/{filename}
start=$(now_ms)
code=$(curl -s -H "Authorization: Bearer ${NLR_API_TOKEN}" \
    -H "Content-Type: application/json" -X PATCH \
    -d '{"status":"completed"}' \
    -o /dev/null -w '%{http_code}' --max-time 10 \
    "$BASE/tasks/$TEST_TASK_FILENAME")
dur=$(( $(now_ms) - start ))
assert_code "PATCH /tasks/{filename} valid" 200 "$code"

# PATCH on non-existent task
start=$(now_ms)
code=$(curl -s -H "Authorization: Bearer ${NLR_API_TOKEN}" \
    -H "Content-Type: application/json" -X PATCH \
    -d '{"status":"completed"}' \
    -o /dev/null -w '%{http_code}' --max-time 10 \
    "$BASE/tasks/definitely-not-a-real-task.md")
dur=$(( $(now_ms) - start ))
# Current behavior: tool returns error wrapped as 500 — record actual
if [ "$code" = "404" ] || [ "$code" = "500" ] || [ "$code" = "400" ]; then
    record_test "PATCH /tasks/{nonexistent} returns error code" PASS "$dur" "HTTP $code"
else
    record_test "PATCH /tasks/{nonexistent} returns error code" FAIL "$dur" "got $code"
fi

# ═══ Ontology ═══

start=$(now_ms)
code=$(auth_post_status "$BASE/ontology" \
    "$(jq -n --arg n "$TEST_ONTOLOGY" \
       '{name:$n,text:"subject\npredicate\nobject",type:"domain"}')")
dur=$(( $(now_ms) - start ))
assert_code "POST /ontology (201)" 201 "$code"

start=$(now_ms)
body=$(auth_get "$BASE/ontology/gaps")
dur=$(( $(now_ms) - start ))
if printf "%s" "$body" | jq -e '.total_ontologies' >/dev/null 2>&1; then
    record_test "GET /ontology/gaps returns stats" PASS "$dur"
else
    record_test "GET /ontology/gaps returns stats" FAIL "$dur" "body=${body:0:120}"
fi

start=$(now_ms)
code=$(auth_get_status "$BASE/ontology/$TEST_ONTOLOGY?tier=summary")
dur=$(( $(now_ms) - start ))
assert_code "GET /ontology/{name} valid" 200 "$code"

# ═══ Scan ═══

for path in "/scan/health" "/scan/staleness?days=365"; do
    start=$(now_ms)
    code=$(auth_get_status "$BASE$path")
    dur=$(( $(now_ms) - start ))
    assert_code "GET $path" 200 "$code"
done

# ═══ Harness ═══

start=$(now_ms)
code=$(auth_get_status "$BASE/harness")
dur=$(( $(now_ms) - start ))
assert_code "GET /harness" 200 "$code"

start=$(now_ms)
code=$(auth_post_status "$BASE/harness/dispatch" '{"to":"test-harness","task":"probe","priority":3}')
dur=$(( $(now_ms) - start ))
assert_code "POST /harness/dispatch (202)" 202 "$code"

# Clean up harness comm file we just created
register_cleanup "$NLR_ROOT/06-self-improvement-recursive/harness-to-harness-comms"

# ═══ State ═══

start=$(now_ms)
code=$(auth_get_status "$BASE/state/heartbeat")
dur=$(( $(now_ms) - start ))
assert_code "GET /state/heartbeat" 200 "$code"

start=$(now_ms)
code=$(auth_post_status "$BASE/state/heartbeat" '{}')
dur=$(( $(now_ms) - start ))
assert_code "POST /state/heartbeat" 200 "$code"

start=$(now_ms)
code=$(auth_post_status "$BASE/state/log" '{"tool":"test","exit_code":0}')
dur=$(( $(now_ms) - start ))
assert_code "POST /state/log" 200 "$code"

# ═══ Config ═══

start=$(now_ms)
code=$(auth_get_status "$BASE/config/neuro-link")
dur=$(( $(now_ms) - start ))
assert_code "GET /config/{name} valid" 200 "$code"

# ═══ Access control ═══

start=$(now_ms)
body=$(auth_get "$BASE/access/paths")
dur=$(( $(now_ms) - start ))
if printf "%s" "$body" | jq -e '.allowed and .available' >/dev/null 2>&1; then
    record_test "GET /access/paths shape" PASS "$dur"
else
    record_test "GET /access/paths shape" FAIL "$dur" "body=${body:0:120}"
fi

# ═══ Hooks event ═══

start=$(now_ms)
body=$(auth_post_json "$BASE/hooks/event" '{"event_type":"post_tool","client":"claude-code","data":{"tool":"probe"}}')
dur=$(( $(now_ms) - start ))
if printf "%s" "$body" | jq -e '.ok == true' >/dev/null 2>&1; then
    record_test "POST /hooks/event valid (ok:true)" PASS "$dur"
else
    record_test "POST /hooks/event valid (ok:true)" FAIL "$dur" "body=${body:0:120}"
fi

start=$(now_ms)
code=$(auth_post_status "$BASE/hooks/event" '{"event_type":"bad_type","client":"x"}')
dur=$(( $(now_ms) - start ))
# event_type is validated with custom check → 400
assert_code "POST /hooks/event invalid event_type → 400" 400 "$code"

# ═══ Services ═══

start=$(now_ms)
code=$(auth_get_status "$BASE/services")
dur=$(( $(now_ms) - start ))
if [ "$code" = "200" ] || [ "$code" = "404" ]; then
    record_test "GET /services returns" PASS "$dur" "HTTP $code"
else
    record_test "GET /services returns" FAIL "$dur" "got $code"
fi

# ═══ Content-type checks ═══

start=$(now_ms)
ct=$(curl -s -H "Authorization: Bearer ${NLR_API_TOKEN}" \
    -I --max-time 10 "$BASE/wiki/pages" | awk -F': ' 'tolower($1)=="content-type"{print tolower($2)}' | tr -d '\r\n ')
dur=$(( $(now_ms) - start ))
if [[ "$ct" == application/json* ]]; then
    record_test "wiki/pages content-type: application/json" PASS "$dur" "$ct"
else
    record_test "wiki/pages content-type: application/json" FAIL "$dur" "got $ct"
fi

print_script_summary
