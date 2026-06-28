#!/usr/bin/env python3
"""KB.4 — Wikipedia advanced-math ingester for neuro-link-recursive.

Fetches 20 math articles via the MediaWiki API, converts to neuro-link
flavoured markdown with YAML frontmatter, and writes under
``02-KB-main/math/wiki-<slug>.md``.

Run from the repo root:

    python3 scripts/wiki_math/fetch.py
"""
from __future__ import annotations

import argparse
import datetime as _dt
import json
import re
import sys
import time
import urllib.parse
import urllib.request
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable

REPO_ROOT = Path(__file__).resolve().parents[2]
SCRIPT_DIR = Path(__file__).resolve().parent
CACHE_DIR = SCRIPT_DIR / ".cache"
TOPICS_FILE = SCRIPT_DIR / "topics.txt"
OUTPUT_DIR = REPO_ROOT / "02-KB-main" / "math"

USER_AGENT = (
    "neuro-link-recursive/0.2 "
    "(https://github.com/HyperFrequency/neuro-link-recursive)"
)
API_BASE = "https://en.wikipedia.org/w/api.php"
RATE_LIMIT_SECONDS = 1.0
MIN_WORDS = 300


class WikiFetchError(RuntimeError):
    """Raised when fetch/parse of a Wikipedia article fails."""


@dataclass
class WikiPage:
    topic: str
    title: str
    extract: str
    html: str
    url: str

    @property
    def slug(self) -> str:
        return topic_to_slug(self.topic)


# --------------------------------------------------------------------------- #
# HTTP
# --------------------------------------------------------------------------- #
def _get_json(url: str, timeout: int = 30) -> dict:
    req = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        return json.loads(resp.read().decode("utf-8"))


def fetch_extract(topic: str, cache_dir: Path = CACHE_DIR) -> dict:
    """Fetch the plain-text extract JSON for *topic*, with on-disk cache."""
    cache_dir.mkdir(parents=True, exist_ok=True)
    cache_file = cache_dir / f"{topic_to_slug(topic)}.extract.json"
    if cache_file.exists():
        return json.loads(cache_file.read_text(encoding="utf-8"))

    params = {
        "action": "query",
        "prop": "extracts",
        "exintro": "false",
        "explaintext": "true",
        "titles": topic.replace("_", " "),
        "format": "json",
        "redirects": "1",
    }
    url = f"{API_BASE}?{urllib.parse.urlencode(params)}"
    data = _get_json(url)
    cache_file.write_text(json.dumps(data, indent=2), encoding="utf-8")
    return data


def fetch_html(topic: str, cache_dir: Path = CACHE_DIR) -> str:
    """Fetch the parsed HTML section for *topic* so we can preserve <math>."""
    cache_dir.mkdir(parents=True, exist_ok=True)
    cache_file = cache_dir / f"{topic_to_slug(topic)}.html"
    if cache_file.exists():
        return cache_file.read_text(encoding="utf-8")

    params = {
        "action": "parse",
        "page": topic.replace("_", " "),
        "prop": "text",
        "formatversion": "2",
        "format": "json",
        "redirects": "1",
    }
    url = f"{API_BASE}?{urllib.parse.urlencode(params)}"
    data = _get_json(url)
    html = data.get("parse", {}).get("text", "") or ""
    cache_file.write_text(html, encoding="utf-8")
    return html


# --------------------------------------------------------------------------- #
# Parsing helpers
# --------------------------------------------------------------------------- #
_MATH_RE = re.compile(r"<math[^>]*>(.*?)</math>", re.DOTALL | re.IGNORECASE)
_ANNOT_RE = re.compile(
    r'<annotation[^>]*encoding="application/x-tex"[^>]*>(.*?)</annotation>',
    re.DOTALL | re.IGNORECASE,
)
_TAG_RE = re.compile(r"<[^>]+>")
_WS_RE = re.compile(r"\s+")
_HEADING_RE = re.compile(r"^=+\s*(.*?)\s*=+\s*$")


def topic_to_slug(topic: str) -> str:
    """Turn a Wikipedia title into a filesystem-safe slug."""
    s = topic.replace("_", "-")
    s = re.sub(r"[()]+", "", s)
    s = re.sub(r"[^A-Za-z0-9\-]+", "-", s)
    s = re.sub(r"-+", "-", s).strip("-").lower()
    return s


def _html_unescape(s: str) -> str:
    # Minimal entity decode — good enough for math content.
    return (
        s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", '"')
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
    )


def extract_math_blocks(html: str) -> list[str]:
    """Return a list of LaTeX strings found in ``<math>`` / annotation tags."""
    blocks: list[str] = []
    for m in _MATH_RE.finditer(html or ""):
        inner = m.group(1)
        # Prefer the TeX annotation if MathML wrapped it.
        ann = _ANNOT_RE.search(inner)
        tex = (ann.group(1) if ann else inner).strip()
        tex = _html_unescape(tex)
        tex = _TAG_RE.sub("", tex).strip()
        if tex:
            blocks.append(tex)
    return blocks


def convert_math_tags(text: str) -> str:
    """Replace ``<math>E=mc^2</math>`` with ``$E=mc^2$``."""

    def _repl(m: re.Match[str]) -> str:
        inner = m.group(1)
        ann = _ANNOT_RE.search(inner)
        tex = (ann.group(1) if ann else inner).strip()
        tex = _html_unescape(tex)
        tex = _TAG_RE.sub("", tex).strip()
        if not tex:
            return ""
        # Use inline math ($…$) by default; promote to display math ($$…$$)
        # when the content contains a structural marker (align/matrix/frac over
        # multiple lines).
        if any(tok in tex for tok in ("\\begin{", "\\\\")) or "\n" in tex:
            return f"$$\n{tex}\n$$"
        return f"${tex}$"

    return _MATH_RE.sub(_repl, text or "")


def _strip_html(html: str) -> str:
    return _html_unescape(_TAG_RE.sub("", html or ""))


def _strip_displaystyle(text: str) -> str:
    """Unwrap ``{\\displaystyle …}`` wrappers using a balanced-brace walker.

    Wikipedia's plaintext extracts sometimes leak MathML display markers such
    as ``{\\displaystyle \\|f\\|_{p}}``. A naive ``[^}]*`` regex trips on the
    nested ``_{p}`` group and mangles the LaTeX. Walking character-by-character
    with a brace depth counter lets us find the matching outer ``}`` and keep
    the inner TeX intact.
    """
    if not text or "\\displaystyle" not in text:
        return text
    out: list[str] = []
    i = 0
    n = len(text)
    while i < n:
        # Look for the "{\displaystyle" prefix.
        if text[i] == "{" and text.startswith("\\displaystyle", i + 1):
            start_inner = i + 1 + len("\\displaystyle")
            # Skip whitespace between "\displaystyle" and content.
            while start_inner < n and text[start_inner] in (" ", "\t", "\n"):
                start_inner += 1
            depth = 1
            j = start_inner
            while j < n and depth > 0:
                c = text[j]
                if c == "\\" and j + 1 < n:
                    # Preserve escaped braces / characters verbatim.
                    j += 2
                    continue
                if c == "{":
                    depth += 1
                elif c == "}":
                    depth -= 1
                    if depth == 0:
                        break
                j += 1
            if depth == 0 and j < n:
                # Emit the inner TeX (unwrap the {\displaystyle …} wrapper)
                # and advance past the closing brace.
                out.append(text[start_inner:j])
                i = j + 1
                continue
            # Unmatched wrapper: fall through and emit the literal char.
        out.append(text[i])
        i += 1
    return "".join(out)


# --------------------------------------------------------------------------- #
# Markdown build
# --------------------------------------------------------------------------- #
def parse_extract_sections(extract: str) -> list[tuple[int, str, str]]:
    """Split a plain-text extract into ``(level, heading, body)`` triples.

    MediaWiki returns sections as ``== Heading ==`` markers in the extract.
    The leading (untitled) block becomes the lead paragraph and is returned
    as ``(0, "", body)``.
    """
    if not extract:
        return []
    lines = extract.splitlines()
    sections: list[tuple[int, str, list[str]]] = [(0, "", [])]
    for line in lines:
        m = _HEADING_RE.match(line.strip())
        if m:
            raw = line.strip()
            level = (len(raw) - len(raw.lstrip("="))) or 2
            sections.append((level, m.group(1).strip(), []))
        else:
            sections[-1][2].append(line)
    out: list[tuple[int, str, str]] = []
    for level, heading, buf in sections:
        body = "\n".join(buf).strip()
        out.append((level, heading, body))
    return out


def word_count(text: str) -> int:
    return len(_WS_RE.split(text.strip())) if text.strip() else 0


def count_math_blocks(md: str) -> int:
    inline = len(re.findall(r"(?<!\$)\$(?!\$)[^$\n]+?\$(?!\$)", md))
    display = len(re.findall(r"\$\$[\s\S]+?\$\$", md))
    return inline + display


def build_markdown(page: WikiPage, today: str) -> str:
    """Render the wiki page markdown for *page*."""
    sections = parse_extract_sections(page.extract)

    # Heading-to-math lookup so we can sprinkle LaTeX back into the text.
    math_blocks = extract_math_blocks(page.html)

    # Lead = untitled first block (first 2–3 paragraphs kept verbatim).
    lead = ""
    body_sections: list[tuple[int, str, str]] = []
    for level, heading, body in sections:
        if not heading and not lead:
            paragraphs = [p.strip() for p in re.split(r"\n{2,}", body) if p.strip()]
            lead = "\n\n".join(paragraphs[:3])
            continue
        if heading:
            body_sections.append((level, heading, body))

    # Re-inject LaTeX: find each math block's rendered plaintext in the extract
    # and replace with the TeX form.  Wikipedia plaintext usually drops the
    # formula or replaces with a placeholder like ``{\\displaystyle …}``.
    # The previous regex-based stripper used ``[^}]*`` which stopped at the
    # first ``}`` and mangled nested-brace expressions like
    # ``{\displaystyle \|f\|_{p}}`` — the inner ``_{p}`` closed the outer
    # group prematurely, leaving stray ``}`` and eating the TeX body.
    # ``_strip_displaystyle`` walks the string with a brace counter so the
    # matching outer ``}`` is always chosen.
    def _restore_math(text: str) -> str:
        return _strip_displaystyle(text)

    lead = _restore_math(lead)
    body_sections = [(lvl, h, _restore_math(b)) for lvl, h, b in body_sections]

    # Prefer sections that look substantive.
    keep_keywords = (
        "definition", "properties", "example", "application",
        "history", "construction", "motivation", "overview",
        "formal", "general", "introduction", "theorem",
    )
    preferred: list[tuple[int, str, str]] = []
    other: list[tuple[int, str, str]] = []
    for lvl, h, b in body_sections:
        if not b.strip():
            continue
        if any(k in h.lower() for k in keep_keywords):
            preferred.append((lvl, h, b))
        else:
            other.append((lvl, h, b))

    selected = preferred + other
    # Trim to keep pages focused — but always include at least ~1200 words.
    kept: list[tuple[int, str, str]] = []
    running = word_count(lead)
    for lvl, h, b in selected:
        kept.append((lvl, h, b))
        running += word_count(b)
        if running >= 1500 and len(kept) >= 4:
            break
    if not kept:
        kept = selected

    # Build the markdown body.
    parts: list[str] = []
    parts.append(f"# {page.title}\n")
    parts.append("## Overview\n")
    parts.append(lead + "\n" if lead else
                 f"{page.title} is a topic in advanced mathematics covered on Wikipedia.\n")

    for lvl, heading, body in kept:
        hashes = "#" * max(2, min(lvl, 4))
        parts.append(f"{hashes} {heading}\n")
        parts.append(body.strip() + "\n")

    # Append a Formulas block so LaTeX from MathML is preserved verbatim
    # (Wikipedia's plain-text extract drops it otherwise).
    if math_blocks:
        parts.append("## Formulas\n")
        parts.append(
            "The following LaTeX expressions appear in the source article "
            "and are preserved verbatim for downstream ingestion:\n"
        )
        for tex in math_blocks[:30]:
            tex_clean = tex.strip()
            if (
                "\n" not in tex_clean
                and "\\begin{" not in tex_clean
                and "\\\\" not in tex_clean
                and len(tex_clean) <= 80
            ):
                parts.append(f"- ${tex_clean}$\n")
            else:
                parts.append(f"$$\n{tex_clean}\n$$\n")

    parts.append("## Sources\n")
    parts.append(
        f"[source:wikipedia] *{page.title}* — Wikipedia. "
        f"{page.url}. Retrieved {today}. Confidence: medium.\n"
    )

    body_md = "\n".join(parts)
    body_md = convert_math_tags(body_md)
    return body_md


def build_frontmatter(page: WikiPage, today: str, wc: int) -> str:
    """Render the YAML frontmatter block."""
    return (
        "---\n"
        f"title: {page.title}\n"
        "domain: math\n"
        "source: wikipedia\n"
        f"source_url: {page.url}\n"
        "confidence: medium\n"
        f"last_updated: {today}\n"
        f"word_count: {wc}\n"
        "---\n\n"
    )


def render_page(page: WikiPage, today: str) -> tuple[str, int, int]:
    """Return ``(markdown, word_count, math_blocks_count)``."""
    body = build_markdown(page, today)
    wc = word_count(body)
    if wc < MIN_WORDS:
        raise WikiFetchError(
            f"{page.topic}: extracted body is only {wc} words "
            f"(<{MIN_WORDS}); cannot publish."
        )
    frontmatter = build_frontmatter(page, today, wc)
    markdown = frontmatter + body
    if "TODO:" in markdown:
        raise WikiFetchError(f"{page.topic}: body contains literal 'TODO:'")
    return markdown, wc, count_math_blocks(markdown)


# --------------------------------------------------------------------------- #
# Pipeline
# --------------------------------------------------------------------------- #
def _parse_extract_response(data: dict, topic: str) -> tuple[str, str]:
    pages = (data or {}).get("query", {}).get("pages", {})
    if not pages:
        raise WikiFetchError(f"{topic}: no pages in API response")
    page = next(iter(pages.values()))
    if "missing" in page:
        raise WikiFetchError(f"{topic}: Wikipedia article missing")
    title = page.get("title") or topic.replace("_", " ")
    extract = page.get("extract") or ""
    return title, extract


_HTML_HEADING_RE = re.compile(
    r'<h([23])[^>]*>\s*(?:<span[^>]*>\s*)?(.*?)\s*(?:</span>\s*)?</h\1>',
    re.DOTALL | re.IGNORECASE,
)


def html_to_sectioned_extract(html: str) -> str:
    """Convert Wikipedia parse-HTML to a plaintext extract with ``== Heading ==``
    markers, matching ``explaintext`` output."""
    if not html:
        return ""
    # Drop references, tables of contents, infobox gunk.
    html = re.sub(r"<style[^>]*>.*?</style>", "", html, flags=re.DOTALL | re.IGNORECASE)
    html = re.sub(r"<script[^>]*>.*?</script>", "", html, flags=re.DOTALL | re.IGNORECASE)
    html = re.sub(r'<table[^>]*class="[^"]*(infobox|navbox|sidebar|metadata)[^"]*"[^>]*>.*?</table>',
                  "", html, flags=re.DOTALL | re.IGNORECASE)
    html = re.sub(r"<sup[^>]*class=\"reference\"[^>]*>.*?</sup>",
                  "", html, flags=re.DOTALL | re.IGNORECASE)
    # Inline <math> -> temp marker we can restore later.
    # We keep <math> tags intact; the caller will run convert_math_tags.
    # Split on h2/h3 so we can prepend "== Heading ==" markers.
    pieces: list[str] = []
    last = 0
    for m in _HTML_HEADING_RE.finditer(html):
        before = html[last:m.start()]
        pieces.append(before)
        level = int(m.group(1))
        heading_text = _strip_html(m.group(2)).strip()
        if heading_text.lower() in {"references", "external links", "see also",
                                    "further reading", "notes", "bibliography"}:
            heading_text = f"__SKIP__{heading_text}"
        marks = "=" * (level)
        pieces.append(f"\n\n{marks} {heading_text} {marks}\n\n")
        last = m.end()
    pieces.append(html[last:])
    merged = "".join(pieces)
    # Convert paragraphs to double newlines.
    merged = re.sub(r"</p>", "\n\n", merged, flags=re.IGNORECASE)
    merged = re.sub(r"<br\s*/?>", "\n", merged, flags=re.IGNORECASE)
    merged = re.sub(r"<li[^>]*>", "\n- ", merged, flags=re.IGNORECASE)
    merged = re.sub(r"</li>", "", merged, flags=re.IGNORECASE)
    # Strip remaining tags, keeping <math>…</math> text intact.
    placeholder = {}

    def _ph(m: re.Match[str]) -> str:
        key = f"@@MATH{len(placeholder)}@@"
        placeholder[key] = m.group(0)
        return key

    merged = _MATH_RE.sub(_ph, merged)
    text = _html_unescape(_TAG_RE.sub("", merged))
    for k, v in placeholder.items():
        text = text.replace(k, v)
    # Drop skipped sections.
    text = re.sub(r"={2,}\s*__SKIP__.*", "", text)
    # Strip Wikipedia UI noise.
    text = re.sub(r"\[\s*edit\s*\]", "", text, flags=re.IGNORECASE)
    text = re.sub(r"\[\s*hide\s*\]", "", text, flags=re.IGNORECASE)
    # Collapse runs of blank lines.
    text = re.sub(r"\n{3,}", "\n\n", text)
    return text.strip()


def build_page(topic: str, cache_dir: Path = CACHE_DIR) -> WikiPage:
    data = fetch_extract(topic, cache_dir=cache_dir)
    title, extract = _parse_extract_response(data, topic)
    html = fetch_html(topic, cache_dir=cache_dir)
    # Fall back to HTML-derived extract when the API extract is thin.
    if word_count(extract) < 600 and html:
        rich = html_to_sectioned_extract(html)
        if word_count(rich) > word_count(extract):
            extract = rich
    url = f"https://en.wikipedia.org/wiki/{urllib.parse.quote(topic)}"
    return WikiPage(topic=topic, title=title, extract=extract, html=html, url=url)


def run(topics: Iterable[str], *, rate_limit: float = RATE_LIMIT_SECONDS) -> list[dict]:
    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    today = _dt.date.today().isoformat()
    summary: list[dict] = []
    first = True
    for topic in topics:
        topic = topic.strip()
        if not topic:
            continue
        if not first:
            time.sleep(rate_limit)
        first = False
        page = build_page(topic)
        markdown, wc, math_n = render_page(page, today)
        out_path = OUTPUT_DIR / f"wiki-{page.slug}.md"
        out_path.write_text(markdown, encoding="utf-8")
        summary.append({
            "topic": topic,
            "title": page.title,
            "path": str(out_path),
            "word_count": wc,
            "math_blocks": math_n,
        })
    return summary


def _print_summary(rows: list[dict]) -> None:
    if not rows:
        print("no pages generated", file=sys.stderr)
        return
    width = max(len(r["topic"]) for r in rows)
    print(f"\n{'Topic':<{width}}  {'Words':>6}  {'Math':>5}")
    print("-" * (width + 15))
    for r in rows:
        print(f"{r['topic']:<{width}}  {r['word_count']:>6}  {r['math_blocks']:>5}")
    wcs = [r["word_count"] for r in rows]
    print("-" * (width + 15))
    print(
        f"count={len(rows)}  "
        f"avg={sum(wcs)//len(wcs)}  "
        f"min={min(wcs)}  max={max(wcs)}"
    )


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--topics", type=Path, default=TOPICS_FILE,
                    help="Path to topics.txt (one per line).")
    ap.add_argument("--rate-limit", type=float, default=RATE_LIMIT_SECONDS,
                    help="Seconds to sleep between requests (default: 1.0).")
    args = ap.parse_args(argv)

    topics = [t for t in args.topics.read_text().splitlines() if t.strip()]
    rows = run(topics, rate_limit=args.rate_limit)
    _print_summary(rows)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
