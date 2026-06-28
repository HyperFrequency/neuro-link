---
skill_name: neuro-link-setup
trigger: /neuro-link-setup, "set up neuro-link", "configure the brain"
invokes: [skill-creator]
---

# /neuro-link-setup

Interactive first-run bootstrap for the neuro-link system.

## Scope

Walks the user through, in order:

1. **Prerequisites check** — Rust ≥1.90, Python 3.11+, Node 20+ or Bun, Docker
   (for Qdrant + Neo4j), `llama-server` binary, `huggingface-cli`, `ngrok`,
   Caddy (for the MCP auth proxy).
2. **Vault verification** — confirm new file structure (`00-neuro-link/`,
   `01-raw/`, `02-KB-main/`, `03-Ontology-main/{workflow,agents}/`,
   `04-Agent-Memory/`, `05-insights-HITL/`, `06-Recursive/`,
   `07-self-improvement-HITL/`, `08-code-docs/`).
3. **Secrets configuration** — populate `secrets/.env` with:
   - `NLR_BEARER_TOKEN` (shared by Obsidian plugin, Caddy, TurboVault)
   - `NGROK_AUTHTOKEN`, `NGROK_DOMAIN` (if reserved)
   - `QDRANT_URL`, `NEO4J_URL`, `NEO4J_AUTH`
   - HF / OpenAI / Anthropic API keys
4. **Model acquisition** — download Octen-Embedding-8B.Q8_0 via
   `huggingface-cli download mradermacher/Octen-Embedding-8B-GGUF`.
5. **MCP server registration** — write Claude Code `~/.claude.json` entries for:
   - `neuro-link-http` (Rust server, internal)
   - `neuro-link-recursive` (Rust server, tool-dense)
   - `turbovault` (public face behind Caddy+ngrok)
6. **Hook registration** — install UserPromptSubmit `auto-rag-inject.sh` and
   PostToolUse `neuro-grade.sh` into Claude Code settings.
7. **Skill installation** — run `/skill-creator generate` against each file
   in `00-neuro-link/`.
8. **First-run verification** — `state/heartbeat.json` status transitions
   from `initialized` → `ready`.

## Failure modes

- Missing binary → print exact install command, do not auto-install.
- Vault structure mismatch → show diff, ask before mutating.
- Secret collision → never overwrite existing secrets without `--force`.
- Model download interrupted → resume via `huggingface-cli`'s built-in resume.

## Deliverables

A populated `state/heartbeat.json` with `status: "ready"`, all MCP servers
reachable via `/mcp`, `/auto-rag preview` returning hits.
