# LaTeX canonicalization contract

Contract: every `$$...$$` block produced by the ingest pipeline is preserved
byte-for-byte in the markdown. A sidecar `.meta.json` holds the canonical
form computed for each equation. Never rewrite display math inline — keep
the markdown as authors wrote it, and store canonical forms separately so
Qdrant / `nlr_math_lookup` can do structural search without mutating the
human-readable body.

## Pipeline

```
LaTeX source string
  │
  ▼
macro expansion (pylatexenc with custom macro table)
  │
  ▼
parse (latex2sympy2_extended)
  │
  ▼
SymPy expression tree
  │
  ▼
srepr (SymPy's structural string representation)
  │
  ▼
canonical_srepr (stored in sidecar)
```

## Macro expansion table

Default + extensions live in `references/latex-macros.md`. Examples:

```python
macros = {
    # Number sets
    r'\R':      r'\mathbb{R}',
    r'\N':      r'\mathbb{N}',
    r'\Z':      r'\mathbb{Z}',
    r'\Q':      r'\mathbb{Q}',
    r'\C':      r'\mathbb{C}',

    # Differentials
    r'\dd':     r'\mathrm{d}',
    r'\partial_t': r'\frac{\partial}{\partial t}',
    r'\partial_x': r'\frac{\partial}{\partial x}',

    # Operators
    r'\E':      r'\mathbb{E}',
    r'\Var':    r'\text{Var}',
    r'\Cov':    r'\text{Cov}',

    # Quant-finance shorthand
    r'\Rplus':  r'\mathbb{R}_{+}',
}
```

The table expands. Users add macros via:

```yaml
# in config/neuro-link.md frontmatter
latex_macros:
  \\foo: \\bar
```

## Why canonicalize

Two reasons:

1. **Exact-match retrieval on structure.** Qdrant's keyword index on
   `canonical_srepr` lets you find every occurrence of
   `\int_0^1 x^2 dx` regardless of whether the author wrote it as
   `\int_0^1 x^2 \, dx`, `\int_{0}^{1} x^{2}\,dx`, or
   `\int_0^1 x^{2}dx`. They all `srepr` to the same string.
2. **Semantic search on canonical form.** When you embed
   `canonical_srepr` instead of raw LaTeX, the vector encodes the
   mathematical structure, not surface typography.

## Failure modes — never exception

Canonicalization failures are common and expected:

- Author-defined macros not in our expansion table → `pylatexenc` emits
  partial AST.
- Non-standard LaTeX (TikZ, custom environments, figures-as-math) →
  `latex2sympy2` fails to parse.
- Unicode math symbols (U+2200 FOR ALL) → depends on pylatexenc version.

When any stage fails, record `canonical_srepr: null` and the failure
stage (`expansion | parse | srepr`) in the sidecar. **Never raise** —
the pipeline continues and the equation is preserved as-is in the
markdown. The user can review null-srepr equations in bulk later via
`/neuro-scan`.

## Sidecar format

`01-raw/<sha256>-<slug>.meta.json`:

```json
{
  "doc_sha256": "abc...",
  "equations": [
    {
      "index": 0,
      "position": 1243,         // char offset in markdown body
      "latex": "\\int_0^1 x^2 \\, dx",
      "expanded": "\\int_{0}^{1} x^{2} \\, dx",
      "canonical_srepr": "Integral(Pow(Symbol('x'), Integer(2)), Tuple(Symbol('x'), Integer(0), Integer(1)))",
      "confidence": 1.0
    },
    {
      "index": 1,
      "position": 2890,
      "latex": "\\text{some figure-like thing}",
      "expanded": null,
      "canonical_srepr": null,
      "failure_stage": "parse",
      "confidence": 0.0
    }
  ]
}
```

Write indices to Qdrant at vector-index time, not at ingest time. The
keyword index on `canonical_srepr` is what drives exact-structure
retrieval in `nlr_math_lookup`.

## Macro expansion vs no expansion

Some pipelines skip expansion and pass raw LaTeX to `latex2sympy2`. We
expand first because `latex2sympy2` chokes on author-defined macros it
doesn't know about. Expansion costs ~5ms/equation; worth it.

Edge case: if an expansion would change rendering semantics (rare — only
for conflicting custom definitions), flag the equation and preserve raw
LaTeX. The canonicalizer should be a no-op on rendering.

## Unicode normalization

Before expansion, normalize Unicode:

```python
import unicodedata
s = unicodedata.normalize('NFC', s)
```

This folds equivalent sequences (combining accents, etc) so they match
when comparing via `srepr`.
