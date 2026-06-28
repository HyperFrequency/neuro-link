#!/usr/bin/env bash
# Domain: Hybrid RAG precision@k.
# Runs 30 queries from fixtures/rag-gold-set.jsonl through nlr_rag_query
# and computes precision@1, precision@3, precision@5.
#
# Pass criterion: P@1 >= 0.5 (50% — conservative while wiki is still growing).

set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=../lib/common.sh
source "$SCRIPT_DIR/lib/common.sh"

init_test_script "rag"
require_tools curl jq python3
require_token

GOLD="$SCRIPT_DIR/fixtures/rag-gold-set.jsonl"
if [ ! -f "$GOLD" ]; then
    record_test "gold set fixture present" FAIL 0 "missing $GOLD"
    print_script_summary
fi

# Count entries
total=$(wc -l < "$GOLD" | tr -d ' ')
record_test "gold set loaded: $total queries" PASS 0

# Run each query, tally P@1/P@3/P@5
p1=0 p3=0 p5=0
queries_run=0

RESULTS_TMP=$(mktemp /tmp/nlr-rag-results-XXXXXXXX)
register_cleanup "$RESULTS_TMP"

while IFS= read -r entry; do
    [ -z "$entry" ] && continue
    query=$(printf "%s" "$entry" | jq -r '.query')
    expected=$(printf "%s" "$entry" | jq -r '.expected_top1')

    start=$(now_ms)
    # URL-encode query param. Extended timeout — embedding+vector lookup can be 5-30s per query.
    encoded=$(python3 -c "import sys, urllib.parse; print(urllib.parse.quote(sys.argv[1]))" "$query")
    resp=$(curl --silent --show-error --max-time 90 \
        -H "Authorization: Bearer ${NLR_API_TOKEN}" \
        "$LOCAL_BASE/api/v1/rag/query?q=$encoded&limit=5")
    dur=$(( $(now_ms) - start ))

    # results is array of {path, score, preview, source}
    paths=$(printf "%s" "$resp" | jq -r '.results[]?.path' 2>/dev/null)
    if [ -z "$paths" ]; then
        record_test "rag: '$query'" FAIL "$dur" "no results"
        queries_run=$((queries_run + 1))
        continue
    fi

    # Read top 5 paths into a bash array
    top_paths=()
    while IFS= read -r p; do
        top_paths+=("$p")
    done <<< "$paths"

    # Check rank of expected_top1
    rank=0
    for i in "${!top_paths[@]}"; do
        if [ "${top_paths[$i]}" = "$expected" ]; then
            rank=$((i + 1))
            break
        fi
    done

    # Tally
    if [ "$rank" -eq 1 ]; then
        p1=$((p1 + 1)); p3=$((p3 + 1)); p5=$((p5 + 1))
        record_test "rag: '$query'" PASS "$dur" "rank=1"
    elif [ "$rank" -le 3 ] && [ "$rank" -gt 0 ]; then
        p3=$((p3 + 1)); p5=$((p5 + 1))
        record_test "rag: '$query'" PASS "$dur" "rank=$rank (p3 hit)"
    elif [ "$rank" -le 5 ] && [ "$rank" -gt 0 ]; then
        p5=$((p5 + 1))
        record_test "rag: '$query'" PASS "$dur" "rank=$rank (p5 hit)"
    else
        # Not in top-5 at all
        top_preview=$(printf "%s" "${top_paths[@]:0:3}" | tr ' ' ',')
        record_test "rag: '$query'" FAIL "$dur" "expected=$expected, got top3=$top_preview"
    fi
    queries_run=$((queries_run + 1))
    printf '%s|%s|%d\n' "$query" "$expected" "$rank" >> "$RESULTS_TMP"
done < "$GOLD"

# Final metrics
p1_pct=$(python3 -c "print(f'{$p1/$queries_run:.3f}')" 2>/dev/null || echo "0")
p3_pct=$(python3 -c "print(f'{$p3/$queries_run:.3f}')" 2>/dev/null || echo "0")
p5_pct=$(python3 -c "print(f'{$p5/$queries_run:.3f}')" 2>/dev/null || echo "0")

record_test "P@1 metric" PASS 0 "$p1/$queries_run = $p1_pct"
record_test "P@3 metric" PASS 0 "$p3/$queries_run = $p3_pct"
record_test "P@5 metric" PASS 0 "$p5/$queries_run = $p5_pct"

# Pass bar: P@1 >= 0.5
p1_ok=$(python3 -c "print(1 if $p1/$queries_run >= 0.5 else 0)" 2>/dev/null)
if [ "$p1_ok" = "1" ]; then
    record_test "P@1 >= 0.5 (pass bar)" PASS 0 "$p1_pct"
else
    record_test "P@1 >= 0.5 (pass bar)" FAIL 0 "$p1_pct below threshold"
fi

print_script_summary
