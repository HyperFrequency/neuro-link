#!/usr/bin/env bash
# install_serena_pine_mod.sh — NLR install.sh step for Serena Pine Script support
#
# Serena's Language enum is closed (CSHARP, PYTHON, RUST, JAVA, ...). To
# let Serena treat .pine files as a first-class language that uses the
# pinelsp binary from step 13, we install a Python module that
# monkey-patches Serena at import time to:
#   1. Add `Language.PINE = "pine"` to the enum.
#   2. Register a PineLanguageServer class that spawns pinelsp via stdio.
#   3. Associate *.pine with PineLanguageServer in the factory.
#
# The mod ships as `serena-pine-mod`, installable via uv. Once installed,
# any Serena invocation that imports solidlsp will pick up the patch via
# the `SERENA_EXTRA_LANGUAGES` env var we set in
# ~/.claude.json's serena MCP entry.
#
# Idempotent; safe to re-run.

set -euo pipefail

: "${DRY_RUN:=0}"
: "${SERENA_PINE_MOD_DIR:=}"
: "${CLAUDE_CONFIG:=$HOME/.claude.json}"

# Default to the mod source shipped alongside this script
if [ -z "$SERENA_PINE_MOD_DIR" ]; then
  SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
  SERENA_PINE_MOD_DIR="$(cd "$SCRIPT_DIR/../serena-pine-mod" 2>/dev/null && pwd || true)"
fi

log()  { printf '[serena-pine] %s\n' "$*"; }
run()  { if [ "$DRY_RUN" = 1 ]; then printf '[dry] %s\n' "$*"; else eval "$@"; fi; }

need() {
  command -v "$1" >/dev/null 2>&1 || { printf 'MISSING: %s — %s\n' "$1" "$2" >&2; exit 4; }
}

need uv "install uv (curl -LsSf https://astral.sh/uv/install.sh | sh)"

# The mod is a library (no CLI entrypoints), so it cannot be installed
# as a uv *tool* on its own. The correct pattern is to install
# serena-agent as the tool and inject serena-pine-mod into that tool's
# environment via --with. This guarantees serena-pine-mod sits on the
# same sys.path as solidlsp/serena at runtime.
if [ -z "$SERENA_PINE_MOD_DIR" ] || [ ! -f "$SERENA_PINE_MOD_DIR/pyproject.toml" ]; then
  log "ERROR: serena-pine-mod source not found at \$SERENA_PINE_MOD_DIR"
  log "expected: $SERENA_PINE_MOD_DIR/pyproject.toml"
  exit 4
fi

log "installing serena-agent (primary tool) + serena-pine-mod (--with) via uv"
run "uv tool install serena-agent --with '$SERENA_PINE_MOD_DIR' --force"

# Export env for the mod's activation hook
if ! command -v jq >/dev/null 2>&1; then
  log "jq not installed — cannot auto-wire env into Serena MCP config. manual step below."
  log ""
  log "  To activate the mod, set this env var on Serena's MCP invocation:"
  log "    SERENA_EXTRA_LANGUAGES=pine"
  log ""
  exit 0
fi

if [ "$DRY_RUN" = 1 ]; then
  log "[dry] would set mcpServers.serena.env.SERENA_EXTRA_LANGUAGES=pine in $CLAUDE_CONFIG"
  exit 0
fi

if [ ! -f "$CLAUDE_CONFIG" ]; then
  echo '{}' > "$CLAUDE_CONFIG"
fi
cp "$CLAUDE_CONFIG" "$CLAUDE_CONFIG.backup.$(date +%s)"

tmp="$(mktemp)"
jq '
  .mcpServers //= {} |
  .mcpServers.serena //= {} |
  .mcpServers.serena.env //= {} |
  .mcpServers.serena.env.SERENA_EXTRA_LANGUAGES = "pine"
' "$CLAUDE_CONFIG" > "$tmp" && mv "$tmp" "$CLAUDE_CONFIG"

log "serena MCP env: SERENA_EXTRA_LANGUAGES=pine"
log "done."
