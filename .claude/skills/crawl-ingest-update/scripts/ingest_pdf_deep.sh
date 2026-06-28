#!/usr/bin/env bash
# Deep PDF ingest: runs Marker + MinerU in parallel, then merges outputs via
# cross-check, canonicalizes LaTeX, and writes to 01-raw/<sha256>-<slug>.md.
#
# Usage: ingest_pdf_deep.sh <path-to-pdf>

set -euo pipefail

PDF="${1:?path to PDF required}"
[[ -f "$PDF" ]] || { echo "file not found: $PDF" >&2; exit 1; }

REPO_ROOT="${NLR_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)}"
RAW_DIR="$REPO_ROOT/01-raw"
WORKDIR="$(mktemp -d -t nlr-ingest.XXXXXX)"
trap "rm -rf $WORKDIR" EXIT

SHA="$(shasum -a 256 "$PDF" | awk '{print $1}')"
BASENAME="$(basename "$PDF" .pdf)"
SLUG="$(echo "$BASENAME" | tr '[:upper:]' '[:lower:]' | tr -c 'a-z0-9' '-' | sed 's/-*$//')"
[[ -z "$SLUG" ]] && SLUG="untitled"

echo "SHA256: $SHA"
echo "slug:   $SLUG"
echo "workdir: $WORKDIR"

# Dedup check
if ls "$RAW_DIR/${SHA}-"*.md >/dev/null 2>&1; then
  echo "Already ingested: $(ls "$RAW_DIR/${SHA}-"*.md)"
  exit 0
fi

mkdir -p "$RAW_DIR" "$WORKDIR/marker" "$WORKDIR/mineru"

# Launch both extractors in parallel
(
  echo "[marker] starting"
  marker_single "$PDF" \
    --use_llm \
    --output_format markdown \
    --output "$WORKDIR/marker" \
    > "$WORKDIR/marker.log" 2>&1
  echo "[marker] done"
) &
MARKER_PID=$!

(
  echo "[mineru] starting"
  mineru \
    --backend vlm-mlx \
    --input "$PDF" \
    --output "$WORKDIR/mineru" \
    > "$WORKDIR/mineru.log" 2>&1
  echo "[mineru] done"
) &
MINERU_PID=$!

wait $MARKER_PID || echo "marker failed; continuing with mineru only"
wait $MINERU_PID || echo "mineru failed; continuing with marker only"

# Locate outputs
MARKER_MD="$(find "$WORKDIR/marker" -name '*.md' | head -1 || true)"
MINERU_MD="$(find "$WORKDIR/mineru" -name '*.md' | head -1 || true)"

if [[ -z "$MARKER_MD" && -z "$MINERU_MD" ]]; then
  echo "ERROR: both extractors failed. See $WORKDIR/*.log" >&2
  exit 1
fi

# Cross-check if both succeeded; otherwise use whichever survived
if [[ -n "$MARKER_MD" && -n "$MINERU_MD" ]]; then
  echo "[crosscheck] merging"
  python3 "$(dirname "${BASH_SOURCE[0]}")/math_crosscheck.py" \
    "$MARKER_MD" "$MINERU_MD" \
    -o "$WORKDIR/merged.md"
  CANDIDATE="$WORKDIR/merged.md"
else
  CANDIDATE="${MARKER_MD:-$MINERU_MD}"
  echo "[crosscheck] skipped (only one extractor produced output)"
fi

# Write to 01-raw with frontmatter
OUTPUT="$RAW_DIR/${SHA}-${SLUG}.md"
cat > "$OUTPUT" <<EOF
---
source: pdf-deep-ingest
source_path: $PDF
sha256: $SHA
pipeline: $(if [[ -n "$MARKER_MD" && -n "$MINERU_MD" ]]; then echo "marker+mineru-crosscheck"; elif [[ -n "$MARKER_MD" ]]; then echo "marker-only"; else echo "mineru-only"; fi)
ingested_at: $(date -Iseconds)
confidence: 0.85
last_updated: $(date +%Y-%m-%d)
open_questions: []
---

EOF
cat "$CANDIDATE" >> "$OUTPUT"

echo "Wrote $OUTPUT"

# Run canonicalization
CANON="$(dirname "${BASH_SOURCE[0]}")/canonicalize.py"
if [[ -x "$CANON" ]]; then
  echo "[canonicalize] running"
  python3 "$CANON" "$OUTPUT"
fi

# Copy images if present
if [[ -d "$WORKDIR/marker" ]]; then
  IMG_SRC="$(find "$WORKDIR/marker" -type d -name images | head -1 || true)"
  if [[ -n "$IMG_SRC" ]]; then
    cp -r "$IMG_SRC" "$RAW_DIR/${SHA}-${SLUG}.images/"
  fi
fi

echo "Done."
