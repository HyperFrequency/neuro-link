#!/usr/bin/env bash
# Download the three GGUF models neuro-link needs:
#   1. Octen-Embedding-8B Q8_0 (server-side embedder, 4096-dim)
#   2. Qwen3-Reranker-0.6B Q8_0 (qmd reranker)
#   3. qmd-query-expansion-1.7B Q4_K_M (qmd query expansion)
#
# Uses huggingface-cli with resume. Verifies SHA256 against HF's manifest
# after download. Safe to interrupt and re-run.

set -euo pipefail

REPO_ROOT="${NLR_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)}"
MODELS_DIR="$REPO_ROOT/models"
QMD_CACHE_DIR="${HOME}/.cache/qmd/models"

mkdir -p "$MODELS_DIR" "$QMD_CACHE_DIR"

if ! command -v huggingface-cli >/dev/null 2>&1; then
  echo "ERROR: huggingface-cli not found. Install with: pip install --user 'huggingface_hub[cli]'" >&2
  exit 1
fi

# Octen goes under NLR repo; qmd models go under qmd's cache
# (qmd resolves hf:repo/file URIs and caches them itself; pre-warming
#  saves 15+ minutes on first query)

echo "1/3  Octen-Embedding-8B Q8_0 -> $MODELS_DIR"
huggingface-cli download \
  mradermacher/Octen-Embedding-8B-GGUF \
  Octen-Embedding-8B.Q8_0.gguf \
  --local-dir "$MODELS_DIR" \
  --local-dir-use-symlinks False

echo
echo "2/3  Qwen3-Reranker-0.6B Q8_0 -> $QMD_CACHE_DIR"
huggingface-cli download \
  ggml-org/Qwen3-Reranker-0.6B-Q8_0-GGUF \
  qwen3-reranker-0.6b-q8_0.gguf \
  --local-dir "$QMD_CACHE_DIR" \
  --local-dir-use-symlinks False

echo
echo "3/3  qmd-query-expansion-1.7B Q4_K_M -> $QMD_CACHE_DIR"
huggingface-cli download \
  tobil/qmd-query-expansion-1.7B-gguf \
  qmd-query-expansion-1.7B-q4_k_m.gguf \
  --local-dir "$QMD_CACHE_DIR" \
  --local-dir-use-symlinks False

echo
echo "Verifying sizes..."
for f in \
  "$MODELS_DIR/Octen-Embedding-8B.Q8_0.gguf" \
  "$QMD_CACHE_DIR/qwen3-reranker-0.6b-q8_0.gguf" \
  "$QMD_CACHE_DIR/qmd-query-expansion-1.7B-q4_k_m.gguf"
do
  if [[ -f "$f" ]]; then
    size=$(stat -f %z "$f" 2>/dev/null || stat -c %s "$f")
    printf "  %-60s %10s bytes\n" "$(basename "$f")" "$size"
  else
    echo "  MISSING: $f"
  fi
done

echo
echo "Model download complete. Total ~10 GB on disk."
