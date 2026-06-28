# Marker v1.10+ pipeline

Primary PDF→markdown extractor. Native PyTorch MPS on Apple Silicon.

## Install

```bash
# Marker uses Surya for layout; both are PyPI-installable.
pip install marker-pdf

# Confirm MPS is picked up (should print "mps")
python -c "import torch; print(torch.backends.mps.is_available(), torch.device('mps'))"
```

## Core invocation

```bash
marker_single \
  <input.pdf> \
  --use_llm \
  --output_format markdown \
  --output <output_dir>
```

### Flags worth knowing

| Flag | Purpose |
|---|---|
| `--use_llm` | Route borderline math/table blocks through an LLM for cleanup. Biggest quality lever for math papers. |
| `--output_format markdown` | Default. Also supports `json`, `html`. |
| `--output <dir>` | Output directory; Marker creates `<slug>/<slug>.md` inside. |
| `--max_pages N` | Useful for smoke tests. |
| `--batch_multiplier N` | GPU batch size multiplier; 4 is a reasonable starting point on 32 GB Macs. |
| `--processors layout,text,math,table,...` | Fine-grained processor selection; default is everything. |

## `--use_llm` backend

By default, `--use_llm` uses Google's Gemini free tier. We want local — use
a compatible endpoint:

```bash
export MARKER_LLM_SERVICE=openai_compatible
export MARKER_LLM_BASE_URL=http://localhost:8400/v1  # llama-server Octen
export MARKER_LLM_MODEL=octen  # match what llama-server exposes
```

Wait — Octen is an embedder, not a generator. Use a small generator instead,
like Qwen3-1.7B-Instruct (already downloaded for qmd query expansion). Point
Marker at a separate llama-server instance serving that:

```bash
llama-server -m models/Qwen3-1.7B-Instruct.gguf --port 8401 &
export MARKER_LLM_BASE_URL=http://localhost:8401/v1
```

The quality improvement from `--use_llm` on math papers is ~4 percentage
points on olmOCR-Bench. Worth the extra process.

## Output structure

```
<output_dir>/
  <slug>/
    <slug>.md              # markdown with $$...$$ blocks
    <slug>_meta.json       # page-level metadata
    <slug>.pdf             # copy of input (Marker always includes)
    images/                # extracted figures
      page_0_fig_1.png
      ...
```

## Post-processing

After Marker runs, the ingest pipeline:

1. Moves `<slug>.md` → `01-raw/<sha256>-<slug>.marker.md`
2. Moves `images/` → `01-raw/<sha256>-<slug>.images/`
3. Keeps `<slug>_meta.json` as sidecar for later use
4. Deletes the `<slug>.pdf` copy (we already have the original)

## Known quirks

- **Page numbers in body text**: Marker sometimes emits "[Page 47]" headers
  mid-paragraph. Strip with regex `^\[Page \d+\]$` before canonicalization.
- **Math in tables**: Marker occasionally puts inline math in table cells as
  escaped text instead of `$...$`. `--use_llm` fixes most of these.
- **Multi-column PDFs with footnotes**: footnotes sometimes get merged into
  main body text. Compare against MinerU output (cross-check step) — MinerU
  handles column flow better.
- **Scanned PDFs**: Marker works on scanned PDFs but slower. For these,
  consider running OCR preprocessing with `ocrmypdf` first.

## Timing

On M3 Pro, typical book chapter (30 pages, math-dense):

- Without `--use_llm`: ~3 minutes
- With `--use_llm`: ~8 minutes (LLM calls dominate)

For full-book ingestion (500 pages), expect 2–3 hours with `--use_llm`.
Worth it — the alternative is re-doing it when you discover a formula is
garbled.

## When to skip Marker

- Source is LaTeX (`.tex`) → use LaTeXML instead
- Source is HTML → use `markitdown` directly
- Source is arXiv → use ar5iv first (see `arxiv-ar5iv.md`)
- Source is a Stacks Project chapter → use their JSON API
