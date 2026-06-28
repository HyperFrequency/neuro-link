#!/usr/bin/env python3
"""End-to-end arXiv ingestion. Tries ar5iv HTML → arXiv S3 LaTeX → Marker PDF
in that order. Writes to 01-raw/<sha256>-<slug>.md plus sidecar metadata.

Usage:
    ingest_arxiv.py <arxiv-id-or-url> [--force] [--skip-canonicalize]
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
from pathlib import Path
from typing import Optional

import requests

REPO_ROOT = Path(os.environ.get("NLR_ROOT", Path(__file__).resolve().parents[4]))
RAW_DIR = REPO_ROOT / "01-raw"
SORTED_DIR = REPO_ROOT / "01-sorted" / "arxiv"
MIRRORS_DIR = REPO_ROOT / "state" / "mirrors" / "arxiv-source"


def parse_arxiv_id(raw: str) -> str:
    """Extract the canonical id (e.g. 2512.09874) from any input form."""
    m = re.search(r"(\d{4}\.\d{4,5})(v\d+)?", raw)
    if not m:
        raise ValueError(f"Not a recognized arXiv id: {raw!r}")
    return m.group(1)


def fetch_ar5iv(arxiv_id: str) -> Optional[str]:
    url = f"https://ar5iv.labs.arxiv.org/html/{arxiv_id}"
    r = requests.get(url, timeout=30)
    if r.status_code == 404:
        return None
    r.raise_for_status()
    return r.text


def fetch_arxiv_metadata(arxiv_id: str) -> dict:
    """Fetch structured metadata from arXiv API (title, authors, abstract, cats)."""
    url = "https://export.arxiv.org/api/query"
    r = requests.get(url, params={"id_list": arxiv_id}, timeout=30)
    r.raise_for_status()
    # Minimal Atom parsing — we only need a few fields.
    from xml.etree import ElementTree as ET
    ns = {"a": "http://www.w3.org/2005/Atom", "x": "http://arxiv.org/schemas/atom"}
    root = ET.fromstring(r.text)
    entry = root.find("a:entry", ns)
    if entry is None:
        return {}
    return {
        "title": (entry.findtext("a:title", "", ns) or "").strip(),
        "authors": [a.findtext("a:name", "", ns).strip()
                    for a in entry.findall("a:author", ns)],
        "abstract": (entry.findtext("a:summary", "", ns) or "").strip(),
        "categories": [c.get("term") for c in entry.findall("a:category", ns)],
        "published": entry.findtext("a:published", "", ns),
        "doi": entry.findtext("x:doi", "", ns),
    }


def html_to_markdown(html: str) -> str:
    """Convert ar5iv HTML (with MathML) to markdown preserving $$...$$ blocks."""
    try:
        from markitdown import MarkItDown
    except ImportError:
        print("ERROR: pip install markitdown", file=sys.stderr)
        sys.exit(1)
    md = MarkItDown()
    # markitdown accepts a file-like or a string
    import io
    result = md.convert_stream(io.BytesIO(html.encode("utf-8")),
                               file_extension=".html")
    return result.text_content


def slugify(title: str) -> str:
    slug = re.sub(r"[^a-zA-Z0-9\s-]", "", title.lower())
    slug = re.sub(r"\s+", "-", slug).strip("-")
    return slug[:80] or "untitled"


def compute_sha256(s: str) -> str:
    return hashlib.sha256(s.encode("utf-8")).hexdigest()


def check_existing(sha: str) -> Optional[Path]:
    for p in RAW_DIR.glob(f"{sha}-*.md"):
        return p
    return None


def write_artifacts(sha: str, slug: str, markdown: str,
                    frontmatter: dict, arxiv_id: str) -> Path:
    RAW_DIR.mkdir(parents=True, exist_ok=True)
    SORTED_DIR.mkdir(parents=True, exist_ok=True)

    raw_path = RAW_DIR / f"{sha}-{slug}.md"
    sidecar_path = RAW_DIR / f"{sha}-{slug}.meta.json"

    # Compose frontmatter + body
    import yaml
    body = "---\n" + yaml.safe_dump(frontmatter, sort_keys=False) + "---\n\n" + markdown
    raw_path.write_text(body)

    sidecar_path.write_text(json.dumps({
        "doc_sha256": sha,
        "arxiv_id": arxiv_id,
        "ingested_at": os.popen("date -Iseconds").read().strip(),
        "pipeline": frontmatter.get("source", "arxiv"),
    }, indent=2))

    # Sorted symlink
    sorted_link = SORTED_DIR / f"{slug}.md"
    if sorted_link.exists() or sorted_link.is_symlink():
        sorted_link.unlink()
    sorted_link.symlink_to(raw_path.resolve())

    return raw_path


def try_ar5iv_path(arxiv_id: str, force: bool) -> Optional[Path]:
    print(f"[1/3] ar5iv HTML for {arxiv_id}...")
    html = fetch_ar5iv(arxiv_id)
    if html is None:
        print("  404 — ar5iv doesn't have this paper")
        return None

    sha = compute_sha256(html)
    existing = check_existing(sha)
    if existing and not force:
        print(f"  already ingested at {existing}")
        return existing

    print(f"  SHA256: {sha[:16]}...")
    markdown = html_to_markdown(html)
    meta = fetch_arxiv_metadata(arxiv_id)
    slug = slugify(meta.get("title", arxiv_id))
    frontmatter = {
        "title": meta.get("title", f"arXiv:{arxiv_id}"),
        "arxiv_id": arxiv_id,
        "authors": meta.get("authors", []),
        "categories": meta.get("categories", []),
        "doi": meta.get("doi") or None,
        "published": meta.get("published") or None,
        "abstract": meta.get("abstract", ""),
        "source": "ar5iv",
        "confidence": 0.9,
        "last_updated": os.popen("date +%Y-%m-%d").read().strip(),
        "open_questions": [],
    }
    return write_artifacts(sha, slug, markdown, frontmatter, arxiv_id)


def try_s3_path(arxiv_id: str, force: bool) -> Optional[Path]:
    print(f"[2/3] arXiv S3 LaTeX source for {arxiv_id}...")
    print("  NOTE: requires AWS credentials with requester-pays access")
    print("  (implementation deferred — see references/arxiv-ar5iv.md)")
    return None


def try_marker_path(arxiv_id: str, force: bool) -> Optional[Path]:
    print(f"[3/3] Marker on arXiv PDF for {arxiv_id}...")
    pdf_url = f"https://arxiv.org/pdf/{arxiv_id}.pdf"
    pdf_path = MIRRORS_DIR / f"{arxiv_id}.pdf"
    pdf_path.parent.mkdir(parents=True, exist_ok=True)
    r = requests.get(pdf_url, timeout=120)
    r.raise_for_status()
    pdf_path.write_bytes(r.content)
    print(f"  downloaded {len(r.content)} bytes to {pdf_path}")
    print(f"  invoking Marker via scripts/ingest_pdf_deep.sh")
    script = Path(__file__).parent / "ingest_pdf_deep.sh"
    if not script.exists():
        print("  ERROR: ingest_pdf_deep.sh not found", file=sys.stderr)
        return None
    subprocess.run(["bash", str(script), str(pdf_path)], check=True)
    # ingest_pdf_deep.sh writes to 01-raw; find the result
    for p in RAW_DIR.glob(f"*-{arxiv_id}*.md"):
        return p
    return None


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("source")
    ap.add_argument("--force", action="store_true")
    ap.add_argument("--skip-canonicalize", action="store_true")
    args = ap.parse_args()

    arxiv_id = parse_arxiv_id(args.source)
    print(f"arXiv id: {arxiv_id}")

    result = (
        try_ar5iv_path(arxiv_id, args.force)
        or try_s3_path(arxiv_id, args.force)
        or try_marker_path(arxiv_id, args.force)
    )

    if result is None:
        print("ERROR: all three paths failed", file=sys.stderr)
        return 1

    print(f"\nIngested: {result}")

    if not args.skip_canonicalize:
        canonicalize = Path(__file__).parent / "canonicalize.py"
        if canonicalize.exists():
            print(f"\nRunning canonicalization...")
            subprocess.run([sys.executable, str(canonicalize), str(result)], check=True)

    return 0


if __name__ == "__main__":
    sys.exit(main())
