---
title: MCP subsystem
parent: [[../index]]
tool: vectorbtpro
subsystem: mcp
last_updated: 2026-04-20
---

# MCP subsystem

`vectorbtpro.mcp_server` exposes the library's knowledge base to any
MCP-speaking LLM client (Claude Code, Claude Desktop, Cursor) via
stdio.

## Leaves

- `[[mcp_server]]` — `vectorbtpro.mcp_server`. Entry point:
  `python -m vectorbtpro.mcp_server`. Provides stdio MCP protocol
  over the knowledge base. Tool set: `search`, `find`, `run_code`,
  `get_source`.
- `[[mcp]]` — `vectorbtpro.mcp` — tool registry internals. Where
  individual MCP tools are registered via `@register_tool`.
- `[[knowledge]]` — `vectorbtpro.knowledge` subpackage. `AssetFunc`
  base class + a corpus of curated code examples and API snippets
  indexed for retrieval. `vbt.phelp(Portfolio.from_signals)` dispatches
  here.
- `[[cli]]` — `vectorbtpro.cli` — includes chat commands for interactive
  Q&A over the knowledge base without running a separate MCP server
  process.

## Registration (Claude Code)

```json
{
  "mcpServers": {
    "vectorbtpro": {
      "command": "python",
      "args": ["-m", "vectorbtpro.mcp_server"],
      "env": {
        "VECTORBTPRO_TOKEN": "ghp_..."
      }
    }
  }
}
```

## Usage from LLM

- `search("from_signals stops")` — fuzzy-match the knowledge base.
- `find("SizeType.TargetPercent")` — exact symbol lookup.
- `run_code("import vectorbtpro as vbt; print(vbt.__version__)")` —
  execute Python in the server's interpreter (returns stdout/stderr).
- `get_source("vectorbtpro.portfolio.base.Portfolio.from_signals")` —
  dump the source of any symbol by qualified name.

## See also

- Canonical wiki § MCP / Knowledge base:
  `hyperfrequency/docs/deep-tool-wiki/vectorbtpro/wiki.md#api-surface`
- `pinelsp` Claude-side registration: see container `.claude.json`.
- Pvt URL rotation: `~/.claude/skills/vectorbt/scripts/get-pvt-url.sh`.
