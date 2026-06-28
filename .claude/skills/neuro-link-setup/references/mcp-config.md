# MCP server configuration

Three MCP servers. Each serves a different role and uses a different
transport for a reason.

## 1. neuro-link-recursive (stdio, internal)

```json
{
  "neuro-link-recursive": {
    "type": "stdio",
    "command": "/absolute/path/to/neuro-link",
    "args": ["mcp"],
    "env": {
      "NLR_ROOT": "/absolute/path/to/repo"
    }
  }
}
```

**Why stdio:** Claude Code talks to stdio servers with zero network overhead
and zero auth surface. This server has the full tool surface (`nlr_*`
tools), so we don't want it reachable by anything but Claude Code on this
machine.

## 2. neuro-link-http (http, localhost-only)

```json
{
  "neuro-link-http": {
    "type": "http",
    "url": "http://127.0.0.1:8787/mcp",
    "headers": {
      "Authorization": "Bearer ${NLR_API_TOKEN}"
    }
  }
}
```

**Why HTTP:** some integrations (e.g., the Obsidian plugin's existing code,
external harnesses via `harness-bridge`) need HTTP. This binds to 127.0.0.1
only — not publicly reachable. Bearer auth is defense in depth; the primary
gate is the bind interface.

## 3. turbovault (http, public via Caddy + ngrok)

```json
{
  "turbovault": {
    "type": "http",
    "url": "http://127.0.0.1:3001/mcp",
    "headers": {
      "Authorization": "Bearer ${NLR_API_TOKEN}"
    }
  }
}
```

**Why separate from neuro-link-http:** TurboVault ships with no auth
built-in. Its HTTP port 3001 is wrapped by Caddy on 443 with bearer-token
enforcement before the request ever reaches TurboVault. When ngrok exposes
Caddy's 443, the public internet hits the auth proxy, not TurboVault
directly. Locally, Claude Code still talks to TurboVault on 127.0.0.1:3001
because the bearer gets enforced at the ngrok edge anyway.

## Order of startup

1. Qdrant (docker)
2. Neo4j (docker)
3. llama-server (Octen, 127.0.0.1:8400)
4. neuro-link Rust server (spawns as both stdio client and http server on
   8787 when invoked with `mcp --http-port 8787`)
5. TurboVault (127.0.0.1:3001)
6. Caddy (proxies :443 → :3001 with bearer check)
7. ngrok (443 → Caddy)

`scripts/start_all.sh` (in the repo root, not this skill) handles ordered
startup.

## Caddy config for TurboVault proxy

Stored at `config/Caddyfile`:

```
https://{$NGROK_DOMAIN} {
    @authed {
        header Authorization "Bearer {$NLR_API_TOKEN}"
    }
    reverse_proxy @authed 127.0.0.1:3001

    respond "Unauthorized" 401
}
```

This is a strict allow-list — only requests with the correct bearer token
reach TurboVault. Everything else gets 401.

## Troubleshooting

**"tool not found" after restart**: Claude Code caches the tool list.
`/mcp reload` forces a refresh.

**"Connection refused" on 8787**: `neuro-link` Rust server not running.
`ps aux | grep neuro-link`.

**"401 Unauthorized" on turbovault**: bearer mismatch between `.env` and
what the client is sending. Check `~/.claude.json` env interpolation is
working — it needs the `${NLR_API_TOKEN}` syntax with the shell export
active.
