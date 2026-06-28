---
name: crawl-ingest-update
description: Source ingestion pipeline that deposits raw content into 01-raw/ (SHA256-deduped), classifies into 01-sorted/, and optionally synthesizes wiki pages to 02-KB-main/. Use this whenever the user says /crawl-ingest-update, /crawl-ingest, asks to ingest a paper/book/repo/video/URL/vault, or drops a link expecting it to be added to the brain. Also trigger when the user says "grab this for the brain", "add this source", "read this paper into the KB", or forwards a PDF/URL expecting it indexed. Supports web pages, web crawls, GitHub repos, YouTube transcripts, standard PDFs, math-dense PDFs (via Marker v1.10+ + MinerU 2.5 MLX cross-check), arXiv papers (prefers ar5iv HTML → falls back to arXiv S3 LaTeX → last resort Marker on the PDF), math books, Stacks Project chapters (JSON API), nLab dumps, Wikipedia math bulk dumps, and Obsidian vault imports. MathWorld scraping is blocked (Wolfram TOS). Always preserves $$...$$ blocks byte-for-byte; canonicalizes equations via pylatexenc + latex2sympy2_extended + SymPy srepr.
---

# /crawl-ingest-update

Deep-ingest pipeline. Route by source type, never hand-roll PDF parsing, preserve math byte-for-byte.

## Source routing table

Match the source and use the matching pipeline. If unclear, ask the user before running any network I/O.

| Source shape                                    | Pipeline                                                    |
| ----------------------------------------------- | ----------------------------------------------------------- |
| `https://arxiv.org/abs/*` or `arxiv:2512.09874` | **ar5iv HTML** → markdown (see `references/arxiv-ar5iv.md`) |
| arXiv PDF not in ar5iv                          | arXiv S3 LaTeX source → LaTeXML → markdown                  |
| Math book PDF (dense)                           | **Marker v1.10+** `--use_llm` + **MinerU 2.5** (MLX backend) cross-check |
| Standard PDF                                    | `/pdf` skill or `markitdown`                                |
| Web page                                        | Parallel API (`parallel-web`) or `WebFetch`                 |
| Web domain (crawl)                              | Parallel Crawl                                              |
| GitHub repo                                     | `gh repo clone` + `README.md` + `docs/`                     |
| YouTube video URL                               | `yt-dlp` transcript; if unavailable, `whisper` local        |
| Stacks Project chapter                          | Official JSON API (`https://stacks.math.columbia.edu/api/tag/<tag>`) |
| nLab page                                       | Official dump (requested once; no scraping)                 |
| Wikipedia math article                          | Wikipedia bulk dump (pre-downloaded, queried locally)       |
| Obsidian vault directory                        | Recursive import + frontmatter normalization                |
| MathWorld                                       | **BLOCKED** — Wolfram TOS forbids bulk scraping             |

## Output layout

Every ingest writes three artifacts:

1. **Raw** — `01-raw/<sha256>-<slug>.<ext>` (immutable; never modified)
2. **Sorted** — `01-sorted/<domain>/<slug>.<ext>` (symlink into 01-raw)
3. **Synthesis** — if `auto_curate: Y` (default), `02-KB-main/<topic>/<slug>.md` via `/wiki-curate`

The *why* for SHA256-prefixed raw filenames: the content hash is the source of truth. Two ingests of the same PDF from different URLs dedupe by hash, saving storage and preventing "two wiki pages from the same doc" bugs.

## Math-dense PDF handling

This is the pipeline the old `math_ingest/` tried to be and failed. Do it right this time.

### Step 1 — Run Marker v1.10+ with hybrid LLM mode

```bash
marker_single <input.pdf> \
  --use_llm \
  --output_format markdown \
  --output <01-raw>/marker/<sha256>/
```

Marker on M-series uses PyTorch MPS natively. `--use_llm` routes borderline math blocks through an LLM for inline formula cleanup — this is where it beats Nougat.

### Step 2 — Run MinerU 2.5 on the same PDF

```bash
mineru \
  --backend vlm-mlx \
  --input <input.pdf> \
  --output <01-raw>/mineru/<sha256>/
```

MinerU's `vlm-mlx` backend is native Apple Silicon — 2–3× faster than MPS PyTorch. MinerU's MFR (Math Formula Recognition) model outputs LaTeX cleaner than Marker in some cases.

### Step 3 — Cross-check

Run `scripts/math_crosscheck.py <marker.md> <mineru.md>` which:

- Extracts every `$$...$$` block from both
- Pairs them by position
- Flags mismatches (different LaTeX at same position)
- Produces a merged output, preferring the version with valid `srepr` parse

The merged markdown goes to `01-raw/<sha256>-<slug>.md`.

### Step 4 — LaTeX canonicalization

```python
from scripts.canonicalize import canonicalize_equations
canonicalize_equations('01-raw/<sha256>-<slug>.md')
```

The canonicalizer:

- Preserves every `$$...$$` block byte-for-byte (never rewrites display math)
- Runs each equation through `pylatexenc` → `latex2sympy2_extended` → `SymPy.srepr`
- Stores the canonical `srepr` in sidecar `.meta.json` (not inline — keeps the markdown clean)
- Uses the macro expansion table in `references/latex-macros.md` to collapse notational variants (`\R` → `\mathbb{R}`, `\partial_t` → `\frac{\partial}{\partial t}`, etc.)

Failed parses yield `canonical_srepr: null` — never an exception. Keep the equation as-is.

See `references/latex-canonicalization.md` for the full canonicalization contract and `references/marker-pipeline.md` / `references/mineru-pipeline.md` for each tool's configuration.

## arXiv fast path

For any arXiv paper:

1. Try ar5iv first (`https://ar5iv.labs.arxiv.org/html/<id>`). It serves HTML+MathML for the whole paper. No PDF extraction needed. This is 10× faster than Marker and more accurate on equations.
2. If ar5iv returns 404 (paper not in dataset), fall back to arXiv S3 bulk (`requester-pays`). Fetch the LaTeX source, run LaTeXML → markdown.
3. Last resort: Marker on the PDF from `arxiv.org/pdf/<id>.pdf`.

See `references/arxiv-ar5iv.md` for the ID parsing and fallback logic.

## Stacks Project + nLab + Wikipedia math

These have official bulk APIs or dumps. Never scrape them with Marker.

- **Stacks Project**: `https://stacks.math.columbia.edu/api/tag/<tag>` returns structured JSON with LaTeX content. Use `scripts/ingest_stacks.py`.
- **nLab**: request the dump once from the maintainers; store locally; query offline. `scripts/ingest_nlab.py` handles format.
- **Wikipedia math**: use the bulk dump (20-ish GB decompressed). `scripts/ingest_wikipedia_math.py` extracts articles from the math portal.

## LLM-assisted source discovery

When the user's prompt is vague (e.g., "ingest stuff about Brownian motion"), and `LLM_assisted: Y` (default):

1. Call an LLM with the topic + the list of allowed source types
2. LLM proposes 3–5 authoritative sources with URLs
3. Present proposals to the user; they approve the subset to ingest
4. Run the approved ingests

Don't skip this step even when the user seems to want a single source — authoritative cross-referencing is the whole point of having multiple sources in the brain.

## Dedup

Before starting any pipeline, compute SHA256 of the source. Check `01-raw/<sha256>-*`. If exists:

- Report to user: "already ingested on <date>, re-run? (y/N)"
- Default N — an already-ingested source doesn't need re-ingestion unless the user explicitly wants to force a re-sync (e.g., Marker version bumped).

## References

- `references/marker-pipeline.md` — Marker v1.10+ config, best-practices, known failure modes
- `references/mineru-pipeline.md` — MinerU 2.5 MLX setup and tuning
- `references/arxiv-ar5iv.md` — ID parsing, fallback logic, requester-pays setup
- `references/latex-canonicalization.md` — contract for equation normalization
- `references/latex-macros.md` — macro expansion table used by canonicalizer

## Scripts

- `scripts/ingest_arxiv.py` — arXiv end-to-end (ar5iv → S3 → Marker fallback)
- `scripts/ingest_pdf_deep.sh` — Marker + MinerU + cross-check for math-dense PDFs
- `scripts/ingest_stacks.py` — Stacks Project JSON API
- `scripts/math_crosscheck.py` — merge Marker + MinerU outputs
- `scripts/canonicalize.py` — LaTeX canonicalization w/ macro expansion
