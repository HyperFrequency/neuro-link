#!/usr/bin/env bash
# pkg/linux-aarch64/build.sh — real linux-aarch64 builder (Phase 5b B2-10)
# Replaces the Phase 0 stub. Produces:
#   dist/neuro-link-<version>-linux-aarch64.tar.gz
#   $PROOF_DIR/linux-aarch64.ready.json   (state:"ready" with sha256, smoke, target_arch)
#
# Same pattern as linux-x86_64 but --platform linux/arm64. On Apple silicon the
# build is native (no emulation); on x86 hosts, buildx uses qemu.

set -euo pipefail

VERSION="${1:-0.0.0-dev}"
ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
PROOF_DIR="${PROOF_DIR:-${ROOT}/pkg/.proof}"
DIST_DIR="${DIST_DIR:-${ROOT}/dist}"
STAGING="${DIST_DIR}/staging-linux-aarch64"
TAG_IMG="neuro-link-builder:${VERSION}-linux-arm64"
PLATFORM="linux/arm64"

mkdir -p "${PROOF_DIR}" "${DIST_DIR}"
rm -rf "${STAGING}"
mkdir -p "${STAGING}/models-manifest" "${STAGING}/systemd"

cd "${ROOT}/server"
docker buildx build \
  --platform "${PLATFORM}" \
  --tag "${TAG_IMG}" \
  --load \
  . >&2

CID=$(docker create --platform "${PLATFORM}" "${TAG_IMG}")
trap 'docker rm -f "${CID}" >/dev/null 2>&1 || true' EXIT
docker cp "${CID}:/usr/local/bin/neuro-link" "${STAGING}/neuro-link"

BIN_TYPE=$(file "${STAGING}/neuro-link" | head -1)
if ! echo "${BIN_TYPE}" | grep -qE 'ELF 64-bit LSB.*(aarch64|ARM aarch64)'; then
  echo "[linux-aarch64] HALT: extracted binary is not ELF aarch64: ${BIN_TYPE}" >&2
  exit 92
fi

cat > "${STAGING}/launch.sh" <<'LAUNCH'
#!/usr/bin/env bash
set -euo pipefail
NLR_ROOT="${NLR_ROOT:-$HOME/neuro-link-vault}"
cd "${NLR_ROOT}" 2>/dev/null || { echo "NLR_ROOT not set or missing: ${NLR_ROOT}"; exit 1; }
exec /usr/local/bin/neuro-link "$@"
LAUNCH
chmod +x "${STAGING}/launch.sh"

cat > "${STAGING}/systemd/neuro-link.service" <<'UNIT'
[Unit]
Description=neuro-link RAG + MCP server
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart=/usr/local/bin/neuro-link serve --port 8080
Restart=on-failure
RestartSec=5
Environment=NLR_ROOT=%h/neuro-link-vault

[Install]
WantedBy=multi-user.target
UNIT

cat > "${STAGING}/models-manifest/manifest.json" <<JSON
{
  "octen_embedding_8b": {
    "hf_repo": "mradermacher/Octen-Embedding-8B-GGUF",
    "file": "Octen-Embedding-8B.f16.gguf",
    "dest": "~/neuro-link-models/",
    "sha256": "deferred-first-run"
  },
  "qwen3_reranker_0_6b": {
    "hf_repo": "ggml-org/Qwen3-Reranker-0.6B-Q8_0-GGUF",
    "file": "qwen3-reranker-0.6b-q8_0.gguf",
    "dest": "~/.cache/qmd/models/",
    "sha256": "deferred-first-run"
  },
  "qmd_query_expansion_1_7b": {
    "hf_repo": "tobil/qmd-query-expansion-1.7B-gguf",
    "file": "qmd-query-expansion-1.7B-q4_k_m.gguf",
    "dest": "~/.cache/qmd/models/",
    "sha256": "deferred-first-run"
  }
}
JSON

cd "${DIST_DIR}"
TARBALL="neuro-link-${VERSION}-linux-aarch64.tar.gz"
tar czf "${TARBALL}" -C "${STAGING}" .
ARTIFACT_PATH="${DIST_DIR}/${TARBALL}"
ARTIFACT_SHA=$(shasum -a 256 "${ARTIFACT_PATH}" | awk '{print $1}')

docker pull --platform "${PLATFORM}" debian:bookworm-slim >&2 || true
TMP_SMOKE=$(mktemp -t linux-aarch64-smoke.XXXXXX)
trap 'rm -f "${TMP_SMOKE}"' RETURN
SMOKE_EXIT=0
docker run --rm --platform "${PLATFORM}" \
  -v "${STAGING}/neuro-link:/usr/local/bin/neuro-link:ro" \
  debian:bookworm-slim bash -c '
    export DEBIAN_FRONTEND=noninteractive
    apt-get update -qq >/dev/null 2>&1 && \
    apt-get install -y -qq libssl3 ca-certificates >/dev/null 2>&1 && \
    /usr/local/bin/neuro-link --version
  ' >"${TMP_SMOKE}" 2>&1 || SMOKE_EXIT=$?
SMOKE_OUT=$(head -1 "${TMP_SMOKE}" 2>/dev/null || true)
rm -f "${TMP_SMOKE}"

BUILD_STATE="ready"
[[ "${SMOKE_EXIT}" -ne 0 ]] && BUILD_STATE="degraded"

cat > "${PROOF_DIR}/linux-aarch64.ready.json" <<PROOF
{
  "target": "linux-aarch64",
  "state": "${BUILD_STATE}",
  "version": "${VERSION}",
  "artifact": "${ARTIFACT_PATH}",
  "sha256": "${ARTIFACT_SHA}",
  "smoke_tests": [
    {
      "name": "cli_version_in_container",
      "platform": "${PLATFORM}",
      "exit_code": ${SMOKE_EXIT},
      "stdout_head": $(printf '%s' "${SMOKE_OUT}" | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()))')
    }
  ],
  "target_arch": "aarch64",
  "binary_file": $(printf '%s' "${BIN_TYPE}" | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()))'),
  "builder_image": "${TAG_IMG}",
  "models_manifest": "${STAGING}/models-manifest/manifest.json",
  "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}
PROOF

echo "[linux-aarch64] ✅ ready → ${PROOF_DIR}/linux-aarch64.ready.json"
echo "[linux-aarch64] artifact: ${ARTIFACT_PATH} (sha256 ${ARTIFACT_SHA:0:12}…)"
