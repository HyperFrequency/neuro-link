---
version: 1
bridge_mode: mcp2cli
ngrok:
  enabled: false
  auth_token_env: NGROK_AUTH_TOKEN
  domain: ""
  port: 8080
mcp_servers:
  infranodus:
    type: stdio
    command: mcporter
    required: true
    status: configured
  firecrawl:
    type: stdio
    required: true
    status: configured
  turbovault:
    type: stdio
    required: true
    status: configured
  context7:
    type: stdio
    required: true
    status: configured
  auggie:
    type: http
    url: https://api.augmentcode.com/mcp
    required: true
    status: configured
  qdrant:
    type: http
    url: http://localhost:6333
    required: false
    status: not_installed
  neo4j:
    type: bolt
    url: bolt://localhost:7687
    user_env: NEO4J_USER
    password_env: NEO4J_PASSWORD
    database: neo4j
    required: false
    status: not_installed
cloud_dispatch:
  modal:
    url: https://api.modal.com
    token_env: MODAL_TOKEN_ID
    secret_env: MODAL_TOKEN_SECRET
  ray:
    url: http://localhost:8265
  dask:
    url: http://localhost:8786
permissions:
  auto_rag_inject: true
  auto_curate: true
  auto_scan: true
  auto_delete: false
  auto_ontology_update: false
logging:
  session_log: true
  tool_log: true
  score_history: true
  deviation_log: true
  log_format: jsonl
  max_log_size_mb: 100
---

# neuro-link-config — System Configuration

Technical config for the neuro-link-recursive runtime. This file relays YAML settings for mcp2mcp2cli or mcp/api bridge.

## Bridge Mode

```yaml
bridge_mode: mcp2cli  # or ngrok_api or direct
```

- **mcp2cli**: Skills call MCP servers via mcp2cli CLI commands. Fast, local, no network overhead.
- **ngrok_api**: MCP servers exposed via Ngrok HTTPS tunnels. Required for remote harnesses.
- **direct**: stdin/stdout pipes between local processes.

## Ngrok Settings

When `ngrok.enabled: true`:
```bash
ngrok http ${ngrok.port} --domain=${ngrok.domain}
```

This exposes:
- Obsidian vault operations (TurboVault)
- Knowledge graph queries (InfraNodus)
- Wiki CRUD operations
- Task queue management
- Harness-to-harness message routing

## MCP Server Registry

Lists all MCP servers the system interacts with. `required: true` servers must be available for Phase 1 functionality.

### Adding a New MCP Server

1. Add config entry to the `mcp_servers` block above
2. Add the server to `~/.claude.json` under `mcpServers`
3. Run `/neuro-link status` to verify connectivity
4. Update `config/main-codebase-tools.md` if it's a code documentation server

## Permissions

Controls what the system can do automatically vs. what requires HITL approval:
- `auto_rag_inject` — inject wiki context into prompts (hook-driven)
- `auto_curate` — synthesize wiki pages after ingestion
- `auto_scan` — run periodic brain scans
- `auto_delete` — delete stale files (DANGEROUS — default false)
- `auto_ontology_update` — update reasoning ontologies without confirmation

## Logging

All logs are JSONL format in `state/`. Rotation happens when files exceed `max_log_size_mb`.

## Secrets Location

API keys are loaded from `secrets/.env` (gitignored). The system reads this file at skill invocation time. Keys can also be set as environment variables or in `~/.claude.json` MCP server configs.

## Editing This Config

Edit the YAML code block above or modify the frontmatter directly. Changes take effect on the next skill invocation — no restart required.
