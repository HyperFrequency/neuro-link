# MinerU 2.5 pipeline

Second opinion on math-dense PDFs. Runs after Marker; the cross-check
script merges their outputs.

## Why MinerU alongside Marker

MinerU 2.5 ships dedicated MFD (Math Formula Detection) and MFR (Math
Formula Recognition) models. On OmniDocBench v1.5, it scores 86.2 overall.
Where Marker occasionally emits malformed `\frac{a}{b}{c}`, MinerU tends
to produce clean LaTeX. And vice versa — Marker is stronger on inline math
in prose. Running both and picking the version with a valid sympy parse
per-equation beats either alone.

## Install

```bash
pip install mineru

# MinerU has an MLX-native backend on Apple Silicon — 2-3x faster than MPS
pip install 'mineru[vlm-mlx]'
```

## Core invocation

```bash
mineru \
  --backend vlm-mlx \
  --input <input.pdf> \
  --output <output_dir>
```

### Backends

| Backend | Use |
|---|---|
| `vlm-mlx` | **Apple Silicon preferred** — native MLX, ~2-3x faster than alternatives. |
| `pipeline` | Classic multi-model pipeline. Slower but more robust on edge cases. |
| `vlm-sglang` | CUDA-only; don't use on Mac. |

## Output structure

```
<output_dir>/
  <stem>/
    auto/
      <stem>.md              # primary markdown
      <stem>_content_list.json
      <stem>_layout.pdf      # annotated overlay
      <stem>_spans.pdf       # span-level overlay
      <stem>_model.json      # per-element detection results
      images/
```

## Post-processing

Ingest pipeline moves:

- `auto/<stem>.md` → `01-raw/<sha256>-<slug>.mineru.md`
- `auto/<stem>_content_list.json` → sidecar for cross-check
- `auto/images/` → `01-raw/<sha256>-<slug>.images.mineru/` (keep separate
  from Marker's images; the two tools extract overlapping but non-identical
  image sets)

## Cross-check with Marker output

`scripts/math_crosscheck.py` takes both Marker and MinerU markdown, pairs
equations by position (character offset within extracted body), and picks
the best version per pair:

1. If both parse to valid sympy `srepr`, prefer the one with shorter
   canonical form (usually cleaner).
2. If only one parses, pick it.
3. If neither parses, pick Marker (usually has better surrounding text).
4. Record the source in sidecar `.meta.json`: `{"eq_index": 42, "source": "mineru"}`.

This produces the canonical merged markdown at `01-raw/<sha256>-<slug>.md`.

## Timing

On M3 Pro, same 30-page math chapter:

- MinerU pipeline backend: ~5 minutes
- MinerU vlm-mlx backend: ~2 minutes

Run in parallel with Marker — they're independent.

## Known quirks

- **Disk use**: MinerU's intermediate files are large (~3× input PDF size).
  Prune `auto/*.pdf` overlays after extraction; they're only useful for
  debugging.
- **Table handling**: MinerU's tables are markdown-formatted but sometimes
  lose row/col headers on merged-cell layouts. Compare against Marker.
- **Formula confidence**: MinerU emits per-formula confidence in
  `_model.json`. Low-confidence (<0.5) formulas should be flagged for HITL
  review.

## Environment

MLX backend needs `MPS_AVAILABLE` env:

```bash
export PYTORCH_ENABLE_MPS_FALLBACK=1
```

Optional; MinerU will warn and proceed without it.

## Known issues with vlm-mlx

- Some checkpoints require explicit conversion. `mineru doctor --backend vlm-mlx` diagnoses.
- OOM on documents >100 pages at default batch size. Set
  `--batch_size 4` for long books.
