#!/usr/bin/env bash
# pkg/cloud/modal/deploy.sh — Modal deploy with cred-aware fork-handoff.
# Phase 5b B2-10 (fork decision F2 — creds absent path).
#
# Behavior:
#   - If MODAL_TOKEN_ID + MODAL_TOKEN_SECRET are SET and `modal` CLI is present,
#     deploys pkg/cloud/modal/app.py and emits state:"ready" proof.
#   - If either creds or the CLI is missing, emits state:"skipped_no_creds"
#     proof (honest state, not a lie) and exits 0 so `make all` doesn't fail
#     the whole bundle. The aggregate proof then declines to stamp green.

set -euo pipefail

VERSION="${1:-0.0.0-dev}"
ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
PROOF_DIR="${PROOF_DIR:-${ROOT}/pkg/.proof}"
mkdir -p "${PROOF_DIR}"

# Cred + tooling probe (never echo the value, only its presence)
HAVE_TOKEN_ID=$([ -n "${MODAL_TOKEN_ID:-}" ] && echo true || echo false)
HAVE_TOKEN_SECRET=$([ -n "${MODAL_TOKEN_SECRET:-}" ] && echo true || echo false)
HAVE_TOKEN_FILE=$([ -f "${HOME}/.modal.toml" ] && echo true || echo false)
HAVE_CLI=$(command -v modal >/dev/null 2>&1 && echo true || echo false)

CREDS_OK=false
if { [ "${HAVE_TOKEN_ID}" = "true" ] && [ "${HAVE_TOKEN_SECRET}" = "true" ]; } \
   || [ "${HAVE_TOKEN_FILE}" = "true" ]; then
  CREDS_OK=true
fi

if [ "${CREDS_OK}" = "false" ] || [ "${HAVE_CLI}" = "false" ]; then
  REASON="cloud creds unavailable (MODAL_TOKEN_ID/SECRET unset, ~/.modal.toml missing, or modal CLI not installed)"
  cat > "${PROOF_DIR}/modal.ready.json" <<PROOF
{
  "target": "modal",
  "state": "skipped_no_creds",
  "version": "${VERSION}",
  "artifact": null,
  "sha256": null,
  "target_arch": "deferred",
  "reason": "${REASON}",
  "creds_probe": {
    "MODAL_TOKEN_ID_set": ${HAVE_TOKEN_ID},
    "MODAL_TOKEN_SECRET_set": ${HAVE_TOKEN_SECRET},
    "token_file_exists": ${HAVE_TOKEN_FILE},
    "cli_present": ${HAVE_CLI}
  },
  "fork_decision": "fork-F2c — CHOSEN: skipped_no_creds honest state; ALT: prompt user for creds via open-questions queue",
  "next_action": "set MODAL_TOKEN_ID + MODAL_TOKEN_SECRET (or run 'modal token new') and re-run 'make cloud-modal'",
  "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}
PROOF
  echo "[modal] ⏭  skipped_no_creds — ${REASON}"
  exit 0
fi

# Real deploy path (runs when creds are present)
APP_PATH="${ROOT}/pkg/cloud/modal/app.py"
if [ ! -f "${APP_PATH}" ]; then
  echo "[modal] HALT: ${APP_PATH} missing" >&2
  exit 1
fi

DEPLOY_LOG="$(mktemp -t modal-deploy.XXXXXX)"
trap 'rm -f "${DEPLOY_LOG}"' EXIT

DEPLOY_EXIT=0
modal deploy "${APP_PATH}" 2>&1 | tee "${DEPLOY_LOG}" || DEPLOY_EXIT=$?

# Extract app URL from deploy output
APP_URL=$(grep -oE 'https://[a-z0-9-]+\.modal\.run' "${DEPLOY_LOG}" | head -1 || echo "")
APP_NAME=$(grep -oE '✓ App deployed.*name=[a-z0-9-]+' "${DEPLOY_LOG}" | sed -E 's/.*name=//' | head -1 || basename "${APP_PATH}" .py)

# Smoke: hit /healthz if URL was returned
SMOKE_EXIT=1
SMOKE_OUT="no-url-captured"
if [ -n "${APP_URL}" ]; then
  SMOKE_OUT=$(curl -fsS -o /dev/null -w "%{http_code}" --max-time 30 "${APP_URL}/healthz" 2>&1 || echo "000")
  [ "${SMOKE_OUT}" = "200" ] && SMOKE_EXIT=0
fi

STATE="ready"
[ "${DEPLOY_EXIT}" -ne 0 ] && STATE="degraded"
[ "${SMOKE_EXIT}" -ne 0 ] && STATE="degraded"

cat > "${PROOF_DIR}/modal.ready.json" <<PROOF
{
  "target": "modal",
  "state": "${STATE}",
  "version": "${VERSION}",
  "artifact": "${APP_URL}",
  "sha256": null,
  "target_arch": "modal-container",
  "app_name": "${APP_NAME}",
  "deploy_exit": ${DEPLOY_EXIT},
  "smoke_tests": [
    {
      "name": "https_healthz",
      "url": "${APP_URL}/healthz",
      "http_code": "${SMOKE_OUT}",
      "exit_code": ${SMOKE_EXIT}
    }
  ],
  "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}
PROOF

echo "[modal] state=${STATE} url=${APP_URL}"
[ "${STATE}" = "ready" ] && exit 0 || exit 3
