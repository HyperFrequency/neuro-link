#!/usr/bin/env bash
# Check all neuro-link-recursive prerequisites. Prints a table and exits
# non-zero if any required tool is missing. Does not install anything —
# surfaces exact install commands instead.

set -uo pipefail

# Platform detection for install-hint output
OS="$(uname -s)"
ARCH="$(uname -m)"
IS_MAC=0
[[ "$OS" == "Darwin" ]] && IS_MAC=1

missing=0

check() {
  local name="$1"
  local cmd="$2"
  local ver_flag="${3:---version}"
  local install_hint="$4"
  local min_version="${5:-}"

  if command -v "$cmd" >/dev/null 2>&1; then
    local v
    v="$("$cmd" "$ver_flag" 2>&1 | head -1 || true)"
    printf "  %-20s %-10s %s\n" "$name" "OK" "$v"
  else
    printf "  %-20s %-10s %s\n" "$name" "MISSING" "$install_hint"
    missing=$((missing + 1))
  fi
}

echo "neuro-link-recursive prerequisites"
echo "  platform: $OS $ARCH"
echo
printf "  %-20s %-10s %s\n" "tool" "status" "install / version"
printf "  %-20s %-10s %s\n" "----" "------" "-----------------"

check "Rust (rustc)" rustc --version "rustup install stable (MSRV 1.90+)" "1.90"
check "Cargo" cargo --version "comes with rustup" ""
check "Python" python3 --version "brew install python@3.12" "3.11"
check "Node (or Bun)" node --version "brew install node  OR  curl -fsSL https://bun.sh/install | bash"
check "Docker" docker --version "https://docs.docker.com/desktop/install/mac-install/"
check "llama-server" llama-server --version "build llama.cpp: git clone https://github.com/ggml-org/llama.cpp && cd llama.cpp && make -j"
check "huggingface-cli" huggingface-cli --version "pip install --user 'huggingface_hub[cli]'"
check "ngrok" ngrok --version "brew install --cask ngrok  OR  https://ngrok.com/download"
check "caddy" caddy version "brew install caddy"
check "gh (GitHub CLI)" gh --version "brew install gh"
check "jq" jq --version "brew install jq"
check "rsync" rsync --version "preinstalled on macOS; brew install rsync on Linux"

echo
if [[ $missing -gt 0 ]]; then
  echo "ERROR: $missing prerequisite(s) missing. Install them and re-run." >&2
  exit 1
fi
echo "All prerequisites present."
