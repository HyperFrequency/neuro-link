# Tool namespaces: `nlr_*` vs `tv_*`

Two MCP servers expose two tool namespaces that partially overlap. The rules
below keep them from stepping on each other.

## Quick decision tree

```
Need to read from 02-KB-main/ (curated wiki)?
  → nlr_wiki_read          (schema-aware)

Need to read from anywhere else in the vault?
  → tv_read_note  or  tv_search

Need to write/edit 02-KB-main/ or 03-Ontology-main/?
  → nlr_wiki_update / nlr_wiki_create    (schema enforced)
  → /reasoning-ontology                  (ontology-aware)

Need to write elsewhere in the vault?
  → tv_write_note / tv_edit_note

Need full hybrid RAG (BM25 + dense + rerank + query expansion)?
  → nlr_rag_query          (5-way RRF: BM25 + Octen + Context7 + Auggie + qmd)

Need vault link graph / centrality / SQL over frontmatter?
  → tv_get_backlinks, tv_get_hub_notes, tv_query_frontmatter_sql, ...
```

## Why two namespaces

The Rust server (`nlr_*`) enforces the schema on `02-KB-main/` wiki pages:

- Required frontmatter fields: `title`, `domain`, `sources`, `confidence`,
  `last_updated`, `open_questions`
- Required sections in body, in order
- Wikilink and citation formatting

TurboVault (`tv_*`) is generic — it treats the vault as plain markdown.
That's exactly right for 90% of vault operations, but wrong for `02-KB-main/`
where the schema is what makes the wiki usable by InfraNodus, the auto-RAG
index, and downstream consumers.

So: **TurboVault for everything except the schema-bound paths**. The
schema-bound paths go through `nlr_*` tools, which reject writes that would
break the schema. The write fails loud instead of silently corrupting the
wiki.

## Full tool map

### nlr_* (internal / localhost-only)

| Tool | Purpose |
|---|---|
| `nlr_wiki_read` | Read an `02-KB-main/*.md` page with frontmatter parsed |
| `nlr_wiki_create` | Create a new wiki page; rejects if frontmatter invalid |
| `nlr_wiki_update` | Update a wiki page; preserves frontmatter unless `--frontmatter-only` |
| `nlr_wiki_list` | List wiki pages with schema-aware filtering (by domain, confidence, etc.) |
| `nlr_wiki_search` | Text search within wiki pages (uses Tantivy via TurboVault under the hood) |
| `nlr_rag_query` | 5-way RRF: BM25 + Qdrant/Octen + Context7 + Auggie + qmd |
| `nlr_rag_query_verified` | Same as above but with grading-agent verification |
| `nlr_rag_embed` | Trigger re-embedding of a path |
| `nlr_math_lookup` | Math-specific 3-way RRF (BM25 + vectors + canonical srepr) |
| `nlr_graph_traverse` | Reasoning ontology traversal (Neo4j) |
| `nlr_task_*` | Task queue CRUD |
| `nlr_harness_dispatch` | Delegate to non-Claude harnesses |
| `nlr_state_heartbeat` | Update heartbeat |
| `nlr_state_log` | Append to agent memory logs |
| `nlr_session_context` | Current session metadata |
| `nlr_trace_read` | Read a session trace |

### tv_* (public via Caddy + ngrok)

| Tool | Purpose |
|---|---|
| `tv_read_note` | Read any markdown file in the vault |
| `tv_write_note` | Create/overwrite a markdown file |
| `tv_edit_note` | SEARCH/REPLACE edit with hash conflict detection |
| `tv_delete_note` | Delete with link tracking |
| `tv_move_note` | Rename; auto-updates wikilinks |
| `tv_search` | Tantivy BM25 across all notes (<100ms at 10k notes) |
| `tv_advanced_search` | BM25 with tag/frontmatter/path filters |
| `tv_search_by_frontmatter` | Find notes by fm key-value |
| `tv_recommend_related` | ML-powered related-note suggestions |
| `tv_query_metadata` | Frontmatter pattern queries |
| `tv_query_frontmatter_sql` | GlueSQL over frontmatter (needs `sql` feature) |
| `tv_get_backlinks` | Inbound wikilinks to a note |
| `tv_get_forward_links` | Outbound wikilinks from a note |
| `tv_get_related_notes` | Multi-hop graph traversal on wikilinks |
| `tv_get_hub_notes` | Top connected notes |
| `tv_get_dead_end_notes` | Notes with inbound but no outbound |
| `tv_get_isolated_clusters` | Disconnected subgraphs |
| `tv_quick_health_check` | 0-100 vault health score (<100ms) |
| `tv_full_health_analysis` | Comprehensive audit |
| `tv_get_broken_links` | All dangling wikilinks |
| `tv_detect_cycles` | Circular reference chains |
| `tv_batch_execute` | Atomic multi-file transactions with rollback |
| `tv_suggest_links` | AI-suggested new wikilinks |
| `tv_get_link_strength` | Connection strength (0.0–1.0) |
| `tv_get_centrality_ranking` | betweenness / closeness / eigenvector |
| ...plus `create_vault`, `add_vault`, templates, ofm helpers... |

## Write-through-schema examples

**Wrong** — bypasses schema enforcement:
```
tv_write_note(path="02-KB-main/math/new-topic.md", content="# ...")
```

**Right**:
```
nlr_wiki_create(
  topic="math/new-topic",
  title="New Topic",
  domain="math",
  sources=[...],
  confidence=0.7,
  body="# ..."
)
```

The `nlr_wiki_create` tool builds the frontmatter from structured args and
validates that required sections (Overview, Conceptual Model, etc.) are
present before writing.

## Ambiguous cases

**Q: Edit a typo in a wiki page body.**
A: Use `nlr_wiki_update` — schema fields don't change but you still want the
append-to-log behavior, which only nlr tools do.

**Q: Move a wiki page from `02-KB-main/math/old.md` to `02-KB-main/math/new.md`.**
A: Use `tv_move_note` (it updates backlinks automatically) and then
`nlr_wiki_update` to bump `last_updated`.

**Q: Batch-update 20 pages to add a new required frontmatter field.**
A: `tv_batch_execute` with the edits, then run the schema validator
(`nlr_wiki_list --validate`) after to confirm no pages were corrupted.
