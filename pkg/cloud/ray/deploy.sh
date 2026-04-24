#!/usr/bin/env bash
# pkg/cloud/ray/deploy.sh — Ray cluster-up with AWS cred-aware fork.
# Phase 5b B2-10 (fork decision F2 — creds absent path).

set -euo pipefail

VERSION="${1:-0.0.0-dev}"
ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
PROOF_DIR="${PROOF_DIR:-${ROOT}/pkg/.proof}"
CLUSTER_YAML="${ROOT}/pkg/cloud/ray/cluster.yaml"
mkdir -p "${PROOF_DIR}"

HAVE_AWS_KEY=$([ -n "${AWS_ACCESS_KEY_ID:-}" ] && echo true || echo false)
HAVE_AWS_SECRET=$([ -n "${AWS_SECRET_ACCESS_KEY:-}" ] && echo true || echo false)
HAVE_AWS_FILE=$([ -f "${HOME}/.aws/credentials" ] && echo true || echo false)
HAVE_RAY_CLI=$(command -v ray >/dev/null 2>&1 && echo true || echo false)

AWS_OK=false
if { [ "${HAVE_AWS_KEY}" = "true" ] && [ "${HAVE_AWS_SECRET}" = "true" ]; } \
   || [ "${HAVE_AWS_FILE}" = "true" ]; then
  AWS_OK=true
fi

if [ "${AWS_OK}" = "false" ] || [ "${HAVE_RAY_CLI}" = "false" ] || [ ! -f "${CLUSTER_YAML}" ]; then
  REASON="AWS creds unavailable (${HAVE_AWS_FILE}/file or ${HAVE_AWS_KEY}+${HAVE_AWS_SECRET}/env) or ray CLI missing (${HAVE_RAY_CLI}) or cluster.yaml missing"
  cat > "${PROOF_DIR}/ray.ready.json" <<PROOF
{
  "target": "ray",
  "state": "skipped_no_creds",
  "version": "${VERSION}",
  "artifact": null,
  "sha256": null,
  "target_arch": "deferred",
  "reason": "${REASON}",
  "creds_probe": {
    "AWS_ACCESS_KEY_ID_set": ${HAVE_AWS_KEY},
    "AWS_SECRET_ACCESS_KEY_set": ${HAVE_AWS_SECRET},
    "aws_credentials_file_exists": ${HAVE_AWS_FILE},
    "ray_cli_present": ${HAVE_RAY_CLI}
  },
  "fork_decision": "fork-F2e — CHOSEN: skipped_no_creds honest state; ALT: launch minimal 1-head-node cluster after user OK",
  "next_action": "install ray via 'arch -arm64 uv pip install ray[default]' and set AWS creds; re-run 'make cloud-ray'",
  "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}
PROOF
  echo "[ray] ⏭  skipped_no_creds — ${REASON}"
  exit 0
fi

# Real cluster-up path
RAY_LOG="$(mktemp -t ray-up.XXXXXX)"
trap 'rm -f "${RAY_LOG}"' EXIT

UP_EXIT=0
ray up -y "${CLUSTER_YAML}" 2>&1 | tee "${RAY_LOG}" || UP_EXIT=$?

# Smoke: ray job submit a trivial "print(1)"
SUB_EXIT=0
ray job submit --working-dir /tmp -- python3 -c 'print(1)' >/dev/null 2>&1 || SUB_EXIT=$?

STATE="ready"
[ "${UP_EXIT}" -ne 0 ] && STATE="degraded"
[ "${SUB_EXIT}" -ne 0 ] && STATE="degraded"

cat > "${PROOF_DIR}/ray.ready.json" <<PROOF
{
  "target": "ray",
  "state": "${STATE}",
  "version": "${VERSION}",
  "artifact": "${CLUSTER_YAML}",
  "sha256": null,
  "target_arch": "ray-cluster-aws",
  "smoke_tests": [
    {"name": "ray_up", "exit_code": ${UP_EXIT}},
    {"name": "job_submit_print_1", "exit_code": ${SUB_EXIT}}
  ],
  "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}
PROOF

echo "[ray] state=${STATE}"
[ "${STATE}" = "ready" ] && exit 0 || exit 3
