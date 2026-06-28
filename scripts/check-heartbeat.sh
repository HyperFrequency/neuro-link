#!/usr/bin/env bash
# P16 — Heartbeat freshness check.
#
# Reads $NLR_ROOT/state/heartbeat.json (NLR_ROOT defaults to ~/neuro-link).
# Asserts that the 'last_check' timestamp is within the last 2 minutes.
# Also falls back to the file mtime if last_check is missing/unparseable.
#
# Exit 0 if fresh, 1 if stale (or missing).

set -uo pipefail

NLR_ROOT="${NLR_ROOT:-$HOME/neuro-link}"
HEARTBEAT="$NLR_ROOT/state/heartbeat.json"
MAX_AGE_SEC=120

fail() {
    printf "FAIL: %s\n" "$1" >&2
    exit 1
}

if [ ! -f "$HEARTBEAT" ]; then
    fail "heartbeat.json not found at $HEARTBEAT"
fi

now_epoch=$(date +%s)

# Try to parse last_check field (RFC3339 / ISO8601).
last_check=""
if command -v python3 >/dev/null 2>&1; then
    last_check=$(python3 -c '
import json, sys, datetime
try:
    with open(sys.argv[1]) as f:
        data = json.load(f)
    lc = data.get("last_check", "")
    if not lc:
        sys.exit(0)
    # Handle trailing Z as UTC.
    if lc.endswith("Z"):
        lc = lc[:-1] + "+00:00"
    dt = datetime.datetime.fromisoformat(lc)
    print(int(dt.timestamp()))
except Exception:
    pass
' "$HEARTBEAT" 2>/dev/null || true)
fi

# mtime fallback (cross-platform: GNU stat vs BSD stat).
mtime_epoch=$(stat -c %Y "$HEARTBEAT" 2>/dev/null || stat -f %m "$HEARTBEAT" 2>/dev/null || echo 0)

# Use the more recent of last_check and mtime.
best_epoch=0
if [ -n "$last_check" ] && [ "$last_check" -gt "$best_epoch" ]; then
    best_epoch=$last_check
fi
if [ "$mtime_epoch" -gt "$best_epoch" ]; then
    best_epoch=$mtime_epoch
fi

if [ "$best_epoch" -eq 0 ]; then
    fail "could not determine heartbeat age (no parseable last_check, no mtime)"
fi

age=$((now_epoch - best_epoch))
if [ "$age" -lt 0 ]; then
    # Clock skew: treat as fresh.
    age=0
fi

if [ "$age" -gt "$MAX_AGE_SEC" ]; then
    fail "heartbeat stale: ${age}s old (max ${MAX_AGE_SEC}s) at $HEARTBEAT"
fi

printf "OK: heartbeat fresh (%ds old, max %ds)\n" "$age" "$MAX_AGE_SEC"
exit 0
