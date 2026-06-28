#!/usr/bin/env python3
"""
Math source adapter.

Reads cached MathWorld HTML (state/mirrors/mathworld/<slug>.html) and optionally
nLab mirror pages, normalises LaTeX, and writes wiki pages to
02-KB-main/math/<slug>.md.

Frontmatter follows 02-KB-main/schema.md. For every `$$...$$` block parseable
as a sympy expression, the canonical srepr is embedded in the frontmatter
`equations:` list (null for unparseable blocks).
"""

from __future__ import annotations

import argparse
import hashlib
import logging
import re
import sys
from dataclasses import dataclass
from datetime import date
from pathlib import Path
from typing import Iterable, List, Optional

from bs4 import BeautifulSoup
from pylatexenc.latex2text import LatexNodes2Text

try:  # sympy is a transitive dep of latex2sympy2_extended
    import sympy
except Exception:  # pragma: no cover - defensive
    sympy = None  # type: ignore[assignment]

try:
    from latex2sympy2_extended import latex2sympy
except Exception:  # pragma: no cover - optional runtime dep
    latex2sympy = None  # type: ignore[assignment]

LOG = logging.getLogger("math_adapter")

MATHWORLD_BASE = "https://mathworld.wolfram.com"

# Matches $$ ... $$ blocks (non-greedy, across newlines).
DOLLAR_BLOCK_RE = re.compile(r"\$\$(?P<tex>.+?)\$\$", re.DOTALL)


def _repo_root() -> Path:
    return Path(__file__).resolve().parents[2]


@dataclass
class Equation:
    tex: str
    srepr: Optional[str]


@dataclass
class ParsedPage:
    slug: str
    title: str
    body: str              # normalised markdown with $$...$$ preserved verbatim
    equations: List[Equation]
    source_url: str
    source_type: str       # "mathworld" | "nlab"


def _slugify(title: str) -> str:
    s = re.sub(r"[^A-Za-z0-9\-]+", "-", title).strip("-").lower()
    return s or "untitled"


# ---------------------------------------------------------------------------
# LaTeX -> sympy srepr
# ---------------------------------------------------------------------------

def _canonical_srepr(tex: str) -> Optional[str]:
    """Return sympy srepr of `tex`, or None if it can't be parsed."""
    if latex2sympy is None or sympy is None:
        return None
    cleaned = tex.strip()
    if not cleaned:
        return None
    try:
        expr = latex2sympy(cleaned)
    except Exception as exc:
        LOG.debug("latex2sympy failed on %r: %s", cleaned[:60], exc)
        return None
    # latex2sympy2_extended may return AST objects or lists; coerce.
    try:
        if isinstance(expr, list):
            if not expr:
                return None
            expr = expr[0]
        return sympy.srepr(expr)
    except Exception as exc:
        LOG.debug("srepr failed on %r: %s", cleaned[:60], exc)
        return None


def extract_equations(body: str) -> List[Equation]:
    """Pull every $$...$$ block out of `body` and attempt srepr normalisation."""
    out: List[Equation] = []
    seen: set[str] = set()
    for match in DOLLAR_BLOCK_RE.finditer(body):
        tex = match.group("tex").strip()
        if not tex or tex in seen:
            continue
        seen.add(tex)
        out.append(Equation(tex=tex, srepr=_canonical_srepr(tex)))
    return out


# ---------------------------------------------------------------------------
# MathWorld HTML -> wiki page
# ---------------------------------------------------------------------------

def _extract_mathworld_body(soup: BeautifulSoup) -> tuple[str, str]:
    """Return (title, markdown_body)."""
    title_tag = soup.find("title")
    raw_title = title_tag.get_text(strip=True) if title_tag else "Untitled"
    # MathWorld titles look like: "Manifold -- from Wolfram MathWorld"
    title = re.split(r"\s*--\s*from\s+Wolfram", raw_title, maxsplit=1)[0].strip()

    # Prefer the main content container; fall back to full body.
    container = (
        soup.find(id="mainContent")
        or soup.find(id="content")
        or soup.find("div", class_="entry-content")
        or soup.body
        or soup
    )

    paragraphs: List[str] = []
    for p in container.find_all(["p", "h2", "h3", "li"]):
        text = p.get_text(separator=" ", strip=True)
        if not text:
            continue
        if p.name in ("h2", "h3"):
            paragraphs.append(f"\n### {text}\n")
        else:
            paragraphs.append(text)

    # Inline math: MathWorld uses <img alt="..."> for TeX snippets. Convert a
    # handful of these into $$...$$ blocks so downstream srepr has something
    # real to parse.
    for img in container.find_all("img"):
        alt = img.get("alt") or ""
        if not alt.strip():
            continue
        if any(ch in alt for ch in "\\^_{}=+") or alt.strip().startswith("f("):
            paragraphs.append(f"$${alt.strip()}$$")

    body_md = "\n\n".join(paragraphs)
    # Final cleanup: drop stray latex-text residue produced by weird OCR.
    body_md = re.sub(r"\n{3,}", "\n\n", body_md).strip()
    return title, body_md


def parse_mathworld(slug: str, html: str) -> ParsedPage:
    soup = BeautifulSoup(html, "html.parser")
    title, body = _extract_mathworld_body(soup)
    equations = extract_equations(body)
    return ParsedPage(
        slug=_slugify(slug),
        title=title or slug,
        body=body,
        equations=equations,
        source_url=f"{MATHWORLD_BASE}/{slug}.html",
        source_type="mathworld",
    )


# ---------------------------------------------------------------------------
# nLab page -> wiki page
# ---------------------------------------------------------------------------

_LATEX_TO_TEXT = LatexNodes2Text(keep_comments=False, math_mode="verbatim")


def parse_nlab(slug: str, path: Path) -> ParsedPage:
    raw = path.read_text(errors="replace")
    # nLab uses Instiki/MathML; $$...$$ already works in their syntax. Keep it
    # verbatim and normalise stray \[ ... \] into $$...$$ too.
    body = re.sub(r"\\\[(.+?)\\\]", r"$$\1$$", raw, flags=re.DOTALL)
    body = re.sub(r"\\\((.+?)\\\)", r"$$\1$$", body, flags=re.DOTALL)
    equations = extract_equations(body)
    title = slug.replace("_", " ")
    return ParsedPage(
        slug=_slugify(slug),
        title=title,
        body=body.strip(),
        equations=equations,
        source_url=f"https://ncatlab.org/nlab/show/{slug}",
        source_type="nlab",
    )


# ---------------------------------------------------------------------------
# Serialisation
# ---------------------------------------------------------------------------

def render_page(page: ParsedPage) -> str:
    """Render a ParsedPage to markdown matching 02-KB-main/schema.md."""
    today = date.today().isoformat()
    sha = hashlib.sha256(page.body.encode("utf-8")).hexdigest()

    eq_lines: List[str]
    if page.equations:
        eq_lines = ["equations:"]
        for eq in page.equations:
            # Ensure TeX is safe on a single YAML line; fall back to block scalar.
            tex_inline = eq.tex.replace("\n", " ").strip()
            if "'" in tex_inline or len(tex_inline) > 200:
                eq_lines.append("  - tex: |")
                for ln in eq.tex.splitlines() or [eq.tex]:
                    eq_lines.append(f"      {ln}")
            else:
                eq_lines.append(f"  - tex: '{tex_inline}'")
            if eq.srepr is None:
                eq_lines.append("    canonical_srepr: null")
            else:
                eq_lines.append(f"    canonical_srepr: \"{eq.srepr}\"")
    else:
        eq_lines = ["equations: []"]

    frontmatter = "\n".join(
        [
            "---",
            f"title: {page.title}",
            "domain: math",
            "sources:",
            f"  - slug: {page.source_type}-{page.slug}",
            f"    url: {page.source_url}",
            f"    type: {'article' if page.source_type == 'mathworld' else 'docs'}",
            f"    ingested: {today}",
            "    confidence: medium",
            "confidence: medium",
            f"last_updated: {today}",
            f"sha256: {sha}",
            *eq_lines,
            "---",
        ]
    )

    header = f"# {page.title}\n\n"
    attribution = (
        f"_Ingested from {page.source_type.title()}: <{page.source_url}>._\n\n"
    )
    return f"{frontmatter}\n\n{header}{attribution}{page.body}\n"


def write_page(page: ParsedPage, out_dir: Path) -> Path:
    out_dir.mkdir(parents=True, exist_ok=True)
    out_path = out_dir / f"{page.slug}.md"
    out_path.write_text(render_page(page))
    return out_path


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def _iter_mathworld_pages(cache_dir: Path, slugs: Optional[Iterable[str]]) -> Iterable[ParsedPage]:
    allow = set(slugs) if slugs else None
    for html_path in sorted(cache_dir.glob("*.html")):
        slug = html_path.stem
        if allow and slug not in allow:
            continue
        try:
            yield parse_mathworld(slug, html_path.read_text(errors="replace"))
        except Exception as exc:
            LOG.error("failed to parse %s: %s", html_path, exc)


def _parse_args() -> argparse.Namespace:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument(
        "--mathworld-cache",
        default=str(_repo_root() / "state" / "mirrors" / "mathworld"),
    )
    ap.add_argument(
        "--out-dir",
        default=str(_repo_root() / "02-KB-main" / "math"),
    )
    ap.add_argument(
        "--only",
        nargs="*",
        default=None,
        help="restrict to these MathWorld slugs",
    )
    ap.add_argument("--verbose", action="store_true")
    return ap.parse_args()


def main() -> int:
    args = _parse_args()
    logging.basicConfig(
        level=logging.DEBUG if args.verbose else logging.INFO,
        format="%(levelname)s %(name)s %(message)s",
    )

    cache_dir = Path(args.mathworld_cache)
    out_dir = Path(args.out_dir)
    count = 0
    for page in _iter_mathworld_pages(cache_dir, args.only):
        path = write_page(page, out_dir)
        LOG.info(
            "wrote %s (%d eqs, %d parseable)",
            path,
            len(page.equations),
            sum(1 for e in page.equations if e.srepr),
        )
        count += 1
    LOG.info("adapter done: %d pages written", count)
    return 0


if __name__ == "__main__":
    sys.exit(main())
