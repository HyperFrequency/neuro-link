#!/usr/bin/env bash
# install_pinelsp.sh — NLR install.sh step for pinelsp
#
# Clones HyperFrequency/pinelsp, installs deps, builds, registers the
# built `pine-lsp` binary as an MCP stdio server in ~/.claude.json.
# Idempotent; safe to re-run.
#
# Called from install.sh as step 13 (after step 12 verify).
#
# Env vars (all optional):
#   PINELSP_REPO_URL      default: https://github.com/HyperFrequency/pinelsp
#   PINELSP_INSTALL_DIR   default: $HOME/.local/share/neuro-link/pinelsp
#   PINELSP_BRANCH        default: main
#   DRY_RUN               0|1, respects parent install.sh
#   CLAUDE_CONFIG         default: $HOME/.claude.json

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=/dev/null
[ -f "$SCRIPT_DIR/_common.sh" ] && . "$SCRIPT_DIR/_common.sh" || true

: "${DRY_RUN:=0}"
: "${PINELSP_REPO_URL:=https://github.com/HyperFrequency/pinelsp}"
: "${PINELSP_INSTALL_DIR:=$HOME/.local/share/neuro-link/pinelsp}"
: "${PINELSP_BRANCH:=main}"
: "${CLAUDE_CONFIG:=$HOME/.claude.json}"

log()  { printf '[pinelsp] %s\n' "$*"; }
run()  { if [ "$DRY_RUN" = 1 ]; then printf '[dry] %s\n' "$*"; else eval "$@"; fi; }

need() {
  command -v "$1" >/dev/null 2>&1 || { printf 'MISSING: %s — %s\n' "$1" "$2" >&2; exit 4; }
}

need git  "install git first"
need node "install Node 20+ first"

NODE_MAJOR="$(node -v | sed 's/^v\([0-9]*\).*/\1/')"
if [ "$NODE_MAJOR" -lt 20 ]; then
  printf 'pinelsp requires Node >= 20; found: %s\n' "$(node -v)" >&2
  exit 4
fi

# corepack pnpm
if ! command -v pnpm >/dev/null 2>&1; then
  log "enabling corepack + activating pnpm"
  run "corepack enable"
  run "corepack prepare pnpm@10.28.0 --activate"
fi

# clone or pull
mkdir -p "$(dirname "$PINELSP_INSTALL_DIR")"
if [ ! -d "$PINELSP_INSTALL_DIR/.git" ]; then
  log "cloning $PINELSP_REPO_URL (branch $PINELSP_BRANCH) -> $PINELSP_INSTALL_DIR"
  run "git clone --branch '$PINELSP_BRANCH' '$PINELSP_REPO_URL' '$PINELSP_INSTALL_DIR'"
else
  log "existing clone — pulling latest"
  run "git -C '$PINELSP_INSTALL_DIR' fetch origin '$PINELSP_BRANCH'"
  run "git -C '$PINELSP_INSTALL_DIR' checkout '$PINELSP_BRANCH'"
  run "git -C '$PINELSP_INSTALL_DIR' pull --ff-only"
fi

# install + build
log "installing deps"
run "cd '$PINELSP_INSTALL_DIR' && pnpm install --prefer-offline --frozen-lockfile=false"

log "running build"
run "cd '$PINELSP_INSTALL_DIR' && pnpm run build && pnpm run build:tsc"

# MCP registration — only if we have jq
if ! command -v jq >/dev/null 2>&1; then
  log "jq not installed — skipping MCP registration. install jq and re-run."
  exit 0
fi

LSP_BIN="$PINELSP_INSTALL_DIR/dist/packages/lsp/bin/pine-lsp.js"
if [ "$DRY_RUN" = 1 ]; then
  log "[dry] would jq-merge pinelsp MCP entry into $CLAUDE_CONFIG"
  exit 0
fi

if [ ! -f "$LSP_BIN" ]; then
  log "expected LSP binary at $LSP_BIN but it is missing — build may have failed"
  exit 5
fi

if [ ! -f "$CLAUDE_CONFIG" ]; then
  log "creating minimal $CLAUDE_CONFIG"
  echo '{}' > "$CLAUDE_CONFIG"
fi
cp "$CLAUDE_CONFIG" "$CLAUDE_CONFIG.backup.$(date +%s)"

tmp="$(mktemp)"
jq --arg bin "$LSP_BIN" '
  .mcpServers //= {} |
  .mcpServers.pinelsp = {
    "command": "node",
    "args": [ $bin, "--stdio" ],
    "type": "stdio",
    "env": {}
  }
' "$CLAUDE_CONFIG" > "$tmp" && mv "$tmp" "$CLAUDE_CONFIG"

log "registered MCP server: pinelsp  ->  node $LSP_BIN --stdio"
log "done."
