---
skill_name: crawl-ingest-update
trigger: /crawl-ingest-update <source>, /crawl-ingest <source>
invokes: [parallel-web, markitdown, pdf, wiki-curate]
LLM_assisted: Y
---

# /crawl-ingest-update

Source ingestion pipeline. Deposits raw content into `01-raw/` (SHA256-deduped),
classifies into `01-sorted/`, and optionally synthesizes into `02-KB-main/`.

## Supported source types

- Web URLs (single page) — via Parallel API / WebFetch
- Web domains (crawl) — via Parallel Crawl API
- GitHub repos — clone + README + docs
- YouTube videos — transcript via yt-dlp + whisper fallback
- PDFs (standard) — via `/pdf` skill + `markitdown`
- **PDFs (math-dense)** — via **Marker v1.10+** with `--use_llm` + MinerU 2.5
  MLX backend as second opinion
- arXiv papers — prefer **ar5iv HTML+MathML** dataset; fall back to
  arXiv S3 LaTeX source; last resort Marker on the PDF
- Obsidian vaults (external) — folder import

## Deep-ingest stack for math sources

| Source type                | Tool chain                                                 |
| -------------------------- | ---------------------------------------------------------- |
| arXiv paper (any domain)   | ar5iv HTML → markdown; fallback Marker on PDF              |
| Math book (PDF, dense)     | Marker `--use_llm` + MinerU 2.5 (MLX) cross-check          |
| Stacks Project chapter     | Official JSON API — no scraping                            |
| nLab page                  | Official dump → markdown                                   |
| Wikipedia math article     | Official bulk dump → markdown                              |
| MathWorld                  | **BLOCKED** — Wolfram TOS forbids bulk scraping            |

## LaTeX canonicalization (preserved from v1)

All `$$...$$` blocks preserved byte-for-byte. Per-equation canonical `srepr`
computed via pylatexenc → latex2sympy2_extended → SymPy. Expanded macro table
collapses notational variants (`\R`→`\mathbb{R}`, differential shorthands).

## LLM-assisted source discovery

When `LLM_assisted=Y` (default): before crawling, LLM proposes additional
authoritative sources given the topic. User approves or edits the list.

## Output paths

- Raw content: `01-raw/<sha256>-<slug>.<ext>` (never mutated)
- Sorted: symlink from `01-sorted/<domain>/<slug>.<ext>` → `01-raw/`
- Synthesized (if auto_curate=Y): `02-KB-main/<topic>/<slug>.md`
