"""Unit tests for ``scripts.wiki_math.fetch``.

Run with:

    python3 -m pytest scripts/wiki_math/test_fetch.py -q
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

import pytest

# Make ``scripts/wiki_math/fetch.py`` importable without installing the repo.
sys.path.insert(0, str(Path(__file__).resolve().parent))

import fetch as wiki  # noqa: E402


# --------------------------------------------------------------------------- #
# Fixtures
# --------------------------------------------------------------------------- #
_EXTRACT_BODY = (
    "Optimal transport is the mathematical theory that studies the most "
    "efficient way to move mass from a source distribution to a target "
    "distribution. The problem was originally posed by Gaspard Monge in "
    "1781 as the 'minimum-cost mass transfer' problem.\n\n"
    "A relaxation due to Leonid Kantorovich allows mass to be split "
    "between destinations. The resulting linear program has a dual "
    "interpretation as a price-matching game.\n\n"
    "== Definition ==\n"
    "Given probability measures {\\displaystyle \\mu } and {\\displaystyle "
    "\\nu } on metric spaces X and Y, the Monge problem seeks a measurable "
    "map T: X -> Y pushing forward mu to nu that minimises the cost "
    "integral against a cost function c(x, y). In the Kantorovich "
    "relaxation we instead minimise over joint couplings.\n\n"
    "The cost functional is an affine function of the coupling and the "
    "minimum is attained when the marginals constraint is enforced. "
    "This produces a well-posed linear program on the simplex of "
    "couplings.\n\n"
    "== Properties ==\n"
    "The Kantorovich dual identifies the optimal cost with a supremum "
    "over 1-Lipschitz potentials when the cost is a metric. For squared "
    "Euclidean cost Brenier's theorem shows that the optimal map is the "
    "gradient of a convex potential. These dualities underpin numerical "
    "schemes such as Sinkhorn iteration and entropic regularisation.\n\n"
    "Solutions are unique under mild assumptions on the source measure, "
    "and regularity theory of Caffarelli gives interior smoothness of "
    "the Brenier potential on convex domains.\n\n"
    "== Applications ==\n"
    "Optimal transport underlies Wasserstein distances, used in machine "
    "learning to compare distributions, in image processing for colour "
    "transfer, and in economics for resource allocation. It also gives "
    "a geometric view of diffusion equations via the Jordan-Kinderlehrer-"
    "Otto gradient flow and of curvature in metric measure spaces via "
    "the Lott-Sturm-Villani synthetic Ricci bound.\n"
)

_EXTRACT_JSON = {
    "query": {
        "pages": {
            "12345": {
                "title": "Optimal transport",
                "extract": _EXTRACT_BODY,
            }
        }
    }
}

_HTML_FIXTURE = (
    "<p>Optimal transport studies efficient mass transfer.</p>"
    '<p>The Monge formulation minimises <math display="inline">'
    '<annotation encoding="application/x-tex">c(x, T(x))</annotation>'
    "</math> over transport maps.</p>"
    '<p>A trivial identity: <math><annotation encoding="application/x-tex">'
    "E=mc^2</annotation></math>.</p>"
)


@pytest.fixture()
def mocked_wiki(monkeypatch, tmp_path):
    cache = tmp_path / "cache"
    cache.mkdir()

    def _fake_extract(topic, cache_dir=cache):
        return _EXTRACT_JSON

    def _fake_html(topic, cache_dir=cache):
        return _HTML_FIXTURE

    monkeypatch.setattr(wiki, "CACHE_DIR", cache)
    monkeypatch.setattr(wiki, "fetch_extract", _fake_extract)
    monkeypatch.setattr(wiki, "fetch_html", _fake_html)
    return cache


# --------------------------------------------------------------------------- #
# Tests
# --------------------------------------------------------------------------- #
def test_topic_to_slug():
    assert wiki.topic_to_slug("Manifold") == "manifold"
    assert wiki.topic_to_slug("Lie_group") == "lie-group"
    assert wiki.topic_to_slug("Measure_(mathematics)") == "measure-mathematics"
    assert wiki.topic_to_slug("Itô_calculus").startswith("it")


def test_parse_extract_sections_splits_headings():
    sections = wiki.parse_extract_sections(_EXTRACT_BODY)
    headings = [h for _, h, _ in sections if h]
    assert "Definition" in headings
    assert "Properties" in headings
    assert "Applications" in headings


def test_extract_math_blocks_pulls_tex():
    blocks = wiki.extract_math_blocks(_HTML_FIXTURE)
    assert "c(x, T(x))" in blocks
    assert "E=mc^2" in blocks


def test_convert_math_tags_inline_vs_display():
    inline = wiki.convert_math_tags("<math>E=mc^2</math>")
    assert inline == "$E=mc^2$"
    display = wiki.convert_math_tags(
        "<math>\\begin{align} a &= b \\\\ c &= d \\end{align}</math>"
    )
    assert display.startswith("$$")
    assert display.rstrip().endswith("$$")


def test_build_page_and_render(mocked_wiki):
    page = wiki.build_page("Optimal_transport")
    assert page.title == "Optimal transport"
    assert "Kantorovich" in page.extract
    markdown, wc, math_n = wiki.render_page(page, today="2026-04-17")
    assert markdown.startswith("---\n")
    assert "title: Optimal transport" in markdown
    assert "domain: math" in markdown
    assert "source: wikipedia" in markdown
    assert "source_url: https://en.wikipedia.org/wiki/Optimal_transport" in markdown
    assert f"word_count: {wc}" in markdown
    assert wc >= 300
    assert math_n >= 1
    # LaTeX preservation:
    assert "$E=mc^2$" in markdown
    assert "c(x, T(x))" in markdown
    # No stub markers.
    assert "TODO:" not in markdown


def test_render_page_rejects_short_bodies(mocked_wiki, monkeypatch):
    short = wiki.WikiPage(
        topic="Tiny", title="Tiny", extract="A very short stub article.",
        html="", url="https://en.wikipedia.org/wiki/Tiny",
    )
    with pytest.raises(wiki.WikiFetchError) as exc:
        wiki.render_page(short, today="2026-04-17")
    assert "300" in str(exc.value)


def test_run_writes_file(tmp_path, monkeypatch, mocked_wiki):
    out_dir = tmp_path / "02-KB-main" / "math"
    monkeypatch.setattr(wiki, "OUTPUT_DIR", out_dir)
    rows = wiki.run(["Optimal_transport"], rate_limit=0.0)
    assert len(rows) == 1
    path = Path(rows[0]["path"])
    assert path.exists()
    text = path.read_text(encoding="utf-8")
    assert text.startswith("---\n")
    assert "source: wikipedia" in text
    assert rows[0]["word_count"] >= 300


def test_cache_roundtrip(tmp_path):
    cache = tmp_path / "cache"
    cache.mkdir()
    fake = {"query": {"pages": {"1": {"title": "Foo", "extract": "bar"}}}}
    cache_file = cache / f"{wiki.topic_to_slug('Foo')}.extract.json"
    cache_file.write_text(json.dumps(fake), encoding="utf-8")
    data = wiki.fetch_extract("Foo", cache_dir=cache)
    assert data == fake


def test_strip_displaystyle_preserves_nested_braces():
    # Banach-space fixture: the naive ``[^}]*`` regex stripped at the first
    # ``}`` (inside ``_{p}``), mangling the LaTeX. The balanced-brace walker
    # must unwrap ``{\displaystyle …}`` while keeping ``\|f\|_{p}`` intact.
    inp = "$${\\displaystyle \\|f\\|_{p}}$$"
    out = wiki._strip_displaystyle(inp)
    assert "\\|f\\|_{p}" in out, f"expected inner TeX preserved, got: {out!r}"
    assert "\\displaystyle" not in out
    # Surrounding ``$$`` delimiters still intact.
    assert out.startswith("$$") and out.endswith("$$")


def test_strip_displaystyle_handles_deeply_nested():
    inp = "before {\\displaystyle a_{i_{j}}^{k}} after"
    out = wiki._strip_displaystyle(inp)
    assert out == "before a_{i_{j}}^{k} after"


def test_strip_displaystyle_passthrough_without_marker():
    # No \displaystyle → identity.
    assert wiki._strip_displaystyle("plain text {x}") == "plain text {x}"
