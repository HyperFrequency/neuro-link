# neuro-link-recursive

Unified context, memory & behavior control plane. Hybrid RAG + LLM-Wiki system with auto-curation, reasoning ontologies, and recursive self-improvement. Rust binary (`neuro-link`) + TurboVault SDK + qmd sidecar.

## Stack

| Layer                 | Component                                 |
| --------------------- | ----------------------------------------- |
| Vault interface (MCP) | **TurboVault** v1.4.0 (Rust SDK)          |
| Public face           | TurboVault HTTP + Caddy + ngrok + bearer  |
| Retrieval             | **qmd** (local BM25 + vector + rerank + query-expand) |
| Embedder              | **Octen-Embedding-8B Q8_0** (4096 dim)    |
| Reranker              | Qwen3-Reranker-0.6B                       |
| Query expansion       | Qwen3-1.7B                                |
| Internal orchestrator | `neuro-link` Rust server (5-way RRF)      |
| Vector DB             | Qdrant (`nlr_wiki`, `math_symbols`, 4096d, cosine) |
| Graph DB              | Neo4j (reasoning ontologies)              |

## Vault file structure

| Path                             | Purpose                                          |
| -------------------------------- | ------------------------------------------------ |
| `00-neuro-link/`                 | Default LLM instruction specs + task queue       |
| `01-raw/`                        | Immutable ingested sources (SHA256-named)        |
| `01-sorted/`                     | Classified sources (symlinks into `01-raw/`)     |
| `02-KB-main/`                    | LLM-synthesized wiki pages (schema-enforced)     |
| `03-Ontology-main/workflow/`     | Workflow ontologies (state, phase, goal)         |
| `03-Ontology-main/agents/`       | Agent ontologies (by-agent, by-state, by-HITL)   |
| `04-Agent-Memory/`               | Logs, consolidated memory, perf grades           |
| `05-insights-HITL/`              | Human-in-the-loop review queue (daily/weekly/all)|
| `06-Recursive/`                  | Recursive self-improvement reports               |
| `07-self-improvement-HITL/`      | Approved improvement proposals awaiting execution|
| `08-code-docs/my-repos/`         | User's main repos                                |
| `08-code-docs/toolbox/`          | Adjacent (not owned) tools                       |
| `08-code-docs/forked-up/`        | Forked repos with upstream diff tracking         |
| `.claude/`                       | Project-level Claude Code config                 |
| `config/`                        | YAML-frontmatter markdown config files           |
| `state/`                         | Runtime state (JSONL, heartbeat, logs)           |
| `secrets/`                       | `.env` + signing keys (gitignored)               |
| `server/`                        | Rust server source                               |
| `scripts/`                       | Ingest pipelines (Marker, MinerU, arXiv, Stacks) |
| `obsidian-plugin/`               | Existing TypeScript plugin (ngrok + bearer auth) |

## MCP tool surface

Two coexisting namespaces:

- `nlr_*` (internal, schema-enforced) — canonical write path for `02-KB-main/`
- `tv_*` (public via ngrok, generic) — TurboVault's 47 tools

Wiki writes go through `nlr_wiki_*` to preserve frontmatter schema.
Link-graph / centrality / vault-health / frontmatter-SQL / batch-execute use `tv_*`.

## Skills

Each skill has its spec in `00-neuro-link/<name>.md` and is generated into `.claude/skills/<name>/` via `/skill-creator`:

| Skill                           | Spec file                          | Purpose                                                    |
| ------------------------------- | ---------------------------------- | ---------------------------------------------------------- |
| `/neuro-link-setup`             | `neuro-link-setup.md`              | Bootstrap (prereqs, secrets, models, MCP, hooks)           |
| `/neuro-link`                   | `neuro-link.md`                    | Main orchestrator                                          |
| `/recursive-self-improvement`   | `recursive-self-improvement.md`    | Consortium-graded improvement loop                         |
| `/neuro-scan`                   | `neuro-scan.md`                    | Brain scanner (jobs, failures, gaps, stale wikis)          |
| `/neuro-surgery`                | `neuro-surgery.md`                 | HITL repair                                                |
| `/hyper-sleep`                  | `hyper-sleep.md`                   | Non-HITL maintenance                                       |
| `/crawl-ingest-update`          | `crawl-ingest-update.md`           | Deep ingest (Marker + MinerU + arXiv S3 + ar5iv + Stacks)  |
| `/main-codebase-tools`          | `main-codebase-tools.md`           | Index user's main repos (Context7 + Auggie)                |
| `/adjacent-tools-code-docs`     | `adjacent-tools-code-docs.md`      | Toolbox wiki for third-party tools                         |
| `/forked-repos-with-changes`    | `forked-repos-with-changes.md`     | Fork diff tracking + supplementary wikis                   |

## Subagents

- `@neuro` (`.claude/agents/neuro.md`) — orchestrator with restricted tool set and vault-scoped system prompt.

## Hooks

- `UserPromptSubmit`: `.claude/hooks/auto-rag-inject.sh` — routes between qmd (vault content) and `/docs-dual-lookup` (external library APIs)
- `PostToolUse`: `.claude/hooks/neuro-grade.sh` — append to `04-Agent-Memory/logs.md`

## Wiki page schema

Every `02-KB-main/*.md` has frontmatter:

```yaml
---
title: ...
domain: ...
sources: [...]
confidence: 0.0 .. 1.0
last_updated: YYYY-MM-DD
open_questions: [...]
---
```

Sections (in order): Overview (≤3 sentences) → Conceptual Model → Details → Contradictions → Open Questions → Sources.

Use `[[wikilinks]]` for InfraNodus-compatible entity linking. Use `[source:slug]` for inline citations. When sources disagree, add a Contradictions section with both positions + confidence.

## Ontology format

InfraNodus wikilink format:
```
[[entity1]] relation description [[entity2]] [relationCode]
```
Relation codes: `[isA]`, `[partOf]`, `[hasAttribute]`, `[causedBy]`, `[relatedTo]`, `[interactsWith]`, `[requires]`, `[produces]`, `[enables]`, `[contradicts]`.

Two tiers per topic: high-level summary (30–60 triples) + ultra-detailed (200–400 triples).

## Task queue

Files in `00-neuro-link/tasks/*.md` are job specs:
```yaml
---
type: ingest | curate | scan | repair | report | ontology | deep-reasoning
status: pending | running | completed | failed
priority: 1-5
created: YYYY-MM-DD
depends_on: []
assigned_harness: claude-code
source: neuro-scan | user | hyper-sleep | recursive-self-improvement
---
```
`/job-scanner` processes pending tasks sorted by priority.

## Rules

- Never modify files in `01-raw/` — immutable.
- Never write to `02-KB-main/` except via `nlr_wiki_*` or `/wiki-curate`.
- Never write to `03-Ontology-main/` except via `/reasoning-ontology`.
- Ontology edits require HITL approval (queue to `05-insights-HITL/`).
- Auto-synthesis capped at `confidence: 0.6`; higher needs HITL.
- Append, never rewrite, `04-Agent-Memory/logs.md`.
- `state/heartbeat.json` is the only file in `state/` that gets overwritten.
- Secrets in `secrets/.env` are gitignored — never read or display.
- Public ngrok exposure of TurboVault MUST go through the Caddy auth proxy.
