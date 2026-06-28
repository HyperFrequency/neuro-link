#!/usr/bin/env bash
# neuro-link-recursive — one-shot installer (post-rebuild orchestration).
#
# Two ways to use this:
#   1. From a clone:   ./install.sh [--mode local|cloud|hybrid] [--dry-run] [--with-tunnel]
#   2. curl | bash:    curl -fsSL https://raw.githubusercontent.com/HyperFrequency/neuro-link-recursive/master/install.sh | bash
#
# The curl|bash path will: (a) clone the repo to ~/neuro-link-recursive if
# missing, (b) cd into it, (c) re-exec install.sh from that clone.
# Idempotent — safe to re-run; every step detects what's already installed
# and skips.
#
# What this script orchestrates (delegates to dedicated scripts wherever
# possible, so logic lives in one place):
#
#   1. Prereq check   — .claude/skills/neuro-link-setup/scripts/check_prereqs.sh
#   2. Vault verify   — .claude/skills/neuro-link-setup/scripts/verify_vault_structure.sh
#   3. TurboVault     — cargo install --git ahuserious/turbovault --features full
#   4. Rust build     — cargo build --release (in server/)
#   5. Plugin build   — cd obsidian-plugin && bun install && bun run build
#   6. Secrets        — mint NLR_API_TOKEN (never overwrite an existing one)
#   7. Data services  — docker compose up -d qdrant neo4j
#   8. Models         — .claude/skills/neuro-link-setup/scripts/download_models.sh
#   9. llama-server   — LaunchAgent (macOS) or systemd user unit (Linux)
#  10. Skills         — .claude/skills/neuro-link-setup/scripts/install_skills.sh
#  11. MCP register   — .claude/skills/neuro-link-setup/scripts/install_mcp_servers.sh
#  12. Verify         — .claude/skills/neuro-link/scripts/status.sh
#  13. Tunnel (opt)   — scripts/start_public_tunnel.sh  (only with --with-tunnel)

set -euo pipefail

# ── curl|bash bootstrap ─────────────────────────────────────────────────────
# If we were piped from curl (no script file), clone the repo and re-exec.
# This preserves the one-liner install UX from the README quickstart.
if [[ "${BASH_SOURCE[0]:-}" == "" || "${BASH_SOURCE[0]}" == "stdin" || ! -f "${BASH_SOURCE[0]}" ]]; then
  CLONE_DIR="${NLR_INSTALL_DIR:-$HOME/neuro-link-recursive}"
  if [[ ! -d "$CLONE_DIR/.git" ]]; then
    echo "[install] cloning HyperFrequency/neuro-link-recursive -> $CLONE_DIR"
    git clone --depth 1 https://github.com/HyperFrequency/neuro-link-recursive.git "$CLONE_DIR"
  else
    echo "[install] repo present at $CLONE_DIR — pulling latest"
    (cd "$CLONE_DIR" && git pull --ff-only) || true
  fi
  exec bash "$CLONE_DIR/install.sh" "$@"
fi

# ── Colors + loggers ─────────────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

ok()    { printf "${GREEN}[OK]${NC}   %s\n" "$1"; }
warn()  { printf "${YELLOW}[WARN]${NC} %s\n" "$1"; }
fail()  { printf "${RED}[FAIL]${NC} %s\n" "$1"; }
info()  { printf "${CYAN}[INFO]${NC} %s\n" "$1"; }
step()  { printf "\n${BOLD}=== %s ===${NC}\n" "$1"; }
dry()   { printf "${YELLOW}[DRY]${NC}  %s\n" "$1"; }

# ── Paths ────────────────────────────────────────────────────────────────────
NLR_ROOT="$(cd "$(dirname "$0")" && pwd)"
export NLR_ROOT

OS_NAME="$(uname -s)"
IS_MAC=0
IS_LINUX=0
case "$OS_NAME" in
  Darwin) IS_MAC=1 ;;
  Linux)  IS_LINUX=1 ;;
  *)      ;;
esac

BINARY_PATH="${NLR_ROOT}/server/target/release/neuro-link"
PLUGIN_DIR="${NLR_ROOT}/obsidian-plugin"
SETUP_SCRIPTS="${NLR_ROOT}/.claude/skills/neuro-link-setup/scripts"
STATUS_SCRIPT="${NLR_ROOT}/.claude/skills/neuro-link/scripts/status.sh"

# ── Flag parsing ─────────────────────────────────────────────────────────────
MODE="local"           # local | cloud | hybrid
DRY_RUN=0
WITH_TUNNEL=0

print_help() {
  cat <<EOF
Usage: ./install.sh [options]

Options:
  --mode local|cloud|hybrid   Services posture (default: local)
                              local  — run Qdrant/Neo4j/llama-server locally (default)
                              cloud  — skip local services; use cloud endpoints in secrets/.env
                              hybrid — per-service, edit config/neuro-link.md after install
  --dry-run                   Print what would happen without executing anything
  --with-tunnel               After verification, start the public tunnel
                              (scripts/start_public_tunnel.sh). Off by default.
  -h, --help                  Show this message

Environment:
  NLR_INSTALL_DIR             Override clone target for curl|bash bootstrap
                              (default: \$HOME/neuro-link-recursive)

Idempotency: every step checks before it acts. Re-running the script skips
whatever is already in place.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --mode=local|--mode=cloud|--mode=hybrid) MODE="${1#--mode=}" ;;
    --mode) shift; MODE="${1:-local}" ;;
    --dry-run)     DRY_RUN=1 ;;
    --with-tunnel) WITH_TUNNEL=1 ;;
    -h|--help)     print_help; exit 0 ;;
    *)             warn "unknown flag: $1 (use --help for options)" ;;
  esac
  shift
done

case "$MODE" in
  local|cloud|hybrid) : ;;
  *) fail "invalid --mode: $MODE (want local|cloud|hybrid)"; exit 2 ;;
esac

# ── Summary tracking ─────────────────────────────────────────────────────────
SUMMARY_OK=()
SUMMARY_WARN=()
SUMMARY_FAIL=()

track_ok()   { SUMMARY_OK+=("$1"); }
track_warn() { SUMMARY_WARN+=("$1"); }
track_fail() { SUMMARY_FAIL+=("$1"); }

die() { fail "$1"; [[ -n "${2:-}" ]] && fail "  diagnose: $2"; exit 1; }

# Runs a command unless --dry-run. Prints the command either way.
run() {
  if [[ $DRY_RUN -eq 1 ]]; then
    dry "$*"
    return 0
  fi
  "$@"
}

# Runs a shell pipeline via `bash -c` unless --dry-run. Used for anything
# with pipes, redirects, or subshells.
run_sh() {
  if [[ $DRY_RUN -eq 1 ]]; then
    dry "sh -c: $*"
    return 0
  fi
  bash -c "$*"
}

# ── Banner ───────────────────────────────────────────────────────────────────
echo ""
printf '%bneuro-link-recursive installer%b\n' "$BOLD" "$NC"
printf "  root:     %s\n" "$NLR_ROOT"
printf "  os:       %s (%s)\n" "$OS_NAME" "$(uname -m)"
printf "  mode:     %s\n" "$MODE"
printf "  dry-run:  %s\n" "$([[ $DRY_RUN -eq 1 ]] && echo yes || echo no)"
printf "  tunnel:   %s\n" "$([[ $WITH_TUNNEL -eq 1 ]] && echo yes || echo no)"
echo ""

# ═══════════════════════════════════════════════════════════════════════════
step "1/12  Prerequisites"
# ═══════════════════════════════════════════════════════════════════════════
# check_prereqs.sh is non-destructive: it surfaces missing tools and prints
# install hints, but never installs anything on its own. The task spec
# forbids silent auto-install / sudo — so we honor that and stop if anything
# is missing, except in dry-run mode where we just warn.

if [[ -x "$SETUP_SCRIPTS/check_prereqs.sh" ]]; then
  if [[ $DRY_RUN -eq 1 ]]; then
    dry "$SETUP_SCRIPTS/check_prereqs.sh"
    track_ok "Prerequisites (dry-run skipped)"
  elif "$SETUP_SCRIPTS/check_prereqs.sh"; then
    ok "All prerequisites present"
    track_ok "Prerequisites"
  else
    fail "Prerequisites missing. Install them (see hints above) and re-run:"
    fail "  bash $SETUP_SCRIPTS/check_prereqs.sh"
    exit 1
  fi
else
  warn "check_prereqs.sh not found — skipping prereq gate"
  track_warn "Prerequisites (checker missing)"
fi

# ═══════════════════════════════════════════════════════════════════════════
step "2/12  Vault Structure"
# ═══════════════════════════════════════════════════════════════════════════
# verify_vault_structure.sh enforces the post-2026-04-18 layout. If a
# directory is missing, we create it — the rebuild expects an empty tree
# to be populated over time, and `mkdir -p` is safe.

if [[ -x "$SETUP_SCRIPTS/verify_vault_structure.sh" ]]; then
  if [[ $DRY_RUN -eq 1 ]]; then
    dry "$SETUP_SCRIPTS/verify_vault_structure.sh (+ mkdir -p for any missing dirs)"
    track_ok "Vault structure (dry-run skipped)"
  elif "$SETUP_SCRIPTS/verify_vault_structure.sh" >/dev/null 2>&1; then
    ok "Vault structure OK"
    track_ok "Vault structure"
  else
    info "Creating missing vault directories..."
    # Re-run to capture the missing-dir report, then mkdir each one.
    while read -r line; do
      dir="${line#*MISSING }"
      [[ -n "$dir" && -d "$dir" ]] && continue
      [[ -n "$dir" ]] && mkdir -p "$NLR_ROOT/$dir" && info "  mkdir $dir"
    done < <("$SETUP_SCRIPTS/verify_vault_structure.sh" 2>/dev/null | grep '^  MISSING ' || true)
    if "$SETUP_SCRIPTS/verify_vault_structure.sh" >/dev/null 2>&1; then
      ok "Vault structure repaired"
      track_ok "Vault structure"
    else
      warn "Some vault dirs still missing — see: $SETUP_SCRIPTS/verify_vault_structure.sh"
      track_warn "Vault structure (partial)"
    fi
  fi
else
  warn "verify_vault_structure.sh not found — skipping vault check"
  track_warn "Vault structure (checker missing)"
fi

# ═══════════════════════════════════════════════════════════════════════════
step "3/12  TurboVault (fork: ahuserious/turbovault, features=full)"
# ═══════════════════════════════════════════════════════════════════════════
# The fork at ahuserious/turbovault adds the subscribe_vault_events tool and
# fixes the FileRenamed emission bug. --features full enables STDIO + HTTP +
# WebSocket + TCP + Unix transports. Idempotent: cargo install reuses cached
# builds when the git SHA matches.

TV_BIN="$HOME/.cargo/bin/turbovault"
if [[ -x "$TV_BIN" ]]; then
  TV_VER="$("$TV_BIN" --version 2>/dev/null | head -1 || echo unknown)"
  ok "turbovault already installed: $TV_VER"
  track_ok "TurboVault"
else
  info "Installing turbovault from ahuserious/turbovault with --features full..."
  if run cargo install --git https://github.com/ahuserious/turbovault --features full; then
    if [[ $DRY_RUN -eq 0 ]]; then
      ok "turbovault installed to $TV_BIN"
    else
      ok "turbovault install (dry-run)"
    fi
    track_ok "TurboVault"
  else
    track_fail "TurboVault"
    die "cargo install turbovault failed" "cd /tmp && cargo install --git https://github.com/ahuserious/turbovault --features full -v"
  fi
fi

# ═══════════════════════════════════════════════════════════════════════════
step "4/12  Build neuro-link Rust Server"
# ═══════════════════════════════════════════════════════════════════════════
# server/Cargo.toml declares a single package (neuro-link-mcp) with a
# [[bin]] named `neuro-link`. A `-p neuro-link-server` flag would fail
# because no such package exists — we build the directory directly, which
# is what the repo's Makefile and the pre-rebuild install.sh already do.
#
# Not "cargo build --release -p neuro-link-server" (README lists that
# form for the future workspace layout; the crate hasn't been split yet).

if [[ -x "$BINARY_PATH" ]]; then
  ok "neuro-link binary already built ($BINARY_PATH)"
  track_ok "neuro-link binary"
else
  info "Building neuro-link (cargo build --release in server/)..."
  if run_sh "cd '${NLR_ROOT}/server' && cargo build --release"; then
    [[ $DRY_RUN -eq 0 && -x "$BINARY_PATH" ]] && ok "neuro-link built" || ok "neuro-link build (dry-run)"
    track_ok "neuro-link binary"
  else
    track_fail "neuro-link binary"
    die "cargo build --release failed" "cd ${NLR_ROOT}/server && cargo build --release -v"
  fi
fi

# ═══════════════════════════════════════════════════════════════════════════
step "5/12  Obsidian Plugin (bun install + bun run build)"
# ═══════════════════════════════════════════════════════════════════════════
# Bun is preferred (tables in README) but we fall back to npm if bun is not
# on PATH — the existing plugin build uses esbuild which works with either.

if [[ -f "${PLUGIN_DIR}/main.js" && -f "${PLUGIN_DIR}/manifest.json" ]]; then
  ok "Obsidian plugin already built (main.js exists)"
  track_ok "Obsidian plugin"
else
  if command -v bun >/dev/null 2>&1; then
    info "Building plugin with bun..."
    if run_sh "cd '$PLUGIN_DIR' && bun install && bun run build"; then
      ok "Obsidian plugin built (bun)"
      track_ok "Obsidian plugin"
    else
      track_fail "Obsidian plugin"
      die "bun build failed" "cd $PLUGIN_DIR && bun install && bun run build"
    fi
  elif command -v npm >/dev/null 2>&1; then
    warn "bun not found — falling back to npm"
    if run_sh "cd '$PLUGIN_DIR' && npm ci && npm run build"; then
      ok "Obsidian plugin built (npm)"
      track_ok "Obsidian plugin (npm fallback)"
    else
      track_fail "Obsidian plugin"
      die "npm build failed" "cd $PLUGIN_DIR && npm ci && npm run build"
    fi
  else
    track_fail "Obsidian plugin"
    die "neither bun nor npm found" "brew install node  (or: curl -fsSL https://bun.sh/install | bash)"
  fi
fi

# ═══════════════════════════════════════════════════════════════════════════
step "6/12  Secrets (secrets/.env + NLR_API_TOKEN)"
# ═══════════════════════════════════════════════════════════════════════════
# Generate secrets/.env from the template if missing. Mint NLR_API_TOKEN
# ONLY if the key is absent or empty — a pre-existing token is left alone.

SECRETS_DIR="${NLR_ROOT}/secrets"
ENV_FILE="${SECRETS_DIR}/.env"
ENV_TEMPLATE="${SECRETS_DIR}/.env.example"

mkdir -p "$SECRETS_DIR"

if [[ -f "$ENV_FILE" ]]; then
  ok "secrets/.env already exists"
else
  if [[ -f "$ENV_TEMPLATE" ]]; then
    info "Creating secrets/.env from $ENV_TEMPLATE..."
    if [[ $DRY_RUN -eq 0 ]]; then
      cp "$ENV_TEMPLATE" "$ENV_FILE"
      chmod 600 "$ENV_FILE"
    else
      dry "cp $ENV_TEMPLATE $ENV_FILE && chmod 600 $ENV_FILE"
    fi
    ok "secrets/.env created (mode 0600)"
  else
    warn "$ENV_TEMPLATE missing — creating an empty secrets/.env"
    if [[ $DRY_RUN -eq 0 ]]; then
      touch "$ENV_FILE"
      chmod 600 "$ENV_FILE"
    else
      dry "touch $ENV_FILE && chmod 600 $ENV_FILE"
    fi
  fi
fi

# NLR_API_TOKEN — never overwrite if already set. Accept both "KEY=value"
# and "KEY= value" shapes; treat whitespace-only as empty.
if [[ $DRY_RUN -eq 0 ]]; then
  EXISTING_TOKEN="$(grep '^NLR_API_TOKEN=' "$ENV_FILE" 2>/dev/null | head -1 | cut -d= -f2- | tr -d ' ' || true)"
else
  EXISTING_TOKEN=""
fi

if [[ -n "$EXISTING_TOKEN" ]]; then
  ok "NLR_API_TOKEN already set (preserved)"
  track_ok "NLR_API_TOKEN"
else
  info "Minting NLR_API_TOKEN (openssl rand -hex 32)..."
  if [[ $DRY_RUN -eq 1 ]]; then
    dry "openssl rand -hex 32 >> secrets/.env as NLR_API_TOKEN=..."
    track_ok "NLR_API_TOKEN (dry-run)"
  else
    TOKEN="$(openssl rand -hex 32 2>/dev/null || python3 -c 'import secrets; print(secrets.token_hex(32))')"
    if [[ -z "$TOKEN" ]]; then
      track_fail "NLR_API_TOKEN"
      die "failed to generate token" "openssl rand -hex 32  OR  python3 -c 'import secrets; print(secrets.token_hex(32))'"
    fi
    # Remove any empty NLR_API_TOKEN= line, then append the fresh token.
    # macOS sed wants "-i ''"; GNU sed accepts "-i". Detect via --version.
    if sed --version >/dev/null 2>&1; then
      sed -i '/^NLR_API_TOKEN=$/d; /^NLR_API_TOKEN= *$/d' "$ENV_FILE"
    else
      sed -i '' '/^NLR_API_TOKEN=$/d; /^NLR_API_TOKEN= *$/d' "$ENV_FILE"
    fi
    printf "NLR_API_TOKEN=%s\n" "$TOKEN" >> "$ENV_FILE"
    chmod 600 "$ENV_FILE"
    ok "NLR_API_TOKEN minted"
    track_ok "NLR_API_TOKEN"
  fi
fi

# Persist NLR_ROOT so hooks and skills can find the tree.
mkdir -p "${HOME}/.claude/state"
if [[ $DRY_RUN -eq 0 ]]; then
  printf '%s' "${NLR_ROOT}" > "${HOME}/.claude/state/nlr_root"
  ok "NLR_ROOT persisted to ~/.claude/state/nlr_root"
else
  dry "printf '%s' $NLR_ROOT > ~/.claude/state/nlr_root"
fi

# ═══════════════════════════════════════════════════════════════════════════
step "7/12  Data Services (Qdrant + Neo4j)"
# ═══════════════════════════════════════════════════════════════════════════
# Runs `docker compose up -d qdrant neo4j` in local mode. Cloud mode skips
# this; user is expected to set cloud endpoints in secrets/.env. Hybrid is
# treated like local for now — tuning per-service requires editing
# config/neuro-link.md after install.

if [[ "$MODE" == "cloud" ]]; then
  info "Cloud mode: skipping docker compose. Set cloud endpoints in secrets/.env:"
  info "  QDRANT_URL=https://<your-qdrant-cloud>"
  info "  NEO4J_URI=neo4j+s://<your-aura-uri>"
  track_ok "Data services (cloud mode, skipped)"
else
  if ! command -v docker >/dev/null 2>&1; then
    track_fail "Data services (docker missing)"
    die "docker not on PATH" "Install Docker Desktop: https://docs.docker.com/desktop/install/mac-install/"
  fi
  if ! docker info >/dev/null 2>&1; then
    info "Docker daemon not running — attempting to start Docker Desktop..."
    if [[ $IS_MAC -eq 1 ]]; then
      run_sh "open -a Docker 2>/dev/null || true"
    fi
    # Wait up to 60s for daemon readiness.
    if [[ $DRY_RUN -eq 0 ]]; then
      for _ in $(seq 1 30); do
        if docker info >/dev/null 2>&1; then break; fi
        sleep 2
      done
    fi
    if [[ $DRY_RUN -eq 0 ]] && ! docker info >/dev/null 2>&1; then
      track_fail "Data services (docker daemon)"
      die "docker daemon did not come up" "open -a Docker; then re-run ./install.sh"
    fi
  fi

  # NEO4J_PASSWORD is a :?required variable in docker-compose.yml. If
  # missing, generate a 32-char random one and persist — never overwrite.
  NEO4J_PW_LINE="$(grep '^NEO4J_PASSWORD=' "$ENV_FILE" 2>/dev/null | head -1 | cut -d= -f2- | tr -d ' ' || true)"
  if [[ -z "$NEO4J_PW_LINE" && $DRY_RUN -eq 0 ]]; then
    info "Generating 32-char NEO4J_PASSWORD (preserved thereafter)..."
    NEO4J_PW="$(LC_ALL=C tr -dc 'A-Za-z0-9' </dev/urandom | head -c 32)"
    if sed --version >/dev/null 2>&1; then
      sed -i '/^NEO4J_PASSWORD=$/d; /^NEO4J_PASSWORD= *$/d' "$ENV_FILE"
    else
      sed -i '' '/^NEO4J_PASSWORD=$/d; /^NEO4J_PASSWORD= *$/d' "$ENV_FILE"
    fi
    printf "NEO4J_PASSWORD=%s\n" "$NEO4J_PW" >> "$ENV_FILE"
    chmod 600 "$ENV_FILE"
    ok "NEO4J_PASSWORD generated"
  fi

  info "Starting Qdrant + Neo4j via docker compose..."
  if run_sh "cd '$NLR_ROOT' && docker compose up -d qdrant neo4j"; then
    ok "docker compose up -d qdrant neo4j"
    track_ok "Data services"
  else
    track_fail "Data services"
    die "docker compose failed" "cd $NLR_ROOT && docker compose up -d qdrant neo4j"
  fi
fi

# ═══════════════════════════════════════════════════════════════════════════
step "8/12  Model Downloads (3 GGUFs, ~10 GB)"
# ═══════════════════════════════════════════════════════════════════════════
# download_models.sh handles resume + size verification. huggingface-cli is
# already gated by check_prereqs.sh.

if [[ "$MODE" == "cloud" ]]; then
  info "Cloud mode: skipping local model downloads. Point EMBEDDING_API_URL at your cloud endpoint."
  track_ok "Models (cloud mode, skipped)"
elif [[ -x "$SETUP_SCRIPTS/download_models.sh" ]]; then
  # Quick check: if all three files are present, we can skip the script run
  # entirely (download_models.sh is idempotent but still hits HF API for
  # manifest verification, which is unnecessary when files are complete).
  OCTEN="${NLR_ROOT}/models/Octen-Embedding-8B.Q8_0.gguf"
  QWEN_RERANK="${HOME}/.cache/qmd/models/qwen3-reranker-0.6b-q8_0.gguf"
  QWEN_EXPAND="${HOME}/.cache/qmd/models/qmd-query-expansion-1.7B-q4_k_m.gguf"
  if [[ -s "$OCTEN" && -s "$QWEN_RERANK" && -s "$QWEN_EXPAND" ]]; then
    ok "All 3 GGUF models already present"
    track_ok "Models"
  else
    info "Running download_models.sh (resumes partial downloads)..."
    if run_sh "NLR_ROOT='$NLR_ROOT' bash '$SETUP_SCRIPTS/download_models.sh'"; then
      ok "Models downloaded"
      track_ok "Models"
    else
      track_warn "Models"
      warn "Model download failed — re-run: bash $SETUP_SCRIPTS/download_models.sh"
    fi
  fi
else
  track_warn "Models (script missing)"
  warn "$SETUP_SCRIPTS/download_models.sh not found"
fi

# ═══════════════════════════════════════════════════════════════════════════
step "9/12  llama-server for Octen (port 8400, as system service)"
# ═══════════════════════════════════════════════════════════════════════════
# Install a LaunchAgent (macOS) or systemd user unit (Linux) so Octen comes
# up automatically on login. Cloud mode skips this — embeddings come from
# EMBEDDING_API_URL in secrets/.env.

install_launchagent_mac() {
  local plist="$HOME/Library/LaunchAgents/com.neurolink.llama-server.plist"
  local octen="$NLR_ROOT/models/Octen-Embedding-8B.Q8_0.gguf"
  local llama_bin
  llama_bin="$(command -v llama-server || echo /usr/local/bin/llama-server)"
  mkdir -p "$HOME/Library/LaunchAgents" "$HOME/Library/Logs/neuro-link"

  if [[ $DRY_RUN -eq 1 ]]; then
    dry "write LaunchAgent plist -> $plist (model: $octen, port: 8400)"
    dry "launchctl bootstrap gui/\$UID $plist"
    return 0
  fi

  # Compute thread count once — LaunchAgent plists don't shell-expand.
  local threads
  threads="$(sysctl -n hw.logicalcpu 2>/dev/null || echo 4)"

  cat > "$plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.neurolink.llama-server</string>
    <key>ProgramArguments</key>
    <array>
        <string>${llama_bin}</string>
        <string>--embeddings</string>
        <string>--model</string>
        <string>${octen}</string>
        <string>--port</string>
        <string>8400</string>
        <string>--host</string>
        <string>127.0.0.1</string>
        <string>--ctx-size</string>
        <string>4096</string>
        <string>--batch-size</string>
        <string>512</string>
        <string>--threads</string>
        <string>${threads}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>${HOME}/Library/Logs/neuro-link/llama-server.out.log</string>
    <key>StandardErrorPath</key>
    <string>${HOME}/Library/Logs/neuro-link/llama-server.err.log</string>
    <key>EnvironmentVariables</key>
    <dict>
        <key>PATH</key>
        <string>/usr/local/bin:/opt/homebrew/bin:/usr/bin:/bin</string>
    </dict>
</dict>
</plist>
PLIST

  # Reload (unload-then-load) so param edits on re-run actually take effect.
  launchctl bootout "gui/$UID/com.neurolink.llama-server" 2>/dev/null || true
  launchctl bootstrap "gui/$UID" "$plist" 2>/dev/null \
    || launchctl load "$plist"
  launchctl enable "gui/$UID/com.neurolink.llama-server" 2>/dev/null || true
  launchctl kickstart -k "gui/$UID/com.neurolink.llama-server" 2>/dev/null || true
}

install_systemd_linux() {
  local unit_dir="$HOME/.config/systemd/user"
  local unit="$unit_dir/neurolink-llama-server.service"
  local octen="$NLR_ROOT/models/Octen-Embedding-8B.Q8_0.gguf"
  local llama_bin
  llama_bin="$(command -v llama-server || echo /usr/local/bin/llama-server)"
  mkdir -p "$unit_dir"

  if [[ $DRY_RUN -eq 1 ]]; then
    dry "write systemd user unit -> $unit (model: $octen, port: 8400)"
    dry "systemctl --user daemon-reload && systemctl --user enable --now neurolink-llama-server"
    return 0
  fi

  local threads
  threads="$(nproc 2>/dev/null || echo 4)"

  cat > "$unit" <<UNIT
[Unit]
Description=neuro-link Octen embedding server (llama.cpp)
After=network.target

[Service]
Type=simple
ExecStart=${llama_bin} --embeddings --model ${octen} --port 8400 --host 127.0.0.1 --ctx-size 4096 --batch-size 512 --threads ${threads}
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
UNIT

  systemctl --user daemon-reload
  systemctl --user enable --now neurolink-llama-server.service
}

if [[ "$MODE" == "cloud" ]]; then
  info "Cloud mode: skipping llama-server install. Set EMBEDDING_API_URL in secrets/.env."
  track_ok "llama-server (cloud mode, skipped)"
elif ! command -v llama-server >/dev/null 2>&1; then
  track_warn "llama-server (binary missing)"
  warn "llama-server not on PATH — install llama.cpp (see check_prereqs output) and re-run"
else
  OCTEN="${NLR_ROOT}/models/Octen-Embedding-8B.Q8_0.gguf"
  if [[ ! -s "$OCTEN" && $DRY_RUN -eq 0 ]]; then
    track_warn "llama-server (Octen model missing)"
    warn "Octen GGUF missing at $OCTEN — re-run step 8 (download_models.sh) first"
  elif [[ $IS_MAC -eq 1 ]]; then
    info "Installing macOS LaunchAgent com.neurolink.llama-server..."
    if install_launchagent_mac; then
      ok "LaunchAgent installed (listen 127.0.0.1:8400)"
      track_ok "llama-server (LaunchAgent)"
    else
      track_warn "llama-server (LaunchAgent)"
      warn "LaunchAgent install hit issues — run manually: bash scripts/embedding-server.sh"
    fi
  elif [[ $IS_LINUX -eq 1 ]]; then
    info "Installing systemd user unit neurolink-llama-server..."
    if install_systemd_linux; then
      ok "systemd user unit installed (listen 127.0.0.1:8400)"
      track_ok "llama-server (systemd)"
    else
      track_warn "llama-server (systemd)"
      warn "systemd install hit issues — run manually: bash scripts/embedding-server.sh"
    fi
  else
    track_warn "llama-server ($OS_NAME service unknown)"
    warn "Unsupported OS for service install — run manually: bash scripts/embedding-server.sh"
  fi
fi

# ═══════════════════════════════════════════════════════════════════════════
step "10/12  Install Skills (copy, no symlinks)"
# ═══════════════════════════════════════════════════════════════════════════
# install_skills.sh rsyncs the 10 skills into ~/.claude/skills/, purging
# legacy symlinks from older installs. Plain copy keeps the user's Claude
# Code install self-contained.

if [[ -x "$SETUP_SCRIPTS/install_skills.sh" ]]; then
  if [[ $DRY_RUN -eq 1 ]]; then
    run "$SETUP_SCRIPTS/install_skills.sh" --dry-run
    track_ok "Skills (dry-run)"
  elif NLR_ROOT="$NLR_ROOT" "$SETUP_SCRIPTS/install_skills.sh"; then
    ok "Skills installed to ~/.claude/skills/"
    track_ok "Skills"
  else
    track_warn "Skills"
    warn "install_skills.sh reported errors — re-run: NLR_ROOT=$NLR_ROOT bash $SETUP_SCRIPTS/install_skills.sh"
  fi
else
  track_warn "Skills (script missing)"
  warn "$SETUP_SCRIPTS/install_skills.sh not found"
fi

# ═══════════════════════════════════════════════════════════════════════════
step "11/12  Register MCP Servers in ~/.claude.json"
# ═══════════════════════════════════════════════════════════════════════════
# install_mcp_servers.sh uses jq to merge three entries into ~/.claude.json
# without clobbering pre-existing servers. It also backs up the file first.

if [[ -x "$SETUP_SCRIPTS/install_mcp_servers.sh" ]]; then
  if [[ $DRY_RUN -eq 1 ]]; then
    dry "$SETUP_SCRIPTS/install_mcp_servers.sh  (merges 3 entries into ~/.claude.json)"
    track_ok "MCP registration (dry-run)"
  elif NLR_ROOT="$NLR_ROOT" NLR_BIN="$BINARY_PATH" "$SETUP_SCRIPTS/install_mcp_servers.sh"; then
    ok "MCP servers registered"
    track_ok "MCP registration"
  else
    track_warn "MCP registration"
    warn "install_mcp_servers.sh reported errors — see ~/.claude.json.bak.*"
  fi
else
  track_warn "MCP registration (script missing)"
  warn "$SETUP_SCRIPTS/install_mcp_servers.sh not found"
fi

# ═══════════════════════════════════════════════════════════════════════════
step "12/12  Verify (status.sh)"
# ═══════════════════════════════════════════════════════════════════════════
# status.sh runs a fast health probe. It expects services to be up — note
# that on a fresh install the neuro-link HTTP server (port 8787) and
# turbovault (port 3001) are NOT started here; they're started by
# `neuro-link serve` and `turbovault serve` respectively, which the user
# runs on demand. status.sh colour-codes those as WARN, not FAIL.

if [[ -x "$STATUS_SCRIPT" ]]; then
  if [[ $DRY_RUN -eq 1 ]]; then
    dry "$STATUS_SCRIPT"
  else
    # status.sh exits 1 on any critical failure — we treat that as a warning
    # for the installer's purposes (partial install is informative).
    echo ""
    NLR_ROOT="$NLR_ROOT" "$STATUS_SCRIPT" || warn "status.sh reported some services as down (see above)"
  fi
else
  warn "$STATUS_SCRIPT not found — skipping verification"
fi

# ═══════════════════════════════════════════════════════════════════════════
# Optional: public tunnel (gated behind --with-tunnel)
# ═══════════════════════════════════════════════════════════════════════════
# start_public_tunnel.sh runs in the foreground and blocks — so we only
# invoke it when explicitly requested. It also requires a valid
# NGROK_AUTH_TOKEN and that TurboVault is already running on :3001.

if [[ $WITH_TUNNEL -eq 1 ]]; then
  step "Optional  Public tunnel (Caddy + ngrok)"
  TUNNEL_SCRIPT="$NLR_ROOT/scripts/start_public_tunnel.sh"
  if [[ -x "$TUNNEL_SCRIPT" ]]; then
    if [[ $DRY_RUN -eq 1 ]]; then
      dry "$TUNNEL_SCRIPT  (blocks in foreground until SIGINT)"
    else
      info "Starting tunnel — Ctrl-C to stop (runs Caddy + ngrok)..."
      exec "$TUNNEL_SCRIPT"
    fi
  else
    warn "$TUNNEL_SCRIPT not found or not executable"
  fi
fi

# ═══════════════════════════════════════════════════════════════════════════
step "Summary"
# ═══════════════════════════════════════════════════════════════════════════

echo ""
printf "${GREEN}${BOLD}Installed:${NC}\n"
for item in "${SUMMARY_OK[@]+"${SUMMARY_OK[@]}"}"; do
  printf "  ${GREEN}+${NC} %s\n" "$item"
done

if [[ ${#SUMMARY_WARN[@]} -gt 0 ]]; then
  echo ""
  printf "${YELLOW}${BOLD}Warnings:${NC}\n"
  for item in "${SUMMARY_WARN[@]}"; do
    printf "  ${YELLOW}~${NC} %s\n" "$item"
  done
fi

if [[ ${#SUMMARY_FAIL[@]} -gt 0 ]]; then
  echo ""
  printf "${RED}${BOLD}Failed:${NC}\n"
  for item in "${SUMMARY_FAIL[@]}"; do
    printf "  ${RED}x${NC} %s\n" "$item"
  done
fi

echo ""
info "Config:   edit ${ENV_FILE} to fill in API keys"
info "Docs:     see INSTALL.md for full reference"
info "Status:   bash $STATUS_SCRIPT"
info "Uninstall: bash $NLR_ROOT/scripts/uninstall.sh  (keeps data intact)"
echo ""

# Non-zero exit if any step reported FAIL (WARN is tolerated).
if [[ ${#SUMMARY_FAIL[@]} -gt 0 ]]; then
  exit 1
fi
exit 0
