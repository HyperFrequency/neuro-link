---
title: neuro-link-recursive — llm-wiki navigation
tool: neuro-link-recursive
upstream: HyperFrequency/neuro-link (original)
canonical_wiki: ../../../docs/deep-tool-wiki/neuro-link-recursive/wiki.md
last_updated: 2026-04-20
---

# neuro-link-recursive — llm-wiki

Tree-navigable offline context for the neuro-link-recursive system
itself — the infrastructure that hosts the rest of this wiki.

## Sitemap

```
neuro-link-recursive/
├── Vault/
│   ├── [[Vault/00-neuro-link]]           — default LLM instruction specs + task queue
│   ├── [[Vault/01-raw]]                  — immutable SHA256-named source ingests
│   ├── [[Vault/01-sorted]]               — classified symlinks into 01-raw/
│   ├── [[Vault/02-KB-main]]              — llm-wiki (this vault's main artifact)
│   ├── [[Vault/03-Ontology-main]]        — workflow + agent ontologies
│   ├── [[Vault/04-Agent-Memory]]         — logs, consolidated memory, perf grades
│   ├── [[Vault/05-insights-HITL]]        — human-in-the-loop review queue
│   ├── [[Vault/06-Recursive]]            — recursive self-improvement reports
│   ├── [[Vault/07-self-improvement-HITL]] — approved improvement proposals
│   └── [[Vault/08-code-docs]]            — my-repos / toolbox / forked-up
├── Retrieval/
│   ├── [[Retrieval/qmd]]                 — BM25 + vector + rerank + query-expand orchestrator
│   ├── [[Retrieval/RRF]]                 — Reciprocal Rank Fusion of BM25 and dense scores
│   ├── [[Retrieval/nlr_rag_query]]       — raw MCP endpoint
│   └── [[Retrieval/nlr_rag_query_verified]] — confidence-gated wrapper (≥0.6 + cited)
├── Embeddings/
│   ├── [[Embeddings/Octen-8B]]           — Octen-Embedding-8B f16, 4096-dim (server-side)
│   ├── [[Embeddings/llama-server]]       — llama.cpp server on :8400 hosts Octen
│   └── [[Embeddings/Qdrant-collections]] — nlr_wiki / math_symbols / tool_refgraph
├── Reranking/
│   └── [[Reranking/Qwen3-0.6B]]          — Qwen3-Reranker-0.6B Q8_0 (qmd cache)
├── QueryExpansion/
│   └── [[QueryExpansion/Qwen3-1.7B]]     — qmd-query-expansion-1.7B Q4_K_M
├── VectorDB/
│   └── [[VectorDB/Qdrant]]               — localhost:6333; 2+ collections, cosine
├── GraphDB/
│   ├── [[GraphDB/Neo4j]]                 — localhost:7474/7687; reasoning ontologies + RefGraphs
│   └── [[GraphDB/Schema]]                — node labels + edge types
├── MCP/
│   ├── [[MCP/neuro-link-recursive-stdio]] — /usr/local/bin/neuro-link mcp
│   ├── [[MCP/neuro-link-http]]           — TurboVault HTTP via Caddy + ngrok + bearer
│   └── [[MCP/tool-namespaces]]           — nlr_* (internal, schema-enforced) vs tv_* (public)
├── Server/
│   ├── [[Server/rust-binary]]            — server/target/release/neuro-link
│   ├── [[Server/5-way-RRF]]              — internal orchestrator blending BM25 + dense + reranker
│   └── [[Server/bm25-rs]]                — server/src/bm25.rs sparse retrieval
├── Hooks/
│   ├── [[Hooks/auto-rag-inject]]         — UserPromptSubmit hook routing qmd vs /docs-dual-lookup
│   └── [[Hooks/neuro-grade]]             — PostToolUse hook appending to logs.md
├── LSPs/
│   ├── [[LSPs/pinelsp]]                  — Pine Script v6 LSP fork + tree-sitter-pine WASM
│   └── [[LSPs/serena-pine-mod]]          — Serena monkey-patch enabling Pine
├── Install/
│   ├── [[Install/install.sh]]            — 15+ step installer
│   ├── [[Install/download_models.sh]]    — 3 GGUF models via huggingface-cli
│   ├── [[Install/status.sh]]             — health probe
│   └── [[Install/nlr-extension]]         — pinelsp + Serena Pine mod patch
└── Operations/
    ├── [[Operations/state-heartbeat]]    — state/heartbeat.json last-known-good
    ├── [[Operations/task-queue]]         — 00-neuro-link/tasks/*.md job specs
    ├── [[Operations/logs]]               — 04-Agent-Memory/logs.md append-only grade log
    └── [[Operations/secrets]]            — secrets/.env gitignored
```

→ **Canonical wiki body:** `hyperfrequency/docs/deep-tool-wiki/neuro-link-recursive/wiki.md`
  (NOTE: this wiki page is not yet written — the current page is this
  llm-wiki. The canonical page will be built by the ingest pipeline from
  this scaffold + the neuro-link repo README + code documentation.)

## Pitfalls

See `[[pitfalls]]` for the full list. Top recurring ones:

- **Stale `~/.claude/state/nlr_root` alias.** When the pointer file
  targets a Finder Alias (not a real dir), every `PostToolUse` hook fails
  and tool-call logs never flow. Use absolute real paths only.
- **Broken `/usr/local/bin/neuro-link` symlink.** The Rust binary moved
  from `Desktop/HyperFrequency/...` to `Dev/neuro-link/...` during a
  repo rename; the symlink must be re-pointed for the stdio MCP to work.
- **QMD models missing from disk.** `download_models.sh` is idempotent
  and HF-CLI-based, but if it never ran, reranker + query-expansion
  aren't available and qmd silently falls back to BM25-only.
- **Octen filename quant mismatch.** `install.sh` hardcodes
  `Octen-Embedding-8B.f16.gguf`; manual placement of `f16.gguf` makes
  the guard check fail and the embedding path stays dead.
- **Empty Qdrant collections.** `install.sh` creates the containers +
  collections but there's no built-in initial-ingestion step. After
  install, run a one-shot pass that reads `02-KB-main/` and writes
  points, otherwise RAG returns empty.

## Status

- [x] `index.md` (this page) — 2026-04-20
- [x] `overview.md` — populated
- [x] `pitfalls.md` — populated (via this document's Pitfalls section; deeper consolidation deferred)
- [ ] Subsystem leaf pages (`Retrieval/qmd.md`, `Embeddings/Octen-8B.md`, etc.) — pending
- [ ] Canonical wiki body (`docs/deep-tool-wiki/neuro-link-recursive/wiki.md`) — pending full ingest

## See also

- Canonical install: `/Users/DanBot/Dev/neuro-link/install.sh`
- Extension patch: `/Users/DanBot/hyperfrequency/neuro-link/dev/nlr-extension/`
- Server source: `/Users/DanBot/Dev/neuro-link/server/`
- TurboVault MCP (public): `https://petal-privacy-disrupt.ngrok-free.dev/mcp`
- Global project CLAUDE.md: `/Users/DanBot/Dev/neuro-link/CLAUDE.md`
