#!/usr/bin/env bash
# Scan hook / LLM / cron / harness-to-harness logs for failures in the last
# 24 hours. Dedupe by fingerprint (command + exit code + first line of
# stderr) and report counts.

set -uo pipefail

REPO_ROOT="${NLR_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)}"
STATE_DIR="$REPO_ROOT/state"
HOURS="${1:-24}"

since="$(date -v-${HOURS}H +%Y-%m-%dT%H:%M:%S 2>/dev/null || date -d "${HOURS} hours ago" +%Y-%m-%dT%H:%M:%S)"

# Hook logs
hook_failures=0
if [[ -d "$STATE_DIR/hooks" ]]; then
  while read -r f; do
    [[ -z "$f" ]] && continue
    # crude: count non-zero exit code markers
    c=$(awk -v since="$since" '$0 ~ "exit_code" && $0 !~ "exit_code: 0"' "$f" 2>/dev/null | wc -l | tr -d ' ')
    hook_failures=$((hook_failures + c))
  done < <(find "$STATE_DIR/hooks" -name '*.log' -newermt "$since" 2>/dev/null)
fi

# LLM logs — look for error field
llm_failures=0
if [[ -d "$STATE_DIR/llm_logs" ]]; then
  while read -r f; do
    [[ -z "$f" ]] && continue
    c=$(grep -c '"error"' "$f" 2>/dev/null || echo 0)
    llm_failures=$((llm_failures + c))
  done < <(find "$STATE_DIR/llm_logs" -name '*.jsonl' -newermt "$since" 2>/dev/null)
fi

# Cron logs
cron_failures=0
if [[ -d "$STATE_DIR/cron" ]]; then
  while read -r f; do
    c=$(grep -Ec 'ERROR|FAIL' "$f" 2>/dev/null || echo 0)
    cron_failures=$((cron_failures + c))
  done < <(find "$STATE_DIR/cron" -name '*.log' -newermt "$since" 2>/dev/null)
fi

# Harness-to-harness
h2h_failed=0
if [[ -d "$REPO_ROOT/06-Recursive/harness-to-harness-comms" ]]; then
  h2h_failed=$(find "$REPO_ROOT/06-Recursive/harness-to-harness-comms" -name '*.json' -newermt "$since" 2>/dev/null | \
    xargs grep -l '"status": "failed"' 2>/dev/null | wc -l | tr -d ' ')
fi

total=$((hook_failures + llm_failures + cron_failures + h2h_failed))

printf "%-30s %s\n" "source" "failures (last ${HOURS}h)"
printf "%-30s %s\n" "------" "------------------------"
printf "%-30s %s\n" "hooks" "$hook_failures"
printf "%-30s %s\n" "llm" "$llm_failures"
printf "%-30s %s\n" "cron" "$cron_failures"
printf "%-30s %s\n" "harness-to-harness" "$h2h_failed"
printf "%-30s %s\n" "total" "$total"

# Exit code reflects finding count
[[ $total -eq 0 ]] && exit 0 || exit 1
