#!/usr/bin/env bash
# P19 — RAG latency benchmark.
#
# Reads 20 pre-canned queries from scripts/bench_queries.txt, hits both the
# local REST RAG endpoint and (if reachable) the ngrok tunnel. For each query
# records: latency ms, top-1 hit path, RAG score. Computes p50 and p95 and
# writes a CSV to state/bench/<iso_date>.csv.
#
# Always exits 0 — this is informational, not a gate. Human summary prints
# to stderr so CSV on stdout is usable.

set -uo pipefail

NLR_ROOT="${NLR_ROOT:-$HOME/neuro-link}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
QUERIES_FILE="${NLR_BENCH_QUERIES:-$SCRIPT_DIR/bench_queries.txt}"
LOCAL_BASE="${NLR_LOCAL_BASE:-http://localhost:8080}"
OUT_DIR="$NLR_ROOT/state/bench"
DATE_ISO="$(date -u +%Y-%m-%dT%H-%M-%SZ)"
OUT_CSV="$OUT_DIR/${DATE_ISO}.csv"

log() { printf "%s\n" "$*" >&2; }

if [ ! -f "$QUERIES_FILE" ]; then
    log "FAIL: queries file not found at $QUERIES_FILE"
    exit 0
fi

# Pull API token from secrets/.env if not in env.
if [ -z "${NLR_API_TOKEN:-}" ] && [ -f "$NLR_ROOT/secrets/.env" ]; then
    NLR_API_TOKEN=$(grep -E '^NLR_API_TOKEN=' "$NLR_ROOT/secrets/.env" 2>/dev/null | tail -1 | cut -d= -f2-)
fi
TOKEN="${NLR_API_TOKEN:-}"

# Discover ngrok URL (best effort).
PUBLIC_BASE=""
if command -v curl >/dev/null 2>&1; then
    PUBLIC_BASE=$(curl -s --max-time 2 http://localhost:4040/api/tunnels 2>/dev/null \
        | python3 -c '
import json, sys
try:
    data = json.load(sys.stdin)
    for t in data.get("tunnels", []):
        if t.get("public_url", "").startswith("https://"):
            print(t["public_url"]); break
except Exception:
    pass
' 2>/dev/null)
fi
PUBLIC_BASE="${PUBLIC_BASE:-${NLR_PUBLIC_BASE:-}}"

mkdir -p "$OUT_DIR"
# CSV header.
printf "target,query,latency_ms,status_code,top1_path,score\n" > "$OUT_CSV"

# Run one query against one base URL. Appends one CSV row. Echoes latency_ms to stdout.
run_one() {
    local target="$1" base="$2" query="$3"
    local body t0 t1 latency_ms status tmp_resp tmp_headers top1 score auth_arg

    [ -z "$base" ] && return 0

    tmp_resp=$(mktemp)
    tmp_headers=$(mktemp)
    body=$(printf '{"query":%s,"limit":3,"mode":"hybrid-rrf"}' "$(printf '%s' "$query" | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()))')")

    auth_arg=()
    if [ -n "$TOKEN" ]; then
        auth_arg=(-H "Authorization: Bearer $TOKEN")
    fi

    t0=$(python3 -c 'import time;print(int(time.time()*1000))')
    status=$(curl -s -o "$tmp_resp" -D "$tmp_headers" -w '%{http_code}' \
        --max-time 30 \
        -X POST "$base/api/v1/rag/query" \
        -H 'Content-Type: application/json' \
        "${auth_arg[@]}" \
        --data "$body" 2>/dev/null || echo "000")
    t1=$(python3 -c 'import time;print(int(time.time()*1000))')
    latency_ms=$((t1 - t0))

    # Parse top-1 hit path + score.
    read -r top1 score < <(python3 -c '
import json, sys
try:
    with open(sys.argv[1]) as f:
        data = json.load(f)
    hits = data.get("hits") or data.get("results") or []
    if hits:
        h = hits[0]
        path = h.get("path") or h.get("source") or h.get("id") or ""
        score = h.get("score") or h.get("rrf_score") or h.get("rank") or ""
        # Collapse whitespace in path.
        path = " ".join(str(path).split())
        print(path, score)
    else:
        print("-", "")
except Exception:
    print("-", "")
' "$tmp_resp" 2>/dev/null || echo "- -")

    # CSV row (escape commas in path).
    local safe_path
    safe_path=$(printf '%s' "$top1" | sed 's/"/""/g')
    printf '%s,"%s",%d,%s,"%s",%s\n' \
        "$target" "$query" "$latency_ms" "$status" "$safe_path" "$score" \
        >> "$OUT_CSV"

    rm -f "$tmp_resp" "$tmp_headers"
    echo "$latency_ms"
}

# Collect latencies per target for percentile computation.
declare -a LOCAL_LAT=()
declare -a NGROK_LAT=()

log "bench: queries=$QUERIES_FILE  local=$LOCAL_BASE  ngrok=${PUBLIC_BASE:-<none>}  out=$OUT_CSV"

while IFS= read -r line || [ -n "$line" ]; do
    # Trim + skip blanks/comments.
    q=$(printf '%s' "$line" | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//')
    [ -z "$q" ] && continue
    case "$q" in \#*) continue ;; esac

    lat_local=$(run_one "local" "$LOCAL_BASE" "$q")
    [ -n "$lat_local" ] && LOCAL_LAT+=("$lat_local")

    if [ -n "$PUBLIC_BASE" ]; then
        lat_ngrok=$(run_one "ngrok" "$PUBLIC_BASE" "$q")
        [ -n "$lat_ngrok" ] && NGROK_LAT+=("$lat_ngrok")
    fi
done < "$QUERIES_FILE"

# Compute p50 / p95 via python (handles empty gracefully).
percentiles() {
    local label="$1"; shift
    local n="$#"
    if [ "$n" -eq 0 ]; then
        log "  $label: no samples"
        return
    fi
    local stats
    stats=$(python3 -c '
import sys, statistics
vals = sorted(int(x) for x in sys.argv[1:])
n = len(vals)
def pct(p):
    if n == 1: return vals[0]
    k = (n - 1) * p
    f = int(k); c = min(f+1, n-1)
    return int(vals[f] + (vals[c] - vals[f]) * (k - f))
print(f"n={n} p50={pct(0.5)}ms p95={pct(0.95)}ms min={vals[0]}ms max={vals[-1]}ms mean={int(statistics.mean(vals))}ms")
' "$@" 2>/dev/null)
    log "  $label: $stats"
}

log "── summary ──"
percentiles "local" "${LOCAL_LAT[@]}"
percentiles "ngrok" "${NGROK_LAT[@]}"
log "CSV: $OUT_CSV"

exit 0
