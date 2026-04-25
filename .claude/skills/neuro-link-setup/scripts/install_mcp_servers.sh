#!/usr/bin/env bash
# Register the three neuro-link MCP servers in ~/.claude.json. Preserves any
# existing mcpServers entries via jq merge.

set -euo pipefail

CLAUDE_JSON="${HOME}/.claude.json"
REPO_ROOT="${NLR_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)}"

if ! command -v jq >/dev/null 2>&1; then
  echo "ERROR: jq not found. brew install jq" >&2
  exit 1
fi

if [[ ! -f "$CLAUDE_JSON" ]]; then
  echo "{}" > "$CLAUDE_JSON"
fi

# Backup before any mutation — ~/.claude.json also holds project-level
# session state, not something we want to lose.
cp "$CLAUDE_JSON" "$CLAUDE_JSON.bak.$(date +%s)"

# Build the three server entries
# Gate-62 BBBB1+BBBB2: probe triple-specific build outputs FIRST
# (matches what `make pkg` actually produces), then Linux package
# staging dirs (dist/staging-linux-*), then the generic local-cargo
# path as a true last-resort fallback. NLR_BIN= override always wins.
# Bias toward host arch: aarch64 candidates first on arm64 hosts,
# x86_64 first on x86 — newest matching artifact wins, stale generic
# never preempts a fresh triple-specific build.
if [[ -z "${NLR_BIN:-}" ]]; then
  HOST_ARCH=$(uname -m)
  HOST_OS=$(uname -s)
  CANDIDATES=()
  case "$HOST_OS:$HOST_ARCH" in
    Darwin:arm64)
      CANDIDATES+=("$REPO_ROOT/server/target/aarch64-apple-darwin/release/neuro-link")
      CANDIDATES+=("$REPO_ROOT/server/target/x86_64-apple-darwin/release/neuro-link")
      ;;
    Darwin:x86_64)
      CANDIDATES+=("$REPO_ROOT/server/target/x86_64-apple-darwin/release/neuro-link")
      CANDIDATES+=("$REPO_ROOT/server/target/aarch64-apple-darwin/release/neuro-link")
      ;;
    Linux:x86_64|Linux:amd64)
      CANDIDATES+=("$REPO_ROOT/server/target/x86_64-unknown-linux-gnu/release/neuro-link")
      CANDIDATES+=("$REPO_ROOT/dist/staging-linux-x86_64/server/neuro-link")
      CANDIDATES+=("$REPO_ROOT/dist/staging-linux-x86_64/neuro-link")
      ;;
    Linux:aarch64|Linux:arm64)
      CANDIDATES+=("$REPO_ROOT/server/target/aarch64-unknown-linux-gnu/release/neuro-link")
      CANDIDATES+=("$REPO_ROOT/dist/staging-linux-aarch64/server/neuro-link")
      CANDIDATES+=("$REPO_ROOT/dist/staging-linux-aarch64/neuro-link")
      ;;
  esac
  # Generic local-cargo path is the last fallback (rather than the
  # first); a fresh triple-specific build wins over a stale generic.
  CANDIDATES+=("$REPO_ROOT/server/target/release/neuro-link")
  for candidate in "${CANDIDATES[@]}"; do
    if [[ -x "$candidate" ]]; then
      NLR_BIN="$candidate"
      break
    fi
  done
  # If nothing exists, register the triple-specific generic path so
  # post-install validation (verify.py + install-mirror.sh) reports a
  # missing binary instead of silently passing.
  NLR_BIN="${NLR_BIN:-$REPO_ROOT/server/target/release/neuro-link}"
fi
TV_BIN="${TV_BIN:-$HOME/.cargo/bin/turbovault}"

cat > /tmp/nlr-mcp-patch.json <<JSON
{
  "mcpServers": {
    "neuro-link-recursive": {
      "type": "stdio",
      "command": "$NLR_BIN",
      "args": ["mcp"],
      "env": {
        "NLR_ROOT": "$REPO_ROOT",
        "NLR_WORKSPACE_ID": "${NLR_WORKSPACE_ID:-}"
      }
    },
    "neuro-link-http": {
      "type": "http",
      "url": "http://127.0.0.1:8787/mcp",
      "headers": {
        "Authorization": "Bearer \${NLR_API_TOKEN}"
      }
    },
    "turbovault": {
      "type": "http",
      "url": "http://127.0.0.1:3001/mcp",
      "headers": {
        "Authorization": "Bearer \${NLR_API_TOKEN}"
      }
    }
  }
}
JSON

# Merge without clobbering existing entries
jq -s '.[0] * .[1]' "$CLAUDE_JSON" /tmp/nlr-mcp-patch.json > "$CLAUDE_JSON.new"
mv "$CLAUDE_JSON.new" "$CLAUDE_JSON"
rm /tmp/nlr-mcp-patch.json

echo "MCP servers registered in $CLAUDE_JSON"
echo "  neuro-link-recursive  (stdio, internal)"
echo "  neuro-link-http       (http, 127.0.0.1:8787)"
echo "  turbovault            (http, 127.0.0.1:3001, served through Caddy at production)"
echo
echo "Backup saved to $CLAUDE_JSON.bak.*"
echo
echo "NLR_API_TOKEN must be set in secrets/.env for the HTTP servers to work."
