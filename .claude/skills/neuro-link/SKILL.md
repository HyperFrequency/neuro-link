---
name: neuro-link
description: Main orchestrator for the neuro-link-recursive knowledge/memory control plane. Use this whenever the user says /neuro-link, asks about brain status, wants to trigger a scan, ingest a source, curate a wiki topic, check or dispatch the task queue, edit config, or get a system overview. Also trigger when the user says "what's in the brain right now", "run neuro", "what's pending", or refers to the vault's overall health. This is the primary entry point — it does not do heavy work itself; it dispatches to the right specialized skill (neuro-scan, neuro-surgery, hyper-sleep, crawl-ingest-update, job-scanner, wiki-curate, reasoning-ontology) and composes their outputs. Prefer this over invoking specialized skills directly when the user's intent is ambiguous.
---

# /neuro-link

Top-level orchestrator. Think of this skill as a dispatcher with taste: it parses the user's intent, picks the right downstream skill(s), assembles their outputs, and reports back with a brief executive summary.

## Subcommand map

Match the user's message against this table. If ambiguous, ask a single disambiguating question rather than guessing.

| Pattern                                                      | Dispatch to                                                 |
| ------------------------------------------------------------ | ----------------------------------------------------------- |
| `/neuro-link status` or "how's the brain" or "is X running"  | `scripts/status.sh` (no subagent needed)                    |
| `/neuro-link scan` or "scan for issues" or "find gaps"       | `/neuro-scan`                                               |
| `/neuro-link surgery` or "fix the failures" or "repair"      | `/neuro-surgery`                                            |
| `/neuro-link sleep` or "do nightly maintenance"              | `/hyper-sleep`                                              |
| `/neuro-link ingest <source>` or "ingest this paper"         | `/crawl-ingest-update`                                      |
| `/neuro-link tasks` or "what's pending" or "run the queue"   | list `00-neuro-link/tasks/*.md` + dispatch `/job-scanner`   |
| `/neuro-link config` or "edit neuro-link config"             | open `config/neuro-link.md` frontmatter only                |
| `/neuro-link health` or "vault health"                       | `tv_quick_health_check` + `tv_get_broken_links` via MCP     |
| `/neuro-link curate <topic>` or "write wiki about"           | `/wiki-curate`                                              |
| `/neuro-link ontology <domain>` or "refresh ontology"        | `/reasoning-ontology`                                       |

## Tool preferences

For reads, prefer these MCP tools in this order:

1. **TurboVault** (`tv_*`) — for vault-wide lexical search, link graph, frontmatter SQL, centrality. Use `tv_search` over `tv_read_note` + grep.
2. **neuro-link-recursive** (`nlr_*`) — for schema-aware wiki reads (`nlr_wiki_read`, `nlr_wiki_list`) and full RAG (`nlr_rag_query`, `nlr_rag_query_verified`).
3. **Direct file reads** (`Read`, `Glob`) — only for infrastructure files (`config/`, `.claude/`, `server/`, `scripts/`). Never for `02-KB-main/` — use `nlr_wiki_read` so access is logged.

For writes to knowledge artifacts, there is one rule: **always go through the schema-aware tool**, never `tv_write_note` or `Write` directly.

- `02-KB-main/*.md` → `nlr_wiki_create` / `nlr_wiki_update` (frontmatter schema enforced)
- `03-Ontology-main/**/*.md` → `/reasoning-ontology` (maintains both tiers + InfraNodus sync)
- `04-Agent-Memory/logs.md` → append via `scripts/log.sh` only (preserves JSONL ordering)

The *why*: direct writes bypass schema validation and break the auto-RAG index, which keyword-matches against frontmatter. Silent breakage is worse than a loud tool error.

## Auto-RAG delegation

`/neuro-link` never fetches context itself. The UserPromptSubmit hook at `hooks/auto-rag-inject.sh` runs before every prompt and injects relevant vault content. This skill reads that injection from the hook output and extends it only if the subcommand needs something the hook didn't grab (e.g., ontology detail for `/ontology` subcommand).

## Output contract

Always end with a 3-line executive summary the user can act on:

```
Did: <what happened>
State: <current system status after the action>
Next: <one recommended follow-up or "nothing">
```

This matters because the user typically uses `/neuro-link` as a status probe while they're doing other work. A 3-line summary fits in their peripheral attention; a long prose reply pulls them away from what they were doing.

## When to refuse / escalate

- **Destructive requests without `--force`**: if the user asks to delete a collection, wipe memory, or reset state, refuse and require explicit `--force` with a description of intent. Log the denial to `04-Agent-Memory/logs.md`.
- **Unknown subcommands**: don't guess — ask "did you mean X, Y, or Z?" referencing the subcommand table above.
- **Conflicts between MCP servers**: if `nlr_*` and `tv_*` return contradictory results for the same read, report the conflict rather than picking one. This usually signals an index drift (auto-rag hook log in `state/llm_logs/` will show when it last indexed).

## References

- `references/tool-namespaces.md` — full `nlr_*` vs `tv_*` tool map with rationale
- `references/subcommand-dispatch.md` — deeper dispatch logic including edge cases

## Scripts

- `scripts/status.sh` — fast health probe (heartbeat + Qdrant + Neo4j + MCP)
- `scripts/log.sh` — append-only log helper for `04-Agent-Memory/logs.md`
