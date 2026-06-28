---
title: TurboVault + qmd + Octen Rebuild — Master PRD
status: draft
created: 2026-04-18
owner: Dan Repaci
---

# TurboVault + qmd + Octen Rebuild

## Goal

Rebuild neuro-link-recursive's vault-interaction, retrieval, and RAG-injection
layers around three decoupled best-of-breed components, and expose the vault
publicly via ngrok so external LLM harnesses can query it through MCP.

- **Vault interface**: TurboVault (Rust SDK + MCP server, OFM-aware, link graph)
- **Retrieval**: qmd (local hybrid BM25 + dense + rerank + query expansion)
- **Embedder**: existing Octen-Embedding-8B (4096-dim) — replace qmd's default
- **Public API**: TurboVault MCP server behind ngrok
- **RAG**: new `/auto-rag` skill + UserPromptSubmit hook that routes between
  qmd (vault content) and `/docs-dual-lookup` (external library docs)

## Out of scope for v1

- Live multi-vault switching (single vault for now)
- Non-Claude harness integration beyond the existing MCP envelope
- Migration of code-indexing to the new stack (Phase 2+ item)

## Non-negotiable decisions (locked)

1. TurboVault is the public MCP face exposed via ngrok; neuro-link-recursive's
   Rust server is internal.
2. qmd keeps its own Qwen3-1.7B query expansion + Qwen3-Reranker-0.6B; only
   the embedder is swapped to Octen-Embedding-8B (4096-dim).
3. All current embeddings are thrown out. Vault markdown content in
   `02-KB-main/math/` produced by the old shallow `math_ingest/` pipeline is
   also thrown out — the new pipeline targets deep/full-document ingestion
   (research pending on exact tool stack; see `01-research-findings.md`).
4. Existing `nlr_rag_*` MCP tools are replaced, not extended. Old skills
   (`neuro-link`, `neuro-link-setup`, `neuro-scan`, `neuro-surgery`,
   `hyper-sleep`, `self-improve-hitl`, `self-improve-recursive`,
   `reasoning-ontology`, `auto-rag`, `harness-bridge`, `job-scanner`,
   `knowledge-gap`) are deleted and regenerated via `/skill-creator` against
   the new PRD docs in `00-neuro-link/`.
5. The math subsystem (`nlr_math_lookup` 3-way RRF: FTS + dense +
   canonical-srepr) is preserved structurally but re-implemented over qmd.
   The `math_symbols` Qdrant collection stays at 4096-dim (matches Octen).

## Stack map

```
                    ┌──────────────────────┐
                    │   ngrok public URL   │
                    └──────────┬───────────┘
                               │ MCP over HTTP
                    ┌──────────▼───────────┐
                    │   TurboVault MCP     │  <-- 47 tools, OFM parser,
                    │   (Rust crate)       │      link graph, BM25, SQL FM
                    └──────┬───────────────┘
                           │ embeds & delegates
              ┌────────────┼────────────────────┐
              ▼            ▼                    ▼
         ┌─────────┐  ┌─────────┐        ┌─────────────┐
         │ qmd CLI │  │ Qdrant  │        │ neuro-link  │
         │ (hybrid │  │ (dense, │        │ Rust server │
         │ search, │  │  4096d, │        │ (internal,  │
         │ rerank, │  │ Octen)  │        │ orchestr.)  │
         │ expand) │  └─────────┘        └─────────────┘
         └────┬────┘
              │ shells out to
              ▼
         ┌─────────────────┐
         │ node-llama-cpp  │ ← Octen-Embedding-8B GGUF
         │                 │ ← Qwen3-Reranker-0.6B GGUF
         │                 │ ← Qwen3 query-exp 1.7B GGUF
         └─────────────────┘
```

## New vault file structure (migration)

See `02-file-structure-migration.md` for the old → new path mapping. Summary:

| New path                          | Purpose                              |
| --------------------------------- | ------------------------------------ |
| `00-neuro-link/`                  | Default LLM instruction `.md` files  |
| `01-raw/`                         | Ingested sources, untouched (SHA256) |
| `01-sorted/`                      | Classified sources                   |
| `02-KB-main/<topic>/`             | Canonical wiki pages                 |
| `03-Ontology-main/{workflow,agents}/` | Neo4j-backed ontology graphs    |
| `04-Agent-Memory/`                | Agent logs, consolidated memory      |
| `05-insights-HITL/`               | Human-in-the-loop review stream      |
| `06-Recursive/`                   | Recursive self-improvement reports   |
| `07-self-improvement-HITL/`       | HITL-approved changes                |
| `08-code-docs/{my-repos,toolbox,forked-up}/` | Code wikis                |

## Default `.md` files in `00-neuro-link/` (to be generated)

Each becomes a `/skill-creator`-produced skill under `.claude/skills/`:

- `neuro-link-setup.md`
- `neuro-link.md` (main orchestrator, uses TurboVault commands)
- `recursive-self-improvement.md`
- `neuro-scan.md`
- `neuro-surgery.md`
- `hyper-sleep.md`
- `crawl-ingest-update.md`
- `main-codebase-tools.md`
- `adjacent-tools-code-docs.md`
- `forked-repos-with-changes.md`

## Phased execution plan

- **Phase 0** (now): PRD + migration plan written, research agents dispatched.
  No code changes, no deletions.
- **Phase 1**: Research findings consolidated (math ingest tools, TurboVault
  integration surface, qmd+Octen config). HITL gate before Phase 2.
- **Phase 2**: Vault restructure (renames + content quarantine), `.claude/`
  directory with skill/hook/agent declarations, `@neuro` Claude Code subagent.
- **Phase 3**: **HITL gate** — explicit user confirm before any deletion of
  old skills, old embeddings, old `02-KB-main/math/` content.
- **Phase 4**: TurboVault crate wired into Rust server (replaces BM25 +
  wiki CRUD). Obsidian plugin audit.
- **Phase 5**: qmd installed, configured for Octen, re-index over new vault.
- **Phase 6**: Math subsystem (`nlr_math_lookup`) ported to qmd. Deep-ingest
  pipeline per Phase-1 research recommendation.
- **Phase 7**: New `/auto-rag` skill + UserPromptSubmit hook with qmd /
  docs-dual-lookup router.
- **Phase 8**: ngrok exposure of TurboVault MCP with auth.
- **Phase 9**: Install package (brew/cargo/npm combo), Obsidian plugin release.

## Open questions (unblocking Phase 2)

1. Router classifier for `/auto-rag`: LLM call per prompt vs keyword heuristics?
   (default: heuristics first, LLM fallback if ambiguous)
2. Old content quarantine strategy: `git rm` or move to `.archive/YYYY-MM-DD/`?
   (default: archive, easier rollback)
3. `@neuro` subagent: does it have its own system prompt or inherit from the
   `neuro-link` skill?
4. Obsidian plugin: merge with existing `obsidian-plugin/` or fresh build?
   Depends on research agent 2's findings.

## Deliverable checklist

- [ ] `00-PRD.md` (this file)
- [ ] `01-research-findings.md` (consolidated agent output)
- [ ] `02-file-structure-migration.md` (old → new map + commands)
- [ ] `03-skill-generation-queue.md` (each old skill → replacement spec)
- [ ] `04-mcp-tool-surface.md` (what TurboVault exposes, what's internal)
- [ ] `05-rag-router-design.md` (qmd vs docs-dual-lookup routing logic)
- [ ] `06-ngrok-mcp-auth.md` (auth strategy for public exposure)
- [ ] `07-deletion-manifest.md` (exact list of files to remove, HITL-gated)
