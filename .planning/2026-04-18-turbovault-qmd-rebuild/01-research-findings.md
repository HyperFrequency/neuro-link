---
title: Research Findings (Agents 1–3)
status: complete
created: 2026-04-18
---

# Research Findings

## Agent 1 — qmd + Octen integration

### Critical correction to the PRD

The Rust server **already uses Octen-Embedding-8B at 4096 dim**. `server/src/embed.rs:14–47,86–108` calls `http://localhost:8400/v1/embeddings` (llama-server). The `math_symbols` Qdrant collection (`scripts/math_ingest/qdrant_index.py:21–22`) and the `nlr_wiki` collection are both already 4096-dim/cosine. **No Qdrant re-index is required.** The "swap to Octen" narrative only applies to qmd's own internal SQLite/in-memory vector store, which is new.

### Model choice

- f16 works in node-llama-cpp but **recommend Q8_0** (~8 GB) — near-lossless for retrieval, fits unified memory budget.
- Available quants at `mradermacher/Octen-Embedding-8B-GGUF`: Q2_K through f16.
- All-resident budget (Octen + Qwen3-Reranker-0.6B + Qwen3-expansion-1.7B + KV cache):
  - f16 ≈ 20–22 GB (tight on 32 GB Mac)
  - Q8_0 ≈ 12–13 GB ← **choice**
  - Q4_K_M ≈ 8–9 GB (fallback for 16 GB Macs)

### qmd configuration (no fork)

Env var at `src/llm.ts:504–506` supports overrides:
```
export QMD_EMBED_MODEL=hf:mradermacher/Octen-Embedding-8B-GGUF/Octen-Embedding-8B.Q8_0.gguf
# or local path:
export QMD_EMBED_MODEL=/Users/DanBot/Desktop/HyperFrequency/neuro-link/models/Octen-Embedding-8B.Q8_0.gguf
```

### Mandatory source patch

qmd's embedding-format logic (`src/llm.ts:29–58`) has a Qwen3-embed branch gated by regex `/qwen.*embed/i` or `/embed.*qwen/i`. Octen (fine-tune of Qwen3-Embedding-8B) won't match → falls through to embeddinggemma format → bad embeddings. Patch:

1. Widen the regex at L29: `/\b(qwen.*embed|embed.*qwen|octen)\b/i`
2. In `formatDocumentForEmbedding`: prepend `"- "` for Octen (per Octen README "Known Issues").
3. Queries use Qwen3 instruct format: `Instruct: Given a web search query...\nQuery: {q}`.

Submit upstream PR to `tobi/qmd`; fork in the meantime.

### Hybrid fusion decision

**Keep qmd as a sidecar**, not a replacement. Existing Rust server has 2-way and 4-way RRF at `server/src/tools/rag.rs:154,183+` (BM25 + Qdrant/Octen + Context7 + Auggie, k=60). It's wired into `nlr_rag_query`, `nlr_rag_query_verified`, and the grading pipeline. Replacing it would break all three.

Add qmd as the **5th RRF input** via `qmd search --json ...` shelled out from the Rust server. qmd's strength (query expansion + cross-encoder rerank) becomes an additional signal, not a replacement.

### Action checklist

1. `huggingface-cli download mradermacher/Octen-Embedding-8B-GGUF Octen-Embedding-8B.Q8_0.gguf`
2. `export QMD_EMBED_MODEL=hf:mradermacher/Octen-Embedding-8B-GGUF/Octen-Embedding-8B.Q8_0.gguf`
3. Fork `tobi/qmd`; patch `src/llm.ts:29` regex + doc-prefix; open upstream PR.
4. `qmd collection add /path/to/02-KB-main --name kb && qmd embed`
5. Add qmd as 5th input to `rrf_merge` in `server/src/tools/rag.rs`.

---

## Agent 2 — TurboVault + Obsidian plugin

### Version / maintenance status

- **turbovault 1.4.0** on crates.io, published **2026-04-09** (9 days ago — actively maintained).
- All 8 sub-crates synchronized at 1.4.0: `turbovault-{core,parser,graph,vault,tools,sql,batch,export}`.
- Also a `turbovault-audit` crate in-repo (may be published).
- MSRV: Rust 1.90+.

### Existing Obsidian plugin — DO NOT REBUILD

`/Users/DanBot/Desktop/HyperFrequency/neuro-link/obsidian-plugin/` already exists. v0.1.0, id `neuro-link-recursive`, built ~2026-04-16. 8 files, ~108 KB of TypeScript:

- `main.ts`, `mcp-setup.ts`, `harness-setup.ts`, `api-router.ts`, `chatbot.ts`, `commands.ts`, `stats.ts`, `settings.ts`
- Already ships an **Ngrok tunnel setup wizard** ("NLR: Start Ngrok Bridge")
- Already implements **bearer-token auth** via `Authorization: Bearer ${token}` sourced from `secrets/.env` (`mcp-setup.ts:161–182`)
- Talks to Rust server via `nlr` CLI (not direct HTTP)

TurboVault ships **no** Obsidian plugin. Extend the existing one — add TurboVault tool registrations alongside `nlr_*` tools. Don't rebuild.

### ngrok exposure

- TurboVault default transport: **STDIO**. Must install with `--features http`:
  ```
  cargo install turbovault --features http
  turbovault --transport http --port 3000 --profile production
  ```
- Other options: `--features websocket`, `--features full`.
- Bind behavior: `127.0.0.1:{port}` by default (main.rs:298,310,322).

### ⚠️ HIGH RISK — TurboVault has zero auth

**TurboVault's HTTP/WS/TCP transports have no bearer token, no API key, no CORS, no TLS.** Tunneling straight through ngrok gives the public internet full read/write/delete access: `write_note`, `delete_note`, `batch_execute`, `move_note`.

**Required mitigation** (pick one, do not skip):

1. ngrok's built-in OAuth or basic-auth on the tunnel edge (simplest)
2. Caddy/nginx reverse proxy in front with bearer-token check (reuse `mcp-setup.ts:161–182` pattern)
3. Cloudflare Tunnel with Access policy

Recommendation: option 2. Proxy lives on localhost, checks bearer against the same `secrets/.env` token the Obsidian plugin already uses, forwards to TurboVault on 127.0.0.1:3000. ngrok points at the proxy, not TurboVault.

### Tool namespace strategy

**Semantic overlaps with existing `nlr_wiki_*`:**

| TurboVault | NLR equivalent | Decision |
|---|---|---|
| `read_note` | `nlr_wiki_read` | Keep both; TV is generic, NLR is schema-enforced |
| `write_note` | `nlr_wiki_create` | NLR canonical for `02-KB-main/` (preserves schema); TV for generic vault writes |
| `edit_note` | `nlr_wiki_update` | TV is *better* (SEARCH/REPLACE + hash conflict detection) — consider migrating NLR to use TV primitives underneath |
| `search` / `advanced_search` | `nlr_wiki_search` | TV is *better* (Tantivy BM25) — NLR search should route to TV |
| list ops | `nlr_wiki_list` | Partial overlap, both keep |

**Namespace TurboVault tools as `tv_*`** in the MCP surface so both coexist. `nlr_wiki_*` stays the canonical write path to preserve frontmatter schema enforcement.

### Top 5 genuinely-new capabilities

1. `get_backlinks` / `get_forward_links` — wikilink graph queries (NLR has no equivalent; `nlr_graph_traverse` is the reasoning ontology, not vault links)
2. `get_hub_notes` / `get_centrality_ranking` — petgraph-based centrality on the actual vault
3. `quick_health_check` / `full_health_analysis` / `get_broken_links` — vault integrity
4. `batch_execute` — atomic multi-file transactions with rollback (NLR has no transactional write path)
5. `query_frontmatter_sql` (requires `sql` feature) — GlueSQL over YAML frontmatter; replaces grep hacks on `confidence`/`domain`/`last_updated`

Honorable mentions: `suggest_links`, `detect_cycles`, `get_isolated_clusters`, `move_note` (auto-updates wikilinks).

---

## Agent 3 — LaTeX deep-ingest tool stack

### PDF → LaTeX+Markdown extractor comparison (April 2026)

| Tool | Math quality | macOS M-series | Maintenance | License |
|---|---|---|---|---|
| **Marker v1.10+** (`datalab-to/marker`) | Saturates olmOCR-Bench (95.67 heuristic); `--use_llm` hybrid handles inline math | **Native MPS** via Surya (PyTorch) | **Very active**, ~monthly releases | GPL v3 (free for personal/research) |
| **MinerU 2.5** (`opendatalab/MinerU`) | 86.2 on OmniDocBench v1.5; dedicated MFD/MFR models output LaTeX | **Native MLX backend** (`vlm-mlx-engine`), 2–3× faster than MPS-transformers | **Very active** | Apache-2.0 / AGPL-3.0 dual |
| **olmOCR 2** (Allen AI) | ~82.4 olmOCR-Bench; SOTA on handwritten/historical | Painful on Apple Silicon — vLLM+CUDA designed; community Metal plugins fragile | Very active | Apache-2.0 |
| **Chandra 2** (`datalab-to/chandra`) | Released March 2026; advertised math/table improvements over Marker | MPS via same stack | Active | GPL family |
| **Nougat** (Meta) | Pioneered the category; now exceeded by all above | Works on MPS | **Abandoned** (no commits since late 2023) | MIT |
| **LaTeXML / ar5ivist** | Gold standard *if* you have `.tex` source — not a PDF tool | `brew install latexml` or Docker | Active | Public domain |
| pix2tex, Grobid | Equation-only / structure-only — not full-doc contenders | — | — | — |

**CUDA-only flag**: olmOCR 2 in its intended fast-path config. Use Marker + MinerU on M-series.

### LaTeX canonicalization — keep your existing design

No mature open-source tool normalizes notational variants (`\R` vs `\mathbb{R}`, `\partial_t` vs `\frac{\partial}{\partial t}`) out of the box. Your old pipeline's **pylatexenc + latex2sympy2_extended → SymPy `srepr`** approach is already near-SOTA. Improvement: **front-load a richer macro-expansion table** (collapse `\R`→`\mathbb{R}`, expand differential shorthands, common physics notation) before the `srepr` normalization step.

### Math-book corpora — legal bulk sources

- **arXiv S3 bulk buckets** — ~2.9 TB of LaTeX source, `requester-pays`. Use `export.arxiv.org` for fresh mirroring. **Primary corpus.**
- **ar5iv 04.2024 dataset** (SIGMathLing/KWARC) — pre-rendered HTML5+MathML of entire arXiv. License Agreement form required; free for research. **Skips PDF extraction entirely** for anything arXiv-hosted.
- **Stacks Project** — official **JSON API** + public GitHub repo (LaTeX source). GFDL. No scraping needed.
- **nLab** — CC-BY-SA, dumps available on request; scraping tolerated at slow rates.
- **Wikipedia math portal** — official bulk dumps, CC-BY-SA.
- **Springer open books** (`ahmadassaf/springer-free-books`) — curated list, per-URL PDFs (no bulk API). SpringerOpen proper is CC-BY.
- **AIM Open Textbook Initiative** — curated list, individual CC licenses, no API.

**Avoid**: MathWorld (Wolfram TOS forbids bulk scraping — old pipeline was borderline, drop it); PlanetMath (unclear stewardship).

### Recommended stack (5 pieces)

1. **Marker v1.10+** — primary PDF→Markdown with inline math, native MPS, saturates olmOCR-Bench.
2. **MinerU 2.5 with MLX backend** — second opinion on math-dense pages; clean LaTeX where Marker struggles.
3. **arXiv S3 + ar5iv HTML dataset** — primary source at scale; ar5iv skips PDF extraction for arXiv content.
4. **pylatexenc + latex2sympy2_extended + SymPy srepr** — canonicalization pipeline, richer macro table.
5. **Stacks Project JSON API + nLab dumps + Wikipedia math dump** — non-arXiv book-length corpus, all legally bulk-accessible.

**Skip**: Nougat (abandoned), olmOCR (CUDA-first). Revisit olmOCR only if ingestion moves to the GH200 box.

### Action checklist

1. `pip install marker-pdf mineru` (both M-series friendly).
2. Request [ar5iv 04.2024 dataset](https://sigmathling.kwarc.info/resources/ar5iv-dataset-2024/) access via SIGMathLing form (free, ~1 business day).
3. Set up arXiv S3 `requester-pays` access (AWS CLI, cost ~$0.09/GB on egress).
4. Clone `stacks/stacks-project` + consume the JSON API for structured chapter retrieval.
5. Replace `scripts/math_ingest/mathworld_crawl.py` with arXiv + ar5iv + Stacks ingestion scripts.
6. Keep `scripts/math_ingest/adapter.py` canonicalization logic; extend the pylatexenc macro table.

### Sources

- [Marker repo](https://github.com/datalab-to/marker)
- [Datalab: Saturating the olmOCR Benchmark](https://www.datalab.to/blog/saturating-the-olmocr-benchmark)
- [MinerU docs](https://opendatalab.github.io/MinerU/) · [MinerU GitHub](https://github.com/opendatalab/MinerU)
- [olmOCR paper](https://olmocr.allenai.org/papers/olmocr.pdf)
- [Benchmarking Document Parsers on Math Formula Extraction (arXiv 2512.09874)](https://arxiv.org/html/2512.09874v1)
- [OmniDocBench CVPR 2025](https://openaccess.thecvf.com/content/CVPR2025/papers/Ouyang_OmniDocBench_Benchmarking_Diverse_PDF_Document_Parsing_with_Comprehensive_Annotations_CVPR_2025_paper.pdf)
- [LaTeXML / ar5ivist](https://github.com/dginev/ar5ivist)
- [ar5iv dataset 04.2024](https://sigmathling.kwarc.info/resources/ar5iv-dataset-2024/)
- [arXiv Bulk Data (S3)](https://info.arxiv.org/help/bulk_data_s3.html)
- [Stacks Project API](https://stacks.math.columbia.edu/api)

---

## Consolidated impact summary

### Scope reductions vs original PRD

| Original PRD line | Reality after research | Net effect |
|---|---|---|
| "Throw out current embeddings, use Octen" | Octen is already the server embedder; Qdrant collections are already 4096-dim | **-2 days** (no re-index) |
| "Install Obsidian plugin" | Plugin already exists with ngrok wizard + bearer auth | **-3 days** (extend, not build) |
| "qmd replaces existing RAG" | qmd slots in as 5th RRF input; Rust RRF stays | **-1 day** (no rewrite) |
| "Math RAG was garbage, find meta-harness tools" | "Meta-harness" is an arXiv paper *category*, not a tool. Real SOTA is Marker + MinerU + arXiv/ar5iv bulk | Clarity only |

### Scope additions vs original PRD

| Net-new work | Cost |
|---|---|
| ngrok auth proxy (TurboVault has zero auth) | ~0.5 day (Caddy + bearer reuse) |
| Fork/patch `tobi/qmd` for Octen prompt format | ~0.5 day + upstream PR |
| Replace MathWorld crawler with arXiv/ar5iv/Stacks ingesters | ~2 days |
| Marker + MinerU deep-ingest wiring | ~2 days |
| Deletion/archive of 47 shallow math stubs | 1 hour (HITL-gated) |

### Revised total estimate

Original gut-feel: ~3 weeks. Revised: **~1.5 weeks** of focused work, driven mostly by the discovery that Octen and the Obsidian plugin are already in place.

