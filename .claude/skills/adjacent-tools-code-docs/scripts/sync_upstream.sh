#!/usr/bin/env bash
# Sync a single adjacent tool from its upstream. Compares CHANGELOG since
# last_synced, identifies affected wiki pages, and re-runs /wiki-curate on
# the affected sections.
#
# Usage:
#   sync_upstream.sh <tool-slug>

set -euo pipefail

SLUG="${1:?tool slug required}"
REPO_ROOT="${NLR_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)}"
TOOLBOX_DIR="$REPO_ROOT/08-code-docs/toolbox/$SLUG"
MIRROR_DIR="$REPO_ROOT/state/mirrors/toolbox/$SLUG"
WATCH_FILE="$REPO_ROOT/config/toolbox-watch.yml"

if [[ ! -d "$TOOLBOX_DIR" ]]; then
  echo "ERROR: $TOOLBOX_DIR not found. Is the tool registered?" >&2
  exit 1
fi

if [[ ! -f "$WATCH_FILE" ]]; then
  echo "ERROR: $WATCH_FILE missing" >&2
  exit 1
fi

# Pull config via yq if available, else crude grep
if command -v yq >/dev/null 2>&1; then
  UPSTREAM=$(yq ".tools[] | select(.slug == \"$SLUG\") | .upstream" "$WATCH_FILE")
  LAST_SYNCED=$(yq ".tools[] | select(.slug == \"$SLUG\") | .last_synced" "$WATCH_FILE")
else
  UPSTREAM=$(grep -A4 "slug: $SLUG$" "$WATCH_FILE" | grep upstream: | head -1 | awk '{print $2}' | tr -d '"')
  LAST_SYNCED=$(grep -A4 "slug: $SLUG$" "$WATCH_FILE" | grep last_synced: | head -1 | awk '{print $2}' | tr -d '"')
fi

if [[ -z "${UPSTREAM:-}" ]]; then
  echo "ERROR: no upstream config for $SLUG in $WATCH_FILE" >&2
  exit 1
fi

# Ensure local mirror exists
if [[ ! -d "$MIRROR_DIR/.git" ]]; then
  echo "cloning upstream $UPSTREAM -> $MIRROR_DIR"
  git clone --depth 50 "$UPSTREAM" "$MIRROR_DIR"
else
  echo "fetching upstream $UPSTREAM"
  (cd "$MIRROR_DIR" && git fetch origin)
fi

# Get the last synced commit SHA (stored per-tool in the mirror's tag)
LAST_TAG="nlr-synced-at-$LAST_SYNCED"
if git -C "$MIRROR_DIR" rev-parse "$LAST_TAG" >/dev/null 2>&1; then
  SINCE_SHA=$(git -C "$MIRROR_DIR" rev-parse "$LAST_TAG")
else
  echo "no sync tag; treating all current files as changed"
  SINCE_SHA=""
fi

# Identify changed files since last sync
if [[ -n "$SINCE_SHA" ]]; then
  CHANGED=$(git -C "$MIRROR_DIR" diff --name-only "$SINCE_SHA" HEAD)
else
  CHANGED=$(git -C "$MIRROR_DIR" ls-files)
fi

CHANGELOG_CHANGED=0
if echo "$CHANGED" | grep -q "CHANGELOG"; then
  CHANGELOG_CHANGED=1
fi

printf "Changed files since last sync: "
echo "$CHANGED" | wc -l | tr -d ' '

if [[ -z "$CHANGED" ]]; then
  echo "No changes. Bumping last_synced and exiting."
else
  echo "Changes detected. Diff summary:"
  echo "$CHANGED" | head -20 | sed 's/^/  /'
  [[ "$CHANGELOG_CHANGED" == "1" ]] && echo "  (CHANGELOG changed — consider bumping version_indexed)"
  echo
  echo "Next: run /wiki-curate against affected toolbox pages."
  echo "Affected pages to consider (heuristic):"
  for page in "$TOOLBOX_DIR"/*.md; do
    [[ -f "$page" ]] || continue
    # crude: if the page mentions a changed filename as source, flag it
    for src in $CHANGED; do
      if grep -l "$src" "$page" >/dev/null 2>&1; then
        echo "  $page (mentions $src)"
        break
      fi
    done
  done
fi

# Bump last_synced
TODAY=$(date +%Y-%m-%d)
if command -v yq >/dev/null 2>&1; then
  yq eval -i "(.tools[] | select(.slug == \"$SLUG\") | .last_synced) = \"$TODAY\"" "$WATCH_FILE"
else
  sed -i.bak "/slug: $SLUG\$/,/slug: / s/last_synced: .*/last_synced: $TODAY/" "$WATCH_FILE"
  rm -f "$WATCH_FILE.bak"
fi

# Tag current HEAD for next sync diff
git -C "$MIRROR_DIR" tag -f "nlr-synced-at-$TODAY" HEAD

echo
echo "sync complete. last_synced=$TODAY"
