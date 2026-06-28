---
name: neuro-link-setup
description: Interactive guided first-run bootstrap for the neuro-link-recursive system. Use this whenever the user says /neuro-link-setup, asks to "set up neuro-link", "configure the brain", "bootstrap the vault", "install neuro-link", or when state/heartbeat.json shows status=initialized. Also use when the user says the system isn't working on first install, when MCP servers aren't reachable on fresh clone, or when they need to reconfigure secrets/models/hooks. Walks conversationally through prereqs, vault structure verification, secrets population, Octen/Qwen3 model downloads, MCP server registration, hook installation, skill generation, and first-run verification. Never auto-installs missing binaries — always prints the exact command so the user can review before running.
---

# /neuro-link-setup

Guided setup for the neuro-link-recursive system. This skill is conversational — it asks one question at a time, confirms answers, and only proceeds after each step succeeds. Setup is deliberately slow and auditable because getting the brain's substrate wrong silently corrupts every downstream skill.

The ordered stages below are the canonical path. If the user is coming back after a partial setup, inspect `state/heartbeat.json` first to figure out which stage they're in; skip the completed ones.

## Stage 0 — Locate the repo root

Before touching anything, confirm the vault root.

1. Read `NLR_VAULT_ROOT` from `.claude/settings.json` env if present.
2. Otherwise ask the user for the absolute path.
3. Verify the path has `CLAUDE.md`, `config/`, `server/`, `00-neuro-link/`, and `state/`. If any are missing, stop and report which are missing.

Set `export NLR_ROOT=<path>` for all subsequent stages.

## Stage 1 — Prerequisites

Run `scripts/check_prereqs.sh` from this skill's directory. It prints a checklist:

- Rust 1.90+ (for `cargo install turbovault`)
- Python 3.11+ (for ingest pipelines)
- Node 20+ or Bun (for qmd)
- Docker (for Qdrant + Neo4j containers)
- `llama-server` binary (llama.cpp release build)
- `huggingface-cli` (for model downloads)
- `ngrok` (for public MCP exposure)
- `caddy` (for auth proxy in front of TurboVault)

**Never auto-install.** For each missing item, show the exact install command (homebrew, cargo, pip) and ask the user to run it. Re-run the checker after they report completion.

The *why*: TurboVault's Rust MSRV is 1.90+; older toolchains produce confusing link errors. `llama-server` must be the llama.cpp release binary — the Python `llama-cpp-python` wrapper has a different CLI and won't serve the `/v1/embeddings` endpoint the Rust server expects.

## Stage 2 — Vault structure verification

Confirm the current file structure matches the post-2026-04-18 layout documented in `CLAUDE.md`. Run `scripts/verify_vault_structure.sh`. Report any missing directories; offer to create them with `mkdir -p`. Never rename or move existing content — if the user is on an older layout, point them at `.planning/2026-04-18-turbovault-qmd-rebuild/02-file-structure-migration.md` and stop.

## Stage 3 — Secrets

Populate `secrets/.env` using the schema in `references/secrets-schema.md`. Required keys:

- `NLR_BEARER_TOKEN` — single token shared by Obsidian plugin, Caddy auth proxy, TurboVault (generate via `openssl rand -hex 32` if missing)
- `NGROK_AUTHTOKEN` and optionally `NGROK_DOMAIN` (reserved domain)
- `QDRANT_URL` (default `http://localhost:6333`)
- `NEO4J_URL`, `NEO4J_AUTH` (for ontology graph)
- `HF_TOKEN`, `OPENAI_API_KEY`, `ANTHROPIC_API_KEY` (for ingest + LLM calls)

**Never overwrite an existing secret without explicit `--force`**. If the file exists, read it first and only prompt for missing keys. Never echo secrets back to the conversation — just confirm "set" or "missing".

## Stage 4 — Model downloads

The system uses three GGUF models locally plus Octen for server embeddings:

| Model | Purpose | Target path |
|---|---|---|
| `Octen-Embedding-8B.Q8_0.gguf` | Server-side embedder (4096 dim) | `models/` |
| `Qwen3-Reranker-0.6B-Q8_0.gguf` | qmd reranker | qmd cache |
| `qmd-query-expansion-1.7B-q4_k_m.gguf` | qmd query expansion | qmd cache |

Run `scripts/download_models.sh`. It uses `huggingface-cli` with resume semantics — safe to interrupt and restart. The script verifies SHA256 against the HF manifest after download.

The *why* we chose Q8_0 for Octen: near-lossless for retrieval, ~8 GB resident vs 16 GB for f16, fits comfortably in unified memory alongside the two qmd models.

## Stage 5 — MCP server registration

Claude Code reads `~/.claude.json` to discover MCP servers. Add these three entries (script: `scripts/install_mcp_servers.sh`):

1. `neuro-link-recursive` — internal Rust server (stdio transport)
2. `neuro-link-http` — HTTP variant for localhost-only tool calls
3. `turbovault` — public face, served via Caddy+ngrok

See `references/mcp-config.md` for the full JSON blocks and why each is configured differently.

**Never modify an existing `~/.claude.json` without reading it first and preserving the user's other entries.** Use `jq` to merge, not overwrite.

## Stage 6 — Hook installation

Project-level hooks live in `.claude/settings.json` at the repo root. They are already configured by the skeleton; verify by reading that file. If the hooks point at `hooks/auto-rag-inject.sh` and `hooks/neuro-grade.sh`, ensure those files exist and are executable (`chmod +x`). If not, halt and point the user at `/auto-rag-setup` (part of the Phase 7 build).

## Stage 7 — Skill generation

Copy all 10 skills from `.claude/skills/` in the repo to `~/.claude/skills/`. Use `scripts/install_skills.sh` — it does **plain copy, not symlink** (per user preference). The script preserves SKILL.md, references/, scripts/, and assets/ subdirectories faithfully.

After copying, verify each skill triggers in Claude Code by running `claude /neuro-link status` and watching for the `/neuro-link` skill to be consulted. If it doesn't trigger, something is wrong with the description field — run `/skill-creator` against the spec to re-optimize.

## Stage 8 — First-run verification

Flip `state/heartbeat.json`'s `status` to `ready` only after all of these pass:

1. `cargo build --release -p neuro-link-server` succeeds
2. `neuro-link status` returns all green
3. Qdrant reachable at `$QDRANT_URL/collections` and lists `nlr_wiki`, `math_symbols`
4. Neo4j reachable with valid auth
5. `qmd --version` prints a version
6. `/auto-rag preview "test"` returns at least one hit
7. All 10 skills present in `~/.claude/skills/`

If any check fails, leave `status: initialized` and print the exact failing check so the user can diagnose. Do not silently mark `ready`.

## If the user wants to rerun a single stage

Accept subcommand arguments: `/neuro-link-setup stage-4` reruns the models stage only. Match against stage numbers 0–8. This is useful when a download fails partway.

## Escalation paths

If the user hits something this skill doesn't understand (obscure install error, unusual OS, corporate proxy), do not guess. Tell them exactly what failed, what you were trying to do, and suggest they open an issue on the repo with the error trace. Never "fix" a setup issue by editing config in ways that might later cause the brain to behave unexpectedly.
