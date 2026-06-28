# neuro-link-recursive

A unified context, memory, and behavior control plane for AI agents — built on a local-first retrieval stack, a schema-enforced knowledge base, and a programmable Obsidian vault. Purpose-built for a single operator running multiple AI harnesses (Claude Code, Claude Desktop via MCP, and external LLMs via ngrok-tunneled MCP) against the same long-lived corpus.

Status (2026-04-18): post-rebuild. The vault has been reset to the new file structure; the retrieval stack is transitioning from a bespoke embeddings pipeline to qmd + Octen-Embedding-8B; the MCP surface is moving from the upstream TurboVault to our fork at [`ahuserious/turbovault`](https://github.com/ahuserious/turbovault) (adds `subscribe_vault_events` + the `FileRenamed` emission fix).

---

## Table of contents

1. [What this is](#what-this-is)
2. [Architecture at a glance](#architecture-at-a-glance)
3. [Stack](#stack)
4. [Quickstart](#quickstart)
5. [Full install](#full-install)
6. [Vault file structure](#vault-file-structure)
7. [Skills](#skills)
8. [MCP tool namespaces](#mcp-tool-namespaces)
9. [Retrieval stack](#retrieval-stack)
10. [Deep-ingest pipeline (math & papers)](#deep-ingest-pipeline-math--papers)
11. [@neuro chat agent](#neuro-chat-agent)
12. [Public exposure (ngrok + Caddy)](#public-exposure-ngrok--caddy)
13. [Configuration](#configuration)
14. [CLI](#cli)
15. [Observability](#observability)
16. [Development](#development)
17. [Credits & attribution](#credits--attribution)
18. [License](#license)

---

## What this is

A thinking augment layer. Three concrete problems it solves:

- **Build a persistent brain from ingested sources.** Papers, textbooks, code repos, notes, YouTube transcripts, Obsidian vaults — all get normalised through a deep-ingest pipeline into schema-enforced wiki pages and reasoning ontologies that any agent can query.
- **Inject the right context automatically.** A UserPromptSubmit hook runs on every prompt and routes to either `/auto-rag` (vault content, local qmd) or `/docs-dual-lookup` (external library APIs via Context7 + Augment Code), so the model always starts with relevant grounded context.
- **Let external harnesses drive the brain.** Obsidian plugin + TurboVault MCP + ngrok tunnel = any LLM can query the vault remotely behind a single bearer token. File drops into `00-neuro-link/` automatically generate scheduled tasks via a file-watch subscription.

It is explicitly a **single-operator** system. HITL gates are the default; auto-apply paths are reserved for narrowly-scoped maintenance.

---

## Architecture at a glance

```
                   ┌────────────────────────────────────────┐
                   │ External client / remote harness       │
                   └──────────────────┬─────────────────────┘
                                      │ HTTPS + Bearer
                           ┌──────────▼──────────┐
                           │       ngrok         │
                           └──────────┬──────────┘
                                      │
                           ┌──────────▼──────────┐
                           │ Caddy (auth gate)   │
                           │ bearer → proxy      │
                           └──────────┬──────────┘
                                      │ 127.0.0.1
                                      ▼
   ┌────────────────────────────────────────────────────────────────┐
   │              TurboVault (HyperFrequency fork)                  │
   │  - 47 base tools + subscribe_vault_events (new)                │
   │  - WebSocket notifications for vault events                    │
   │  - HTTP MCP endpoints                                          │
   │  - --features full (STDIO+HTTP+WebSocket+TCP+Unix)             │
   └────────┬─────────────────┬────────────────────┬────────────────┘
            │                 │                    │
    Vault MCP            File watch          Link-graph analytics
            │                 │                    │
            ▼                 ▼                    ▼
    ┌──────────────────────────────────────────────────────────────┐
    │        Obsidian plugin  (right-side chat + @neuro agent)     │
    │  - Provider-abstracted LLM client (OpenRouter / direct / ...) │
    │  - MCP subscription consumer (00-neuro-link/ file drops)      │
    │  - File-drop dispatcher → task spec generator                 │
    └──────────────────────┬───────────────────────────────────────┘
                           │ reads
                           ▼
                 .claude/skills/<name>/
                 00-neuro-link/<specs>.md
                 config/neuro-link.md

    ┌──────────────────────────────────────────────────────────────┐
    │         Retrieval backplane (local-first, 5-way RRF)         │
    │  BM25 (Tantivy) ─┐                                           │
    │                  ├─► 5-way RRF (k=60) ── rerank ── results   │
    │  Qdrant (4096d)  ├─► Octen-Embedding-8B via llama-server     │
    │  Context7 docs   │                                           │
    │  Auggie semantic │                                           │
    │  qmd hybrid      │  (sidecar: its own BM25 + vectors +       │
    │                  │   Qwen3 query expansion + cross-encoder)  │
    └──────────────────────────────────────────────────────────────┘

    ┌──────────────────────────────────────────────────────────────┐
    │                Knowledge graph & ontologies                  │
    │  Neo4j (reasoning ontologies)    InfraNodus (gaps, topics)   │
    └──────────────────────────────────────────────────────────────┘
```

---

## Stack

| Layer | Component | Why this one |
|---|---|---|
| Vault MCP | [TurboVault](https://github.com/ahuserious/turbovault) v1.4.0+ (our fork) | 47-tool surface over Obsidian-flavored Markdown; built on TurboMCP; our fork adds `subscribe_vault_events` + fixes the `FileRenamed` emission bug |
| Retrieval (local) | [qmd](https://github.com/tobi/qmd) | BM25 (SQLite FTS5) + dense + Qwen3 reranker + Qwen3 query expansion, all local via node-llama-cpp |
| Embedder | [Octen-Embedding-8B](https://huggingface.co/mradermacher/Octen-Embedding-8B-GGUF) Q8_0 | 8B-param encoder, 4096-dim, already wired into Rust server at `localhost:8400` |
| Reranker | [Qwen3-Reranker-0.6B](https://huggingface.co/ggml-org/Qwen3-Reranker-0.6B-Q8_0-GGUF) | Cross-encoder; qmd default; ~0.7 GB resident |
| Query expansion | [qmd-query-expansion-1.7B](https://huggingface.co/tobil/qmd-query-expansion-1.7B-gguf) | Qwen3-derived variant generator; ~1 GB resident |
| Vector store | [Qdrant](https://qdrant.tech/) | Collections: `nlr_wiki` (4096d), `math_symbols` (4096d + keyword-indexed `canonical_srepr`) |
| Graph store | [Neo4j](https://neo4j.com/) Community Edition | Reasoning ontologies — workflow & agent graphs |
| Full-text | [Tantivy](https://github.com/quickwit-oss/tantivy) (inside TurboVault) | BM25 lexical search, <100ms on 10k-note vaults |
| File watch | [notify](https://crates.io/crates/notify) v6 (inside TurboVault) | Cross-platform (inotify / FSEvents / ReadDirectoryChangesW) |
| Glob filter | [globset](https://crates.io/crates/globset) | DoS-safe, gitignore-style semantics — used by `subscribe_vault_events` filter |
| Auth proxy | [Caddy](https://caddyserver.com/) 2.x | Bearer-token gate in front of TurboVault's authless HTTP/WS |
| Tunnel | [ngrok](https://ngrok.com/) v3 | Reserved-domain public exposure with TLS termination at the edge |
| Harness 1 | [Claude Code](https://claude.com/claude-code) | Interactive session (user-initiated) |
| Harness 2 | [Obsidian](https://obsidian.md/) plugin | Autonomous session (event-initiated) — chat panel + file-watch dispatcher |
| Ingest: math papers | [Marker](https://github.com/datalab-to/marker) v1.10+ | PDF → Markdown with `--use_llm` inline math cleanup, native MPS on Apple Silicon |
| Ingest: math (second pass) | [MinerU](https://github.com/opendatalab/MinerU) 2.5 | MFD/MFR models with native MLX backend; used as cross-check against Marker |
| Ingest: arXiv | [ar5iv dataset](https://sigmathling.kwarc.info/resources/ar5iv-dataset-2024/) + [arXiv S3](https://info.arxiv.org/help/bulk_data_s3.html) | HTML+MathML for ~entire arXiv; skips PDF extraction where possible |
| Ingest: math books | [Stacks Project API](https://stacks.math.columbia.edu/api) + nLab dumps + Wikipedia math portal | Legally bulk-accessible canonical corpora |
| LaTeX canonicalisation | [pylatexenc](https://pypi.org/project/pylatexenc/) + [latex2sympy2_extended](https://pypi.org/project/latex2sympy2/) + SymPy `srepr` | Every `$$…$$` preserved byte-for-byte; canonical form cached in sidecar for exact-structure search |
| Knowledge graph analytics | [InfraNodus](https://infranodus.com/) MCP | Content gaps, topic clusters, adversarial review of ontologies |
| Upstream docs | [Context7](https://context7.com/) MCP | Library API snippets for ingested code dependencies |
| Semantic code index | [Augment Code / Auggie](https://www.augmentcode.com/) MCP | Per-repo semantic embeddings for user's main codebases |

---

## Quickstart

Assumes macOS Apple Silicon with ~32 GB unified memory. Linux and Windows are supported but install scripts are macOS-first.

```bash
# 1. Install prerequisites + build (one-shot)
curl -fsSL https://raw.githubusercontent.com/HyperFrequency/neuro-link-recursive/master/install.sh | bash

# 2. Run the guided setup inside Claude Code (recommended)
claude /neuro-link-setup

# 3. Verify
claude /neuro-link status
```

On first run, `/neuro-link-setup` walks you through:

1. Prerequisites check (Rust ≥1.90, Python 3.11+, Node 20+/Bun, Docker, llama-server, huggingface-cli, ngrok, caddy)
2. Vault structure verification
3. Secrets population (`secrets/.env`)
4. Model downloads (Octen Q8_0 ~8 GB + Qwen3 reranker + Qwen3 expansion — total ~10 GB)
5. MCP server registration (`~/.claude.json`)
6. Hook installation
7. Skill generation (plain copy into `~/.claude/skills/` — no symlinks)
8. First-run verification

No step auto-installs a missing binary. The checker prints the exact command — you review before running.

---

## Full install

### Prerequisites

| Tool | Install | Why |
|---|---|---|
| Rust ≥ 1.90 | `curl -fsSL https://sh.rustup.rs \| sh` | TurboVault MSRV is 1.90 |
| Python 3.11+ | `brew install python@3.12` | Ingest pipelines + LaTeX canonicalisation |
| Node 20+ or Bun | `brew install node` or `curl -fsSL https://bun.sh/install \| bash` | qmd runtime |
| Docker | [Docker Desktop](https://docs.docker.com/desktop/install/mac-install/) | Qdrant + Neo4j |
| llama-server | `git clone https://github.com/ggml-org/llama.cpp && cd llama.cpp && make -j` | Octen embeddings endpoint |
| huggingface-cli | `pip install --user 'huggingface_hub[cli]'` | Model downloads |
| ngrok | `brew install --cask ngrok` | Public tunnel |
| Caddy | `brew install caddy` | Bearer auth proxy |
| gh | `brew install gh` | GitHub CLI (used by `/forked-repos-with-changes`) |
| jq | `brew install jq` | MCP config merging |

### Step-by-step

```bash
# 1. Clone
git clone https://github.com/HyperFrequency/neuro-link-recursive.git
cd neuro-link-recursive

# 2. Install TurboVault from our fork with full feature flags
cargo install --git https://github.com/ahuserious/turbovault --features full

# 3. Build the Rust server
cargo build --release -p neuro-link-server

# 4. Start data services
docker compose up -d qdrant neo4j

# 5. Download models (~10 GB)
bash .claude/skills/neuro-link-setup/scripts/download_models.sh

# 6. Start llama-server for Octen
llama-server -m models/Octen-Embedding-8B.Q8_0.gguf --port 8400 &

# 7. Populate secrets/.env — see references/secrets-schema.md
openssl rand -hex 32  # → NLR_API_TOKEN
$EDITOR secrets/.env

# 8. Register MCP servers
bash .claude/skills/neuro-link-setup/scripts/install_mcp_servers.sh

# 9. Install skills (copy, no symlinks)
bash .claude/skills/neuro-link-setup/scripts/install_skills.sh

# 10. Verify
bash .claude/skills/neuro-link/scripts/status.sh
```

If `install.sh` has been run, steps 2–9 are performed automatically.

---

## Vault file structure

The post-rebuild layout. See `.planning/2026-04-18-turbovault-qmd-rebuild/02-file-structure-migration.md` for the old→new mapping.

```
00-neuro-link/          # Default LLM instruction specs + task queue
├── README.md
├── neuro-link-setup.md
├── neuro-link.md
├── recursive-self-improvement.md
├── neuro-scan.md
├── neuro-surgery.md
├── hyper-sleep.md
├── crawl-ingest-update.md
├── main-codebase-tools.md
├── adjacent-tools-code-docs.md
├── forked-repos-with-changes.md
└── tasks/              # Job queue; /job-scanner drains this

01-raw/                 # SHA256-named immutable ingested material
01-sorted/              # Classification-by-domain symlinks into 01-raw/
02-KB-main/             # Schema-enforced wiki pages (via nlr_wiki_*)
03-Ontology-main/
├── workflow/{SOT, state-definitions.md, phase-gating.md, goal-hierarchical.md}
└── agents/{by-agent, by-workflow-state, by-auto-HITL}/

04-Agent-Memory/
├── logs.md             # Append-only JSONL-ish event stream
├── consolidated.md     # Rolling summary
├── consolidated/{agent,workflow}/
└── perf-grade.md

05-insights-HITL/       # Human-in-the-loop review queue
├── daily.md
├── weekly.md
└── all-time.md

06-Recursive/           # /recursive-self-improvement reports
07-self-improvement-HITL/
├── overview.md
├── models/
├── hyperparameters/
├── prompts/
├── features/
└── code-changes/

08-code-docs/
├── my-repos/           # User-owned repos (/main-codebase-tools)
├── toolbox/            # Third-party tools (/adjacent-tools-code-docs)
└── forked-up/          # Forks with diff tracking (/forked-repos-with-changes)

.claude/
├── README.md
├── agents/neuro.md     # @neuro subagent definition
├── settings.json       # Hooks + env vars
├── skills/             # 10 full skill implementations (see §Skills)
└── hooks/              # Project-local hook scripts

config/                 # YAML-frontmatter markdown config files
hooks/                  # Hook scripts (auto-rag-inject.sh, neuro-grade.sh)
models/                 # GGUF binaries (Octen, Qwen3 variants — gitignored)
obsidian-plugin/        # TypeScript plugin source
scripts/                # Ingest pipelines, migration, public-tunnel orchestration
secrets/                # .env + signing keys (gitignored)
server/                 # Rust server source (Cargo workspace)
state/                  # Runtime state (heartbeat, llm_logs, mirrors, cache)
```

### Invariants

- **`01-raw/` is immutable.** SHA256-prefixed. Never modified in place.
- **`02-KB-main/` writes go through `nlr_wiki_*` or `/wiki-curate`**, never bare `Write` or `tv_write_note`. The schema (`title`, `domain`, `sources[]`, `confidence`, `last_updated`, `open_questions[]`) is enforced at write time.
- **`03-Ontology-main/` writes go through `/reasoning-ontology`** so both the summary and ultra-detailed tiers stay in sync, and InfraNodus gets notified.
- **`04-Agent-Memory/logs.md` is append-only.** Consolidation produces separate files in `consolidated/`; the raw log keeps a marker.
- **Auto-synthesis confidence cap 0.6.** Anything higher requires HITL.

---

## Skills

Ten skills live at `.claude/skills/<name>/` in this repo and are copy-installed into `~/.claude/skills/` by the installer (no symlinks — the user's Claude Code install is self-contained).

| Skill | What it does | Key resources |
|---|---|---|
| `/neuro-link-setup` | Interactive first-run bootstrap | 5 scripts (prereqs, vault verify, download models, install MCP, install skills) + 2 refs (secrets schema, MCP config) |
| `/neuro-link` | Main orchestrator, dispatches to other skills | `scripts/status.sh`, `scripts/log.sh`, `references/tool-namespaces.md` |
| `/recursive-self-improvement` | Consortium-graded improvement loop with HITL gate | `scripts/spawn_consortium.py`, `scripts/generate_rollback.py`, `references/consortium-protocol.md` |
| `/neuro-scan` | Six-pass brain scanner — jobs, failures, gaps, stale wikis | `scripts/scan_failures.sh`, `scripts/stale_wikis.sh` |
| `/neuro-surgery` | HITL-gated repair, per-change approval protocol | `references/hitl-protocol.md` (load-bearing) |
| `/hyper-sleep` | Non-HITL nightly maintenance with hard 4h timeout | `scripts/sleep_watchdog.sh`, `scripts/embed_warmup.sh`, `scripts/compact_memory.py`, `references/safety-bounds.md` |
| `/crawl-ingest-update` | Deep-ingest pipeline (see §Deep-ingest) | `scripts/ingest_arxiv.py`, `scripts/ingest_pdf_deep.sh`, `scripts/math_crosscheck.py`, `scripts/canonicalize.py` + 4 references |
| `/main-codebase-tools` | Register & index user's own repos | `scripts/register_repo.sh` |
| `/adjacent-tools-code-docs` | Toolbox wiki for third-party tools (upstream-watched) | `scripts/sync_upstream.sh` |
| `/forked-repos-with-changes` | Fork diff tracking with LLM-based semantic diff | `scripts/compute_fork_diff.py` |

Each skill's `SKILL.md` follows the `/skill-creator` methodology: pushy trigger description, imperative instructions, explained-why body, progressive-disclosure references.

---

## MCP tool namespaces

Two coexisting namespaces:

- **`nlr_*`** — internal, schema-enforced. Canonical write path for `02-KB-main/`. See `server/src/tools/`.
- **`tv_*`** — from TurboVault (our fork), generic vault surface. 47+ tools: file ops, Tantivy BM25, link graph, frontmatter SQL, atomic batch, vault health, and — new in the fork — `subscribe_vault_events` + `unsubscribe_vault_events`.

Full namespace map and decision tree at `.claude/skills/neuro-link/references/tool-namespaces.md`.

### Top `tv_*` capabilities the fork unlocks

| Tool | Purpose |
|---|---|
| `tv_get_backlinks` / `tv_get_forward_links` | Wikilink graph queries |
| `tv_get_hub_notes` / `tv_get_centrality_ranking` | Petgraph-based centrality |
| `tv_quick_health_check` / `tv_full_health_analysis` / `tv_get_broken_links` | Vault integrity |
| `tv_batch_execute` | Atomic multi-file transactions with rollback |
| `tv_query_frontmatter_sql` | GlueSQL over YAML frontmatter |
| `tv_subscribe_vault_events` *(fork)* | Filtered real-time event stream for watchers/clients |

---

## Retrieval stack

Five parallel sources fused via RRF (k=60) inside the Rust server, plus qmd as a local sidecar for its own BM25+rerank+expansion pipeline:

```
          user query
              │
              ▼
     ┌──────────────────┐
     │  qmd expansion   │  ← Qwen3-1.7B variants
     └────────┬─────────┘
              │ (query, variant₁, variant₂)
              ▼
    ┌─────────┴─────────┬──────────┬──────────┬──────────┐
    │                   │          │          │          │
    ▼                   ▼          ▼          ▼          ▼
 Tantivy BM25      Qdrant dense  Context7   Auggie     qmd hybrid
 (via TurboVault)  (Octen-8B)    (code docs)(semantic)  (BM25+vec+rerank)
    │                   │          │          │          │
    └─────────┬─────────┴──────────┴──────────┴──────────┘
              ▼
        5-way RRF fusion
              │
              ▼
        Qwen3-Reranker-0.6B cross-encoder (qmd)
              │
              ▼
       position-aware blend → final results
```

Embedding dimension is **4096** across both Qdrant collections (`nlr_wiki`, `math_symbols`) and Octen's output. Swapping embedders later requires a full re-index; the current setup avoids that by standardising on Octen day-one.

See `server/src/tools/rag.rs` for the RRF implementation and `scripts/math_ingest/qdrant_index.py` for the math-symbols schema.

---

## Deep-ingest pipeline (math & papers)

The old `scripts/math_ingest/` pipeline was replaced because it produced sub-kilobyte "clippings" unsuitable for retrieval. The current stack is:

| Source | Pipeline |
|---|---|
| arXiv paper (any domain) | **ar5iv HTML** → markdown (preserves MathML); fallback **arXiv S3 LaTeX** → LaTeXML → markdown; last resort Marker on the PDF |
| Math book / dense PDF | **Marker v1.10+** `--use_llm` + **MinerU 2.5** (MLX backend) cross-check via `scripts/math_crosscheck.py` |
| Standard PDF | `/pdf` skill + `markitdown` |
| Stacks Project | Official JSON API — no scraping |
| nLab | Official dump |
| Wikipedia math | Official bulk dumps |
| GitHub repo | `gh repo clone` + README + docs |
| YouTube | `yt-dlp` transcript + whisper fallback |
| Web | Parallel API (`/parallel-web`) or WebFetch |
| **MathWorld** | **BLOCKED** — Wolfram TOS forbids bulk scraping |

Every `$$…$$` block is preserved byte-for-byte. `scripts/canonicalize.py` computes a per-equation canonical SymPy `srepr` (with pylatexenc macro expansion + latex2sympy2_extended parsing) and stores it in a sidecar `.meta.json` — never mutating the markdown body. The canonical form is written to Qdrant's `math_symbols` keyword index so clients can search by equation structure regardless of notation variance.

Full docs: `.claude/skills/crawl-ingest-update/references/{arxiv-ar5iv,marker-pipeline,mineru-pipeline,latex-canonicalization}.md`.

---

## @neuro chat agent

The Obsidian plugin ships a right-side chat panel (adapted from [obsidian-copilot](https://github.com/logancyang/obsidian-copilot)'s patterns — see [Credits](#credits--attribution) for license posture).

- **`@neuro`** in the composer triggers agent mode: full tool access to all `tv_*` tools plus the 10 skills, tool-use loop with schema-preserving writes, system prompt loaded from `.claude/agents/neuro.md`.
- **Provider-agnostic LLM client**: OpenRouter (default), direct Anthropic, direct OpenAI, or local llama-server (localhost:8400 Octen, :8401 Qwen3).
- **File-drop dispatcher**: subscribes to TurboVault's `subscribe_vault_events` with the glob `00-neuro-link/*.md`. On create, reads frontmatter, asks the LLM to produce a task spec, writes to `00-neuro-link/tasks/<slug>.md`.

Plugin source: `obsidian-plugin/src/`. Build: `cd obsidian-plugin && bun install && bun run build`.

---

## Public exposure (ngrok + Caddy)

TurboVault's HTTP/WebSocket transports ship with **zero authentication** (they bind 127.0.0.1 only by default). To expose them publicly without punching that security model:

```
internet ──► ngrok edge (TLS) ──► Caddy :8080 ──► TurboVault :3001/:3002
                                    │
                                    ├── Bearer-token gate (NLR_API_TOKEN)
                                    ├── Strip `Authorization` header before upstream
                                    ├── /healthz bypass for ngrok edge probe
                                    └── Access logs with token redaction
```

Start it:

```bash
bash scripts/start_public_tunnel.sh
```

The script:

1. Loads `secrets/.env`; bails if `NLR_API_TOKEN` unset or shorter than 32 chars
2. Verifies TurboVault on `:3001`/`:3002`; starts it if not
3. Launches Caddy in foreground with `config/Caddyfile`
4. Launches ngrok with `config/ngrok.yml`
5. Prints the public URL + a curl one-liner you can use to smoke-test
6. Traps SIGINT to stop Caddy + ngrok cleanly

Token rotation: `/settings rotate-token` in the Obsidian plugin generates a new token, atomic-writes `secrets/.env`, stashes the previous token under `secrets/.env.previous` with a 5-minute TTL for optional overlap, then signals Caddy. Recommended cadence: 30 days, or immediately on any leak.

Hardening in place: CORS off by default (MCP isn't a browser surface), rate limiting at 60 req/min/IP, Caddy strips `Authorization` before forwarding to TurboVault so upstream logs can't leak it, ngrok inspect mode disabled (`web_addr: false`) in production.

---

## Configuration

### `secrets/.env` (gitignored)

```bash
NLR_API_TOKEN=<64 hex chars>    # shared by plugin, Caddy, TurboVault — openssl rand -hex 32
NGROK_AUTHTOKEN=<ngrok token>
NGROK_DOMAIN=<optional reserved domain>
QDRANT_URL=http://localhost:6333
NEO4J_URL=bolt://localhost:7687
NEO4J_AUTH=neo4j:<password>
HF_TOKEN=<hf token>              # model downloads + ar5iv access
OPENAI_API_KEY=<sk-...>
ANTHROPIC_API_KEY=<sk-ant-...>
# Optional:
AWS_ACCESS_KEY_ID=<for arxiv S3 requester-pays>
AWS_SECRET_ACCESS_KEY=<>
INFRANODUS_API_KEY=<>
```

Full schema: `.claude/skills/neuro-link-setup/references/secrets-schema.md`.

### `config/neuro-link.md`

Master config — YAML frontmatter only. Don't edit the body from scripts; that's docs.

### `.claude/settings.json`

Project-level Claude Code config. Hooks and env vars for the `QMD_*` stack live here.

---

## CLI

The `neuro-link` Rust binary is the single entry point for server/CLI operations.

```bash
neuro-link init              # Initialize dirs / scaffolds
neuro-link status            # Check all components
neuro-link config <name>     # Print a config file
neuro-link tasks             # List task queue
neuro-link mcp               # Run as MCP server (stdio)
neuro-link serve --port 8080 --token "$NLR_API_TOKEN"  # HTTP MCP
```

Also available via skills:

```bash
claude /neuro-link status
claude /neuro-link tasks
claude /neuro-scan
claude /hyper-sleep
claude /crawl-ingest-update <source>
```

---

## Observability

- `state/heartbeat.json` — overall status (`initialized` | `ready` | `degraded` | `error`)
- `state/llm_logs/<token_hash>/<date>.jsonl` — per-bearer-token daily LLM interaction log
- `state/hooks/` — hook exit codes + stderr
- `state/cron/` — scheduled run logs
- `04-Agent-Memory/logs.md` — append-only application-level event log
- `06-Recursive/daily.md` — daily scan report + surgery log + sleep report

`/neuro-scan` rolls the above into a single daily health view.

---

## Development

```bash
# Rust server
cargo test --workspace --all-features
cargo clippy --workspace --all-features -- -D warnings

# Obsidian plugin
cd obsidian-plugin && bun run dev  # watch mode

# TurboVault fork — contribute from our fork
cd ../turbovault && git checkout -b feat/your-change
# Make changes, then open a PR against ahuserious/turbovault:main
```

See `.planning/2026-04-18-turbovault-qmd-rebuild/` for ongoing work and adversarial review logs.

---

## Credits & attribution

This project stands on a lot of excellent open-source work. We integrate each of the following — use them properly, credit them visibly, and comply with their licenses.

### Core third-party dependencies

| Project | License | How we use it |
|---|---|---|
| [TurboVault](https://github.com/Epistates/turbovault) (upstream) / [HyperFrequency fork](https://github.com/ahuserious/turbovault) | MIT | Obsidian-vault MCP server — forked to add `subscribe_vault_events` + `FileRenamed` emission fix. Fork changes upstreamable. |
| [TurboMCP](https://github.com/Epistates/turbomcp) | MIT | The MCP server framework TurboVault is built on. Indirect dependency. |
| [qmd](https://github.com/tobi/qmd) | See repo | Local hybrid BM25 + dense + rerank + query expansion — used as retrieval sidecar. We'll submit an upstream PR to make the embedder prompt-format routable to support Octen explicitly. |
| [Octen-Embedding-8B-GGUF](https://huggingface.co/mradermacher/Octen-Embedding-8B-GGUF) | Per Hugging Face model card | 4096-dim encoder used for server-side embedding via llama-server. GGUF quants by mradermacher. Base model derived from Qwen3-Embedding-8B. |
| [Qwen3-Reranker-0.6B-GGUF](https://huggingface.co/ggml-org/Qwen3-Reranker-0.6B-Q8_0-GGUF) | Apache 2.0 (base) | qmd's default cross-encoder reranker. |
| [qmd-query-expansion-1.7B-gguf](https://huggingface.co/tobil/qmd-query-expansion-1.7B-gguf) | Per model card | qmd's default query-expansion generator. |
| [Qdrant](https://qdrant.tech/) | Apache 2.0 | Vector store for dense embeddings — `nlr_wiki` (4096d) and `math_symbols` (4096d + keyword-indexed `canonical_srepr`). |
| [Neo4j](https://neo4j.com/) Community Edition | GPL v3 | Reasoning ontology graph. Community edition run in Docker; no commercial features used. |
| [Tantivy](https://github.com/quickwit-oss/tantivy) | MIT | Full-text BM25 index inside TurboVault. |
| [notify](https://crates.io/crates/notify) | Dual MIT / Apache 2.0 | Cross-platform file watcher underlying our `subscribe_vault_events`. |
| [globset](https://crates.io/crates/globset) | Dual MIT / Unlicense | Gitignore-style glob matching for subscription filters. |
| [tokio](https://tokio.rs/) | MIT | Async runtime. |
| [serde](https://serde.rs/) | Dual MIT / Apache 2.0 | Serialization framework. |
| [petgraph](https://crates.io/crates/petgraph) | Dual MIT / Apache 2.0 | Graph algorithms inside TurboVault. |
| [GlueSQL](https://github.com/gluesql/gluesql) | Apache 2.0 | SQL over YAML frontmatter via TurboVault's `tv_query_frontmatter_sql`. |
| [pulldown-cmark](https://github.com/raphlinus/pulldown-cmark) | MIT | CommonMark parser. |

### Ingest pipeline

| Project | License | How we use it |
|---|---|---|
| [Marker](https://github.com/datalab-to/marker) v1.10+ | GPL v3 (free for personal/research use) | PDF → Markdown with inline math cleanup via `--use_llm`. Primary PDF extractor. |
| [MinerU](https://github.com/opendatalab/MinerU) 2.5 | Apache 2.0 / AGPL-3.0 dual | Second-opinion PDF extractor; dedicated MFD/MFR models for math. |
| [ar5iv dataset](https://sigmathling.kwarc.info/resources/ar5iv-dataset-2024/) (SIGMathLing / KWARC) | Per dataset license | Pre-rendered HTML+MathML of most of arXiv. Request access via the form. |
| [arXiv](https://arxiv.org/) bulk S3 | Per arXiv terms | Requester-pays LaTeX source fallback. |
| [LaTeXML / ar5ivist](https://github.com/dginev/ar5ivist) | Public domain | LaTeX source → HTML+MathML on the S3 fallback path. |
| [Stacks Project](https://stacks.math.columbia.edu/) | GFDL 1.3 | Public JSON API for structured canonical-mathematics content. |
| [pylatexenc](https://pypi.org/project/pylatexenc/) | MIT | LaTeX macro expansion before canonical-form parsing. |
| [latex2sympy2-extended](https://pypi.org/project/latex2sympy2/) | MIT | LaTeX → SymPy expression tree. |
| [SymPy](https://www.sympy.org/) | BSD | Canonical `srepr` generation. |
| [markitdown](https://github.com/microsoft/markitdown) | MIT | HTML / non-PDF document → Markdown. |
| [notify](https://crates.io/crates/notify) | Dual MIT / Apache 2.0 | See above. |

### External services

| Service | How we use it |
|---|---|
| [Claude (Anthropic)](https://claude.com/claude-code) | Primary LLM and harness. This repo is purpose-built for Claude Code; most skills assume it. |
| [OpenRouter](https://openrouter.ai/) | Default provider in the Obsidian plugin (routes to any supported model). |
| [Context7](https://context7.com/) | Upstream library documentation index — MCP consumer. |
| [Augment Code / Auggie](https://www.augmentcode.com/) | Per-repo semantic code embeddings — MCP consumer. |
| [InfraNodus](https://infranodus.com/) | Content-gap analysis + topical clustering over ontologies. |
| [ngrok](https://ngrok.com/) | Tunnelling TurboVault's public HTTP/WS endpoint. |
| [Hugging Face](https://huggingface.co/) | Model hosting + dataset access. |

### Tooling & infra

| Project | License | How we use it |
|---|---|---|
| [Caddy](https://caddyserver.com/) | Apache 2.0 | Bearer-token reverse proxy in front of TurboVault. |
| [Obsidian](https://obsidian.md/) | Proprietary (free for personal use) | Primary vault UI + our plugin's host. |
| [obsidian-copilot](https://github.com/logancyang/obsidian-copilot) | **AGPL-3.0** | We adapt UI patterns from `CopilotView.tsx`, `ChatSingleMessage.tsx`, `langchainStream.ts`, `Mention.ts`. Because this plugin is **not distributed externally** and the chat UI is never reachable over a network by users other than the operator, AGPL §13 is not triggered. If the plugin is ever distributed or exposed to remote users, the entire derivative plugin must be released under AGPL-3.0 with source. Each adapted file carries `SPDX-License-Identifier: AGPL-3.0-only` and the upstream LICENSE is preserved under `THIRD_PARTY/obsidian-copilot/`. |

### Research toolchain

The following are consulted in the codebase and wiki but not bundled:

- [olmOCR](https://olmocr.allenai.org/) (Allen AI) — evaluated, CUDA-first so parked until the Linux/GH200 side comes up.
- [Marker Datalab](https://www.datalab.to/) blog + [OmniDocBench](https://openaccess.thecvf.com/content/CVPR2025/papers/Ouyang_OmniDocBench_Benchmarking_Diverse_PDF_Document_Parsing_with_Comprehensive_Annotations_CVPR_2025_paper.pdf) (CVPR 2025) — benchmarks that informed the Marker + MinerU choice.
- [Nougat (Meta, 2023)](https://github.com/facebookresearch/nougat) — pioneered the category; now effectively abandoned, superseded by Marker / MinerU / olmOCR.

### Upstream PRs planned

- `tobi/qmd` — route embedder prompt format to Qwen3-style when the model matches `/octen/i` (plus the documented doc-prefix `"- "` fix for Octen's documented quirk).
- `Epistates/turbovault` — upstream the `FileRenamed` emission fix; the `subscribe_vault_events` registry is kept in our fork while the TurboMCP notification API stabilises.

If anything here is mislabeled or undercredited, open an issue or PR — we'll fix it.

---

## License

MIT. See [LICENSE](LICENSE) for the full text.

Note that the **Obsidian plugin** in `obsidian-plugin/` contains files adapted from AGPL-3.0 sources and is therefore subject to AGPL-3.0 if ever distributed or exposed as a network service to remote users. The server, skills, configuration, and documentation in the rest of this repository remain under MIT.
