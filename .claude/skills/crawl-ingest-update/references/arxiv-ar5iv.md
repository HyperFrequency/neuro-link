# arXiv ingestion — ar5iv first, S3 fallback, Marker last resort

arXiv is our biggest legal bulk source. Three paths, in preferred order:

## Path 1 — ar5iv HTML (preferred)

`ar5iv.labs.arxiv.org` serves pre-rendered HTML+MathML for the entire arXiv
corpus up to dataset cut-off (04.2024 at time of writing). Mathematical
content is clean MathML — no PDF extraction needed, no Marker invocation,
no MinerU cross-check.

### ID parsing

arXiv IDs come in two formats. Normalize before fetching:

```python
import re

def parse_arxiv_id(raw: str) -> str:
    """Accept any of:
      arXiv:2512.09874
      https://arxiv.org/abs/2512.09874
      https://arxiv.org/abs/2512.09874v2
      https://arxiv.org/pdf/2512.09874.pdf
      2512.09874
      2512.09874v2
    Return: 2512.09874 (strip version suffix; ar5iv serves latest).
    """
    m = re.search(r'(\d{4}\.\d{4,5})(v\d+)?', raw)
    if not m:
        raise ValueError(f"Not an arXiv id: {raw}")
    return m.group(1)
```

### Fetch

```
https://ar5iv.labs.arxiv.org/html/<id>
```

Returns HTML. No auth required. Rate limit: be polite, ~1 req/sec.

### HTML → markdown conversion

Use `markitdown` with the MathML-aware flag:

```python
from markitdown import MarkItDown
md = MarkItDown()
result = md.convert(html_content, stream=True)
# Display math stays wrapped in $$...$$. Inline stays $...$.
```

Verify after conversion: count `$$` occurrences in output, cross-check
against ar5iv's equation count in the HTML (span#S<n>.E<m>.m1 nodes).

## Path 2 — arXiv S3 bulk LaTeX source

Used when ar5iv returns 404 (paper post-2024-04 or processing error).

arXiv's S3 bucket `arxiv-dataset` is `requester-pays`. Egress to
non-AWS ~$0.09/GB. Papers average 3–8 MB compressed TAR, so a single
paper is cents.

### Setup

Credentials via `AWS_*` env vars from `secrets/.env`. Tool:

```bash
aws s3 cp --request-payer requester \
  s3://arxiv/pdf/<yymm>/<id>v1.tar.gz \
  ./state/mirrors/arxiv-source/<id>.tar.gz
```

(Wait — pdf path is for pdfs. LaTeX source is under `s3://arxiv/src/<yymm>/<id>v1.gz`.)

### Extract + LaTeXML

```bash
tar -xzf <id>.tar.gz -C <workdir>
latexml <workdir>/<main>.tex | latexmlpost --dest=<out>.html
# then markitdown the html same as Path 1
```

LaTeXML handles macros and produces valid HTML+MathML. The trickier part
is finding the main `.tex` — arXiv sources often have multiple files. Use
heuristic: the file containing `\documentclass` at top level.

## Path 3 — Marker on the PDF

Only when both ar5iv and S3 fail. Treat as degraded — log a warning.

See `references/marker-pipeline.md` for the pipeline. Note: Marker on
math-dense arXiv papers is slower and less accurate than ar5iv — always
prefer ar5iv when available.

## Metadata enrichment

After body extraction, fetch structured metadata from the arXiv API:

```
https://export.arxiv.org/api/query?id_list=<id>
```

Returns Atom XML with title, authors, abstract, categories, DOI, published
date, comments. Parse and stick into the wiki page frontmatter:

```yaml
---
title: <from api>
arxiv_id: <id>
authors: [<list>]
categories: [math.AT, cs.LG, ...]
doi: <if present>
published: <iso>
source: arxiv
source_path: 01-raw/<sha256>-<slug>.md
---
```

## Dedup strategy

Before Path 1, compute:

```
sha256(html_content_from_ar5iv)
```

Check against existing `01-raw/<sha256>-*`. If exists, skip re-ingestion
unless `--force` is set.

Versioned papers (v2, v3, ...) have different SHA256 — treat as distinct
ingests. Link them via a `superseded_by` / `supersedes` chain in frontmatter.

## Known failure modes

- **ar5iv 500 errors**: transient, retry after 30s. If persistent, fall to Path 2.
- **LaTeX macros undefined**: some authors use custom macros not expanded
  by LaTeXML. Output will have raw `\foo{...}` in the markdown. Flag in
  `open_questions` on the wiki page; user reviews.
- **Papers with proprietary fonts/graphics**: ar5iv won't render; S3 tarball
  usually includes fonts. Extract with `tar -xzf` and keep fonts alongside.
- **Withdrawn papers**: arXiv returns 404 on both HTML and S3. Log and move on.
