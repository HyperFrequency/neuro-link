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


def main(proof_dir: str) -> int:
    root = Path(proof_dir)
    root.mkdir(parents=True, exist_ok=True)
    report: dict[str, object] = {
        "run_id": "20260424-hf-monorepo-deploy-278fae",
        "generated_at": datetime.now(tz=timezone.utc).isoformat(),
        "targets": {},
        "green": False,
    }
    all_green = True
    for tgt in EXPECTED_TARGETS:
        p = root / f"{tgt}.ready.json"
        if not p.exists():
            report["targets"][tgt] = {"state": "missing"}
            all_green = False
            continue
        try:
            data = json.loads(p.read_text())
        except Exception as exc:
            report["targets"][tgt] = {"state": "invalid", "error": str(exc)}
            all_green = False
            continue
        report["targets"][tgt] = data
        if data.get("state") == "stub":
            all_green = False
        elif not data.get("sha256") or not data.get("artifact"):
            all_green = False
    report["green"] = all_green
    json.dump(report, sys.stdout, indent=2, sort_keys=True)
    sys.stdout.write("\n")
    return 0 if all_green else 3


if __name__ == "__main__":
    sys.exit(main(sys.argv[1] if len(sys.argv) > 1 else "pkg/.proof"))
