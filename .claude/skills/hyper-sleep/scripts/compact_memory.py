#!/usr/bin/env python3
"""Roll agent memory log entries older than 7 days into the consolidated
store. Keeps raw log append-only; just marks the cutoff with a "Archived
up to" line so future consolidation knows where to resume.

Usage:
    compact_memory.py [--days 7] [--dry-run]
"""
from __future__ import annotations

import argparse
import os
import re
import sys
from datetime import datetime, timedelta
from pathlib import Path

REPO_ROOT = Path(os.environ.get("NLR_ROOT", Path(__file__).resolve().parents[4]))
LOG_FILE = REPO_ROOT / "04-Agent-Memory" / "logs.md"
CONSOLIDATED_BY_AGENT = REPO_ROOT / "04-Agent-Memory" / "consolidated" / "agent"
CONSOLIDATED_BY_WORKFLOW = REPO_ROOT / "04-Agent-Memory" / "consolidated" / "workflow"

LINE_RE = re.compile(
    r"^- \[(?P<ts>[^\]]+)\] "
    r"action=(?P<action>\S+) "
    r"scope=(?P<scope>\S+) "
    r"outcome=(?P<outcome>\S+)"
    r"(?: skill=(?P<skill>\S+))?"
    r"(?: target=(?P<target>\S+))?"
)

ARCHIVED_RE = re.compile(r"^> Archived up to (?P<ts>.+)$")


def parse_ts(s: str) -> datetime | None:
    try:
        return datetime.fromisoformat(s.replace("Z", "+00:00"))
    except Exception:
        return None


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--days", type=int, default=7)
    ap.add_argument("--dry-run", action="store_true")
    args = ap.parse_args()

    if not LOG_FILE.exists():
        print(f"no log file at {LOG_FILE}")
        return 0

    cutoff = datetime.now().astimezone() - timedelta(days=args.days)
    lines = LOG_FILE.read_text().splitlines()

    # Find last "Archived up to" marker — we don't re-process what's been done.
    last_archived_at: datetime | None = None
    last_archived_idx = -1
    for i, line in enumerate(lines):
        m = ARCHIVED_RE.match(line)
        if m:
            last_archived_at = parse_ts(m.group("ts"))
            last_archived_idx = i

    # Group log entries by skill (for by-agent/) and by scope prefix (for by-workflow/)
    by_agent: dict[str, list[str]] = {}
    by_workflow: dict[str, list[str]] = {}
    rolled_count = 0
    new_cutoff_ts = None

    for i, line in enumerate(lines):
        if i <= last_archived_idx:
            continue
        m = LINE_RE.match(line)
        if not m:
            continue
        ts = parse_ts(m.group("ts"))
        if ts is None or ts >= cutoff:
            continue
        new_cutoff_ts = ts if new_cutoff_ts is None or ts > new_cutoff_ts else new_cutoff_ts
        skill = m.group("skill") or "unknown"
        scope_prefix = m.group("scope").split("/")[0] or "misc"
        by_agent.setdefault(skill, []).append(line)
        by_workflow.setdefault(scope_prefix, []).append(line)
        rolled_count += 1

    if rolled_count == 0:
        print(f"no entries older than {args.days}d to compact")
        return 0

    print(f"compacting {rolled_count} entries (older than {cutoff.date()})")
    if args.dry_run:
        print(f"  by_agent: {len(by_agent)} files")
        print(f"  by_workflow: {len(by_workflow)} files")
        return 0

    # Append to consolidated files
    CONSOLIDATED_BY_AGENT.mkdir(parents=True, exist_ok=True)
    CONSOLIDATED_BY_WORKFLOW.mkdir(parents=True, exist_ok=True)

    for skill, entries in by_agent.items():
        target = CONSOLIDATED_BY_AGENT / f"{skill}.md"
        with target.open("a") as f:
            f.write(f"\n## Compacted {datetime.now().date()}\n")
            f.write("\n".join(entries))
            f.write("\n")

    for workflow, entries in by_workflow.items():
        target = CONSOLIDATED_BY_WORKFLOW / f"{workflow}.md"
        with target.open("a") as f:
            f.write(f"\n## Compacted {datetime.now().date()}\n")
            f.write("\n".join(entries))
            f.write("\n")

    # Append archive marker to raw log — don't rewrite, append only
    with LOG_FILE.open("a") as f:
        f.write(f"\n> Archived up to {new_cutoff_ts.isoformat() if new_cutoff_ts else cutoff.isoformat()}\n")

    print(f"  -> {len(by_agent)} skill files, {len(by_workflow)} workflow files")
    print(f"  marker appended to {LOG_FILE}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
