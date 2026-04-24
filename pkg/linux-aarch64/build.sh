#!/usr/bin/env bash
# Stub for pkg/linux-aarch64/build.sh — scaffold only.
# Phase 4 U9 (run 20260424-hf-monorepo-deploy-278fae) replaces this with a real builder.
# Exits 2 so unattended `make all` does not silently pass with an empty proof.

set -euo pipefail
VERSION="${1:-0.0.0}"
PROOF_DIR="${PROOF_DIR:-$(git rev-parse --show-toplevel 2>/dev/null)/pkg/.proof}"
mkdir -p "$PROOF_DIR"

cat > "$PROOF_DIR/linux-aarch64.ready.json" <<PROOF
{
  "target": "linux-aarch64",
  "state": "stub",
  "reason": "pkg/linux-aarch64/build.sh is scaffold-only; real implementation lands in run 20260424-hf-monorepo-deploy-278fae Phase 4 U9.",
  "version": "$VERSION",
  "artifact": null,
  "sha256": null,
  "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}
PROOF

echo "[linux-aarch64] stub emitted $PROOF_DIR/linux-aarch64.ready.json"
echo "[linux-aarch64] NOT shippable yet — Phase 4 U9 required"
exit 2
