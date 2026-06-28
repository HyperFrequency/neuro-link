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
NLR_BIN="${NLR_BIN:-$REPO_ROOT/server/target/release/neuro-link}"
TV_BIN="${TV_BIN:-$HOME/.cargo/bin/turbovault}"

cat > /tmp/nlr-mcp-patch.json <<JSON
{
  "mcpServers": {
    "neuro-link-recursive": {
      "type": "stdio",
      "command": "$NLR_BIN",
      "args": ["mcp"],
      "env": {
        "NLR_ROOT": "$REPO_ROOT"
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
