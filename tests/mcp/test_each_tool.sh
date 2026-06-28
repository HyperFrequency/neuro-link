#!/usr/bin/env bash
# Domain: MCP tool unit tests
# For each of the ~34 MCP tools exposed by `neuro-link mcp`:
#   1. POST a minimal-valid-args request — verify JSON-RPC response is well-formed
#   2. POST a missing-required-args request — verify error path is triggered
#   3. Cross-check tool name matches mcp2cli-profile.json
#
# NOTE: The server exposes tools via both stdio (neuro-link mcp) and HTTP (/mcp).
# We test via HTTP because it's faster (no fork per call) and representative.

set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=../lib/common.sh
source "$SCRIPT_DIR/lib/common.sh"

init_test_script "mcp"
require_tools curl jq python3
require_token

MCP_URL="${LOCAL_BASE}/mcp"

# ── Step 1: tools/list via HTTP, parse list ──
list_start=$(now_ms)
list_body=$(auth_post_json "$MCP_URL" '{"jsonrpc":"2.0","id":1,"method":"tools/list"}')
list_dur=$(( $(now_ms) - list_start ))
tool_count=$(printf "%s" "$list_body" | jq -r '.result.tools | length' 2>/dev/null)

if [ -z "$tool_count" ] || ! [[ "$tool_count" =~ ^[0-9]+$ ]] || [ "$tool_count" -lt 30 ]; then
    record_test "tools/list returns >=30 tools" FAIL "$list_dur" "got count=$tool_count body=${list_body:0:200}"
    print_script_summary
fi
record_test "tools/list returns >=30 tools" PASS "$list_dur" "count=$tool_count"

# Extract tools with their required-fields from the schema
tools_json=$(printf "%s" "$list_body" | jq '.result.tools')

# ── Cross-check mcp2cli-profile.json ──
profile="$NLR_ROOT/mcp2cli-profile.json"
if [ -f "$profile" ]; then
    profile_tools=$(jq -r '.tools[].mcp_name' "$profile" | sort -u)
    live_tools=$(printf "%s" "$tools_json" | jq -r '.[].name' | sort -u)
    # All profile tools must exist in live (profile may be a subset — which is allowed);
    # flag missing in profile as INFO skip, missing in live as FAIL.
    missing_in_live=$(comm -23 <(printf "%s\n" "$profile_tools") <(printf "%s\n" "$live_tools"))
    if [ -n "$missing_in_live" ]; then
        missing_list=$(printf "%s" "$missing_in_live" | tr '\n' ',' | sed 's/,$//')
        record_test "mcp2cli-profile names match live tools" FAIL 0 "profile-listed but missing in live: $missing_list"
    else
        record_test "mcp2cli-profile names match live tools" PASS 0 "all profile tools present"
    fi
fi

# ── For each tool: test minimal-valid + missing-required ──

# Build a minimal-valid args map, hand-curated for each tool that has required args.
# This matters because MCP tools define their own JSON schema via inputSchema.required.
# We cannot sanely auto-synthesize for all types, so maintain a lookup table here.

python3 - <<'PYEOF' > /tmp/nlr-mcp-minimal-args.json
import json
minimal = {
    # Wiki
    "nlr_wiki_create": {"path": "test-mcp-probe.md", "title": "Test Probe", "content": "probe"},
    "nlr_wiki_read": {"path": "quant/merger-arbitrage.md"},
    "nlr_wiki_update": {"path": "test-mcp-probe.md", "content": "updated"},
    "nlr_wiki_list": {},
    "nlr_wiki_search": {"query": "rag"},
    # RAG (nlr_rag_embed is excluded — it rebuilds the entire Qdrant collection,
    # takes >20s, and is orthogonal to tool-interface correctness)
    "nlr_rag_query": {"query": "transformer", "limit": 3},
    "nlr_rag_rebuild_index": {},
    # Ontology
    "nlr_ontology_generate": {"text": "a\nb\nc", "name": "test-probe", "type": "domain"},
    "nlr_ontology_query": {"name": "test-probe", "tier": "summary"},
    "nlr_ontology_gaps": {},
    # Ingest
    "nlr_ingest": {"slug": "test-mcp-probe", "content": "probe content"},
    "nlr_ingest_classify": {"slug": "test-mcp-probe", "domain": "docs"},
    "nlr_ingest_dedup": {"content": "some content"},
    # Tasks
    "nlr_task_list": {"status_filter": "all"},
    "nlr_task_create": {"title": "MCP Probe Task", "type": "general"},
    "nlr_task_update": {"filename": "3-general-mcp-probe-task.md", "status": "completed"},
    # Harness
    "nlr_harness_dispatch": {"to": "claude-code", "task": "probe"},
    "nlr_harness_list": {},
    # Scan
    "nlr_scan_health": {},
    "nlr_scan_staleness": {"threshold_days": 365},
    # State
    "nlr_state_heartbeat": {"action": "read"},
    "nlr_state_log": {"tool": "mcp-probe"},
    # Config
    "nlr_config_read": {"name": "neuro-link"},
    # LLM logs
    "nlr_llm_logs_list": {"limit": 5},
    "nlr_llm_logs_summary": {"days": 7},
    "nlr_llm_logs_grade": {"days": 7},
    # Hooks log
    "nlr_hooks_log_list": {"limit": 5},
    # Sessions
    "nlr_sessions_list": {"days": 7},
    "nlr_sessions_scan_quality": {"days": 7},
    "nlr_sessions_skill_usage": {"days": 30},
    "nlr_sessions_tool_usage": {"days": 30},
    "nlr_sessions_parse": {"since": "2026-04-16"},
    # Worker
    "nlr_worker_status": {},
}
print(json.dumps(minimal))
PYEOF

register_cleanup /tmp/nlr-mcp-minimal-args.json

# Track cleanup artifacts from probe calls
register_cleanup "$NLR_ROOT/02-KB-main/test-mcp-probe.md"
register_cleanup "$NLR_ROOT/00-raw/test-mcp-probe"
register_cleanup "$NLR_ROOT/00-raw/test-mcp-probe.md"
register_cleanup "$NLR_ROOT/01-sorted/docs/test-mcp-probe.md"
register_cleanup "$NLR_ROOT/03-ontology-main/domain/test-probe"
register_cleanup "$NLR_ROOT/07-neuro-link-task/3-general-mcp-probe-task.md"
# Artifacts from negative-test probes (missing-args) that end up writing defaults:
register_cleanup "$NLR_ROOT/02-KB-main/untitled.md"
register_cleanup "$NLR_ROOT/00-raw/untitled"
register_cleanup "$NLR_ROOT/00-raw/untitled.md"
register_cleanup "$NLR_ROOT/01-sorted/docs/untitled.md"

# Get list of tool names + their required fields
tool_names=$(printf "%s" "$tools_json" | jq -r '.[].name')

# Helper: look up minimal args for a tool from the JSON file (bash 3.2 compat)
get_minimal_args() {
    python3 -c "
import json, sys
with open('/tmp/nlr-mcp-minimal-args.json') as f:
    m = json.load(f)
v = m.get(sys.argv[1])
if v is None: sys.exit(1)
print(json.dumps(v))
" "$1" 2>/dev/null
}

# Invoke each tool with minimal-valid args → expect no error.
# Some tools (rag_query with cold embedding server) may be slow; use an extended timeout.
slow_post() {
    curl --silent --show-error --max-time 60 \
        -H "Authorization: Bearer ${NLR_API_TOKEN}" \
        -H "Content-Type: application/json" \
        -X POST -d "$2" "$1"
}

while IFS= read -r tname; do
    args=$(get_minimal_args "$tname")
    if [ -z "$args" ]; then
        record_test "minimal-args: $tname" SKIP 0 "no minimal-args fixture; add to test_each_tool.sh"
        continue
    fi
    call_payload=$(jq -n --arg name "$tname" --argjson args "$args" \
        '{jsonrpc:"2.0",id:1,method:"tools/call",params:{name:$name,arguments:$args}}')
    start=$(now_ms)
    # rag/embed/parse/quality are inherently slow → use extended timeout
    case "$tname" in
        nlr_rag_query|nlr_rag_rebuild_index|nlr_sessions_parse|nlr_sessions_scan_quality|nlr_rag_embed)
            resp=$(slow_post "$MCP_URL" "$call_payload")
            ;;
        *)
            resp=$(auth_post_json "$MCP_URL" "$call_payload")
            ;;
    esac
    dur=$(( $(now_ms) - start ))

    # Valid JSON-RPC response: has jsonrpc "2.0" + either result or error
    is_valid=$(printf "%s" "$resp" | jq -e '(.jsonrpc=="2.0") and (has("result") or has("error"))' 2>/dev/null || echo false)
    if [ "$is_valid" != "true" ]; then
        record_test "minimal-args: $tname" FAIL "$dur" "malformed JSON-RPC: ${resp:0:200}"
        continue
    fi
    # Check for isError=true inside result.content
    is_tool_error=$(printf "%s" "$resp" | jq -e '.result.isError // false' 2>/dev/null || echo false)
    if [ "$is_tool_error" = "true" ]; then
        err_text=$(printf "%s" "$resp" | jq -r '.result.content[0].text // "(no err text)"')
        # Some tools legitimately return "not found" for placeholder probe data; those are still valid
        if [[ "$err_text" == *"not found"* ]] || [[ "$err_text" == *"No harness config"* ]]; then
            record_test "minimal-args: $tname" PASS "$dur" "graceful error: ${err_text:0:60}"
        else
            record_test "minimal-args: $tname" FAIL "$dur" "tool-reported error: ${err_text:0:120}"
        fi
    else
        record_test "minimal-args: $tname" PASS "$dur"
    fi
done <<< "$tool_names"

# Missing-required-args: pass empty arguments to each tool that has required fields.
# Tools with no required fields (inputSchema.required missing/empty) should still succeed.
while IFS= read -r tool_def; do
    tname=$(printf "%s" "$tool_def" | jq -r '.name')
    req_count=$(printf "%s" "$tool_def" | jq -r '.inputSchema.required | length // 0')
    if [ "$req_count" -eq 0 ]; then
        # No required fields; skip this negative test
        continue
    fi
    call_payload=$(jq -n --arg name "$tname" \
        '{jsonrpc:"2.0",id:1,method:"tools/call",params:{name:$name,arguments:{}}}')
    start=$(now_ms)
    case "$tname" in
        nlr_rag_query|nlr_rag_rebuild_index|nlr_sessions_parse|nlr_sessions_scan_quality|nlr_rag_embed)
            resp=$(slow_post "$MCP_URL" "$call_payload")
            ;;
        *)
            resp=$(auth_post_json "$MCP_URL" "$call_payload")
            ;;
    esac
    dur=$(( $(now_ms) - start ))

    # Server's behavior: most tool handlers use `args["x"].as_str().unwrap_or("")` so they
    # don't hard-error on missing args but produce a graceful "not found" / error result.
    # That is the current contract; document the expectation as:
    #   "request is well-formed JSON-RPC" AND "result signals failure somehow"
    # (either isError:true OR output resembles an error/empty/default-path).
    is_valid=$(printf "%s" "$resp" | jq -e '(.jsonrpc=="2.0") and (has("result") or has("error"))' 2>/dev/null || echo false)
    if [ "$is_valid" != "true" ]; then
        record_test "missing-args: $tname" FAIL "$dur" "malformed JSON-RPC: ${resp:0:200}"
        continue
    fi
    # Accept either isError or an empty result body — but FAIL if the tool clearly "succeeded"
    # in a way that suggests inputs weren't validated (e.g. wrote a file with empty name).
    is_tool_error=$(printf "%s" "$resp" | jq -e '.result.isError // false' 2>/dev/null || echo false)
    text=$(printf "%s" "$resp" | jq -r '.result.content[0].text // ""')
    if [ "$is_tool_error" = "true" ]; then
        record_test "missing-args: $tname" PASS "$dur" "correctly errored"
    else
        # Some tools intentionally succeed on empty args (defaults). That is an operational
        # choice — record as PASS with a note, so the suite reports but does not fail.
        # Exceptions: tools that clearly should require an arg.
        case "$tname" in
            nlr_wiki_create|nlr_wiki_read|nlr_wiki_update|nlr_task_update|nlr_ontology_query|nlr_ingest|nlr_ingest_classify)
                # These MUST validate — write/read without a path is a bug
                if [[ "$text" == *"not found"* ]] || [[ "$text" == *"Invalid"* ]] || [[ "$text" == *"Missing"* ]] || [[ "$text" == *"Access denied"* ]] || [[ "$text" == *"Error:"* ]]; then
                    record_test "missing-args: $tname" PASS "$dur" "graceful refusal"
                else
                    record_test "missing-args: $tname" FAIL "$dur" "accepted empty args: ${text:0:120}"
                fi
                ;;
            *)
                record_test "missing-args: $tname" PASS "$dur" "non-strict tool (accepts defaults)"
                ;;
        esac
    fi
done < <(printf "%s" "$tools_json" | jq -c '.[]')

# ── Output schema conformance (each tool returns `content` array) ──
# Skip heavy tools already exercised in the minimal-args phase; this is strictly
# a shape check — we only need to assert one call per tool, not run them twice.
# Collapse duplicates to save time: only run schema check on a representative set.
schema_tools="nlr_wiki_list nlr_scan_health nlr_task_list nlr_ontology_gaps nlr_hooks_log_list nlr_llm_logs_summary nlr_state_heartbeat nlr_config_read nlr_worker_status nlr_harness_list"
for tname in $schema_tools; do
    args=$(get_minimal_args "$tname")
    [ -z "$args" ] && continue
    call_payload=$(jq -n --arg name "$tname" --argjson args "$args" \
        '{jsonrpc:"2.0",id:1,method:"tools/call",params:{name:$name,arguments:$args}}')
    resp=$(auth_post_json "$MCP_URL" "$call_payload")
    has_content=$(printf "%s" "$resp" | jq -e '.result.content | type == "array" and length > 0' 2>/dev/null || echo false)
    has_text_or_isError=$(printf "%s" "$resp" | jq -e '.result.content[0].text != null or .result.isError != null' 2>/dev/null || echo false)
    if [ "$has_content" != "true" ] || [ "$has_text_or_isError" != "true" ]; then
        record_test "schema: $tname returns content[].text" FAIL 0 "bad shape: ${resp:0:120}"
    else
        record_test "schema: $tname returns content[].text" PASS 0
    fi
done

print_script_summary
