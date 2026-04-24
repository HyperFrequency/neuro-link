#!/usr/bin/env bash
# pkg/docker/build.sh — docker-arm64 image builder (H4 redesign: build-from-source)
#
# Prior version validated pre-existing running host containers
# (qdrant-nlr, neo4j-nlr) and emitted ready based on those. On a fresh
# clone those containers don't exist, so the builder emitted degraded.
#
# This revision builds the neuro-link image from $ROOT/server/Dockerfile
# via `docker buildx build --platform linux/arm64`, smoke-tests the image
# with `--version`, and validates pkg/docker/compose.yaml parses via
# `docker compose config --quiet`. Compose-stack bring-up + service-health
# validation moves to Phase 6 shakedown (not this builder's proof).

set -euo pipefail

VERSION="${1:-0.0.0-dev}"
ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
PROOF_DIR="${PROOF_DIR:-${ROOT}/pkg/.proof}"
SERVER_DIR="${ROOT}/server"
COMPOSE_FILE="${ROOT}/pkg/docker/compose.yaml"
TAG_IMG="neuro-link:${VERSION}"
PLATFORM="linux/arm64"

mkdir -p "${PROOF_DIR}"

# 0. Precheck: submodule initialized + buildx available
if [[ ! -f "${SERVER_DIR}/Dockerfile" ]]; then
  echo "[docker] HALT: ${SERVER_DIR}/Dockerfile missing — run 'git submodule update --init --recursive'" >&2
  exit 91
fi
if ! docker buildx version >/dev/null 2>&1; then
  echo "[docker] HALT: docker buildx not available" >&2
  exit 92
fi

# 1. Build neuro-link image for linux/arm64
echo "[docker] buildx build ${TAG_IMG} platform=${PLATFORM}..." >&2
cd "${SERVER_DIR}"
docker buildx build \
  --platform "${PLATFORM}" \
  --tag "${TAG_IMG}" \
  --load \
  . >&2

# 2. Capture image digest (the one docker stores locally post --load)
IMAGE_ID=$(docker image inspect "${TAG_IMG}" --format '{{.Id}}' 2>/dev/null || echo "")
IMAGE_SIZE=$(docker image inspect "${TAG_IMG}" --format '{{.Size}}' 2>/dev/null || echo "0")
IMAGE_CREATED=$(docker image inspect "${TAG_IMG}" --format '{{.Created}}' 2>/dev/null || echo "?")
IMAGE_ARCH=$(docker image inspect "${TAG_IMG}" --format '{{.Architecture}}' 2>/dev/null || echo "?")
IMAGE_OS=$(docker image inspect "${TAG_IMG}" --format '{{.Os}}' 2>/dev/null || echo "?")

if [[ -z "${IMAGE_ID}" ]]; then
  echo "[docker] HALT: buildx succeeded but image ${TAG_IMG} not present locally" >&2
  exit 93
fi

# 3. Smoke: run --version inside the image
SMOKE_EXIT=1
SMOKE_OUT=""
if docker run --rm --platform "${PLATFORM}" "${TAG_IMG}" --version >/tmp/docker-smoke-$$ 2>&1; then
  SMOKE_EXIT=0
fi
SMOKE_OUT=$(head -1 /tmp/docker-smoke-$$ 2>/dev/null || echo "")
rm -f /tmp/docker-smoke-$$

# 4. Validate compose.yaml parses (pass dummy values for required-substitution vars)
COMPOSE_EXIT=1
COMPOSE_STDERR=""
if [[ -f "${COMPOSE_FILE}" ]]; then
  if NEO4J_PASSWORD="dummy-for-config-check" \
     LITELLM_MASTER_KEY="dummy-for-config-check" \
     OPENAI_API_KEY="dummy-for-config-check" \
     ANTHROPIC_API_KEY="dummy-for-config-check" \
     docker compose --file "${COMPOSE_FILE}" config --quiet 2>/tmp/compose-err-$$; then
    COMPOSE_EXIT=0
  fi
  COMPOSE_STDERR=$(head -1 /tmp/compose-err-$$ 2>/dev/null || echo "")
  rm -f /tmp/compose-err-$$
fi

# 5. Platform check
PLATFORM_DETECTED=$(docker info --format '{{.Architecture}}' 2>/dev/null || echo "?")

# 6. Determine state
STATE="ready"
[[ "${SMOKE_EXIT}" -ne 0 ]] && STATE="degraded"
[[ "${COMPOSE_EXIT}" -ne 0 ]] && STATE="degraded"
[[ "${IMAGE_ARCH}" != "arm64" ]] && STATE="degraded"

cat > "${PROOF_DIR}/docker.ready.json" <<PROOF
{
  "target": "docker",
  "state": "${STATE}",
  "version": "${VERSION}",
  "platform_detected": "${PLATFORM_DETECTED}",
  "target_arch": "arm64",
  "artifact": "docker://${TAG_IMG}",
  "sha256": "${IMAGE_ID#sha256:}",
  "image": "${TAG_IMG}",
  "image_id": "${IMAGE_ID}",
  "image_size_bytes": ${IMAGE_SIZE},
  "image_created": "${IMAGE_CREATED}",
  "image_arch": "${IMAGE_ARCH}",
  "image_os": "${IMAGE_OS}",
  "built_from": "${SERVER_DIR}/Dockerfile",
  "smoke_tests": [
    {
      "name": "container_cli_version",
      "exit_code": ${SMOKE_EXIT},
      "stdout_head": $(printf '%s' "${SMOKE_OUT}" | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()))')
    },
    {
      "name": "compose_yaml_config_parses",
      "exit_code": ${COMPOSE_EXIT},
      "stderr_head": $(printf '%s' "${COMPOSE_STDERR}" | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()))')
    }
  ],
  "compose_file": "${COMPOSE_FILE}",
  "note": "Per-service health checks (qdrant / neo4j Up + healthz) are Phase 6 shakedown concerns, not this builder's proof.",
  "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}
PROOF

echo "[docker] state=${STATE} image_id=${IMAGE_ID:0:20}… smoke_exit=${SMOKE_EXIT} compose_config_exit=${COMPOSE_EXIT}"
[[ "${STATE}" == "ready" ]] && exit 0 || exit 3
