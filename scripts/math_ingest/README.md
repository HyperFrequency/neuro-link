# `scripts/math_ingest/` — math subsystem ingest (E2)

Self-contained pipeline that mirrors high-level math reference content into
`02-KB-main/math/`, normalises LaTeX into canonical sympy `srepr`, and
registers the `math_symbols` Qdrant collection.

## Components

| File                     | Purpose                                                                                 |
| ------------------------ | --------------------------------------------------------------------------------------- |
| `nlab_clone.sh`          | Shallow-clones `ncatlab/nlab-content` (~600 MB) into `state/mirrors/nlab/`.             |
| `mathworld_crawl.py`     | Polite crawler for MathWorld (1 req / 3 s, robots.txt, persistent cache).               |
| `adapter.py`             | Parses cached sources -> wiki pages with preserved `$$...$$` + per-equation `srepr`.    |
| `qdrant_index.py`        | Creates the `math_symbols` collection in Qdrant (vector size 4096, cosine, KW index).   |
| `topics.txt`             | 50 high-level topic slugs (MathWorld URL basenames).                                    |
| `test_adapter.py`        | Unit tests (pytest).                                                                    |

## Operator runbook

```bash
# 0. One-time Python env
python3 -m venv /tmp/e2-math-venv
/tmp/e2-math-venv/bin/pip install pylatexenc latex2sympy2_extended \
    qdrant-client requests beautifulsoup4 pytest sympy
PY=/tmp/e2-math-venv/bin/python

# 1. Mirror nLab (idempotent; ~600 MB)
bash scripts/math_ingest/nlab_clone.sh

# 2. Crawl MathWorld at 1 req / 3 s. Cached pages are skipped on re-run.
$PY scripts/math_ingest/mathworld_crawl.py            # all 50 topics
$PY scripts/math_ingest/mathworld_crawl.py --only Manifold Topology  # smoke

# 3. Parse + emit wiki pages into 02-KB-main/math/
$PY scripts/math_ingest/adapter.py
$PY scripts/math_ingest/adapter.py --only Manifold Topology  # smoke

# 4. Ensure Qdrant schema exists. Non-fatal if Qdrant is down (logs + exits 0).
$PY scripts/math_ingest/qdrant_index.py

# 5. Unit tests
$PY -m pytest scripts/math_ingest/test_adapter.py -v
```

## Behaviour & constraints

- **Rate limit**: `mathworld_crawl.py` enforces a 3-second floor between
  requests via a single rolling timestamp. No concurrency.
- **robots.txt**: fetched once per run via `urllib.robotparser`. If fetch
  fails, we default to "allow" to avoid silent no-ops during dev.
- **User-Agent**: `neuro-link-recursive/0.2 (https://github.com/HyperFrequency/neuro-link-recursive)`.
- **Cache**: every MathWorld page persists to `state/mirrors/mathworld/<slug>.html`.
  Re-running is free — no HTTP calls for cached slugs.
- **LaTeX preservation**: every `$$...$$` block in the rendered markdown is
  the byte-for-byte original. The adapter never rewrites math text. Failed
  sympy parses yield `canonical_srepr: null`, never an exception.
- **Qdrant**: the script never crashes on connectivity failure; CI can run
  without a live Qdrant. Schema: `size=4096`, `Cosine`, keyword index on
  `canonical_srepr`. Embeddings are written by a separate job (out of scope
  for this PR).

## Verified end-to-end smoke (2026-04-17)

Ran the full pipeline against 5 topics, all assertions passed:

| Topic        | Page                               | Words | Equations | Parseable `srepr` |
| ------------ | ---------------------------------- | ----: | --------: | ----------------: |
| HilbertSpace | `02-KB-main/math/hilbertspace.md`  |   308 |         6 |                 4 |
| LieGroup     | `02-KB-main/math/liegroup.md`      |   388 |         2 |                 0 |
| Manifold     | `02-KB-main/math/manifold.md`      |   747 |         3 |                 3 |
| MetricTensor | `02-KB-main/math/metrictensor.md`  |  1473 |        59 |                 9 |
| Topology     | `02-KB-main/math/topology.md`      |  1031 |         9 |                 9 |

All 5 pages exceed the 200-word floor. At least one parseable srepr appears
in 4 of 5 files; all files preserve their `$$...$$` blocks verbatim.

Qdrant collection verified live:

```json
{
  "name": "math_symbols",
  "vectors": {"size": 4096, "distance": "Cosine"},
  "payload_schema": {"canonical_srepr": {"data_type": "keyword"}}
}
```

Unit tests: **4 passed, 0 failed** (`pytest scripts/math_ingest/test_adapter.py`).

## Adding more topics

Edit `topics.txt`, re-run steps 2 + 3. The crawler skips already-cached
slugs, so only new ones incur HTTP traffic. The adapter is idempotent —
re-running overwrites pages in place with a fresh `sha256`.
