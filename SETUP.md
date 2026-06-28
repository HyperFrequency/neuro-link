# neuro-link-recursive Quick Start

Eight steps from zero to running. See **[INSTALL.md](INSTALL.md)** for detailed dependency installation.

## Step 1: Install

Install all prerequisites and the `nlr` binary. See [INSTALL.md](INSTALL.md) for full instructions.

Quick path:

```bash
npm install -g neuro-link-recursive
# or: cargo install --path server
# or: git clone ... && cd server && cargo build --release
```

## Step 2: Initialize

```bash
nlr init
```

Creates the directory tree, symlinks skills, installs hooks, initializes state files.

## Step 3: Configure Secrets

```bash
cp secrets/.env.example secrets/.env
```

Edit `secrets/.env` with your API keys. Required:

| Key | Where to get it |
|-----|-----------------|
| `INFRANODUS_API_KEY` | https://infranodus.com/settings |
| `FIRECRAWL_API_KEY` | https://firecrawl.dev/app/api-keys |
| `OPENROUTER_API_KEY` | https://openrouter.ai/keys |

See [INSTALL.md, section 6](INSTALL.md#6-api-keys) for the full key reference.

## Step 4: Review Master Config

Open `config/neuro-link.md` and review:

- `active_skills` -- which skills are enabled
- `directories.root` -- points to this directory
- `obsidian_vault` -- path to your Obsidian vault
- `scan_interval_minutes` -- how often neuro-scan checks for tasks
- `auto_curate` -- whether crawl-ingest auto-triggers wiki-curate
- `auto_rag` -- whether the auto-rag hook is active

## Step 5: First Ingest + Curate

Ingest a source:

```bash
/crawl-ingest https://karpathy.github.io/2024/01/llm-wiki/
```

Curate it into a wiki page:

```bash
/wiki-curate llm-wiki-concepts
```

Check the result in `02-KB-main/` and the mutation log in `02-KB-main/log.md`.

## Step 6: Set Up Auto-RAG

The `auto-rag-inject.sh` hook is already registered by `nlr init`. It injects relevant wiki context into every prompt.

Test it:

```bash
/auto-rag preview "How does NautilusTrader handle order routing?"
```

For semantic retrieval (instead of keyword matching), ensure Qdrant is running:

```bash
docker run -d -p 6333:6333 -v qdrant_storage:/qdrant/storage qdrant/qdrant
```

## Step 7: Connect Harnesses

Add the `neuro-link-recursive` MCP server to `~/.claude.json`:

```json
{
  "mcpServers": {
    "neuro-link-recursive": {
      "type": "stdio",
      "command": "nlr",
      "args": ["mcp"],
      "env": {"NLR_ROOT": "/path/to/neuro-link-recursive"}
    }
  }
}
```

For K-Dense and ForgeCode harness connections, see [INSTALL.md, section 9](INSTALL.md#9-harness-connections).

## Step 8: Enable Background Maintenance

Set up the heartbeat daemon to run neuro-scan on a schedule:

```bash
/schedule create --name "neuro-heartbeat" --cron "*/15 * * * *" --prompt "/neuro-scan"
```

This periodically checks for pending tasks, stale pages, knowledge gaps, and failures.

## Verify Everything

```bash
nlr status                    # All components OK
nlr config neuro-link        # Master config loaded
nlr tasks                    # Task queue visible
echo '{"jsonrpc":"2.0","method":"initialize","id":1}' | nlr mcp  # MCP responds
```

## Troubleshooting

See [INSTALL.md, section 12](INSTALL.md#12-troubleshooting) for the full troubleshooting table.
