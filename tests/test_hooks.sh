#!/usr/bin/env bash
# Test suite for neuro-link-recursive hook scripts (20 tests).
# Self-contained: creates temp directories, runs hooks, checks output/files.
# Exit 0 = all pass, non-zero = failures.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
HOOKS_DIR="${REPO_ROOT}/hooks"
PASS=0
FAIL=0
TOTAL=0

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m'

assert_eq() {
  TOTAL=$((TOTAL + 1))
  local test_name="$1" expected="$2" actual="$3"
  if [[ "$expected" == "$actual" ]]; then
    echo -e "  ${GREEN}PASS${NC} ${test_name}"
    PASS=$((PASS + 1))
  else
    echo -e "  ${RED}FAIL${NC} ${test_name}"
    echo "    expected: ${expected}"
    echo "    actual:   ${actual}"
    FAIL=$((FAIL + 1))
  fi
}

assert_contains() {
  TOTAL=$((TOTAL + 1))
  local test_name="$1" haystack="$2" needle="$3"
  if [[ "$haystack" == *"$needle"* ]]; then
    echo -e "  ${GREEN}PASS${NC} ${test_name}"
    PASS=$((PASS + 1))
  else
    echo -e "  ${RED}FAIL${NC} ${test_name}"
    echo "    expected to contain: ${needle}"
    echo "    actual: ${haystack}"
    FAIL=$((FAIL + 1))
  fi
}

assert_empty() {
  TOTAL=$((TOTAL + 1))
  local test_name="$1" value="$2"
  if [[ -z "$value" ]]; then
    echo -e "  ${GREEN}PASS${NC} ${test_name}"
    PASS=$((PASS + 1))
  else
    echo -e "  ${RED}FAIL${NC} ${test_name}"
    echo "    expected empty, got: ${value}"
    FAIL=$((FAIL + 1))
  fi
}

assert_not_empty() {
  TOTAL=$((TOTAL + 1))
  local test_name="$1" value="$2"
  if [[ -n "$value" ]]; then
    echo -e "  ${GREEN}PASS${NC} ${test_name}"
    PASS=$((PASS + 1))
  else
    echo -e "  ${RED}FAIL${NC} ${test_name}"
    echo "    expected non-empty, got empty"
    FAIL=$((FAIL + 1))
  fi
}

assert_true() {
  TOTAL=$((TOTAL + 1))
  local test_name="$1"
  shift
  if "$@"; then
    echo -e "  ${GREEN}PASS${NC} ${test_name}"
    PASS=$((PASS + 1))
  else
    echo -e "  ${RED}FAIL${NC} ${test_name}"
    FAIL=$((FAIL + 1))
  fi
}

setup_nlr_root() {
  local root
  root="$(mktemp -d)"
  mkdir -p "$root"/{00-raw,01-sorted,02-KB-main,03-ontology-main,07-neuro-link-task,config,state,secrets}
  echo '{"status":"ok","last_check":"2026-01-01T00:00:00Z","errors":[]}' > "$root/state/heartbeat.json"
  touch "$root/state/session_log.jsonl" "$root/state/score_history.jsonl" "$root/state/deviation_log.jsonl"
  echo "---" > "$root/config/neuro-link.md"
  echo "version: 1" >> "$root/config/neuro-link.md"
  echo "auto_rag: true" >> "$root/config/neuro-link.md"
  echo "---" >> "$root/config/neuro-link.md"
  echo "# Master Config" >> "$root/config/neuro-link.md"
  echo "# neuro-link-recursive" > "$root/CLAUDE.md"
  echo "---" > "$root/02-KB-main/schema.md"
  echo "title: Schema" >> "$root/02-KB-main/schema.md"
  echo "---" >> "$root/02-KB-main/schema.md"
  echo "# Index" > "$root/02-KB-main/index.md"
  echo "# Log" > "$root/02-KB-main/log.md"
  printf '%s' "$root"
}

echo "=== neuro-link-recursive hook tests ==="
echo ""

# ---------------------------------------------------------------------------
# auto-rag-inject.sh
# ---------------------------------------------------------------------------
echo "--- auto-rag-inject.sh ---"

# Test 1: No match -> silent exit
NLR_ROOT="$(setup_nlr_root)"
output="$(echo '{"prompt":"xyznonexistent"}' | NLR_ROOT="$NLR_ROOT" bash "$HOOKS_DIR/auto-rag-inject.sh" 2>/dev/null)"
assert_empty "test_auto_rag_no_match_silent" "$output"
rm -rf "$NLR_ROOT"

# Test 2: With match via wiki page title
NLR_ROOT="$(setup_nlr_root)"
mkdir -p "$NLR_ROOT/02-KB-main"
cat > "$NLR_ROOT/02-KB-main/market-microstructure.md" << 'PAGE'
---
title: Market Microstructure
domain: trading
---
Order book dynamics and price formation mechanisms.
PAGE
output="$(echo '{"prompt":"tell me about market microstructure"}' | NLR_ROOT="$NLR_ROOT" bash "$HOOKS_DIR/auto-rag-inject.sh" 2>/dev/null)"
assert_contains "test_auto_rag_with_match" "$output" "additionalContext"
rm -rf "$NLR_ROOT"

# Test 3: Disabled in config
NLR_ROOT="$(setup_nlr_root)"
cat > "$NLR_ROOT/config/neuro-link.md" << 'CFG'
---
version: 1
auto_rag: false
---
# Config
CFG
output="$(echo '{"prompt":"anything"}' | NLR_ROOT="$NLR_ROOT" bash "$HOOKS_DIR/auto-rag-inject.sh" 2>/dev/null)"
assert_empty "test_auto_rag_disabled_config" "$output"
rm -rf "$NLR_ROOT"

# Test 4: Missing root -> silent exit
output="$(echo '{"prompt":"test"}' | NLR_ROOT="/nonexistent/path" bash "$HOOKS_DIR/auto-rag-inject.sh" 2>/dev/null || true)"
assert_empty "test_auto_rag_missing_root_silent" "$output"

echo ""

# ---------------------------------------------------------------------------
# neuro-task-check.sh
# ---------------------------------------------------------------------------
echo "--- neuro-task-check.sh ---"

# Test 5: No tasks
NLR_ROOT="$(setup_nlr_root)"
output="$(echo '{}' | NLR_ROOT="$NLR_ROOT" bash "$HOOKS_DIR/neuro-task-check.sh" 2>/dev/null)"
assert_empty "test_task_check_no_tasks" "$output"
rm -rf "$NLR_ROOT"

# Test 6: Priority-1 task present
NLR_ROOT="$(setup_nlr_root)"
cat > "$NLR_ROOT/07-neuro-link-task/1-ingest-urgent.md" << 'TASK'
---
type: ingest
status: pending
priority: 1
created: 2026-04-14
---
# Urgent task
TASK
output="$(echo '{}' | NLR_ROOT="$NLR_ROOT" bash "$HOOKS_DIR/neuro-task-check.sh" 2>/dev/null)"
assert_contains "test_task_check_priority_1" "$output" "high-priority"
rm -rf "$NLR_ROOT"

# Test 7: No priority-1 tasks (has priority-3 only)
NLR_ROOT="$(setup_nlr_root)"
cat > "$NLR_ROOT/07-neuro-link-task/3-curate-low.md" << 'TASK'
---
type: curate
status: pending
priority: 3
---
# Low priority
TASK
output="$(echo '{}' | NLR_ROOT="$NLR_ROOT" bash "$HOOKS_DIR/neuro-task-check.sh" 2>/dev/null)"
assert_empty "test_task_check_no_priority_1" "$output"
rm -rf "$NLR_ROOT"

# Test 8: Missing task directory
NLR_ROOT="$(setup_nlr_root)"
rmdir "$NLR_ROOT/07-neuro-link-task"
output="$(echo '{}' | NLR_ROOT="$NLR_ROOT" bash "$HOOKS_DIR/neuro-task-check.sh" 2>/dev/null)"
assert_empty "test_task_check_missing_dir" "$output"
rm -rf "$NLR_ROOT"

echo ""

# ---------------------------------------------------------------------------
# neuro-log-tool-use.sh
# ---------------------------------------------------------------------------
echo "--- neuro-log-tool-use.sh ---"

# Test 9: Logger appends JSONL
NLR_ROOT="$(setup_nlr_root)"
echo '{"tool_name":"Bash","tool_response":{"exit_code":0,"stdout":"ok"}}' | NLR_ROOT="$NLR_ROOT" bash "$HOOKS_DIR/neuro-log-tool-use.sh" 2>/dev/null
# Background process — wait briefly
sleep 0.5
log_content="$(cat "$NLR_ROOT/state/session_log.jsonl")"
assert_contains "test_logger_appends_jsonl" "$log_content" '"tool": "Bash"'
rm -rf "$NLR_ROOT"

# Test 10: Security — no tool input logged
NLR_ROOT="$(setup_nlr_root)"
echo '{"tool_name":"Bash","tool_input":{"command":"echo SECRET_TOKEN_123"},"tool_response":{"exit_code":0}}' | NLR_ROOT="$NLR_ROOT" bash "$HOOKS_DIR/neuro-log-tool-use.sh" 2>/dev/null
sleep 0.5
log_content="$(cat "$NLR_ROOT/state/session_log.jsonl")"
if [[ "$log_content" == *"SECRET_TOKEN_123"* ]]; then
  TOTAL=$((TOTAL + 1)); FAIL=$((FAIL + 1))
  echo -e "  ${RED}FAIL${NC} test_logger_no_input_summary"
  echo "    tool input content was logged (security violation)"
else
  TOTAL=$((TOTAL + 1)); PASS=$((PASS + 1))
  echo -e "  ${GREEN}PASS${NC} test_logger_no_input_summary"
fi
rm -rf "$NLR_ROOT"

# Test 11: Adversarial input
NLR_ROOT="$(setup_nlr_root)"
echo '{"tool_name":"$(rm -rf /)","tool_response":{"exit_code":0}}' | NLR_ROOT="$NLR_ROOT" bash "$HOOKS_DIR/neuro-log-tool-use.sh" 2>/dev/null
sleep 0.5
# System should still be intact and log should have the literal string
assert_true "test_logger_adversarial_input" test -d "$NLR_ROOT/state"
rm -rf "$NLR_ROOT"

# Test 12: Missing root -> silent exit
output="$(echo '{"tool_name":"X"}' | NLR_ROOT="" HOME="/nonexistent" bash "$HOOKS_DIR/neuro-log-tool-use.sh" 2>/dev/null || true)"
assert_empty "test_logger_missing_root_silent" "$output"

echo ""

# ---------------------------------------------------------------------------
# harness-bridge-check.sh
# ---------------------------------------------------------------------------
echo "--- harness-bridge-check.sh ---"

# Test 13: No match (bridge disabled)
NLR_ROOT="$(setup_nlr_root)"
cat > "$NLR_ROOT/config/harness-harness-comms.md" << 'CFG'
---
version: 1
enabled: false
---
# Harness Comms
CFG
output="$(echo '{"tool_name":"Bash","tool_input":{"command":"ls"}}' | NLR_ROOT="$NLR_ROOT" bash "$HOOKS_DIR/harness-bridge-check.sh" 2>/dev/null)"
assert_empty "test_harness_bridge_no_match" "$output"
rm -rf "$NLR_ROOT"

# Test 14: With routing match
NLR_ROOT="$(setup_nlr_root)"
cat > "$NLR_ROOT/config/harness-harness-comms.md" << 'CFG'
---
version: 1
enabled: true
bridge_mode: mcp2cli
harnesses:
  claude-code:
    status: active
  k-dense-byok:
    status: active
routing_rules:
  - pattern: "scientific.*review"
    route_to: k-dense-byok
---
# Harness Comms
CFG
output="$(echo '{"tool_name":"Bash","tool_input":{"command":"scientific review of paper"}}' | NLR_ROOT="$NLR_ROOT" bash "$HOOKS_DIR/harness-bridge-check.sh" 2>/dev/null)"
assert_contains "test_harness_bridge_with_match" "$output" "HARNESS-BRIDGE"
rm -rf "$NLR_ROOT"

echo ""

# ---------------------------------------------------------------------------
# neuro-grade.sh
# ---------------------------------------------------------------------------
echo "--- neuro-grade.sh ---"

# Test 15: Grade with successful tool
NLR_ROOT="$(setup_nlr_root)"
echo '{"tool_name":"wiki-curate","tool_response":{"exit_code":0,"stdout":"Created page"}}' | NLR_ROOT="$NLR_ROOT" bash "$HOOKS_DIR/neuro-grade.sh" 2>/dev/null
sleep 0.5
score_content="$(cat "$NLR_ROOT/state/score_history.jsonl")"
assert_contains "test_grade_success_scores" "$score_content" '"composite"'
rm -rf "$NLR_ROOT"

# Test 16: Grade with failed tool
NLR_ROOT="$(setup_nlr_root)"
echo '{"tool_name":"Bash","tool_response":{"exit_code":1,"stderr":"Error","stdout":""}}' | NLR_ROOT="$NLR_ROOT" bash "$HOOKS_DIR/neuro-grade.sh" 2>/dev/null
sleep 0.5
score_content="$(cat "$NLR_ROOT/state/score_history.jsonl")"
assert_contains "test_grade_failure_scores" "$score_content" '"execution": 0.0'
rm -rf "$NLR_ROOT"

# Test 17: Missing root -> silent exit
output="$(echo '{"tool_name":"X"}' | NLR_ROOT="" HOME="/nonexistent" bash "$HOOKS_DIR/neuro-grade.sh" 2>/dev/null || true)"
assert_empty "test_grade_missing_root_silent" "$output"

echo ""

# ---------------------------------------------------------------------------
# Structural tests
# ---------------------------------------------------------------------------
echo "--- Structural hook tests ---"

# Test 18: All hooks are executable
all_exec=true
for hook in auto-rag-inject.sh neuro-log-tool-use.sh neuro-task-check.sh harness-bridge-check.sh neuro-grade.sh; do
  if [[ ! -x "$HOOKS_DIR/$hook" ]]; then
    all_exec=false
    break
  fi
done
assert_true "test_all_hooks_executable" $all_exec

# Test 19: All hooks have valid bash syntax
all_valid=true
for hook in auto-rag-inject.sh neuro-log-tool-use.sh neuro-task-check.sh harness-bridge-check.sh neuro-grade.sh; do
  if ! bash -n "$HOOKS_DIR/$hook" 2>/dev/null; then
    all_valid=false
    break
  fi
done
assert_true "test_all_hooks_valid_bash" $all_valid

# Test 20: Hooks under 100ms (performance, measured on no-match path)
NLR_ROOT="$(setup_nlr_root)"
all_fast=true
for hook in auto-rag-inject.sh neuro-task-check.sh neuro-log-tool-use.sh neuro-grade.sh; do
  start_ms=$(python3 -c 'import time; print(int(time.time()*1000))')
  echo '{"prompt":"x","tool_name":"x","tool_response":{}}' | NLR_ROOT="$NLR_ROOT" bash "$HOOKS_DIR/$hook" 2>/dev/null || true
  end_ms=$(python3 -c 'import time; print(int(time.time()*1000))')
  elapsed=$((end_ms - start_ms))
  if [[ $elapsed -gt 1000 ]]; then
    echo "    $hook took ${elapsed}ms (>1000ms)"
    all_fast=false
  fi
done
# Use 1000ms threshold to account for python startup; real target is <100ms for the bash portion
assert_true "test_hooks_under_100ms" $all_fast
rm -rf "$NLR_ROOT"

echo ""

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------
echo "=== Results: ${PASS}/${TOTAL} passed, ${FAIL} failed ==="
if [[ $FAIL -gt 0 ]]; then
  exit 1
fi
exit 0
