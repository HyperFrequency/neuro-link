#!/usr/bin/env bash
# pkg/macos/build.sh — mac-arm64 builder (H3 redesign: build-from-source)
#
# Prior versions sourced the binary from a dev-split path
# (/Users/DanBot/Dev/neuro-link/…), which broke install-from-zero on any
# fresh clone. This revision compiles the Rust binary from `$ROOT/server`
# in the checked-out submodule, then stages + tarballs + smoke + proof.
#
# Produces: dist/neuro-link-<version>-macos-arm64.tar.gz
# Proof:    $PROOF_DIR/macos-arm64.ready.json (state:"ready" with target_arch,
#           artifact sha256, smoke exit, binary lipo-archs)

set -euo pipefail

VERSION="${1:-0.0.0-dev}"
ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
PROOF_DIR="${PROOF_DIR:-${ROOT}/pkg/.proof}"
DIST_DIR="${DIST_DIR:-${ROOT}/dist}"
STAGING="${DIST_DIR}/staging-macos-arm64"
SERVER_DIR="${ROOT}/server"
TARGET_TRIPLE="aarch64-apple-darwin"
BUILT_BIN="${SERVER_DIR}/target/${TARGET_TRIPLE}/release/neuro-link"

mkdir -p "${PROOF_DIR}" "${DIST_DIR}"
rm -rf "${STAGING}"
mkdir -p "${STAGING}/launchagents" "${STAGING}/models-manifest"

# 0. Precheck: Apple silicon arm64
if [[ "$(sysctl -n hw.optional.arm64 2>/dev/null)" != "1" ]]; then
  echo "[macos-arm64] HALT: not arm64 host" >&2
  exit 90
fi

# 0b. Precheck: server subtree present (would fail if submodule isn't initialized)
if [[ ! -f "${SERVER_DIR}/Cargo.toml" ]]; then
  echo "[macos-arm64] HALT: ${SERVER_DIR}/Cargo.toml missing — run 'git submodule update --init --recursive'" >&2
  exit 91
fi

# 1. Cargo target install (idempotent)
if ! arch -arm64 rustup target list --installed 2>/dev/null | grep -q "^${TARGET_TRIPLE}$"; then
  arch -arm64 rustup target add "${TARGET_TRIPLE}" >&2
fi

# 2. Build from source — arm64 cargo, arm64 target, release profile
echo "[macos-arm64] cargo build (target=${TARGET_TRIPLE})..." >&2
cd "${SERVER_DIR}"
arch -arm64 cargo build --release --target "${TARGET_TRIPLE}" >&2

if [[ ! -f "${BUILT_BIN}" ]]; then
  echo "[macos-arm64] HALT: cargo build succeeded but ${BUILT_BIN} missing" >&2
  exit 92
fi

# 3. Stage + verify arch
cp "${BUILT_BIN}" "${STAGING}/neuro-link"
chmod +x "${STAGING}/neuro-link"
BIN_FILE=$(file "${STAGING}/neuro-link" | head -1)
BIN_ARCH=$(echo "${BIN_FILE}" | grep -oE 'arm64|x86_64' | head -1)
BIN_LIPO=$(lipo -archs "${STAGING}/neuro-link" 2>/dev/null || echo "n/a")
if [[ "${BIN_ARCH}" != "arm64" ]]; then
  echo "[macos-arm64] HALT: built binary is not arm64 (file says: ${BIN_FILE})" >&2
  exit 93
fi

# 4. Stage launch + LaunchAgent plist + models manifest
cat > "${STAGING}/launch.sh" <<'LAUNCH'
#!/usr/bin/env bash
set -euo pipefail
NLR_ROOT="${NLR_ROOT:-$HOME/neuro-link-vault}"
cd "${NLR_ROOT}" 2>/dev/null || { echo "NLR_ROOT not set or missing: ${NLR_ROOT}"; exit 1; }
exec /usr/local/bin/neuro-link "$@"
LAUNCH
chmod +x "${STAGING}/launch.sh"

cat > "${STAGING}/launchagents/com.hyperfrequency.neuro-link.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>com.hyperfrequency.neuro-link</string>
  <key>ProgramArguments</key>
  <array>
    <string>/usr/local/bin/neuro-link</string>
    <string>serve</string>
    <string>--port</string>
    <string>8080</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <true/>
  <key>StandardOutPath</key>
  <string>/tmp/neuro-link.stdout.log</string>
  <key>StandardErrorPath</key>
  <string>/tmp/neuro-link.stderr.log</string>
</dict>
</plist>
PLIST

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

# 5. Package
cd "${DIST_DIR}"
TARBALL="neuro-link-${VERSION}-macos-arm64.tar.gz"
tar czf "${TARBALL}" -C "${STAGING}" .
ARTIFACT_PATH="${DIST_DIR}/${TARBALL}"
ARTIFACT_SHA=$(shasum -a 256 "${ARTIFACT_PATH}" | awk '{print $1}')
BIN_SHA=$(shasum -a 256 "${STAGING}/neuro-link" | awk '{print $1}')

# 6. Smoke: invoke the binary --version from the staging dir
SMOKE_OUT=$("${STAGING}/neuro-link" --version 2>&1 | head -1 || true)
SMOKE_EXIT=$("${STAGING}/neuro-link" --version >/dev/null 2>&1 && echo 0 || echo $?)

# 7. Emit proof
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
      "stdout_head": $(printf '%s' "${SMOKE_OUT}" | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()))')
    }
  ],
  "target_arch": "arm64",
  "binary_arch": "${BIN_ARCH}",
  "binary_lipo_archs": "${BIN_LIPO}",
  "binary_sha256": "${BIN_SHA}",
  "binary_file": $(printf '%s' "${BIN_FILE}" | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()))'),
  "built_from_source": "${SERVER_DIR}",
  "cargo_target_triple": "${TARGET_TRIPLE}",
  "models_manifest": "${STAGING}/models-manifest/manifest.json",
  "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}
PROOF

echo "[macos-arm64] ✅ ready → ${PROOF_DIR}/macos-arm64.ready.json"
echo "[macos-arm64] artifact: ${ARTIFACT_PATH} (sha256 ${ARTIFACT_SHA:0:12}…)"
echo "[macos-arm64] binary: ${BIN_LIPO} sha256=${BIN_SHA:0:12}…"
