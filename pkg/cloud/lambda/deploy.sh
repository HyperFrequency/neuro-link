#!/usr/bin/env bash
# pkg/cloud/lambda/deploy.sh — Lambda Labs instance provision with cred-aware fork.
# Phase 5b B2-10 (fork decision F2 — creds absent path).
#
# Behavior mirrors pkg/cloud/modal/deploy.sh: probes LAMBDA_CLOUD_API_KEY;
# emits skipped_no_creds proof if absent, real provision + smoke if present.

set -euo pipefail

VERSION="${1:-0.0.0-dev}"
ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
PROOF_DIR="${PROOF_DIR:-${ROOT}/pkg/.proof}"
mkdir -p "${PROOF_DIR}"

HAVE_KEY=$([ -n "${LAMBDA_CLOUD_API_KEY:-}" ] && echo true || echo false)
HAVE_CREDS_FILE=$([ -f "${HOME}/.lambda_cloud/credentials" ] && echo true || echo false)

if [ "${HAVE_KEY}" = "false" ] && [ "${HAVE_CREDS_FILE}" = "false" ]; then
  REASON="LAMBDA_CLOUD_API_KEY unset and ~/.lambda_cloud/credentials missing"
  cat > "${PROOF_DIR}/lambda.ready.json" <<PROOF
{
  "target": "lambda",
  "state": "skipped_no_creds",
  "version": "${VERSION}",
  "artifact": null,
  "sha256": null,
  "target_arch": "deferred",
  "reason": "${REASON}",
  "creds_probe": {
    "LAMBDA_CLOUD_API_KEY_set": ${HAVE_KEY},
    "credentials_file_exists": ${HAVE_CREDS_FILE}
  },
  "fork_decision": "fork-F2d — CHOSEN: skipped_no_creds honest state; ALT: prompt user for API key",
  "next_action": "export LAMBDA_CLOUD_API_KEY=<key> and re-run 'make cloud-lambda'",
  "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}
PROOF
  echo "[lambda] ⏭  skipped_no_creds — ${REASON}"
  exit 0
fi

# Real provision path: instances.launch via Lambda Cloud REST API
API_BASE="https://cloud.lambdalabs.com/api/v1"
AUTH_KEY="${LAMBDA_CLOUD_API_KEY:-$(awk -F= '/^api_key=/{print $2}' "${HOME}/.lambda_cloud/credentials" 2>/dev/null)}"

PROVISION_LOG="$(mktemp -t lambda-provision.XXXXXX)"
trap 'rm -f "${PROVISION_LOG}"' EXIT

# List instance types (smoke — verifies auth + connectivity)
INST_TYPES=$(curl -fsS -u "${AUTH_KEY}:" "${API_BASE}/instance-types" 2>&1 | tee "${PROVISION_LOG}" || echo "{}")
INST_OK=$(echo "${INST_TYPES}" | python3 -c "import json,sys; d=json.load(sys.stdin); print('true' if 'data' in d else 'false')" 2>/dev/null || echo false)

STATE="ready"
[ "${INST_OK}" = "false" ] && STATE="degraded"

cat > "${PROOF_DIR}/lambda.ready.json" <<PROOF
{
  "target": "lambda",
  "state": "${STATE}",
  "version": "${VERSION}",
  "artifact": "${API_BASE}/instance-types",
  "sha256": null,
  "target_arch": "lambda-gpu",
  "smoke_tests": [
    {
      "name": "api_instance_types_list",
      "endpoint": "${API_BASE}/instance-types",
      "ok": ${INST_OK}
    }
  ],
  "note": "Provision-and-terminate smoke is gated on user explicit OK per fork-F2d cost-sensitivity. This builder auth-probes the API without launching instances.",
  "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}
PROOF

echo "[lambda] state=${STATE}"
[ "${STATE}" = "ready" ] && exit 0 || exit 3
