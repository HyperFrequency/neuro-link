#!/usr/bin/env bash
# _obsidian-plugins-build.sh
#
# Build every Obsidian plugin in .obsidian/plugins/ that ships as a source checkout
# (no main.js committed). Safe to re-run — npm install is idempotent and each plugin
# is isolated to its own directory.
#
# Usage:   ./_obsidian-plugins-build.sh                 # build all
#          ./_obsidian-plugins-build.sh dataview        # build a single plugin
#
# Requirements: node >= 18, npm. Installing via nvm is recommended.

set -euo pipefail

VAULT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PLUGINS_DIR="$VAULT_ROOT/.obsidian/plugins"

# Order matters only loosely — dataview first so other plugins that depend on it
# have it available; everything else is independent.
PLUGIN_ORDER=(
  dataview
  templater-obsidian
  table-editor-obsidian
  obsidian-git
  obsidian-kanban
  obsidian-excalidraw-plugin
  graph-analysis
  smart-connections
  neuro-link-recursive
)

build_one() {
  local plug="$1"
  local dir="$PLUGINS_DIR/$plug"

  if [[ ! -d "$dir" ]]; then
    echo "[skip] $plug — directory not found at $dir"
    return 0
  fi

  if [[ -f "$dir/main.js" && ! -f "$dir/package.json" ]]; then
    echo "[ok]   $plug — prebuilt main.js present, no package.json, nothing to build"
    return 0
  fi

  if [[ ! -f "$dir/package.json" ]]; then
    echo "[skip] $plug — no package.json, no main.js; inspect manually"
    return 0
  fi

  echo "[build] $plug"
  pushd "$dir" >/dev/null

  # Prefer ci if lockfile present for reproducibility
  if [[ -f package-lock.json ]]; then
    npm ci --no-audit --no-fund --loglevel=error || npm install --no-audit --no-fund --loglevel=error
  else
    npm install --no-audit --no-fund --loglevel=error
  fi

  # Most Obsidian plugins expose a `build` script. Fall back to common alternatives.
  if npm run | grep -qE '^\s+build\b'; then
    npm run build
  elif npm run | grep -qE '^\s+prod\b'; then
    npm run prod
  elif npm run | grep -qE '^\s+dist\b'; then
    npm run dist
  else
    echo "[warn] $plug — no build/prod/dist script; inspect package.json"
  fi

  if [[ -f main.js ]]; then
    echo "[done] $plug — main.js generated ($(wc -c <main.js) bytes)"
  else
    echo "[fail] $plug — build finished but main.js missing"
  fi

  popd >/dev/null
}

if [[ $# -gt 0 ]]; then
  for plug in "$@"; do build_one "$plug"; done
else
  for plug in "${PLUGIN_ORDER[@]}"; do build_one "$plug"; done
fi

echo
echo "Done. Restart Obsidian to pick up newly built plugins."
