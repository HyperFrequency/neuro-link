#!/usr/bin/env bash
# start_public_tunnel.sh — bring up Caddy + ngrok in front of TurboVault.
#
# Flow:
#   1. Load secrets/.env, require NLR_API_TOKEN.
#   2. Verify TurboVault is listening on :3001 (HTTP) and :3002 (WS); start if not.
#   3. Launch Caddy in the foreground (via trap-managed background PID).
#   4. Launch ngrok.
#   5. Print the public URL and a curl smoke-test command.
#   6. SIGINT -> kill both children cleanly.

set -euo pipefail

# ---------------------------------------------------------------------------
# Paths — resolved relative to the script so it works from any cwd.
# ---------------------------------------------------------------------------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
ENV_FILE="${REPO_ROOT}/secrets/.env"
CADDYFILE="${REPO_ROOT}/config/Caddyfile"
NGROK_CONFIG="${REPO_ROOT}/config/ngrok.yml"

TURBOVAULT_HTTP_PORT="${TURBOVAULT_HTTP_PORT:-3001}"
TURBOVAULT_WS_PORT="${TURBOVAULT_WS_PORT:-3002}"
CADDY_PORT="${CADDY_PORT:-8080}"

# ---------------------------------------------------------------------------
# 1. Load .env — tolerate comments/blank lines, export everything else.
# ---------------------------------------------------------------------------
if [[ ! -f "${ENV_FILE}" ]]; then
  echo "FATAL: ${ENV_FILE} not found" >&2
  exit 1
fi

# shellcheck disable=SC1090
set -a
source "${ENV_FILE}"
set +a

if [[ -z "${NLR_API_TOKEN:-}" ]]; then
  echo "FATAL: NLR_API_TOKEN not set in ${ENV_FILE}" >&2
  echo "       Issue one via Obsidian plugin: Settings -> Rotate Token" >&2
  exit 1
fi

# Sanity-check token shape — a trivially short token is almost certainly
# a misconfiguration and will silently accept guesses.
if [[ ${#NLR_API_TOKEN} -lt 32 ]]; then
  echo "FATAL: NLR_API_TOKEN is shorter than 32 chars (likely misconfigured)" >&2
  exit 1
fi

# ---------------------------------------------------------------------------
# 2. Verify TurboVault listeners. If absent, start the server (idempotent).
#    Uses `nc -z` for the probe — available on every macOS/Linux toolchain.
# ---------------------------------------------------------------------------
is_listening() {
  local port="$1"
  nc -z 127.0.0.1 "${port}" 2>/dev/null
}

turbovault_up() {
  is_listening "${TURBOVAULT_HTTP_PORT}" && is_listening "${TURBOVAULT_WS_PORT}"
}

if ! turbovault_up; then
  echo "[tunnel] TurboVault not listening on :${TURBOVAULT_HTTP_PORT}/:${TURBOVAULT_WS_PORT} — starting..."
  # The real boot command depends on how TurboVault is installed locally.
  # Prefer a systemd/launchd unit if you've got one; the fallback here is
  # a direct binary exec. Adjust the exe path if your install differs.
  if command -v turbovault >/dev/null 2>&1; then
    turbovault --http-port "${TURBOVAULT_HTTP_PORT}" --ws-port "${TURBOVAULT_WS_PORT}" &
    TV_PID=$!
  else
    echo "FATAL: turbovault binary not on PATH; start it manually and re-run" >&2
    exit 1
  fi

  # Wait up to 10s for both ports to come up.
  for _ in $(seq 1 20); do
    turbovault_up && break
    sleep 0.5
  done
  if ! turbovault_up; then
    echo "FATAL: TurboVault failed to bind :${TURBOVAULT_HTTP_PORT}/:${TURBOVAULT_WS_PORT}" >&2
    exit 1
  fi
  echo "[tunnel] TurboVault up (pid=${TV_PID:-unknown})"
else
  echo "[tunnel] TurboVault already running on :${TURBOVAULT_HTTP_PORT}/:${TURBOVAULT_WS_PORT}"
fi

# ---------------------------------------------------------------------------
# 3 + 4. Child process management — SIGINT brings everything down together.
# ---------------------------------------------------------------------------
CADDY_PID=""
NGROK_PID=""

cleanup() {
  echo ""
  echo "[tunnel] shutting down..."
  [[ -n "${NGROK_PID}" ]] && kill "${NGROK_PID}" 2>/dev/null || true
  [[ -n "${CADDY_PID}" ]] && kill "${CADDY_PID}" 2>/dev/null || true
  wait 2>/dev/null || true
  echo "[tunnel] done."
}
trap cleanup INT TERM EXIT

# Caddy — exported env so it can interpolate {$NLR_API_TOKEN} in Caddyfile.
export NLR_API_TOKEN
export CADDY_LOG_LEVEL="${CADDY_LOG_LEVEL:-INFO}"

echo "[tunnel] starting Caddy on :${CADDY_PORT}..."
caddy run --config "${CADDYFILE}" --adapter caddyfile &
CADDY_PID=$!

# Wait for Caddy to bind before launching ngrok — otherwise ngrok sees
# a dead upstream and enters a retry loop.
for _ in $(seq 1 20); do
  is_listening "${CADDY_PORT}" && break
  sleep 0.25
done
if ! is_listening "${CADDY_PORT}"; then
  echo "FATAL: Caddy failed to bind :${CADDY_PORT}" >&2
  exit 1
fi
echo "[tunnel] Caddy up (pid=${CADDY_PID})"

# ngrok — inherits NGROK_DOMAIN from .env if set.
echo "[tunnel] starting ngrok..."
ngrok start --config "${NGROK_CONFIG}" caddy-internal --log=stdout --log-format=json > /tmp/ngrok.log 2>&1 &
NGROK_PID=$!

# ---------------------------------------------------------------------------
# 5. Extract public URL from ngrok's local API (127.0.0.1:4040 is disabled
#    in prod config, so we scrape the log instead).
# ---------------------------------------------------------------------------
PUBLIC_URL=""
for _ in $(seq 1 40); do
  PUBLIC_URL=$(grep -oE '"url":"https://[^"]+"' /tmp/ngrok.log 2>/dev/null \
                | head -n1 \
                | sed -E 's/"url":"([^"]+)"/\1/' || true)
  [[ -n "${PUBLIC_URL}" ]] && break
  sleep 0.5
done

if [[ -z "${PUBLIC_URL}" ]]; then
  echo "WARN: couldn't determine ngrok public URL — check /tmp/ngrok.log"
else
  cat <<EOF

================================================================
  TurboVault public endpoint:

    ${PUBLIC_URL}

  Smoke test (health — should return 200 "ok" with no auth):
    curl -sS ${PUBLIC_URL}/healthz

  Smoke test (authenticated — should return TurboVault root):
    curl -sS -H "Authorization: Bearer \${NLR_API_TOKEN}" ${PUBLIC_URL}/

  Unauthenticated request (should return 401):
    curl -sS -o /dev/null -w "%{http_code}\n" ${PUBLIC_URL}/
================================================================

EOF
fi

# ---------------------------------------------------------------------------
# Block until one of the children dies; trap handles cleanup.
# ---------------------------------------------------------------------------
wait -n "${CADDY_PID}" "${NGROK_PID}"
