#!/usr/bin/env bash
# Domain: ngrok reachability.
# - GET $NGROK_URL/health → 200
# - POST $NGROK_URL/mcp with tools/list → >=30 tools
# - GET $NGROK_URL/api/v1/rag/query?q=test → 200

set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=../lib/common.sh
source "$SCRIPT_DIR/lib/common.sh"

init_test_script "network"
require_tools curl jq
require_token

NGROK_URL="${PUBLIC_BASE}"

# ── 1. /health ──
start=$(now_ms)
code=$(curl -s -H "ngrok-skip-browser-warning: 1" -o /dev/null -w '%{http_code}' --max-time 15 "$NGROK_URL/health")
dur=$(( $(now_ms) - start ))
if [ "$code" = "200" ]; then
    record_test "GET $NGROK_URL/health" PASS "$dur"
else
    record_test "GET $NGROK_URL/health" FAIL "$dur" "got $code"
    # If /health fails, short-circuit
    print_script_summary
fi

# ── 2. POST /mcp tools/list >= 30 ──
start=$(now_ms)
body=$(curl -s -H "Authorization: Bearer ${NLR_API_TOKEN}" \
    -H "Content-Type: application/json" \
    -H "Accept: application/json, text/event-stream" \
    -H "ngrok-skip-browser-warning: 1" \
    --max-time 20 \
    -X POST -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' \
    "$NGROK_URL/mcp")
dur=$(( $(now_ms) - start ))
jsonline=$(printf "%s" "$body" | sed -n 's/^data: //p' | head -n 1)
if [ -z "$jsonline" ]; then jsonline="$body"; fi
count=$(printf "%s" "$jsonline" | jq -r '.result.tools | length // 0' 2>/dev/null)
if [[ "$count" =~ ^[0-9]+$ ]] && [ "$count" -ge 30 ]; then
    record_test "POST /mcp tools/list via ngrok ≥ 30 tools" PASS "$dur" "count=$count"
else
    record_test "POST /mcp tools/list via ngrok ≥ 30 tools" FAIL "$dur" "count=$count body=${body:0:200}"
fi

# ── 3. /api/v1/rag/query ──
start=$(now_ms)
# RAG queries can take 5-60s when the embedding server is cold; allow up to 90s.
code=$(curl -s -H "Authorization: Bearer ${NLR_API_TOKEN}" \
    -H "ngrok-skip-browser-warning: 1" \
    -o /dev/null -w '%{http_code}' --max-time 90 \
    "$NGROK_URL/api/v1/rag/query?q=test")
dur=$(( $(now_ms) - start ))
if [ "$code" = "200" ]; then
    record_test "GET /api/v1/rag/query?q=test via ngrok" PASS "$dur"
else
    record_test "GET /api/v1/rag/query?q=test via ngrok" FAIL "$dur" "got $code"
fi

# ── 4. /api/v1/wiki/pages (sanity: auth works) ──
start=$(now_ms)
code=$(curl -s -H "Authorization: Bearer ${NLR_API_TOKEN}" \
    -H "ngrok-skip-browser-warning: 1" \
    -o /dev/null -w '%{http_code}' --max-time 15 \
    "$NGROK_URL/api/v1/wiki/pages")
dur=$(( $(now_ms) - start ))
if [ "$code" = "200" ]; then
    record_test "GET /api/v1/wiki/pages via ngrok (auth)" PASS "$dur"
else
    record_test "GET /api/v1/wiki/pages via ngrok (auth)" FAIL "$dur" "got $code"
fi

# ── 5. Without auth → 401 (fail-closed over tunnel) ──
start=$(now_ms)
code=$(curl -s -H "ngrok-skip-browser-warning: 1" \
    -o /dev/null -w '%{http_code}' --max-time 15 \
    "$NGROK_URL/api/v1/wiki/pages")
dur=$(( $(now_ms) - start ))
if [ "$code" = "401" ]; then
    record_test "GET /api/v1/wiki/pages via ngrok without auth → 401" PASS "$dur"
else
    record_test "GET /api/v1/wiki/pages via ngrok without auth → 401" FAIL "$dur" "got $code"
fi

print_script_summary
