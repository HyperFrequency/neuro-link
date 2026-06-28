# neuro-link-recursive — Install Guide

Post-2026-04-18 rebuild. The one-line install below is the happy path; the rest of the doc covers what each step does, how to diagnose failures, and how to uninstall.

---

## TL;DR

```bash
curl -fsSL https://raw.githubusercontent.com/HyperFrequency/neuro-link-recursive/master/install.sh | bash
```

That clones the repo to `~/neuro-link-recursive` (override with `NLR_INSTALL_DIR=...`) and runs `install.sh` from the clone. You can also clone manually and run `./install.sh` from inside.

Prefer to preview first:

```bash
./install.sh --dry-run
```

---

## What gets installed

The installer orchestrates 12 steps. Everything it does delegates to a purpose-built script under `.claude/skills/neuro-link-setup/scripts/` so logic lives in one place; `install.sh` is mostly control flow + summary reporting.

| # | Step | What | Idempotent? |
|---|---|---|---|
| 1 | Prereqs | Runs `check_prereqs.sh` — Rust ≥1.90, Python 3.11+, Node 20+/Bun, Docker, llama-server, huggingface-cli, ngrok, caddy, gh, jq | Yes — read-only probe |
| 2 | Vault structure | Runs `verify_vault_structure.sh`; auto-creates any missing directories via `mkdir -p` | Yes |
| 3 | TurboVault | `cargo install --git https://github.com/ahuserious/turbovault --features full` | Yes — cargo skips if up-to-date |
| 4 | Rust server build | `cd server && cargo build --release` | Yes — cargo incremental |
| 5 | Obsidian plugin | `cd obsidian-plugin && bun install && bun run build` (falls back to npm) | Yes |
| 6 | Secrets | Generates `secrets/.env` from `.env.example`; mints `NLR_API_TOKEN` only if absent | Yes — never overwrites |
| 7 | Data services | `docker compose up -d qdrant neo4j`; mints `NEO4J_PASSWORD` if absent; auto-starts Docker Desktop on macOS | Yes |
| 8 | Model downloads | `download_models.sh` — Octen Q8_0 (~8 GB), Qwen3-Reranker-0.6B Q8_0, qmd-query-expansion-1.7B Q4_K_M (~1 GB) | Yes — skips if files present + non-empty |
| 9 | llama-server | Installs `com.neurolink.llama-server` LaunchAgent (macOS) or `neurolink-llama-server.service` user unit (Linux); binds to `127.0.0.1:8400` | Yes — bootout + bootstrap |
| 10 | Skills | `install_skills.sh` — rsyncs 10 skills into `~/.claude/skills/` via plain copy (no symlinks) | Yes |
| 11 | MCP registration | `install_mcp_servers.sh` — jq-merges 3 MCP entries into `~/.claude.json`, backup preserved | Yes |
| 12 | Verify | Runs `status.sh` for a fast health probe across all components | Yes |

A final optional step only runs with `--with-tunnel`:

- **Public tunnel** — `scripts/start_public_tunnel.sh` starts Caddy + ngrok in the foreground. Blocks until SIGINT. Off by default because it's long-running.

---

## Flags

```
--mode local|cloud|hybrid   Services posture (default: local)
                            local  — run Qdrant/Neo4j/llama-server locally (default)
                            cloud  — skip local services; use cloud endpoints in secrets/.env
                            hybrid — per-service, edit config/neuro-link.md after install
--dry-run                   Print what would happen without executing anything
--with-tunnel               After verification, start the public tunnel in foreground
-h, --help                  Show usage
```

Environment:

- `NLR_INSTALL_DIR` — override curl\|bash bootstrap clone target (default `$HOME/neuro-link-recursive`)

---

## What you still have to do manually

The installer never auto-installs prerequisites (no `brew install X` silently, no `sudo` without explicit confirmation). If `check_prereqs.sh` surfaces something missing, it prints the install command. You run it, then re-run `./install.sh` — every step is idempotent.

Once the installer finishes, edit `secrets/.env` to add API keys you want wired:

- `HF_TOKEN` — required for `ar5iv` dataset access and HF model downloads
- `OPENAI_API_KEY` / `ANTHROPIC_API_KEY` — for LLM providers in the Obsidian plugin
- `NGROK_AUTHTOKEN` / `NGROK_DOMAIN` — if using `--with-tunnel`
- `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` — only if you'll ingest arXiv papers not in the ar5iv dataset (S3 requester-pays)
- `INFRANODUS_API_KEY` — if using the hosted InfraNodus MCP

Full schema at `.claude/skills/neuro-link-setup/references/secrets-schema.md`.

---

## Post-install: start the HTTP servers on demand

The installer does NOT start `neuro-link serve` (port 8787, internal HTTP MCP) or `turbovault serve` (port 3001, public-facing HTTP MCP) — those are on-demand. Start them when you need MCP-over-HTTP or the ngrok tunnel:

```bash
# neuro-link's own HTTP MCP (internal harness consumers)
neuro-link serve --http-port 8787 --token "$(grep ^NLR_API_TOKEN secrets/.env | cut -d= -f2)"

# TurboVault HTTP + WebSocket (consumed by Obsidian plugin + Caddy proxy)
turbovault serve --http-port 3001 --ws-port 3002
```

Or use `--with-tunnel` on the installer to go straight to a public tunnel:

```bash
./install.sh --with-tunnel
```

---

## Diagnosis

Each `install.sh` step that fails prints the exact command to reproduce the failure. The big ones:

| Symptom | Fix |
|---|---|
| `cargo install turbovault` fails | `cd /tmp && cargo install --git https://github.com/ahuserious/turbovault --features full -v` — verbose output shows the real error |
| `cargo build --release` in `server/` fails | `cd server && cargo build --release -v` |
| `docker compose up` fails with "daemon not running" | macOS: `open -a Docker` then wait 30s; Linux: `sudo systemctl start docker` |
| `huggingface-cli download` hangs / 401 | Add `HF_TOKEN` to `secrets/.env` then re-run `bash .claude/skills/neuro-link-setup/scripts/download_models.sh` |
| LaunchAgent bootstrap fails ("Bootstrap failed") | Usually: `launchctl` needs an active GUI session. Fallback is `launchctl load`; check logs at `~/Library/Logs/neuro-link/llama-server.err.log`. |
| systemd user unit fails on Linux | `systemctl --user status neurolink-llama-server.service -l` for the real error; may need `loginctl enable-linger $USER` for long-running user units |
| MCP server doesn't show up in Claude Code | `/mcp reload` forces a refresh; verify `~/.claude.json` has the entries via `jq '.mcpServers' ~/.claude.json` |
| `status.sh` shows TurboVault WARN | That's expected on a fresh install — `turbovault serve` isn't started automatically. Run it manually. |

Full status at any time:

```bash
bash .claude/skills/neuro-link/scripts/status.sh
```

---

## Configuration profiles (modes)

- `--mode local` (default): Qdrant, Neo4j, llama-server all run on this machine. This is the safe default and gives you 100% offline operation after model downloads.
- `--mode cloud`: Skip Docker services and llama-server install. You're expected to set cloud endpoints in `secrets/.env`:
  - `QDRANT_URL=https://<your-cluster>.qdrant.cloud`
  - `NEO4J_URI=neo4j+s://<your-aura>.databases.neo4j.io`
  - `EMBEDDING_API_URL=https://<your-inference-endpoint>/v1/embeddings`
- `--mode hybrid`: Treated like `local` on install; tune per-service by editing `config/neuro-link.md` frontmatter afterward.

---

## Uninstall

Run the companion script:

```bash
bash scripts/uninstall.sh
```

What it does (in order):

1. Stops and removes the `com.neurolink.llama-server` LaunchAgent (macOS) or `neurolink-llama-server.service` systemd user unit (Linux).
2. Stops the Qdrant + Neo4j containers (NOT the volumes — embeddings and graph data are preserved).
3. Removes the 10 neuro-link skills from `~/.claude/skills/` (other projects' skills untouched).
4. De-registers the 3 MCP server entries from `~/.claude.json` via jq; a backup is saved as `~/.claude.json.bak.uninstall.<timestamp>`.
5. Removes `~/.claude/state/nlr_root`.

What it explicitly does NOT do:

- Does NOT delete `secrets/.env` (pass `--purge-secrets` if you want that)
- Does NOT delete GGUF model files (pass `--purge-models`)
- Does NOT delete docker volumes (pass `--purge-data`)
- Does NOT uninstall the `turbovault` cargo binary (it may be used by other projects — run `cargo uninstall turbovault` manually)
- Does NOT touch vault content under `NN-*/` directories

Flags:

```
--dry-run       Print what would happen without doing it
--purge-data    Also remove docker VOLUMES (DESTRUCTIVE; asks again)
--purge-models  Also remove GGUF files under models/ and ~/.cache/qmd/models/
--purge-secrets Also remove secrets/.env
--yes           Skip confirmation prompts (non-interactive)
```

---

## For the curious: what each script does

The installer is thin; the real work is in these scripts (each is independently runnable and well-commented):

| Script | Purpose |
|---|---|
| `.claude/skills/neuro-link-setup/scripts/check_prereqs.sh` | Reports which tools are installed, prints install hints for missing ones, exits non-zero if any required tool is absent |
| `.claude/skills/neuro-link-setup/scripts/verify_vault_structure.sh` | Confirms every required vault directory exists; exits non-zero if any missing |
| `.claude/skills/neuro-link-setup/scripts/download_models.sh` | Downloads the 3 GGUFs via `huggingface-cli` with resume + post-download size check |
| `.claude/skills/neuro-link-setup/scripts/install_skills.sh` | rsync-copies `.claude/skills/*/` into `~/.claude/skills/`, removes stale symlinks from older installs |
| `.claude/skills/neuro-link-setup/scripts/install_mcp_servers.sh` | jq-merges 3 MCP entries into `~/.claude.json` with a timestamped backup |
| `.claude/skills/neuro-link/scripts/status.sh` | Fast health probe across heartbeat, Qdrant, Neo4j, llama-server, neuro-link HTTP, TurboVault, pending tasks, installed skills |
| `scripts/start_public_tunnel.sh` | Orchestration: Caddy + ngrok for public TurboVault exposure behind bearer auth |
| `scripts/uninstall.sh` | Reverses what `install.sh` did (preserves data by default) |

Each is designed to be safe to run standalone — you don't have to go through `install.sh` to use them.

---

## See also

- `README.md` — full stack overview and credits
- `.planning/2026-04-18-turbovault-qmd-rebuild/` — design docs and adversarial reviews
- `CLAUDE.md` — instructions Claude Code reads when operating in this repo
