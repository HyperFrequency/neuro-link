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
import os
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


def _discover_run_id(proof_dir: Path) -> str:
    """Resolve run_id from (in priority): NLR_RUN_ID env → parent run_id.txt
    → parent checkpoint.md `run_id:` key → 'unknown-run'. Falls back only when
    no source provides a value."""
    env = os.environ.get("NLR_RUN_ID")
    if env:
        return env
    # Walk up from proof_dir looking for run_id.txt or checkpoint.md
    p = proof_dir.resolve()
    for cand in [p, *p.parents]:
        rid_file = cand / "run_id.txt"
        if rid_file.is_file():
            rid = rid_file.read_text().strip()
            if rid:
                return rid
        chk = cand / "checkpoint.md"
        if chk.is_file():
            for line in chk.read_text().splitlines():
                if line.strip().startswith("run_id:"):
                    return line.split(":", 1)[1].strip()
    return "unknown-run"


def main(proof_dir: str) -> int:
    root = Path(proof_dir)
    root.mkdir(parents=True, exist_ok=True)
    run_id = _discover_run_id(root)
    report: dict[str, object] = {
        "run_id": run_id,
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
            # Gate-20 MM3 + Gate-21 NN2: different ready-state integrity
            # contracts for different target classes.
            #   INSTALL_COMPLETE (meta-proof from verify.py): no binary
            #     artifact exists, so state=ready alone is sufficient.
            #   Cloud targets (modal/lambda/ray deployments): artifact
            #     is a URL/ARN, not a file on disk — there is no local
            #     sha256 to verify against. Require artifact to be
            #     non-null, but don't require sha256.
            #   Builder targets (macos/linux-*/docker): artifact is a
            #     file path. Require BOTH sha256 and artifact to match
            #     what the ship bundle can integrity-check on landing.
            if tgt == "INSTALL_COMPLETE":
                pass  # state=ready alone is sufficient for the meta-proof
            elif tgt in ("modal", "lambda", "ray"):
                # Gate-22 OO1 fix: proof filenames are modal/lambda/ray
                # (from pkg/cloud/{modal,lambda,ray}/deploy.sh), not the
                # Make target names (cloud-modal/cloud-lambda/cloud-ray).
                # Earlier NN2 tuple had the wrong names and silently let
                # credentialed cloud deploys fall through to the
                # builder-path sha256 check, forcing green=false.
                if not data.get("artifact"):
                    all_green = False
                    all_green_excl_skipped = False
            elif not data.get("sha256") or not data.get("artifact"):
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
