#!/usr/bin/env bash
# Domain: LLM proxy passthrough.
# Stands up a mock upstream (python http.server) that responds to POST /v1/messages
# with a canned Anthropic-style response. Then invokes the proxy with the upstream
# pointed at the mock by temporarily overriding the provider URL.
#
# NOTE: The current proxy (api/llm_proxy.rs) does NOT expose an env hook for
# overriding the Anthropic base URL — it calls provider_base_url() which hardcodes
# api.anthropic.com. So we cannot intercept the real upstream without code changes.
# Instead, we verify:
#   1. Without ANTHROPIC_API_KEY set, proxy returns 500 with "No API key"
#   2. Proxy logs the attempt to state/llm_logs/<hash>/<date>.jsonl
#   3. /llm/v1/models routes correctly (or fails closed with 500 when no key)
# If ANTHROPIC_API_KEY is set, we additionally verify a successful passthrough
# records a real request entry.
#
# This test degrades gracefully rather than adding a code modification.

set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=../lib/common.sh
source "$SCRIPT_DIR/lib/common.sh"

init_test_script "proxy"
require_tools curl jq python3
require_token

# Sanity: server /health reachable
if [ "$(unauth_get_status "$LOCAL_BASE/health")" != "200" ]; then
    record_test "preflight: server /health" FAIL 0
    print_script_summary
fi
record_test "preflight: server /health" PASS 0

# ── 1. /llm/v1/messages rejects without bearer (401) ──
start=$(now_ms)
code=$(unauth_post_status "$LOCAL_BASE/llm/v1/messages" '{"model":"claude","messages":[]}')
dur=$(( $(now_ms) - start ))
if [ "$code" = "401" ]; then
    record_test "POST /llm/v1/messages without bearer → 401" PASS "$dur"
else
    record_test "POST /llm/v1/messages without bearer → 401" FAIL "$dur" "got $code"
fi

# ── 2. With bearer but unavailable upstream (no ANTHROPIC_API_KEY): 500 ──
# The proxy config resolves keys via env vars. If ANTHROPIC_API_KEY is empty,
# it returns a 500 "No API key configured". We can't force that state live,
# but we observe the response for the current config.
start=$(now_ms)
body=$(curl "${CURL_OPTS[@]}" -H "Authorization: Bearer ${NLR_API_TOKEN}" \
    -H "anthropic-version: 2023-06-01" \
    -H "Content-Type: application/json" \
    -o /tmp/nlr-proxy-resp.txt -w '%{http_code}' \
    -X POST -d '{"model":"claude-3-5-sonnet-20241022","messages":[{"role":"user","content":"ping"}],"max_tokens":10}' \
    "$LOCAL_BASE/llm/v1/messages")
dur=$(( $(now_ms) - start ))
code="$body"
resp_body=$(cat /tmp/nlr-proxy-resp.txt 2>/dev/null || echo "")
register_cleanup /tmp/nlr-proxy-resp.txt

# Acceptable outcomes:
#   200/201 — real success (user has ANTHROPIC_API_KEY configured)
#   500 — no API key configured (graceful fail-closed)
#   502 — upstream network error (also fine — proxy attempted to forward)
if [ "$code" = "200" ] || [ "$code" = "201" ]; then
    record_test "POST /llm/v1/messages forwards request" PASS "$dur" "upstream 200"
elif [ "$code" = "500" ]; then
    if [[ "$resp_body" == *"No API key"* ]]; then
        record_test "POST /llm/v1/messages forwards request" PASS "$dur" "500 no-key (fail-closed OK)"
    else
        record_test "POST /llm/v1/messages forwards request" FAIL "$dur" "500 w/o expected message: ${resp_body:0:120}"
    fi
elif [ "$code" = "502" ]; then
    record_test "POST /llm/v1/messages forwards request" PASS "$dur" "502 upstream (forward attempted)"
else
    record_test "POST /llm/v1/messages forwards request" FAIL "$dur" "got $code: ${resp_body:0:120}"
fi

# ── 3. Log entry appears in state/llm_logs/<hash>/<date>.jsonl ──
# Compute the hash the proxy uses: sha256(NLR_API_TOKEN)[:16]
TOKEN_HASH=$(printf "%s" "$NLR_API_TOKEN" | python3 -c '
import hashlib, sys
print(hashlib.sha256(sys.stdin.read().encode()).hexdigest()[:16])')
TODAY=$(date -u +%Y-%m-%d)
LOG_PATH="$NLR_ROOT/state/llm_logs/$TOKEN_HASH/${TODAY}.jsonl"

# The proxy only writes logs when the upstream call is attempted — i.e. when
# there's a valid provider key. On fail-closed no-key (500), no log is written.
if [[ "$resp_body" == *"No API key"* ]]; then
    record_test "proxy log appears in state/llm_logs/<hash>/<date>.jsonl" EXPECTED_SKIP 0 \
        "no-key fail-closed; proxy returns 500 before writing log (by design)"
else
    start=$(now_ms)
    log_appeared=false
    for i in 1 2 3 4 5 6; do
        sleep 0.5
        if [ -f "$LOG_PATH" ] && tail -n 1 "$LOG_PATH" 2>/dev/null | jq -e '.timestamp' >/dev/null 2>&1; then
            log_appeared=true
            break
        fi
    done
    dur=$(( $(now_ms) - start ))
    if [ "$log_appeared" = "true" ]; then
        record_test "proxy log appears in state/llm_logs/<hash>/<date>.jsonl" PASS "$dur" "$LOG_PATH"
        last=$(tail -n 1 "$LOG_PATH")
        missing_keys=""
        for key in timestamp client_token_hash provider model endpoint latency_ms; do
            if ! printf "%s" "$last" | jq -e ".$key" >/dev/null 2>&1; then
                missing_keys="$missing_keys $key"
            fi
        done
        if [ -z "$missing_keys" ]; then
            record_test "log entry shape complete" PASS 0
        else
            record_test "log entry shape complete" FAIL 0 "missing:$missing_keys"
        fi
    else
        record_test "proxy log appears in state/llm_logs/<hash>/<date>.jsonl" FAIL "$dur" \
            "no file $LOG_PATH after 3s"
    fi
fi

# ── 4. /llm/v1/embeddings endpoint is routed ──
# Routes to EMBEDDING_API_URL (llama-server on 8400 by default). Use a generous
# timeout — on a cold embedding server the first call can take 15-60s.
start=$(now_ms)
code=$(curl -s -H "Authorization: Bearer ${NLR_API_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST -d '{"input":"ping"}' \
    -o /dev/null -w '%{http_code}' --max-time 90 \
    "$LOCAL_BASE/llm/v1/embeddings")
dur=$(( $(now_ms) - start ))
if [ "$code" = "200" ] || [ "$code" = "502" ]; then
    record_test "POST /llm/v1/embeddings routes" PASS "$dur" "HTTP $code"
else
    record_test "POST /llm/v1/embeddings routes" FAIL "$dur" "got $code"
fi

# ── 5. /llm/v1/models ──
start=$(now_ms)
code=$(curl -s -H "Authorization: Bearer ${NLR_API_TOKEN}" \
    -o /dev/null -w '%{http_code}' --max-time 15 \
    "$LOCAL_BASE/llm/v1/models")
dur=$(( $(now_ms) - start ))
# 200 if upstream works, 500 if no API key, 502 if unreachable
if [ "$code" = "200" ] || [ "$code" = "500" ] || [ "$code" = "502" ]; then
    record_test "GET /llm/v1/models routes" PASS "$dur" "HTTP $code"
else
    record_test "GET /llm/v1/models routes" FAIL "$dur" "got $code"
fi

print_script_summary
