---
version: 1
active_skills:
  - neuro-link
  - neuro-scan
  - wiki-curate
  - crawl-ingest
  - auto-rag
  - job-scanner
  - reasoning-ontology
  - neuro-link-setup
directories:
  root: /Users/DanBot/Desktop/HyperFrequency/neuro-link
  raw: ./00-raw
  sorted: ./01-sorted
  kb_main: ./02-KB-main
  ontology: ./03-ontology-main
  agents_workflows: ./04-KB-agents-workflows
  insights: ./05-insights-gaps
  reports: ./06-progress-reports
  tasks: ./07-neuro-link-task
  code_docs: ./08-code-docs
  business_docs: ./09-business-docs
  config: ./config
  state: ./state
  secrets: ./secrets
scan_interval_minutes: 30
default_llm: claude-sonnet-4-6
wiki_llm: claude-sonnet-4-6
ontology_llm: claude-opus-4-6
obsidian_vault: /Users/DanBot/Vaults/neuro-quant-vault
auto_curate: true
auto_rag: true
allowed_paths: all
# ── Heavy Services Mode: local | cloud | hybrid ──
# local:  run embeddings/Qdrant/Neo4j in Docker on this machine
# cloud:  use cloud APIs (Voyage/OpenAI embeddings, Qdrant Cloud, Neo4j Aura)
# hybrid: per-service override below
services_mode: local
embeddings:
  mode: local  # local | openai | voyage | cohere
  local_url: http://localhost:8400/v1/embeddings
  local_model: Octen/Octen-Embedding-8B
  local_dims: 4096
  cloud_provider: openai
  cloud_model: text-embedding-3-large
  cloud_api_key_env: OPENAI_API_KEY
vector_db:
  mode: local  # local | qdrant_cloud | pinecone
  local_url: http://localhost:6333
  cloud_url: ""
  cloud_api_key_env: QDRANT_CLOUD_API_KEY
graph_db:
  mode: local  # local | neo4j_aura | none
  local_uri: bolt://localhost:7687
  cloud_uri: ""
  cloud_api_key_env: NEO4J_AURA_PASSWORD
harness_bridge:
  enabled: false
  harnesses: []
llm_proxy:
  # Per-token-hash rate limit (requests per rolling 60-second window).
  rate_limit_rpm: 60
  # Per-token-hash daily USD cap (UTC-day); cost estimated from model pricing.
  daily_budget_usd: 50.0
  # Model allowlist (glob patterns). Anthropic native + OpenRouter pass-through.
  allowed_models:
    - "claude-*"
    - "anthropic/*"
    - "openrouter/*"
---

# neuro-link-recursive Master Config

Main orchestrator configuration. Edit the YAML frontmatter to change system behavior.

## Active Skills

All 8 Phase 1 skills are enabled by default. Remove a skill name from `active_skills` to disable it.

## Directory Mappings

The `directories` block maps logical names to filesystem paths. All paths are relative to `root` unless they start with `/`.

## LLM Routing

- `default_llm` — used for most operations (scanning, classification, context injection)
- `wiki_llm` — used for wiki page synthesis (needs strong writing + synthesis ability)
- `ontology_llm` — used for reasoning ontology generation (needs deep causal reasoning)

## Auto Behaviors

- `auto_curate: true` — crawl-ingest automatically triggers wiki-curate after ingestion
- `auto_rag: true` — auto-rag hook injects wiki context into every prompt

## Harness Bridge (Phase 3)

Set `harness_bridge.enabled: true` and add harness configs to `harnesses[]` when ready.
See `config/harness-harness-comms.md` for the full harness bridge configuration.

## Secrets + runtime credentials

All runtime credentials live in `secrets/.env` (mode 0600, gitignored). The following env vars are **required** before `docker compose up` — compose will fail-closed if any are missing:

| Env var | Purpose | Auto-generated? |
|---|---|---|
| `NLR_API_TOKEN` | Bearer token for the HTTP API | yes, on first `neuro-link serve --token auto` |
| `NEO4J_PASSWORD` | Neo4j graph DB password | yes, by `setup-deps.sh` on first run (32-char random) |

Rotation recipes:
- **NEO4J_PASSWORD** → see `docs/rotate-neo4j-password.md`
- **NLR_API_TOKEN** → `neuro-link serve --token auto` regenerates + persists

## LLM proxy guardrails

The `/llm/v1/*` proxy (see `server/src/api/llm_proxy.rs`) enforces three per-token
limits configured in the `llm_proxy:` frontmatter block below. Override via env
(e.g. `NLR_LLM_RPM=120`) or edit the block directly.

- `rate_limit_rpm` — requests per minute per token-hash. 429 when exceeded.
- `daily_budget_usd` — max cumulative cost (estimated) per token-hash per UTC
  day. 402 when exceeded. Ledger at `state/llm_quota.jsonl`.
- `allowed_models` — allowlist for the `model` field. Glob wildcards supported
  (`openrouter/*`, `anthropic/*`, `claude-*`). 400 on disallowed model.

All three default-deny if misconfigured (zero RPM / zero budget / empty list).
See `security-threats.md` T4 for the threat justification.
