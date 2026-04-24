#!/usr/bin/env bash
# Ingest deep-tool-wiki/*/wiki.md into qmd's sqlite store.
#
# Why this exists: qmd 0.1.2 only supports sqlite backend (no qdrant client),
# and its default embedder is Qwen3-Embedding-0.6B (1024-dim) — NOT compatible
# with neuro-link's nlr_wiki qdrant collection (Octen-8B 4096-dim).
#
# Decision (R+2 P4): accept qmd's defaults. Re-ingest the 20+ deep-tool-wiki
# pages into qmd's own sqlite. neuro-link's nlr_wiki qdrant stays as-is for
# the direct-embedder path via llama-server :8400. Two retrieval paths with
# different strengths:
#   - nlr_wiki qdrant (4096-dim Octen):  low-latency, neuro-link-owned
#   - qmd sqlite (1024-dim Qwen3):       rerank + query-expansion capable
#
# Usage:
#   export PYTHON=/Users/$USER/.local/share/uv/python/cpython-3.12.8-macos-aarch64-none/bin/python3.12
#   bash ingest_deep_tool_wiki_into_qmd.sh
#
# Idempotent: re-ingesting the same document-id is a no-op (qmd dedupes on
# (collection, document_id)).

set -euo pipefail

# Default to an arm64 interpreter if PYTHON not set (mirrors download_models.sh)
if [[ -z "${PYTHON:-}" ]]; then
  for cand in \
    "/Users/$USER/.local/share/uv/python/cpython-3.12.8-macos-aarch64-none/bin/python3.12" \
    "/opt/homebrew/bin/python3.13" \
    "/opt/homebrew/bin/python3.12"; do
    if [[ -x "$cand" ]]; then
      PY_ARCH=$("$cand" -c "import platform; print(platform.machine())" 2>/dev/null || echo unknown)
      if [[ "$PY_ARCH" == "arm64" ]]; then
        PYTHON="$cand"
        break
      fi
    fi
  done
fi
if [[ -z "${PYTHON:-}" ]]; then
  echo "ERROR: no arm64 Python ≥ 3.12 found. Install one and set \$PYTHON." >&2
  exit 1
fi

# Ensure qmd installed in a venv next to this script. Fresh if missing.
VENV="${QMD_VENV:-${HOME}/.local/share/neuro-link/qmd-venv}"
if [[ ! -x "${VENV}/bin/qmd" ]]; then
  echo "[ingest] creating qmd venv at ${VENV} with ${PYTHON}..."
  mkdir -p "$(dirname "$VENV")"
  arch -arm64 "$(command -v uv || echo /Users/$USER/.local/bin/uv)" venv "$VENV" --python "$PYTHON"
  arch -arm64 "$(command -v uv || echo /Users/$USER/.local/bin/uv)" pip install \
    --python "$VENV/bin/python" "qmd>=0.1.2"
fi

# deep-tool-wiki location — env override for docker mount; default to local
DTW_ROOT="${DTW_ROOT:-/Users/$USER/hyperfrequency/neuro-quant/deep-tool-wiki}"
DB_PATH="${QMD_DB_PATH:-${HOME}/.local/share/neuro-link/qmd.sqlite}"
COLLECTION="${QMD_COLLECTION:-dtw_wiki}"

mkdir -p "$(dirname "$DB_PATH")"

echo "[ingest] deep-tool-wiki root: $DTW_ROOT"
echo "[ingest] qmd db:              $DB_PATH"
echo "[ingest] collection:          $COLLECTION"
echo

count_ok=0
count_fail=0
for wiki in "$DTW_ROOT"/*/wiki.md; do
  [[ -f "$wiki" ]] || continue
  tool=$(basename "$(dirname "$wiki")")
  # document_id = "<tool>:wiki"
  doc_id="${tool}:wiki"
  if "${VENV}/bin/qmd" --db-path "$DB_PATH" document add \
      --collection "$COLLECTION" \
      --document-id "$doc_id" \
      --markdown-file "$wiki" \
      --metadata-json "$(printf '{"tool":"%s","kind":"wiki"}' "$tool")" \
      >/dev/null 2>&1; then
    echo "  ✓ $tool"
    count_ok=$((count_ok + 1))
  else
    echo "  ✗ $tool (ingest failed)"
    count_fail=$((count_fail + 1))
  fi

  # Also ingest pitfalls.md if present
  pit="$(dirname "$wiki")/pitfalls.md"
  if [[ -f "$pit" ]]; then
    "${VENV}/bin/qmd" --db-path "$DB_PATH" document add \
      --collection "$COLLECTION" \
      --document-id "${tool}:pitfalls" \
      --markdown-file "$pit" \
      --metadata-json "$(printf '{"tool":"%s","kind":"pitfalls"}' "$tool")" \
      >/dev/null 2>&1 && echo "  ✓ $tool/pitfalls.md" || echo "  - $tool/pitfalls.md (skip)"
  fi

  # And code.md if present
  code="$(dirname "$wiki")/code.md"
  if [[ -f "$code" ]]; then
    "${VENV}/bin/qmd" --db-path "$DB_PATH" document add \
      --collection "$COLLECTION" \
      --document-id "${tool}:code" \
      --markdown-file "$code" \
      --metadata-json "$(printf '{"tool":"%s","kind":"code"}' "$tool")" \
      >/dev/null 2>&1 && echo "  ✓ $tool/code.md" || echo "  - $tool/code.md (skip)"
  fi
done

echo
echo "[ingest] ingested $count_ok tool/wiki pairs (failures: $count_fail)"
echo
echo "=== collection info ==="
"${VENV}/bin/qmd" --db-path "$DB_PATH" collection info --collection "$COLLECTION" 2>&1

echo
echo "=== verify: search for 'vectorbtpro percent sizing' ==="
"${VENV}/bin/qmd" --db-path "$DB_PATH" search \
  --collection "$COLLECTION" --query "vectorbtpro percent of equity sizing" \
  --top-k 3 2>&1 | python3 -c "
import json, sys
try:
    d = json.loads(sys.stdin.read())
    for i, h in enumerate(d[:3]):
        m = h.get('metadata', {})
        print(f\"  {i+1}. [{h['score']:.3f}] {m.get('tool','?')}/{m.get('kind','?')} (doc={h['chunk_ref']['document_id']})\")
except Exception as e:
    print('parse failed:', e)
"
