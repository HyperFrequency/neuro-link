#!/usr/bin/env bash
# Sidecar watchdog for hyper-sleep. Monitors wall-clock elapsed and SIGTERMs
# the supervised pid after the budget expires. Also cleans up the pid file
# on normal exit.
#
# Usage:
#   sleep_watchdog.sh <target-pid> [max-seconds]
#
# Default budget: 14400s (4 hours).

set -uo pipefail

TARGET_PID="${1:?target pid required}"
MAX_SECONDS="${2:-14400}"
REPO_ROOT="${NLR_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)}"
PID_FILE="$REPO_ROOT/state/hyper-sleep.pid"

START_TS=$(date +%s)
LOG="$REPO_ROOT/state/hyper-sleep-watchdog.log"
echo "[$(date -Iseconds)] watchdog start pid=$TARGET_PID budget=${MAX_SECONDS}s" >> "$LOG"

while kill -0 "$TARGET_PID" 2>/dev/null; do
  now=$(date +%s)
  elapsed=$((now - START_TS))
  if [[ $elapsed -ge $MAX_SECONDS ]]; then
    echo "[$(date -Iseconds)] budget exceeded (${elapsed}s). SIGTERM pid=$TARGET_PID" >> "$LOG"
    kill -TERM "$TARGET_PID" 2>/dev/null || true
    sleep 30
    # Escalate if still alive
    if kill -0 "$TARGET_PID" 2>/dev/null; then
      echo "[$(date -Iseconds)] SIGKILL pid=$TARGET_PID (didn't respond to TERM)" >> "$LOG"
      kill -KILL "$TARGET_PID" 2>/dev/null || true
    fi
    rm -f "$PID_FILE"
    exit 1
  fi
  sleep 60
done

echo "[$(date -Iseconds)] target exited cleanly after ${elapsed}s" >> "$LOG"
rm -f "$PID_FILE"
