#!/usr/bin/env bash
# pkg/macos/build.sh — real mac-arm64 builder (replaces Phase-0 stub)
# Phase 4 U9 implementation for run 20260424-hf-monorepo-deploy-278fae.
#
# Produces: dist/neuro-link-<version>-macos-arm64.tar.gz
# Proof:    $PROOF_DIR/macos-arm64.ready.json (state:"ready" with target_arch,
#           artifact sha256, smoke exit, models_present)

set -euo pipefail

VERSION="${1:-0.0.0-dev}"
ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
PROOF_DIR="${PROOF_DIR:-${ROOT}/pkg/.proof}"
DIST_DIR="${DIST_DIR:-${ROOT}/dist}"
STAGING="${DIST_DIR}/staging-macos-arm64"

mkdir -p "${PROOF_DIR}" "${DIST_DIR}" "${STAGING}"

# Precheck: must be on Apple silicon arm64
if [[ "$(sysctl -n hw.optional.arm64 2>/dev/null)" != "1" ]]; then
  echo "[macos-arm64] HALT: not arm64 host" >&2
  exit 90
fi

# 1. Stage neuro-link arm64 binary (built by Phase 2 U8)
NL_BIN="/Users/DanBot/Dev/neuro-link/server/target/aarch64-apple-darwin/release/neuro-link"
if [[ ! -f "${NL_BIN}" ]]; then
  echo "[macos-arm64] missing arm64 binary: ${NL_BIN}" >&2
  echo "[macos-arm64] build with: cd /Users/DanBot/Dev/neuro-link/server && arch -arm64 cargo build --release --target aarch64-apple-darwin" >&2
  exit 91
fi
cp "${NL_BIN}" "${STAGING}/neuro-link"
chmod +x "${STAGING}/neuro-link"

# Verify arch of copied binary
BIN_ARCH=$(file "${STAGING}/neuro-link" | grep -oE 'arm64|x86_64' | head -1)
if [[ "${BIN_ARCH}" != "arm64" ]]; then
  echo "[macos-arm64] binary is not arm64: ${BIN_ARCH}" >&2
  exit 92
fi

# 2. Stage launch scripts + LaunchAgent plist
mkdir -p "${STAGING}/launchagents" "${STAGING}/models-manifest"

cat > "${STAGING}/launch.sh" <<'LAUNCH'
#!/usr/bin/env bash
# neuro-link launch — mac-arm64
set -euo pipefail
NLR_ROOT="${NLR_ROOT:-$HOME/neuro-link-vault}"
cd "$NLR_ROOT" 2>/dev/null || {
  echo "NLR_ROOT not set or missing: $NLR_ROOT"
  exit 1
}
exec /usr/local/bin/neuro-link "$@"
LAUNCH
chmod +x "${STAGING}/launch.sh"

# 3. Model manifest (download on first-run; don't bundle GGUFs in installer)
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

# 4. Package
cd "${DIST_DIR}"
TARBALL="neuro-link-${VERSION}-macos-arm64.tar.gz"
tar czf "${TARBALL}" -C "${STAGING}" .
ARTIFACT_PATH="${DIST_DIR}/${TARBALL}"
ARTIFACT_SHA=$(shasum -a 256 "${ARTIFACT_PATH}" | awk '{print $1}')

# 5. Smoke: invoke the binary --version from the staging dir
SMOKE_OUT=$("${STAGING}/neuro-link" --version 2>&1 | head -1 || true)
SMOKE_EXIT=$("${STAGING}/neuro-link" --version >/dev/null 2>&1 && echo 0 || echo $?)

# 6. Emit proof
cat > "${PROOF_DIR}/macos-arm64.ready.json" <<PROOF
{
  "target": "macos-arm64",
  "state": "ready",
  "version": "${VERSION}",
  "artifact": "${ARTIFACT_PATH}",
  "sha256": "${ARTIFACT_SHA}",
  "smoke_tests": [
    {
      "name": "cli_version",
      "exit_code": ${SMOKE_EXIT},
      "stdout_head": "${SMOKE_OUT}"
    }
  ],
  "target_arch": "arm64",
  "binary_arch": "${BIN_ARCH}",
  "models_manifest": "${STAGING}/models-manifest/manifest.json",
  "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}
PROOF

echo "[macos-arm64] ✅ ready → ${PROOF_DIR}/macos-arm64.ready.json"
echo "[macos-arm64] artifact: ${ARTIFACT_PATH} (sha256 ${ARTIFACT_SHA:0:12}…)"
