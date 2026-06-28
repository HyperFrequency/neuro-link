#!/usr/bin/env python3
"""Cross-check Marker and MinerU markdown outputs for the same PDF.

Pairs $$...$$ blocks by position within the extracted body text. For each
pair, prefers the version that parses to a valid sympy srepr. Emits a
merged markdown plus a per-equation decisions sidecar.

Usage:
    math_crosscheck.py <marker.md> <mineru.md> -o <merged.md>
"""
from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import List, Tuple

DOUBLE_DOLLAR = re.compile(r"\$\$(.+?)\$\$", re.DOTALL)


def extract_equations(md: str) -> List[Tuple[int, str]]:
    """Return [(position, latex), ...] in document order."""
    return [(m.start(), m.group(1).strip()) for m in DOUBLE_DOLLAR.finditer(md)]


def try_parse(latex: str):
    """Return (srepr, ok) where ok is True iff parse succeeded."""
    try:
        from latex2sympy2_extended import latex2sympy  # type: ignore
        expr = latex2sympy(latex)
        return repr(expr), True
    except Exception:
        return None, False


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("marker_md", type=Path)
    ap.add_argument("mineru_md", type=Path)
    ap.add_argument("-o", "--output", type=Path, required=True)
    args = ap.parse_args()

    marker_src = args.marker_md.read_text()
    mineru_src = args.mineru_md.read_text()

    marker_eqs = extract_equations(marker_src)
    mineru_eqs = extract_equations(mineru_src)

    # Pair by index — assumes roughly ordered extraction. If counts differ,
    # pair up to the minimum and flag the tail.
    n = min(len(marker_eqs), len(mineru_eqs))
    if len(marker_eqs) != len(mineru_eqs):
        print(f"  WARN: marker has {len(marker_eqs)} eqs, mineru has {len(mineru_eqs)}",
              file=sys.stderr)

    decisions = []
    # Work on marker's markdown; replace equations that mineru does better.
    merged = marker_src
    # Iterate in reverse so slice replacements don't shift later indices.
    for i in range(n - 1, -1, -1):
        m_pos, m_latex = marker_eqs[i]
        u_pos, u_latex = mineru_eqs[i]

        m_srepr, m_ok = try_parse(m_latex)
        u_srepr, u_ok = try_parse(u_latex)

        if m_ok and u_ok:
            # Both parse — prefer shorter canonical form (usually cleaner).
            source = "marker" if len(m_srepr) <= len(u_srepr) else "mineru"
        elif u_ok and not m_ok:
            source = "mineru"
        elif m_ok and not u_ok:
            source = "marker"
        else:
            source = "marker"  # fallback

        decision = {
            "eq_index": i,
            "marker_latex": m_latex,
            "mineru_latex": u_latex,
            "marker_ok": m_ok,
            "mineru_ok": u_ok,
            "chosen": source,
        }
        decisions.append(decision)

        if source == "mineru":
            # Replace the ith equation in merged with mineru's version.
            # Work in reverse, so m_pos is still valid.
            start = m_pos
            end = m_pos + 2 + len(m_latex) + 2  # $$ + body + $$
            merged = merged[:start] + f"$${u_latex}$$" + merged[end:]

    args.output.write_text(merged)
    sidecar = args.output.with_suffix(".crosscheck.json")
    sidecar.write_text(json.dumps({
        "marker_src": str(args.marker_md),
        "mineru_src": str(args.mineru_md),
        "n_pairs": n,
        "marker_only_count": max(0, len(marker_eqs) - n),
        "mineru_only_count": max(0, len(mineru_eqs) - n),
        "decisions": list(reversed(decisions)),
    }, indent=2))

    print(f"Wrote {args.output}")
    print(f"Wrote {sidecar}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
