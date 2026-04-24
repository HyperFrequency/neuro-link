#!/usr/bin/env bash
# Stub for pkg/lambda/build.sh — scaffold only.
# Phase 4 U9 (run 20260424-hf-monorepo-deploy-278fae) replaces this with a real builder.
# Exits 2 so unattended `make all` does not silently pass with an empty proof.

set -euo pipefail
VERSION="${1:-0.0.0}"
PROOF_DIR="${PROOF_DIR:-$(git rev-parse --show-toplevel 2>/dev/null)/pkg/.proof}"
mkdir -p "$PROOF_DIR"

cat > "$PROOF_DIR/lambda.ready.json" <<PROOF
{
  "target": "lambda",
  "state": "stub",
  "reason": "pkg/lambda/build.sh is scaffold-only; real implementation lands in run 20260424-hf-monorepo-deploy-278fae Phase 4 U9.",
  "version": "$VERSION",
  "artifact": null,
  "sha256": null,
  "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}
PROOF

echo "[lambda] stub emitted $PROOF_DIR/lambda.ready.json"
echo "[lambda] NOT shippable yet — Phase 4 U9 required"
exit 2
