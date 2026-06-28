# `wiki_math` — Wikipedia Advanced-Math Ingester (KB.4)

Fetches the 20 advanced-math Wikipedia topics listed in `topics.txt`,
converts them to neuro-link-flavoured markdown, and drops them into
`02-KB-main/math/wiki-<slug>.md` with valid YAML frontmatter.

## Files

| File | Purpose |
| --- | --- |
| `fetch.py` | Ingester: MediaWiki API client, markdown renderer, CLI |
| `topics.txt` | One topic per line (20 Wikipedia page titles) |
| `test_fetch.py` | Unit tests (mocked API, no network) |
| `.cache/` | On-disk cache of raw API responses (`<slug>.extract.json`, `<slug>.html`) |

## Setup

```bash
pip3 install --break-system-packages wikipedia-api pytest
```

`wikipedia-api` is kept as an optional convenience dependency; the
fetcher itself talks to the MediaWiki API directly via `urllib` so it
has no runtime dependencies beyond the Python stdlib.

## Usage

Run from the repo root:

```bash
python3 scripts/wiki_math/fetch.py
```

Override the topic list or sleep interval:

```bash
python3 scripts/wiki_math/fetch.py --topics path/to/topics.txt --rate-limit 1.5
```

The run takes ~25s (20 topics × 2 requests × 1s rate limit, minus
cache hits). Output files land in `02-KB-main/math/wiki-<slug>.md`
and a summary table is printed to stdout:

```
Topic                           Words   Math
--------------------------------------------
Manifold                         1273     39
Lie_group                        1838     41
...
--------------------------------------------
count=20  avg=1348  min=646  max=2586
```

## Etiquette

- **User-Agent**: `neuro-link-recursive/0.2 (https://github.com/HyperFrequency/neuro-link-recursive)`
- **Rate limit**: 1 req/s between topic fetches (configurable).
- **Cache**: Raw JSON/HTML cached under `.cache/` keyed by slug;
  re-runs are offline after the first successful fetch.

Follows the [Wikimedia API etiquette guide](https://www.mediawiki.org/wiki/API:Etiquette):
single-threaded, polite identification, cached bodies.

## Output format

Each generated file starts with YAML frontmatter:

```yaml
---
title: Manifold
domain: math
source: wikipedia
source_url: https://en.wikipedia.org/wiki/Manifold
confidence: medium
last_updated: 2026-04-17
word_count: 1273
---
```

Body structure:

1. `# <Title>`
2. `## Overview` — first 2–3 lead paragraphs from the article.
3. One `##`/`###` section per preserved Wikipedia section
   (Definition / Properties / Examples / Applications / etc. are
   prioritised; other substantive sections are appended).
4. `## Formulas` — every LaTeX expression extracted from `<math>`
   annotations, preserved verbatim (inline `$…$` for short, display
   `$$…$$` for multi-line / environments).
5. `## Sources` — single `[source:wikipedia]` citation with URL and
   retrieval date.

## Verification

After running, the following invariants hold:

- 20 files in `02-KB-main/math/wiki-*.md`.
- Every file ≥ 300 words (enforced by `render_page` — short extracts
  raise `WikiFetchError` so nothing ships as a stub).
- Every file has the YAML frontmatter block and `source: wikipedia`.
- No file body contains the literal string `TODO:`.

Quick audit script:

```bash
for f in 02-KB-main/math/wiki-*.md; do
  wc=$(wc -w <"$f" | tr -d ' ')
  has_src=$(grep -c '^source: wikipedia$' "$f")
  [ "$wc" -lt 300 ] && echo "UNDER 300: $f ($wc)"
  [ "$has_src" -ne 1 ] && echo "MISSING source: $f"
  grep -l 'TODO:' "$f"
done
```

## Unit tests

Mocked (no network):

```bash
python3 -m pytest scripts/wiki_math/test_fetch.py -q
```

Tests cover: slug generation, section parsing, `<math>` extraction,
inline-vs-display formatting, full render pipeline, word-count floor
enforcement, file-writing, and cache round-tripping.

## Adding topics

1. Append the topic to `topics.txt` (one per line, underscores for
   spaces, e.g. `Measure_(mathematics)`).
2. Re-run `python3 scripts/wiki_math/fetch.py`.
3. The fetcher is idempotent — it will overwrite existing files with
   the freshly-fetched content.

## Re-ingesting a single topic

Delete the cache entry and re-run. Or:

```bash
python3 scripts/wiki_math/fetch.py --topics <(echo "Manifold")
```
