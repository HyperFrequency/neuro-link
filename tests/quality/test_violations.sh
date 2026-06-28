#!/usr/bin/env bash
# Domain: Session quality scanner regression.
# Feeds 5 canned session JSONLs (one per flag type) into nlr_sessions_scan_quality
# and asserts flag counts exactly match expected.

set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=../lib/common.sh
source "$SCRIPT_DIR/lib/common.sh"

init_test_script "quality"
require_tools jq python3

# Build a temp CLAUDE_ROOT with all 5 fixtures
TEST_CLAUDE_ROOT=$(mktemp -d /tmp/nlr-test-quality-XXXXXXXX)
register_cleanup "$TEST_CLAUDE_ROOT"

PROJECT_DIR="$TEST_CLAUDE_ROOT/projects/-test-project"
mkdir -p "$PROJECT_DIR"

# Copy fixtures — each as its own sessionId-named file (bash 3.2 compatible)
FIXTURE_DIR="$SCRIPT_DIR/fixtures/session-logs"
# pairs: "sessionid:fixture-filename"
FIXTURES="test-missed-tool-001:flag-missed-tool-call.jsonl
test-err-ignored-001:flag-error-ignored.jsonl
test-repeat-fail-001:flag-repeated-failure.jsonl
test-loop-001:flag-loop-detected.jsonl
test-halluc-001:flag-hallucinated-file.jsonl"

while IFS= read -r entry; do
    sid=$(printf "%s" "$entry" | cut -d: -f1)
    file=$(printf "%s" "$entry" | cut -d: -f2)
    if [ ! -f "$FIXTURE_DIR/$file" ]; then
        record_test "fixture $file exists" FAIL 0 "missing"
        print_script_summary
    fi
    cp "$FIXTURE_DIR/$file" "$PROJECT_DIR/${sid}.jsonl"
done <<< "$FIXTURES"
record_test "all 5 fixtures loaded" PASS 0

# Invoke nlr_sessions_scan_quality with a wide days window
start=$(now_ms)
call='{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"nlr_sessions_scan_quality","arguments":{"days":365}}}'
raw=$(printf '%s\n' "$call" | CLAUDE_ROOT="$TEST_CLAUDE_ROOT" with_timeout 15 "$MCP_BIN" mcp 2>/dev/null | grep -E '^\{' | head -n 1)
dur=$(( $(now_ms) - start ))
inner=$(printf "%s" "$raw" | jq -r '.result.content[0].text // empty')

if [ -z "$inner" ]; then
    record_test "nlr_sessions_scan_quality produces result" FAIL "$dur" "empty response: ${raw:0:200}"
    print_script_summary
fi
record_test "nlr_sessions_scan_quality produces result" PASS "$dur"

# Parse the result
sessions_analyzed=$(printf "%s" "$inner" | jq -r '.sessions_analyzed // 0')
total_flags=$(printf "%s" "$inner" | jq -r '.total_flags // 0')
flags_by_type=$(printf "%s" "$inner" | jq -r '.flags_by_type // {}')

if [ "$sessions_analyzed" -eq 5 ]; then
    record_test "scanner analyzed 5 sessions" PASS 0 "got $sessions_analyzed"
else
    record_test "scanner analyzed 5 sessions" FAIL 0 "got $sessions_analyzed"
fi

if [ "$total_flags" -ge 5 ]; then
    record_test "at least 5 flags total (1+ per fixture)" PASS 0 "total=$total_flags"
else
    record_test "at least 5 flags total (1+ per fixture)" FAIL 0 "total=$total_flags"
fi

# Assert each flag type is present
for ftype in missed_tool_call error_ignored repeated_failure loop_detected hallucinated_file; do
    count=$(printf "%s" "$flags_by_type" | jq -r ".$ftype // 0")
    if [ "$count" -ge 1 ]; then
        record_test "flag type '$ftype' detected (>=1)" PASS 0 "count=$count"
    else
        record_test "flag type '$ftype' detected (>=1)" FAIL 0 "count=$count"
    fi
done

# Double-check: specific flag counts match expectations
# flag-repeated-failure.jsonl has 3x Grep with same pattern → should flag once as repeated_failure
# flag-loop-detected.jsonl has the same 3-turn pattern twice → should flag as loop_detected
# flag-missed-tool-call.jsonl says "I'll edit..." but no Edit → should flag missed_tool_call
# flag-error-ignored.jsonl has is_error:true tool_result + no error language in next turn → should flag error_ignored
# flag-hallucinated-file.jsonl mentions 4 fake paths → should flag at least 1 hallucinated_file

halluc=$(printf "%s" "$flags_by_type" | jq -r '.hallucinated_file // 0')
if [ "$halluc" -ge 1 ]; then
    record_test "hallucinated_file count >= 1 (mentioned 4 fake paths)" PASS 0 "count=$halluc"
else
    record_test "hallucinated_file count >= 1 (mentioned 4 fake paths)" FAIL 0 "count=$halluc"
fi

repeat=$(printf "%s" "$flags_by_type" | jq -r '.repeated_failure // 0')
if [ "$repeat" -ge 1 ]; then
    record_test "repeated_failure >= 1 (3x same-args Grep)" PASS 0 "count=$repeat"
else
    record_test "repeated_failure >= 1 (3x same-args Grep)" FAIL 0 "count=$repeat"
fi

# Sample flags sanity: contains expected types
sample_types=$(printf "%s" "$inner" | jq -r '.sample_flags[]?.flag_type' 2>/dev/null | sort -u | tr '\n' ',' | sed 's/,$//')
record_test "sample_flags contains detected types" PASS 0 "types=$sample_types"

print_script_summary
