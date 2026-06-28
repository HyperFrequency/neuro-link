#!/usr/bin/env python3
"""Spawn N grader subagents in parallel. Each reads the specified period's
logs independently, produces grade artifacts, and deposits them in
05-self-improvement-HITL/grades/<period>/<grader-id>/.

This script is invoked by /recursive-self-improvement. It assumes the
Claude Code CLI is on PATH (to fire off `claude` processes with the
consortium prompt).

Usage:
    spawn_consortium.py --period <YYYY-MM-DD..YYYY-MM-DD> [--graders 3]
"""
from __future__ import annotations

import argparse
import concurrent.futures as futures
import os
import subprocess
import sys
from datetime import datetime
from pathlib import Path
from typing import NamedTuple

REPO_ROOT = Path(os.environ.get("NLR_ROOT", Path(__file__).resolve().parents[4]))
GRADES_DIR = REPO_ROOT / "05-self-improvement-HITL" / "grades"
REFERENCES_DIR = Path(__file__).resolve().parent.parent / "references"


GRADER_PERSONAS = {
    "auditor": """You are a compliance auditor. Your job is to check that every action
recorded in the logs followed its skill's written protocol. Cite specific log
entries (format: [log:ISO-8601-timestamp]) and specific skill clauses. Be
unsympathetic to "the intent was good" excuses — if the protocol said X and
Y happened, flag the violation.""",

    "results": """You are a results-oriented reviewer. You care about outcomes, not
process. For each significant action, ask: did it produce the intended result?
If yes, you're happy regardless of whether every checkbox was ticked. If no,
explain why and what would have produced a better outcome. Cite log entries
for each claim.""",

    "red_team": """You are a red-team adversary. For every action that looked fine, look
harder for what might be subtly wrong: did it silently skip something important?
Did it create a problem that won't surface until later? Did it over-reach
beyond its scope? Be skeptical and specific. Cite log entries for each
concern.""",
}


class GraderSpec(NamedTuple):
    id: str
    persona: str
    period: str
    out_dir: Path


def spawn_grader(spec: GraderSpec) -> tuple[str, int, str]:
    spec.out_dir.mkdir(parents=True, exist_ok=True)
    prompt = f"""
{GRADER_PERSONAS[spec.persona]}

Grading period: {spec.period}.

Read agent memory from 04-Agent-Memory/logs.md and state/llm_logs/ for the
period. Produce three files in {spec.out_dir}:

1. orchestrator-agent-grading.md — evaluates the /neuro-link dispatcher on
   correct subcommand routing, tool-choice quality, output-contract adherence.
2. each-agent-grade.md — per-downstream-skill grade: completion rate, HITL
   protocol adherence, log hygiene.
3. learning-grade.md — did yesterday's changes produce their intended
   improvement? Find specific before/after log entries.

Format each file per the skill's references/consortium-protocol.md template.
Every claim needs at least one [log:timestamp] citation. Every proposal needs
target, change, rationale, risk, rollback.
"""

    # Invoke Claude Code headless
    try:
        result = subprocess.run(
            ["claude", "--headless", "-p", prompt, "--no-tools"],
            capture_output=True,
            text=True,
            timeout=600,
        )
        return spec.id, result.returncode, result.stdout[-2000:] if result.returncode else ""
    except subprocess.TimeoutExpired:
        return spec.id, 124, "timeout after 10 minutes"
    except FileNotFoundError:
        return spec.id, 127, "claude CLI not found on PATH"


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--period", required=True, help="YYYY-MM-DD or YYYY-MM-DD..YYYY-MM-DD")
    ap.add_argument("--graders", type=int, default=3)
    args = ap.parse_args()

    if args.graders < 3:
        print("WARN: consortium size < 3 defeats the purpose (single-grader bias)",
              file=sys.stderr)

    personas = list(GRADER_PERSONAS.keys())
    if args.graders > len(personas):
        print(f"WARN: requested {args.graders} graders but only {len(personas)} personas defined; cycling",
              file=sys.stderr)

    specs = []
    for i in range(args.graders):
        persona = personas[i % len(personas)]
        grader_id = f"{persona}-{i+1}" if args.graders > len(personas) else persona
        out_dir = GRADES_DIR / args.period / grader_id
        specs.append(GraderSpec(id=grader_id, persona=persona, period=args.period, out_dir=out_dir))

    print(f"Spawning {len(specs)} graders for period {args.period}")
    with futures.ThreadPoolExecutor(max_workers=len(specs)) as ex:
        results = list(ex.map(spawn_grader, specs))

    failed = [(gid, stderr) for gid, rc, stderr in results if rc != 0]
    if failed:
        print(f"\n{len(failed)} grader(s) failed:", file=sys.stderr)
        for gid, stderr in failed:
            print(f"  {gid}: {stderr}", file=sys.stderr)
        return 1

    print(f"All {len(specs)} graders completed. Grades at {GRADES_DIR / args.period}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
