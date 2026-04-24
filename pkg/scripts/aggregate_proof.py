#!/usr/bin/env python3
"""pkg/scripts/aggregate_proof.py — aggregate per-target .ready.json files.

Scaffold only. Phase 4 U9 (run 20260424-hf-monorepo-deploy-278fae) replaces this
with the real aggregator that enforces target_arch, sha256, smoke-exit, service
health, dashboard reachability.

Current behavior: walks PROOF_DIR, emits minimum schema so downstream consumers
don't break; exits 0 ONLY IF every expected target has a .ready.json with
state != "stub".
"""
from __future__ import annotations
import json
import sys
from datetime import datetime, timezone
from pathlib import Path

EXPECTED_TARGETS = [
    "macos-arm64",
    "linux-x86_64",
    "linux-aarch64",
    "docker",
    "modal",
    "lambda",
    "ray",
]


def discover_targets(root: Path) -> list[str]:
    """Union EXPECTED_TARGETS (required floor) with any *.ready.json found on disk.

    EXPECTED_TARGETS remains the contract: missing core targets still report
    "missing" state. New targets that land a .ready.json get picked up without
    a code edit. Added 2026-04-24 (run 20260424-hf-monorepo-deploy-278fae
    Round 1 swe-05).
    """
    if not root.exists():
        return list(EXPECTED_TARGETS)
    found = {p.name.removesuffix(".ready.json") for p in root.glob("*.ready.json")}
    # Exclude the aggregate's own output file so re-runs don't see it as a target.
    found.discard("ALL")
    return sorted(set(EXPECTED_TARGETS) | found)


def main(proof_dir: str) -> int:
    root = Path(proof_dir)
    root.mkdir(parents=True, exist_ok=True)
    report: dict[str, object] = {
        "run_id": "20260424-hf-monorepo-deploy-278fae",
        "generated_at": datetime.now(tz=timezone.utc).isoformat(),
        "targets": {},
        "state_counts": {},
        "green": False,
        "green_excluding_skipped": False,
    }
    all_green = True
    all_green_excl_skipped = True
    state_counts: dict[str, int] = {}
    for tgt in discover_targets(root):
        p = root / f"{tgt}.ready.json"
        if not p.exists():
            report["targets"][tgt] = {"state": "missing"}
            state_counts["missing"] = state_counts.get("missing", 0) + 1
            all_green = False
            all_green_excl_skipped = False
            continue
        try:
            data = json.loads(p.read_text())
        except Exception as exc:
            report["targets"][tgt] = {"state": "invalid", "error": str(exc)}
            state_counts["invalid"] = state_counts.get("invalid", 0) + 1
            all_green = False
            all_green_excl_skipped = False
            continue
        state = data.get("state", "unknown")
        state_counts[state] = state_counts.get(state, 0) + 1
        report["targets"][tgt] = data
        if state == "stub":
            all_green = False
            all_green_excl_skipped = False
        elif state == "skipped_no_creds":
            # Honest deferred per fork-F2 — breaks strict green but not
            # green_excluding_skipped. A cred'd re-run flips these to ready.
            all_green = False
        elif state == "ready":
            if not data.get("sha256") or not data.get("artifact"):
                all_green = False
                all_green_excl_skipped = False
        else:
            # degraded / unknown / anything else
            all_green = False
            all_green_excl_skipped = False
    report["state_counts"] = state_counts
    report["green"] = all_green
    report["green_excluding_skipped"] = all_green_excl_skipped
    json.dump(report, sys.stdout, indent=2, sort_keys=True)
    sys.stdout.write("\n")
    # Exit 0 iff green_excluding_skipped (deferred targets are acceptable per
    # fork-F2). Exit 3 only on missing/invalid/stub/degraded.
    return 0 if all_green_excl_skipped else 3


if __name__ == "__main__":
    sys.exit(main(sys.argv[1] if len(sys.argv) > 1 else "pkg/.proof"))
