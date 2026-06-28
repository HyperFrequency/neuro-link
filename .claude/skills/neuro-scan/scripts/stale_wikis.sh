#!/usr/bin/env bash
# Find wiki pages that are stale (>30d since last_updated) and rank them by
# confidence (high-confidence + stale = highest priority to refresh).

set -uo pipefail

REPO_ROOT="${NLR_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)}"
KB_DIR="$REPO_ROOT/02-KB-main"
THRESHOLD_DAYS="${1:-30}"

cutoff="$(date -v-${THRESHOLD_DAYS}d +%Y-%m-%d 2>/dev/null || date -d "${THRESHOLD_DAYS} days ago" +%Y-%m-%d)"

if [[ ! -d "$KB_DIR" ]]; then
  echo "ERROR: $KB_DIR not found" >&2
  exit 1
fi

printf "%-80s %-12s %-12s %s\n" "path" "last_updated" "confidence" "domain"
printf "%-80s %-12s %-12s %s\n" "$(printf '%*s' 80 | tr ' ' '-')" "------------" "------------" "------"

find "$KB_DIR" -name '*.md' -type f 2>/dev/null | while read -r f; do
  # Read frontmatter until second ---
  fm=$(awk '/^---$/{c++; if(c==2) exit; next} c==1' "$f" 2>/dev/null)
  last_updated=$(echo "$fm" | grep -E '^last_updated:' | head -1 | sed 's/^last_updated: *//;s/["'\'']//g' | tr -d ' ')
  confidence=$(echo "$fm" | grep -E '^confidence:' | head -1 | sed 's/^confidence: *//' | tr -d ' ')
  domain=$(echo "$fm" | grep -E '^domain:' | head -1 | sed 's/^domain: *//;s/["'\'']//g' | tr -d ' ')

  # Skip if no last_updated or if not older than cutoff
  [[ -z "$last_updated" ]] && continue
  [[ "$last_updated" > "$cutoff" ]] && continue

  rel="${f#"$REPO_ROOT/"}"
  printf "%-80s %-12s %-12s %s\n" "$rel" "$last_updated" "${confidence:-?}" "${domain:-?}"
done | sort -t$'\t' -k3 -rn -k2 | head -50
