#!/usr/bin/env python3
"""Compute canonical srepr for every $$...$$ block in a markdown file.

Never mutates the body. Writes a sidecar `.meta.json` with per-equation
expansion, parse result, and canonical srepr. Failed parses yield
`canonical_srepr: null` — never an exception.

Usage:
    canonicalize.py <markdown-file> [--macros <macro-table.yaml>]
"""
from __future__ import annotations

import argparse
import json
import re
import sys
import unicodedata
from pathlib import Path
from typing import Optional

DOUBLE_DOLLAR = re.compile(r"\$\$(.+?)\$\$", re.DOTALL)

DEFAULT_MACROS = {
    r"\R":        r"\mathbb{R}",
    r"\N":        r"\mathbb{N}",
    r"\Z":        r"\mathbb{Z}",
    r"\Q":        r"\mathbb{Q}",
    r"\C":        r"\mathbb{C}",
    r"\E":        r"\mathbb{E}",
    r"\dd":       r"\mathrm{d}",
    r"\Var":      r"\text{Var}",
    r"\Cov":      r"\text{Cov}",
    r"\Rplus":    r"\mathbb{R}_{+}",
    r"\partial_t": r"\frac{\partial}{\partial t}",
    r"\partial_x": r"\frac{\partial}{\partial x}",
}


def expand_macros(latex: str, macros: dict) -> str:
    """Very simple macro substitution. Leaves unknown macros intact.

    Expand only standalone macros (preceded and followed by non-alpha chars),
    so that `\R` doesn't eat `\Rank`.
    """
    out = latex
    for macro, replacement in macros.items():
        pattern = re.compile(re.escape(macro) + r"(?![a-zA-Z])")
        out = pattern.sub(replacement, out)
    return out


def try_srepr(latex: str) -> tuple[Optional[str], str]:
    """Return (srepr, stage). stage is 'ok' or 'expansion' | 'parse' | 'srepr'."""
    try:
        from latex2sympy2_extended import latex2sympy  # type: ignore
    except ImportError:
        return None, "expansion"
    try:
        expr = latex2sympy(latex)
    except Exception:
        return None, "parse"
    try:
        return repr(expr), "ok"
    except Exception:
        return None, "srepr"


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("markdown", type=Path)
    ap.add_argument("--macros", type=Path, default=None)
    args = ap.parse_args()

    macros = DEFAULT_MACROS.copy()
    if args.macros and args.macros.exists():
        try:
            import yaml
            macros.update(yaml.safe_load(args.macros.read_text()) or {})
        except ImportError:
            print("WARN: yaml not installed; using default macro table only",
                  file=sys.stderr)

    body = unicodedata.normalize("NFC", args.markdown.read_text())
    equations = []
    for i, m in enumerate(DOUBLE_DOLLAR.finditer(body)):
        raw_latex = m.group(1).strip()
        expanded = expand_macros(raw_latex, macros)
        srepr, stage = try_srepr(expanded)
        equations.append({
            "index": i,
            "position": m.start(),
            "latex": raw_latex,
            "expanded": expanded if expanded != raw_latex else None,
            "canonical_srepr": srepr,
            "failure_stage": None if stage == "ok" else stage,
            "confidence": 1.0 if stage == "ok" else 0.0,
        })

    sidecar_path = args.markdown.with_suffix(".meta.json")
    # Preserve existing sidecar fields if present
    existing = {}
    if sidecar_path.exists():
        try:
            existing = json.loads(sidecar_path.read_text())
        except Exception:
            pass
    existing["equations"] = equations
    sidecar_path.write_text(json.dumps(existing, indent=2))

    parsed = sum(1 for e in equations if e["canonical_srepr"])
    print(f"{args.markdown}: {parsed}/{len(equations)} equations canonicalized")
    print(f"Wrote sidecar: {sidecar_path}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
