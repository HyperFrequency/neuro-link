#!/usr/bin/env bash
# Shared helpers for neuro-link-recursive test suite.
# Sourced by every test script. Provides:
#   - env defaults (base URLs, tokens, MCP binary path)
#   - colored PASS/FAIL output
#   - per-test timing
#   - emit_result (appends to $RESULT_LOG in JSON for the central runner)
#   - require_* preflight checks
#
# Contract: each test script must call `init_test_script <domain>` before any
# test, then use `record_test <name> <status> <duration_ms> [details]` to log.
# `status` is one of PASS, FAIL, SKIP, EXPECTED_SKIP.

set -uo pipefail

# ── Environment defaults ──
LOCAL_BASE="${NLR_LOCAL_BASE:-http://localhost:8080}"
PUBLIC_BASE="${NLR_PUBLIC_BASE:-https://petal-privacy-disrupt.ngrok-free.dev}"
MCP_BIN="${NLR_MCP_BIN:-/usr/local/bin/neuro-link}"
QDRANT_BASE="${NLR_QDRANT_BASE:-http://localhost:6333}"
NEO4J_BASE="${NLR_NEO4J_BASE:-http://localhost:7474}"
NEO4J_AUTH="${NLR_NEO4J_AUTH:-neo4j:neurolink1234}"
OLLAMA_BASE="${NLR_OLLAMA_BASE:-http://localhost:11434}"
LLAMA_BASE="${NLR_LLAMA_BASE:-http://localhost:8400}"
NLR_ROOT="${NLR_ROOT:-/Users/DanBot/Desktop/HyperFrequency/neuro-link}"
RESULT_LOG="${RESULT_LOG:-/tmp/nlr-test-results.jsonl}"

# ── Color output (disable in CI / when piped) ──
if [ "${CI_MODE:-0}" = "1" ] || [ ! -t 1 ]; then
    GREEN=""; RED=""; YELLOW=""; BLUE=""; BOLD=""; RESET=""
else
    GREEN=$'\033[0;32m'
    RED=$'\033[0;31m'
    YELLOW=$'\033[0;33m'
    BLUE=$'\033[0;34m'
    BOLD=$'\033[1m'
    RESET=$'\033[0m'
fi

# ── Counters ──
SCRIPT_PASS=0
SCRIPT_FAIL=0
SCRIPT_SKIP=0
SCRIPT_DOMAIN="unknown"

# ── Init ──
init_test_script() {
    SCRIPT_DOMAIN="$1"
    if [ "${CI_MODE:-0}" != "1" ]; then
        printf "\n%s=== [%s] ===%s\n" "${BOLD}${BLUE}" "$SCRIPT_DOMAIN" "${RESET}"
    fi
}

# ── Preflight ──
require_tools() {
    for bin in "$@"; do
        if ! command -v "$bin" >/dev/null 2>&1; then
            printf "%sERROR: required tool '%s' not found%s\n" "${RED}" "$bin" "${RESET}" >&2
            exit 2
        fi
    done
}

require_token() {
    if [ -z "${NLR_API_TOKEN:-}" ]; then
        # Try loading from secrets/.env
        if [ -f "$NLR_ROOT/secrets/.env" ]; then
            tok="$(grep -E '^NLR_API_TOKEN=' "$NLR_ROOT/secrets/.env" 2>/dev/null | tail -1 | cut -d= -f2-)"
            if [ -n "$tok" ]; then
                export NLR_API_TOKEN="$tok"
                return 0
            fi
        fi
        printf "%sERROR: NLR_API_TOKEN env var is required%s\n" "${RED}" "${RESET}" >&2
        exit 2
    fi
}

# ── JSON-escape a string for safe inclusion in record_test details ──
json_escape() {
    # Use python3 because pure bash escaping is fragile with control chars.
    # Strip trailing newline from the heredoc read.
    python3 -c 'import json,sys; s=sys.stdin.read().rstrip("\n"); sys.stdout.write(json.dumps(s))' <<< "$1"
}

# ── Record a test result ──
# usage: record_test <name> <PASS|FAIL|SKIP|EXPECTED_SKIP> <duration_ms> [details]
record_test() {
    local name="$1" status="$2" duration_ms="${3:-0}" details="${4:-}"
    case "$status" in
        PASS) SCRIPT_PASS=$((SCRIPT_PASS + 1)) ;;
        FAIL) SCRIPT_FAIL=$((SCRIPT_FAIL + 1)) ;;
        SKIP|EXPECTED_SKIP) SCRIPT_SKIP=$((SCRIPT_SKIP + 1)) ;;
    esac

    # Always append machine-readable JSONL
    local esc_details
    esc_details=$(json_escape "$details")
    printf '{"domain":"%s","name":%s,"status":"%s","duration_ms":%s,"details":%s}\n' \
        "$SCRIPT_DOMAIN" \
        "$(json_escape "$name")" \
        "$status" \
        "$duration_ms" \
        "$esc_details" >> "$RESULT_LOG"

    # Human-readable (unless CI mode)
    if [ "${CI_MODE:-0}" != "1" ]; then
        local icon color
        case "$status" in
            PASS) icon="PASS"; color="$GREEN" ;;
            FAIL) icon="FAIL"; color="$RED" ;;
            SKIP) icon="SKIP"; color="$YELLOW" ;;
            EXPECTED_SKIP) icon="SKIP*"; color="$YELLOW" ;;
        esac
        local suffix=""
        if [ -n "$details" ]; then
            # Truncate details to keep line readable
            local trunc
            if [ ${#details} -gt 80 ]; then
                trunc="${details:0:77}..."
            else
                trunc="$details"
            fi
            suffix=" — $trunc"
        fi
        printf "  %s%-6s%s %-52s %5sms%s\n" \
            "$color" "$icon" "$RESET" "$name" "$duration_ms" "$suffix"
    fi
}

# ── Timing helper: run a command block and record result ──
# usage: run_test <name> <command_that_exits_nonzero_on_fail> [expected_skip_reason]
run_test_cmd() {
    local name="$1"; shift
    local start end dur
    start=$(($(date +%s%N 2>/dev/null || python3 -c 'import time; print(int(time.time()*1e9))')))
    local output exit_code
    output=$("$@" 2>&1) && exit_code=0 || exit_code=$?
    end=$(($(date +%s%N 2>/dev/null || python3 -c 'import time; print(int(time.time()*1e9))')))
    dur=$(( (end - start) / 1000000 ))
    if [ "$exit_code" -eq 0 ]; then
        record_test "$name" PASS "$dur" "$output"
    else
        record_test "$name" FAIL "$dur" "$output (exit=$exit_code)"
    fi
}

# ── Timing primitives (monotonic-enough millisecond clock) ──
now_ms() {
    python3 -c 'import time; print(int(time.time()*1000))'
}

# ── HTTP curl helpers ──
CURL_OPTS=(--silent --show-error --max-time 20)

auth_get() {
    local url="$1"
    curl "${CURL_OPTS[@]}" -H "Authorization: Bearer ${NLR_API_TOKEN}" "$url"
}

auth_post_json() {
    local url="$1" body="$2"
    curl "${CURL_OPTS[@]}" -H "Authorization: Bearer ${NLR_API_TOKEN}" \
        -H "Content-Type: application/json" \
        -X POST -d "$body" "$url"
}

auth_post_status() {
    local url="$1" body="$2"
    curl "${CURL_OPTS[@]}" -H "Authorization: Bearer ${NLR_API_TOKEN}" \
        -H "Content-Type: application/json" \
        -X POST -d "$body" -o /dev/null -w '%{http_code}' "$url"
}

auth_get_status() {
    local url="$1"
    curl "${CURL_OPTS[@]}" -H "Authorization: Bearer ${NLR_API_TOKEN}" \
        -o /dev/null -w '%{http_code}' "$url"
}

unauth_get_status() {
    local url="$1"
    curl "${CURL_OPTS[@]}" -o /dev/null -w '%{http_code}' "$url"
}

unauth_post_status() {
    local url="$1" body="$2"
    curl "${CURL_OPTS[@]}" -H "Content-Type: application/json" \
        -X POST -d "$body" -o /dev/null -w '%{http_code}' "$url"
}

# ── Portable timeout (macOS lacks GNU `timeout`) ──
# Usage: with_timeout <seconds> <cmd> [args...]
with_timeout() {
    local secs="$1"; shift
    perl -e 'alarm shift; exec @ARGV' "$secs" "$@"
}

# ── MCP helper: POST to MCP over stdio via neuro-link mcp ──
mcp_stdio_call() {
    local method="$1" params="${2:-}"
    local req
    if [ -z "$params" ]; then
        req=$(printf '{"jsonrpc":"2.0","id":1,"method":"%s"}' "$method")
    else
        req=$(printf '{"jsonrpc":"2.0","id":1,"method":"%s","params":%s}' "$method" "$params")
    fi
    # neuro-link mcp reads stdin line-by-line; close stdin to make it exit cleanly
    printf '%s\n' "$req" | with_timeout 10 "$MCP_BIN" mcp 2>/dev/null | grep -E '^\{' | head -n 1
}

# ── Cleanup registry for test-local artifacts ──
declare -a CLEANUP_PATHS=()

register_cleanup() {
    CLEANUP_PATHS+=("$1")
}

run_cleanup() {
    # bash 3.2 errors on empty array expansion under set -u; guard with length check
    if [ "${#CLEANUP_PATHS[@]:-0}" -gt 0 ]; then
        for p in "${CLEANUP_PATHS[@]}"; do
            rm -rf "$p" 2>/dev/null || true
        done
    fi
    CLEANUP_PATHS=()
}

# ensure cleanup on exit unless NLR_TEST_KEEP_ARTIFACTS=1
trap '[ "${NLR_TEST_KEEP_ARTIFACTS:-0}" = "1" ] || run_cleanup' EXIT

# ── Script summary (for individual script runs) ──
print_script_summary() {
    local total=$((SCRIPT_PASS + SCRIPT_FAIL + SCRIPT_SKIP))
    if [ "${CI_MODE:-0}" != "1" ]; then
        printf "\n  %s[%s]%s %d tests | %s%d pass%s | %s%d fail%s | %s%d skip%s\n" \
            "${BOLD}" "$SCRIPT_DOMAIN" "${RESET}" "$total" \
            "${GREEN}" "$SCRIPT_PASS" "${RESET}" \
            "${RED}" "$SCRIPT_FAIL" "${RESET}" \
            "${YELLOW}" "$SCRIPT_SKIP" "${RESET}"
    fi
    if [ "$SCRIPT_FAIL" -gt 0 ]; then
        exit 1
    fi
    exit 0
}
