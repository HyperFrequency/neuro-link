# obsidian-headless — deferred pending upstream clarification

**Status (2026-04-24):** blocked on architectural ambiguity; escape-hatch per A3 spec.

## Investigation

Ran `grep -L "from 'obsidian'"` across `obsidian-plugin/src/**/*.ts`:
- 14 of 28 source files are **obsidian-API-free** — all providers (anthropic,
  openai, openrouter, local-llama, sse), agent plumbing (neuro-agent,
  safety-gates, system-prompt, tool-manifest, mcp-tool-source), and the
  dispatcher helpers.
- 14 of 28 files import `obsidian`: main.ts, commands.ts, chatbot.ts,
  settings.ts, views/**, harness-setup.ts, api-router.ts, mcp-setup.ts,
  mcp-vault-events.ts, dispatcher/new-spec.ts, stats.ts, agent/trace-logger.ts.
  These depend on Obsidian's `Plugin`, `App`, `Vault`, `TFile`,
  `WorkspaceLeaf`, `MarkdownView`, `Notice`, `TFolder`, etc.

## Key finding — the plugin is a CLIENT, not a server

`obsidian-plugin/src/mcp-vault-events.ts` documents itself as **HTTP
long-poll pull transport** for the TurboVault server running at
`http://localhost:8080/mcp` (settings.ts:236). The plugin does not host
an MCP surface; it consumes the TurboVault server's already-running
surface.

From `obsidian-plugin/src/settings.ts:929`:
> "Port the TurboVault MCP server listens on. Serves vault-event
> subscriptions (subscribe_vault_events, fetch_vault_events,
> unsubscribe_vault_events)."

**TurboVault runs as a separate Rust process** (`turbovault` binary,
port 3001 in the installer default; 8080 in plugin default). The MCP
surface that would need mirroring **already exists headlessly** —
it's just the Rust TurboVault binary + neuro-link HTTP server on 8787.

## What "headless obsidian" would actually mean

Three possible readings, none of which A3's 45-minute budget accommodates:

1. **"Run the chat UI / agent dispatcher without Electron"** — would need a
   DOM-free reimplementation of the chat-view rendering and a
   filesystem-backed Vault shim. ~14 files to refactor. The providers and
   agent core are already DOM-free, so the mechanical port is doable in a
   couple of days; the UX (how does the user talk to the agent?) is the
   real open question.

2. **"Run TurboVault's MCP surface headlessly"** — already done. The Rust
   binary doesn't need Obsidian to host its MCP. Ships from
   `pkg/macos/build.sh` + `pkg/linux-*/build.sh`. No new work.

3. **"Expose the plugin's extensions (custom `nlr_*` tools, chatbot, agent
   harness) as an MCP server so a non-UI client can drive them"** — this
   is the most useful reading but requires new design: which tools? what
   auth? what vault lifecycle (daemon vs one-shot)?

## Proposal

Before writing code, the user should confirm which reading they want. My
suggestion:

- **Near-term (R+4):** skip obsidian-headless entirely. TurboVault + the
  neuro-link Rust server already cover the MCP surface for any non-GUI
  client. The Obsidian plugin is a UI layer; "running the UI headlessly"
  without defining the UX produces something worse than both options.

- **Medium-term (R+5):** if a headless chat/agent is wanted, scaffold a
  Node CLI (`bin/neuro-chat`) that imports the already-DOM-free modules
  (`providers/**`, `agent/neuro-agent.ts`, `agent/safety-gates.ts`,
  `agent/system-prompt.ts`) and wires them to stdin/stdout or a
  WebSocket. This gets ~60% of the plugin's value (agent + LLM
  providers + tool dispatch) without touching Obsidian types.

## Files ready to extract (no refactor needed)

Short list for when this unblocks:

| File | Purpose |
|------|---------|
| `src/providers/base.ts` | Provider interface + streaming types |
| `src/providers/anthropic.ts` | Anthropic SDK client |
| `src/providers/openai.ts` | OpenAI-compatible client |
| `src/providers/openrouter.ts` | OpenRouter wrapper |
| `src/providers/local-llama.ts` | Local llama.cpp HTTP client |
| `src/providers/sse.ts` | SSE stream parsing |
| `src/providers/index.ts` | Provider registry |
| `src/agent/tool-manifest.ts` | Tool schema definitions |
| `src/agent/mcp-tool-source.ts` | MCP tool aggregation |
| `src/agent/system-prompt.ts` | System-prompt composition |
| `src/agent/neuro-agent.ts` | Core agent loop |
| `src/agent/safety-gates.ts` | HITL gating |
| `src/dispatcher/new-spec-helpers.ts` | Pure dispatcher helpers |
| `src/views/streaming-indicator.ts` | UI-agnostic streaming state |

## Decision needed from user

Pick one:

- [ ] **Skip** — `rm -rf pkg/obsidian-headless` and move on. MCP surface
  is already headless via TurboVault + neuro-link Rust binary.
- [ ] **Go with reading (3)** — scaffold a minimal CLI that re-exports
  the 14 DOM-free modules. Budget: ~1 week. Not part of A3.
- [ ] **Go with reading (1)** — full port (chat UI → terminal or web).
  Budget: unknown, requires design spike.

This file blocks A3 completion per the spec's escape hatch clause.
