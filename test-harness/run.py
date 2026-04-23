#!/usr/bin/env python3
"""Entry point for the neuro-link test harness.

Usage:
  python run.py --scenario all [--runtime docker-compose|host|installed] [--target TARGET]
  python run.py --scenario 03_pine_v6_diagnostics --runtime host
  python run.py --list
"""
from __future__ import annotations

import argparse
import json
import subprocess
import sys
from datetime import datetime, timezone
from importlib import import_module
from pathlib import Path

HARNESS = Path(__file__).resolve().parent
SCENARIOS = HARNESS / "scenarios"
REPORTS = HARNESS / "reports"
STATE = HARNESS / "state"


def list_scenarios() -> list[str]:
    return sorted(p.stem for p in SCENARIOS.glob("*.py") if not p.name.startswith("_"))


def run_scenario(name: str, runtime: str, target: str | None) -> dict:
    start = datetime.now(timezone.utc)
    sys.path.insert(0, str(SCENARIOS))
    try:
        mod = import_module(name)
        result = mod.run(runtime=runtime, target=target)
    except Exception as e:
        result = {"result": "FAIL", "error": f"{type(e).__name__}: {e}"}
    finally:
        sys.path.pop(0)
    end = datetime.now(timezone.utc)
    result["name"] = name
    result["runtime"] = runtime
    result["target"] = target
    result["start"] = start.isoformat()
    result["end"] = end.isoformat()
    result["duration_s"] = (end - start).total_seconds()
    return result


def write_report(result: dict) -> Path:
    REPORTS.mkdir(parents=True, exist_ok=True)
    ts = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    path = REPORTS / f"{ts}-{result['name']}.md"
    body = [
        f"# {result['name']} — {result['start']}",
        f"**Runtime:** {result['runtime']}",
        f"**Target:** {result.get('target') or 'N/A'}",
        f"**Duration:** {result['duration_s']:.2f}s",
        f"**Result:** {result['result']}",
        "",
        "## stdout (head 500 lines)",
        result.get("stdout_head", "(empty)"),
        "",
        "## stderr (head 500 lines)",
        result.get("stderr_head", "(empty)"),
        "",
        "## assertions",
    ]
    for a in result.get("assertions", []):
        body.append(f"- **{a['name']}**: {a['result']} — {a.get('evidence', '')}")
    if result.get("error"):
        body += ["", "## error", f"```\n{result['error']}\n```"]
    path.write_text("\n".join(body))

    STATE.mkdir(parents=True, exist_ok=True)
    hist = STATE / "score_history.jsonl"
    with hist.open("a") as f:
        f.write(json.dumps({"ts": result["end"], "scenario": result["name"],
                            "runtime": result["runtime"], "result": result["result"],
                            "duration_s": result["duration_s"]}) + "\n")
    return path


def main() -> int:
    p = argparse.ArgumentParser()
    p.add_argument("--scenario", default="all", help="scenario name or 'all'")
    p.add_argument("--runtime", default="host",
                   choices=["host", "docker-compose", "installed"])
    p.add_argument("--target", default=None,
                   help="target id (macos-arm64, linux-x86_64, ...) when runtime=installed")
    p.add_argument("--list", action="store_true")
    args = p.parse_args()

    available = list_scenarios()
    if args.list:
        for s in available:
            print(s)
        return 0

    if args.scenario == "all":
        to_run = available
    elif args.scenario in available:
        to_run = [args.scenario]
    else:
        print(f"unknown scenario: {args.scenario}", file=sys.stderr)
        print(f"available: {', '.join(available)}", file=sys.stderr)
        return 2

    failures = 0
    for name in to_run:
        result = run_scenario(name, args.runtime, args.target)
        report_path = write_report(result)
        status = "PASS" if result["result"] == "PASS" else "FAIL"
        print(f"[{status}] {name:<40}  {result['duration_s']:>6.2f}s  {report_path}")
        if status == "FAIL":
            failures += 1

    return 0 if failures == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
