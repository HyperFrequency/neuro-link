#!/usr/bin/env bash
# Incrementally embed any 02-KB-main/ pages that don't have qmd vectors
# yet. Idempotent — re-running only touches new/modified files.

set -euo pipefail

REPO_ROOT="${NLR_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)}"

if ! command -v qmd >/dev/null 2>&1; then
  echo "qmd not installed; skipping embed warmup" >&2
  exit 0
fi

# Ensure the kb collection is registered
if ! qmd collection list 2>/dev/null | grep -q '^kb'; then
  qmd collection add "$REPO_ROOT/02-KB-main" --name kb
fi

# Incremental embed — qmd tracks what's new vs what's already in its SQLite
qmd embed --collection kb
