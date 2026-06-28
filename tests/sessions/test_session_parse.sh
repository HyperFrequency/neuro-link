#!/usr/bin/env bash
# Domain: Session logger end-to-end.
# Generates a synthetic Claude Code session JSONL, calls nlr_sessions_parse,
# asserts that a new vault markdown appears with expected shape.

set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=../lib/common.sh
source "$SCRIPT_DIR/lib/common.sh"

init_test_script "sessions"
require_tools curl jq python3
require_token

# Setup: use a temp CLAUDE_ROOT + vault to avoid polluting user data.
TEST_CLAUDE_ROOT=$(mktemp -d /tmp/nlr-test-claude-XXXXXXXX)
TEST_VAULT=$(mktemp -d /tmp/nlr-test-vault-XXXXXXXX)
register_cleanup "$TEST_CLAUDE_ROOT"
register_cleanup "$TEST_VAULT"

# Synthesize a minimal JSONL session — copy from fixtures/session-logs/synthetic-basic.jsonl
FIXTURE="$SCRIPT_DIR/fixtures/session-logs/synthetic-basic.jsonl"
if [ ! -f "$FIXTURE" ]; then
    record_test "fixture present" FAIL 0 "missing $FIXTURE"
    print_script_summary
fi

# Put the fixture at the expected path:
#   $TEST_CLAUDE_ROOT/projects/<project-slug>/<sessionid>.jsonl
PROJECT_SLUG="-Users-DanBot-Desktop-HyperFrequency-neuro-link"
SESSION_ID="test-session-0001"
mkdir -p "$TEST_CLAUDE_ROOT/projects/$PROJECT_SLUG"
cp "$FIXTURE" "$TEST_CLAUDE_ROOT/projects/$PROJECT_SLUG/${SESSION_ID}.jsonl"

# nlr_sessions_parse uses CLAUDE_ROOT env var and takes a vault override in args.
# The MCP server is running with the user's real CLAUDE_ROOT; we cannot swap it
# live. Instead, invoke the MCP binary directly via stdio with CLAUDE_ROOT set.

# ── Test 1: nlr_sessions_list sees the synthetic session ──
start=$(now_ms)
call='{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"nlr_sessions_list","arguments":{"days":365}}}'
raw=$(printf '%s\n' "$call" | CLAUDE_ROOT="$TEST_CLAUDE_ROOT" with_timeout 15 "$MCP_BIN" mcp 2>/dev/null | grep -E '^\{' | head -n 1)
dur=$(( $(now_ms) - start ))
inner=$(printf "%s" "$raw" | jq -r '.result.content[0].text // empty')
if printf "%s" "$inner" | jq -e ".[] | select(.session_id == \"$SESSION_ID\")" >/dev/null 2>&1; then
    record_test "nlr_sessions_list finds synthetic session" PASS "$dur"
else
    record_test "nlr_sessions_list finds synthetic session" FAIL "$dur" "inner=${inner:0:200}"
fi

# ── Test 2: nlr_sessions_parse writes a vault markdown ──
start=$(now_ms)
call=$(jq -cn --arg vault "$TEST_VAULT" \
    '{jsonrpc:"2.0",id:1,method:"tools/call",params:{name:"nlr_sessions_parse",arguments:{vault:$vault}}}')
raw=$(printf '%s\n' "$call" | CLAUDE_ROOT="$TEST_CLAUDE_ROOT" with_timeout 15 "$MCP_BIN" mcp 2>/dev/null | grep -E '^\{' | head -n 1)
dur=$(( $(now_ms) - start ))
inner=$(printf "%s" "$raw" | jq -r '.result.content[0].text // empty')
written=$(printf "%s" "$inner" | jq -r '.written // 0')
if [ "$written" -ge 1 ]; then
    record_test "nlr_sessions_parse writes vault markdown" PASS "$dur" "written=$written"
else
    record_test "nlr_sessions_parse writes vault markdown" FAIL "$dur" "written=$written inner=${inner:0:200}"
fi

# Find the generated markdown file
md_file=$(find "$TEST_VAULT/sessions" -name "*.md" 2>/dev/null | head -n 1)

# ── Test 3: Markdown has expected structure ──
if [ -z "$md_file" ] || [ ! -f "$md_file" ]; then
    record_test "vault markdown exists" FAIL 0 "no .md in $TEST_VAULT/sessions"
else
    record_test "vault markdown exists" PASS 0 "$(basename "$md_file")"

    # Required frontmatter keys
    required_keys="title session_id project model started ended turns tokens cost_usd_estimate"
    missing=""
    for k in $required_keys; do
        if ! grep -q "^${k}:" "$md_file"; then
            missing="$missing $k"
        fi
    done
    if [ -z "$missing" ]; then
        record_test "markdown frontmatter has required keys" PASS 0
    else
        record_test "markdown frontmatter has required keys" FAIL 0 "missing:$missing"
    fi

    # Required sections
    for section in "# Session" "## Timeline" "### Turn"; do
        if grep -qF "$section" "$md_file"; then
            record_test "markdown contains '$section'" PASS 0
        else
            record_test "markdown contains '$section'" FAIL 0 "missing in $(basename "$md_file")"
        fi
    done
fi

# ── Test 4: Token totals & cost estimate are sane ──
if [ -n "$md_file" ] && [ -f "$md_file" ]; then
    input_tokens=$(grep -E '^\s*input:' "$md_file" | head -n1 | awk -F: '{print $2}' | tr -d ' ')
    cost_est=$(grep '^cost_usd_estimate:' "$md_file" | awk -F: '{print $2}' | tr -d ' ')
    # We put 100 input + 50 output tokens in the fixture; any positive value confirms parsing
    if [[ "$input_tokens" =~ ^[0-9]+$ ]] && [ "$input_tokens" -ge 100 ]; then
        record_test "frontmatter captures input tokens" PASS 0 "input_tokens=$input_tokens"
    else
        record_test "frontmatter captures input tokens" FAIL 0 "got '$input_tokens'"
    fi
    # cost float > 0
    if python3 -c "import sys; v=float('$cost_est'); sys.exit(0 if v>0 else 1)" 2>/dev/null; then
        record_test "frontmatter includes cost_usd_estimate > 0" PASS 0 "cost=$cost_est"
    else
        record_test "frontmatter includes cost_usd_estimate > 0" FAIL 0 "got '$cost_est'"
    fi
fi

print_script_summary
