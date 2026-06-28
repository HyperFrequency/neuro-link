---
skill_name: neuro-link
trigger: /neuro-link, "brain status", "trigger a scan", "system overview"
invokes: [neuro-scan, neuro-surgery, hyper-sleep, crawl-ingest-update, job-scanner, auto-rag]
mcp: [turbovault, neuro-link-recursive]
---

# /neuro-link

Main orchestrator for the neuro-link system. Unified context/memory/behavior
control plane. Primary entry point for all neuro-link-recursive operations.

## Subcommands

| Subcommand                | Action                                                                 |
| ------------------------- | ---------------------------------------------------------------------- |
| `/neuro-link status`      | heartbeat.json + Qdrant + Neo4j + MCP health + pending task count      |
| `/neuro-link scan`        | delegates to `/neuro-scan`                                             |
| `/neuro-link surgery`     | delegates to `/neuro-surgery`                                          |
| `/neuro-link sleep`       | delegates to `/hyper-sleep`                                            |
| `/neuro-link ingest <src>`| delegates to `/crawl-ingest-update`                                    |
| `/neuro-link tasks`       | list `00-neuro-link/tasks/*.md`, invoke `/job-scanner` to dispatch     |
| `/neuro-link config`      | read/edit `config/neuro-link.md` frontmatter only                      |
| `/neuro-link health`      | TurboVault `quick_health_check` + `get_broken_links`                   |

## TurboVault tool usage

The skill calls TurboVault MCP tools (prefixed `tv_*`) for all vault interaction:

- `tv_read_note`, `tv_write_note`, `tv_edit_note` — file ops (generic)
- `tv_search` — BM25 lexical search (Tantivy)
- `tv_get_backlinks`, `tv_get_forward_links` — wikilink graph
- `tv_get_hub_notes`, `tv_get_centrality_ranking` — vault centrality
- `tv_query_frontmatter_sql` — GlueSQL over YAML frontmatter
- `tv_batch_execute` — atomic multi-file transactions

`nlr_wiki_*` tools (the schema-enforced variants for `02-KB-main/`) remain the
canonical write path when frontmatter schema must be preserved.

## State reads

- `state/heartbeat.json` — system status
- `state/llm_logs/` — per-token daily jsonl logs
- Qdrant: `nlr_wiki` (4096d) and `math_symbols` (4096d) collections
- Neo4j: ontology graph

## Auto-RAG hand-off

`/neuro-link` never injects context itself. The UserPromptSubmit hook runs the
`/auto-rag` router; `/neuro-link` subcommands read the already-injected context.
