#!/usr/bin/env python3
"""Unit tests for the math ingest adapter + crawler cache behaviour."""

from __future__ import annotations

import sys
from pathlib import Path
from unittest.mock import MagicMock

import pytest

# Make sibling modules importable when pytest is run from repo root.
HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import adapter  # type: ignore  # noqa: E402
import mathworld_crawl  # type: ignore  # noqa: E402


# ---------------------------------------------------------------------------
# Adapter tests
# ---------------------------------------------------------------------------

def test_adapter_preserves_latex_verbatim(tmp_path: Path) -> None:
    body = (
        "Some intro text.\n\n"
        "$$\\int_0^1 x^2 \\, dx = \\frac{1}{3}$$\n\n"
        "Some trailing text."
    )
    page = adapter.ParsedPage(
        slug="roundtrip",
        title="Roundtrip",
        body=body,
        equations=adapter.extract_equations(body),
        source_url="https://example.test/roundtrip",
        source_type="mathworld",
    )
    rendered = adapter.render_page(page)
    # The $$...$$ block must survive unchanged in the rendered markdown.
    assert "$$\\int_0^1 x^2 \\, dx = \\frac{1}{3}$$" in rendered


def test_adapter_extracts_srepr_for_parseable_expressions() -> None:
    body = "$$x^2 + 2x + 1$$"
    eqs = adapter.extract_equations(body)
    assert len(eqs) == 1
    srepr = eqs[0].srepr
    # Exact srepr wording varies slightly across sympy versions, so we assert
    # on the load-bearing structural tokens we promised in the runbook.
    assert srepr is not None, "x^2 + 2x + 1 should parse to a sympy expression"
    assert "Add(" in srepr
    assert "Pow(" in srepr
    assert "Symbol('x')" in srepr
    assert "Integer(2)" in srepr
    # And render_page should embed it in the frontmatter.
    page = adapter.ParsedPage(
        slug="quad",
        title="Quad",
        body=body,
        equations=eqs,
        source_url="https://example.test/quad",
        source_type="mathworld",
    )
    rendered = adapter.render_page(page)
    assert "canonical_srepr:" in rendered
    assert "Add(" in rendered


def test_adapter_handles_unparseable_latex_gracefully() -> None:
    body = "$$\\this is not latex {{{$$"
    # Must not raise.
    eqs = adapter.extract_equations(body)
    assert len(eqs) == 1
    assert eqs[0].srepr is None
    page = adapter.ParsedPage(
        slug="broken",
        title="Broken",
        body=body,
        equations=eqs,
        source_url="https://example.test/broken",
        source_type="mathworld",
    )
    rendered = adapter.render_page(page)
    assert "canonical_srepr: null" in rendered


# ---------------------------------------------------------------------------
# Crawler cache test (mocked HTTP)
# ---------------------------------------------------------------------------

def test_mathworld_crawler_respects_cache(tmp_path: Path) -> None:
    cache_dir = tmp_path / "mathworld"
    cache_dir.mkdir()

    # Mock session: counts all GETs.
    session = MagicMock()
    session.headers = {}
    fake_resp = MagicMock()
    fake_resp.status_code = 200
    fake_resp.content = b"<html><title>Manifold -- from Wolfram MathWorld</title><body>ok</body></html>"
    session.get.return_value = fake_resp

    # Prebuilt robot parser that allows everything (avoids robots.txt call).
    import urllib.robotparser as rp_mod
    rp = rp_mod.RobotFileParser()
    rp.parse(["User-agent: *", "Allow: /"])

    # First run: fetches once.
    stats1 = mathworld_crawl.crawl(
        topics=["Manifold"],
        cache_dir=cache_dir,
        session=session,
        robot_parser=rp,
    )
    assert stats1["fetched"] == 1
    assert stats1["cached"] == 0
    first_call_count = session.get.call_count
    assert first_call_count == 1

    # Second run with identical topic: MUST make zero HTTP calls.
    stats2 = mathworld_crawl.crawl(
        topics=["Manifold"],
        cache_dir=cache_dir,
        session=session,
        robot_parser=rp,
    )
    assert stats2["cached"] == 1
    assert stats2["fetched"] == 0
    assert session.get.call_count == first_call_count, (
        "cache hit must not trigger any HTTP GET"
    )


if __name__ == "__main__":
    sys.exit(pytest.main([__file__, "-v"]))
