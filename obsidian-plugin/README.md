# Neuro-Link Recursive — Obsidian Plugin

Unified GUI for the neuro-link-recursive system inside Obsidian. Manage API keys, execute NLR commands, configure harness bridges, use the BYOK chatbot, and view neuro-stats.

## Installation

### Community Plugins
1. Open Obsidian Settings > Community Plugins > Browse
2. Search for "Neuro-Link Recursive"
3. Install and enable

### Manual
1. Clone or copy `obsidian-plugin/` into your vault's `.obsidian/plugins/neuro-link-recursive/`
2. Run `npm install && npm run build` inside the plugin directory
3. Enable the plugin in Obsidian Settings > Community Plugins

## Setup

### 1. Configure Paths

Open Settings > Neuro-Link Recursive > Paths:
- **NLR Root**: Path to the neuro-link-recursive project. Auto-detected if the vault is inside the project.
- **NLR Binary**: Path to the `nlr` CLI binary. Defaults to `nlr` (expects it on your PATH).

### 2. API Keys

Settings > Neuro-Link Recursive > API Keys:
- Enter keys for each service (InfraNodus, Firecrawl, Context7, OpenRouter, etc.)
- Use "Test" buttons to verify connectivity
- "Save to secrets/.env" writes all keys to `NLR_ROOT/secrets/.env`
- "Load from secrets/.env" reads existing keys

Supported keys:
| Key | Service |
|-----|---------|
| INFRANODUS_API_KEY | Knowledge graphs & gap analysis |
| FIRECRAWL_API_KEY | Web scraping pipeline |
| CONTEXT7_API_KEY | Code docs & API signatures |
| OPENROUTER_API_KEY | LLM routing for chatbot |
| QDRANT_URL | Vector database |
| NEO4J_URI | Graph database |
| NGROK_AUTH_TOKEN | Remote tunnel access |
| KDENSE_API_KEY | K-Dense research harness |
| MODAL_TOKEN_ID | Modal cloud dispatch |

### 3. Harness Bridge Setup

Settings > Harness Connections, or use the command palette > "NLR: Open Harness Setup":
- Load existing harnesses from `config/harness-harness-comms.md`
- Add new harnesses with name, type, URL, API key, role, and capabilities
- Test connectivity for remote harnesses
- Save changes back to the config file

### 4. MCP Server Setup

Command palette > "NLR: Open MCP Setup":
1. **Install NLR binary** — via `cargo install` or build from source
2. **Configure Claude Code** — adds NLR as an MCP server in `~/.claude.json`
3. **mcp2cli-rs profile** — generates the tool mapping profile
4. **API router** — configure the HTTP server port
5. **Ngrok tunnel** — optional HTTPS exposure for remote harnesses

### 5. API Key Routing

Command palette > "NLR: Open API Router":
- Map API keys to provider endpoints (OpenRouter, Anthropic, OpenAI, K-Dense, Modal, custom)
- Test each route's connectivity
- Save routes to plugin settings or write to `config/neuro-link-config.md`

## BYOK Chatbot

Click the brain icon in the ribbon (or command palette > "NLR: Open Chatbot"):
- Type questions about your knowledge base
- Wiki context is automatically injected via RAG before each message
- Context pages used are shown below each response — click to open
- **Save to Wiki**: Creates a new wiki page from the assistant's response
- **Send to K-Dense / ForgeCode**: Dispatches the current task to another harness

Configure the model and system prompt in Settings > Neuro-Link Recursive > Chatbot.

## Neuro-Stats Dashboard

Click the chart icon in the ribbon (or command palette > "NLR: Open Stats"):
- **System Health**: Heartbeat status from `state/heartbeat.json`
- **Summary Cards**: Wiki pages, pending tasks, knowledge gaps, success rate, avg score
- **Tool Usage**: Bar chart of tool call frequencies from `state/session_log.jsonl`
- **Score Trend**: Line chart of session grades from `state/score_history.jsonl`
- **Recent Activity**: Table of last 10 tool calls with timing and status

## Commands

All available via the command palette (Ctrl/Cmd + P):

| Command | Action |
|---------|--------|
| NLR: Check Status | Run `nlr status` |
| NLR: Run Brain Scan | Run `nlr scan` |
| NLR: Ingest Current Note | Ingest the active file |
| NLR: Search Wiki | Open search modal |
| NLR: List Tasks | Show task queue |
| NLR: Create Task | Open task creation modal |
| NLR: Run Heartbeat | Send heartbeat |
| NLR: Start Ngrok Bridge | Start Ngrok tunnel |
| NLR: Rebuild RAG Index | Rebuild the RAG index |
| NLR: Grade Session | Grade current session |
| NLR: Open MCP Setup | MCP server setup wizard |
| NLR: Open Harness Setup | Harness configuration |
| NLR: Open API Router | API key routing |

## Troubleshooting

**"NLR binary not found"**
- Install: `cargo install neuro-link-mcp`
- Or set the full path in Settings > Paths > NLR Binary Path

**API key tests fail**
- Check the key is correct and not expired
- For Qdrant/Neo4j: ensure the service is running locally
- For OpenRouter: verify your account has credits

**No stats data showing**
- Ensure `state/heartbeat.json`, `state/session_log.jsonl`, and `state/score_history.jsonl` exist
- Run a heartbeat command to initialize state files

**Harnesses not loading**
- Verify `config/harness-harness-comms.md` exists and has valid YAML frontmatter
- Check NLR Root path is correct

**Chatbot returns errors**
- Set OPENROUTER_API_KEY in Settings > API Keys
- Verify the model ID is valid (check openrouter.ai/models)

## Development

```bash
cd obsidian-plugin
npm install
npm run dev    # Watch mode
npm run build  # Production build
```

The plugin compiles `src/main.ts` into `main.js` via esbuild. TypeScript types come from the `obsidian` package.
