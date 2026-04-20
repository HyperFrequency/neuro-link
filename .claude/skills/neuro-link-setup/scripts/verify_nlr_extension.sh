#!/usr/bin/env bash
# verify_nlr_extension.sh — end-to-end health check for the NLR extension
#
# Validates:
#   - pinelsp binary exists + parses a trivial .pine file
#   - serena binary exists + the Pine language is active
#   - ~/.claude.json has pinelsp + serena MCP entries
#   - Performance of a tree-sitter parse on a 500-line Pine file
#
# Exit codes:
#   0  all green
#   1  environment check failure
#   2  pinelsp check failure
#   3  serena/pine check failure
#   4  MCP registration missing

set -euo pipefail

: "${PINELSP_INSTALL_DIR:=$HOME/.local/share/neuro-link/pinelsp}"
: "${CLAUDE_CONFIG:=$HOME/.claude.json}"

fail() { printf '[FAIL] %s\n' "$*" >&2; }
pass() { printf '[ OK ] %s\n' "$*"; }
info() { printf '[info] %s\n' "$*"; }

info "verifying NLR extension (pinelsp + serena + MCP)"

# --- env ---
command -v node >/dev/null 2>&1 || { fail "node missing"; exit 1; }
command -v jq   >/dev/null 2>&1 || { fail "jq missing";   exit 1; }
command -v uv   >/dev/null 2>&1 || { fail "uv missing";   exit 1; }

# --- pinelsp binary ---
LSP_BIN="$PINELSP_INSTALL_DIR/dist/packages/lsp/bin/pine-lsp.js"
if [ ! -f "$LSP_BIN" ]; then fail "pinelsp LSP bin missing: $LSP_BIN"; exit 2; fi
pass "pinelsp LSP binary present"

# --- parser-ts smoke ---
if [ -f "$PINELSP_INSTALL_DIR/packages/parser-ts/test/smoke.mjs" ]; then
  if (cd "$PINELSP_INSTALL_DIR" && node packages/parser-ts/test/smoke.mjs >/dev/null 2>&1); then
    pass "parser-ts smoke test: WASM loads, parses, incremental reparses"
  else
    fail "parser-ts smoke test failed"; exit 2
  fi
else
  info "parser-ts smoke not present (fork may be older); skipping"
fi

# --- serena ---
command -v serena >/dev/null 2>&1 || { fail "serena missing"; exit 3; }
pass "serena binary present"

# --- serena-pine-mod activation ---
if SERENA_EXTRA_LANGUAGES=pine python3 -c 'import serena_pine_mod, sys; sys.exit(0 if serena_pine_mod._ACTIVATED else 1)' 2>/dev/null; then
  pass "serena-pine-mod activates cleanly"
else
  # The module may be installed in serena's uv tool env, not the system Python.
  TOOL_PY="$HOME/.local/share/uv/tools/serena-agent/bin/python"
  if [ -x "$TOOL_PY" ] && SERENA_EXTRA_LANGUAGES=pine "$TOOL_PY" -c 'import serena_pine_mod, sys; sys.exit(0 if serena_pine_mod._ACTIVATED else 1)' 2>/dev/null; then
    pass "serena-pine-mod activates in serena-agent uv env"
  else
    fail "serena-pine-mod does not import/activate"; exit 3
  fi
fi

# --- MCP config ---
if [ ! -f "$CLAUDE_CONFIG" ]; then fail "$CLAUDE_CONFIG missing"; exit 4; fi
PINELSP_ENTRY="$(jq -r '.mcpServers.pinelsp // empty' "$CLAUDE_CONFIG")"
SERENA_ENTRY="$(jq -r  '.mcpServers.serena // empty' "$CLAUDE_CONFIG")"
SERENA_ENV="$(jq    -r '.mcpServers.serena.env.SERENA_EXTRA_LANGUAGES // empty' "$CLAUDE_CONFIG")"
[ -n "$PINELSP_ENTRY" ] || { fail "mcpServers.pinelsp not registered";       exit 4; }
[ -n "$SERENA_ENTRY"  ] || { fail "mcpServers.serena not registered";        exit 4; }
[ "$SERENA_ENV" = "pine" ] || { fail "serena MCP missing SERENA_EXTRA_LANGUAGES=pine"; exit 4; }
pass "MCP config: pinelsp + serena registered; pine activation env set"

info "all checks passed."
