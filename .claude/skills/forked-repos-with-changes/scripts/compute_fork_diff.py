#!/usr/bin/env python3
"""Compute functional (semantic) diff between a fork and its upstream.

Walks changed files (since last_merge_base), categorizes into new/modified/
deleted/cosmetic, and produces a summary-per-file by asking an LLM to
describe the functional delta.

Requires the `claude` CLI on PATH for the LLM step.

Usage:
    compute_fork_diff.py <fork-path> <upstream-url> [--since <sha>] [--out <dir>]
"""
from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path
from typing import Any

COSMETIC_RATIO = 0.02  # if <2% of lines changed AND no new symbols, treat as cosmetic


def run(cmd: list[str], cwd: Path | None = None, check: bool = True) -> str:
    out = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, check=check)
    return out.stdout


def categorize_change(fork_path: Path, rel: str, since_sha: str) -> str:
    """Return 'new' | 'modified' | 'deleted' | 'cosmetic'."""
    status = run(["git", "diff", "--name-status", f"{since_sha}..HEAD", "--", rel], cwd=fork_path).strip()
    if not status:
        return "cosmetic"
    tag = status.split()[0]
    if tag == "A":
        return "new"
    if tag == "D":
        return "deleted"
    if tag == "M":
        # Decide cosmetic vs modified
        numstat = run(["git", "diff", "--numstat", f"{since_sha}..HEAD", "--", rel], cwd=fork_path).strip()
        try:
            added, deleted, _ = numstat.split(None, 2)
            added, deleted = int(added), int(deleted)
        except ValueError:
            return "modified"
        # crude: if total change is tiny AND no import/def/class changes, cosmetic
        total = added + deleted
        full_size = len(run(["git", "show", f"HEAD:{rel}"], cwd=fork_path, check=False).splitlines())
        if full_size > 0 and total / full_size < COSMETIC_RATIO:
            return "cosmetic"
        return "modified"
    return "modified"


def llm_semantic_summary(upstream_content: str, fork_content: str, rel: str) -> str:
    prompt = f"""
You are diffing a fork against its upstream. Read both versions of the file `{rel}`.

Upstream:
```
{upstream_content[:30000]}
```

Fork:
```
{fork_content[:30000]}
```

Summarize in 2-3 sentences what the fork changes *functionally*. Ignore
formatting, imports, renames, or cosmetic diffs. Focus on:
- What new capability is added?
- What existing behavior is changed?
- What surface is removed?

If the diff is purely cosmetic, respond with "COSMETIC" and nothing else.
Output ONLY the summary — no preamble.
"""
    try:
        result = subprocess.run(
            ["claude", "--headless", "-p", prompt, "--no-tools"],
            capture_output=True, text=True, timeout=60,
        )
        return result.stdout.strip()
    except (subprocess.TimeoutExpired, FileNotFoundError) as e:
        return f"(LLM call failed: {e})"


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("fork_path", type=Path)
    ap.add_argument("upstream_url")
    ap.add_argument("--since", default=None, help="base SHA; defaults to merge-base with upstream/main")
    ap.add_argument("--out", type=Path, default=None)
    args = ap.parse_args()

    fork = args.fork_path.resolve()
    if not (fork / ".git").exists():
        print(f"ERROR: {fork} not a git repo", file=sys.stderr)
        return 1

    # Ensure upstream remote exists
    remotes = run(["git", "remote"], cwd=fork)
    if "upstream" not in remotes.split():
        run(["git", "remote", "add", "upstream", args.upstream_url], cwd=fork)
    run(["git", "fetch", "upstream"], cwd=fork)

    since_sha = args.since or run(["git", "merge-base", "HEAD", "upstream/main"], cwd=fork).strip()
    print(f"using base commit: {since_sha}")

    changed = run(["git", "diff", "--name-only", f"{since_sha}..HEAD"], cwd=fork).splitlines()
    changed = [f for f in changed if f and not f.startswith((".github/", "vendor/", "node_modules/"))]
    print(f"changed files: {len(changed)}")

    results: list[dict[str, Any]] = []
    for rel in changed:
        cat = categorize_change(fork, rel, since_sha)
        entry: dict[str, Any] = {"path": rel, "category": cat, "summary": ""}
        if cat == "cosmetic" or cat == "deleted":
            entry["summary"] = "" if cat == "cosmetic" else "(deleted in fork)"
        else:
            try:
                upstream_content = run(["git", "show", f"upstream/main:{rel}"], cwd=fork, check=False)
            except Exception:
                upstream_content = ""
            fork_content = run(["git", "show", f"HEAD:{rel}"], cwd=fork, check=False)
            summary = llm_semantic_summary(upstream_content, fork_content, rel)
            if summary.strip() == "COSMETIC":
                entry["category"] = "cosmetic"
                entry["summary"] = ""
            else:
                entry["summary"] = summary
        results.append(entry)
        print(f"  [{entry['category']:9s}] {rel}")

    out_dir = args.out or Path("/tmp/fork-diff")
    out_dir.mkdir(parents=True, exist_ok=True)
    (out_dir / "diff-summary.json").write_text(json.dumps({
        "fork_path": str(fork),
        "upstream_url": args.upstream_url,
        "base_sha": since_sha,
        "files": results,
    }, indent=2))
    print(f"\nwrote {out_dir/'diff-summary.json'}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
