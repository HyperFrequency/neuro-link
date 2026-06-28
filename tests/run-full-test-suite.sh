#!/usr/bin/env bash
# neuro-link-recursive — full test suite runner.
#
# Usage:
#   NLR_API_TOKEN=... ./run-full-test-suite.sh [flags]
#
# Flags:
#   --fast        Skip slow tests (rag precision, network/ngrok, security timing).
#   --full        Run every test (default).
#   --ci          Machine-readable JSON output to stdout. Also disables color.
#   --filter PAT  Only run scripts whose domain/name matches bash-glob PAT.
#   --parallel    Run independent domains in parallel (more stress on server).
#   --keep        Do not clean test artifacts after run.
#   --help
#
# Domains (and their expected runtimes on a warm server):
#   mcp       ~15s   — 34 tools × positive+negative+schema (~100 tests)
#   rest      ~10s   — every route × 3-4 cases (~55 tests)
#   pipeline  ~25s   — filesystem watcher debounce + polling
#   sessions  ~10s   — synthetic JSONL parse
#   quality   ~10s   — 5 fixture scans
#   proxy     ~10s   — LLM proxy endpoints
#   rag       ~60s   — 30 gold queries (slow)
#   neo4j     ~5s    — schema + match smoke
#   network   ~30s   — ngrok + remote latency (slow)
#   security  ~30s   — 200 total HTTP reqs for timing (slow)

set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./lib/common.sh
source "$SCRIPT_DIR/lib/common.sh"

FAST=0
CI_MODE=0
FILTER=""
PARALLEL=0
KEEP=0

while [ $# -gt 0 ]; do
    case "$1" in
        --fast) FAST=1; shift ;;
        --full) FAST=0; shift ;;
        --ci) CI_MODE=1; shift ;;
        --filter) FILTER="$2"; shift 2 ;;
        --parallel) PARALLEL=1; shift ;;
        --keep) KEEP=1; shift ;;
        -h|--help)
            grep -E '^#' "$0" | sed 's/^# \?//'
            exit 0
            ;;
        *)
            printf "Unknown flag: %s\n" "$1" >&2
            exit 2
            ;;
    esac
done

export CI_MODE
export NLR_TEST_KEEP_ARTIFACTS="$KEEP"

# Initialize result log
RESULT_LOG="/tmp/nlr-test-results-$$.jsonl"
export RESULT_LOG
: > "$RESULT_LOG"

# Load token from secrets/.env if not already set
if [ -z "${NLR_API_TOKEN:-}" ]; then
    if [ -f "$NLR_ROOT/secrets/.env" ]; then
        tok=$(grep -E '^NLR_API_TOKEN=' "$NLR_ROOT/secrets/.env" 2>/dev/null | tail -1 | cut -d= -f2-)
        if [ -n "$tok" ]; then
            export NLR_API_TOKEN="$tok"
        fi
    fi
fi

if [ -z "${NLR_API_TOKEN:-}" ]; then
    printf "${RED}ERROR: NLR_API_TOKEN not set and not found in secrets/.env${RESET}\n" >&2
    exit 2
fi

# Domain registry: name, script, is-slow
declare -a DOMAINS=(
    "mcp|mcp/test_each_tool.sh|0"
    "rest|rest/test_endpoints.sh|0"
    "pipeline|pipeline/test_watcher.sh|0"
    "sessions|sessions/test_session_parse.sh|0"
    "quality|quality/test_violations.sh|0"
    "proxy|proxy/test_llm_proxy.sh|0"
    "rag|rag/test_hybrid_precision.sh|1"
    "neo4j|neo4j/test_constraints.sh|0"
    "network|network/test_ngrok.sh|1"
    "security|security/test_auth_and_limits.sh|1"
)

# Pre-flight: make all test scripts executable
for entry in "${DOMAINS[@]}"; do
    IFS='|' read -r _ path _ <<< "$entry"
    chmod +x "$SCRIPT_DIR/$path" 2>/dev/null || true
done
chmod +x "$SCRIPT_DIR/lib/common.sh" 2>/dev/null || true

# P16 — heartbeat freshness check (non-fatal; records a test result either way)
if [ -x "$NLR_ROOT/scripts/check-heartbeat.sh" ]; then hb_out=$("$NLR_ROOT/scripts/check-heartbeat.sh" 2>&1); hb_status=$?; if [ "$hb_status" = "0" ]; then printf '{"domain":"heartbeat","name":"freshness","status":"PASS","duration_ms":0,"details":%s}\n' "$(printf '%s' "$hb_out" | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read().strip()))')" >> "$RESULT_LOG"; else printf '{"domain":"heartbeat","name":"freshness","status":"FAIL","duration_ms":0,"details":%s}\n' "$(printf '%s' "$hb_out" | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read().strip()))')" >> "$RESULT_LOG"; fi; fi

# Print banner
banner() {
    [ "$CI_MODE" = "1" ] && return
    printf "\n"
    printf "%s╔═════════════════════════════════════════════════════════════╗%s\n" "${BOLD}" "${RESET}"
    printf "%s║  neuro-link-recursive — full test suite                     ║%s\n" "${BOLD}" "${RESET}"
    printf "%s║  fast=%d  filter='%s'  parallel=%d  keep=%d                         ║%s\n" "${BOLD}" "$FAST" "$FILTER" "$PARALLEL" "$KEEP" "${RESET}"
    printf "%s╚═════════════════════════════════════════════════════════════╝%s\n" "${BOLD}" "${RESET}"
}
banner

# ── Execute scripts ──
run_domain() {
    local name="$1" path="$2" is_slow="$3"
    local full="$SCRIPT_DIR/$path"

    if [ ! -x "$full" ]; then
        printf '{"domain":"%s","name":"script exists","status":"FAIL","duration_ms":0,"details":"not executable: %s"}\n' "$name" "$full" >> "$RESULT_LOG"
        return 1
    fi

    # Run the script — each script prints its own per-test output and appends to RESULT_LOG
    if "$full"; then
        return 0
    else
        return 1
    fi
}

overall_start=$(date +%s)
declare -a SELECTED=()
for entry in "${DOMAINS[@]}"; do
    IFS='|' read -r name path is_slow <<< "$entry"
    if [ "$FAST" = "1" ] && [ "$is_slow" = "1" ]; then
        # Record a skip marker per slow-domain (so summary shows EXPECTED_SKIP)
        printf '{"domain":"%s","name":"domain run (--fast)","status":"SKIP","duration_ms":0,"details":"skipped in --fast mode"}\n' "$name" >> "$RESULT_LOG"
        continue
    fi
    if [ -n "$FILTER" ] && ! [[ "$name" == $FILTER ]] && ! [[ "$path" == $FILTER ]]; then
        continue
    fi
    SELECTED+=("$entry")
done

if [ ${#SELECTED[@]} -eq 0 ]; then
    printf "${YELLOW}No domains matched filter '%s'${RESET}\n" "$FILTER" >&2
fi

if [ "$PARALLEL" = "1" ]; then
    # Parallel mode: fork each domain, collect exit codes
    declare -a PIDS=()
    declare -a NAMES=()
    for entry in "${SELECTED[@]}"; do
        IFS='|' read -r name path _ <<< "$entry"
        # Each subshell gets its own set of env; RESULT_LOG appends are safe-ish with O_APPEND on POSIX
        ( run_domain "$name" "$path" "$is_slow" ) &
        PIDS+=($!)
        NAMES+=("$name")
    done
    # Wait for all
    for i in "${!PIDS[@]}"; do
        wait "${PIDS[$i]}" || true
    done
else
    for entry in "${SELECTED[@]}"; do
        IFS='|' read -r name path is_slow <<< "$entry"
        run_domain "$name" "$path" "$is_slow" || true
    done
fi

overall_end=$(date +%s)
elapsed=$((overall_end - overall_start))

# ── Summary / output ──

if [ "$CI_MODE" = "1" ]; then
    # Machine-readable JSON
    python3 - "$RESULT_LOG" "$elapsed" <<'PYEOF'
import json, sys, collections
log_path = sys.argv[1]
elapsed = int(sys.argv[2])
results = []
with open(log_path) as f:
    for line in f:
        line = line.strip()
        if not line:
            continue
        try:
            results.append(json.loads(line))
        except json.JSONDecodeError:
            continue
counts = collections.Counter(r["status"] for r in results)
total = len(results)
out = {
    "total": total,
    "passed": counts.get("PASS", 0),
    "failed": counts.get("FAIL", 0),
    "skipped": counts.get("SKIP", 0),
    "expected_skipped": counts.get("EXPECTED_SKIP", 0),
    "elapsed_s": elapsed,
    "results": results,
}
print(json.dumps(out, indent=2))
PYEOF
    # Exit code
    failed=$(grep -c '"status":"FAIL"' "$RESULT_LOG" 2>/dev/null || true)
    [ -z "$failed" ] && failed=0
    if [ "$failed" -gt 0 ]; then exit 1; else exit 0; fi
else
    # Human-readable summary
    total=$(wc -l < "$RESULT_LOG" | tr -d ' ')
    pass=$(grep -c '"status":"PASS"' "$RESULT_LOG" 2>/dev/null || true); [ -z "$pass" ] && pass=0
    fail=$(grep -c '"status":"FAIL"' "$RESULT_LOG" 2>/dev/null || true); [ -z "$fail" ] && fail=0
    skip=$(grep -c '"status":"SKIP"' "$RESULT_LOG" 2>/dev/null || true); [ -z "$skip" ] && skip=0
    xskip=$(grep -c '"status":"EXPECTED_SKIP"' "$RESULT_LOG" 2>/dev/null || true); [ -z "$xskip" ] && xskip=0

    printf "\n%s═══════════════════════════════════════════════════════════%s\n" "${BOLD}" "${RESET}"
    printf "%sSUMMARY%s  %d tests | %s%d passed%s | %s%d failed%s | %s%d skip%s | %s%d expected_skip%s | %ds\n" \
        "${BOLD}" "${RESET}" "$total" \
        "${GREEN}" "$pass" "${RESET}" \
        "${RED}" "$fail" "${RESET}" \
        "${YELLOW}" "$skip" "${RESET}" \
        "${YELLOW}" "$xskip" "${RESET}" \
        "$elapsed"

    if [ "$fail" -gt 0 ]; then
        printf "\n%sFailures:%s\n" "${RED}" "${RESET}"
        python3 - "$RESULT_LOG" <<'PYEOF'
import json, sys
with open(sys.argv[1]) as f:
    for line in f:
        r = json.loads(line)
        if r["status"] != "FAIL": continue
        print(f"  [{r['domain']}] {r['name']} — {r['details'][:200]}")
PYEOF
    fi

    printf "\nFull results: %s\n" "$RESULT_LOG"
    if [ "$fail" -gt 0 ]; then exit 1; else exit 0; fi
fi
