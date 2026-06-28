#!/usr/bin/env bash
# Mirror the nLab content repo into state/mirrors/nlab (shallow).
# Safe to re-run: if repo already exists, `git -C <path> pull` updates it.
set -euo pipefail

REPO_URL="https://github.com/ncatlab/nlab-content.git"

# Resolve target relative to repo root regardless of invocation cwd.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
TARGET="${REPO_ROOT}/state/mirrors/nlab"

mkdir -p "$(dirname "${TARGET}")"

if [[ -d "${TARGET}/.git" ]]; then
  echo "[nlab_clone] existing mirror at ${TARGET}, pulling latest..."
  git -C "${TARGET}" pull --depth 1 --ff-only origin HEAD
else
  echo "[nlab_clone] cloning ${REPO_URL} -> ${TARGET} (shallow)..."
  git clone --depth 1 "${REPO_URL}" "${TARGET}"
fi

# Cheap sanity: a live nLab mirror should have thousands of .md pages.
count=$(find "${TARGET}" -type f \( -name '*.md' -o -name '*.txt' -o -name '*.html' \) | wc -l | tr -d ' ')
echo "[nlab_clone] done. content files: ${count}"
