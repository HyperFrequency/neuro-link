#!/usr/bin/env bash
# neuro-link-recursive — uninstall / teardown companion to install.sh.
#
# Reverses what install.sh set up, in roughly reverse order.
# Explicitly DOES NOT:
#   - delete secrets/.env
#   - delete model files under models/
#   - delete docker volumes (qdrant + neo4j data is preserved)
#   - uninstall the turbovault cargo binary (it may be used by other projects)
#   - remove ~/.claude entirely (other projects live there)
#
# What it does:
#   - stop + remove the LaunchAgent (macOS) / systemd user unit (Linux) for
#     llama-server
#   - stop + remove the qdrant + neo4j containers (NOT volumes)
#   - remove ONLY the neuro-link skills from ~/.claude/skills/
#   - remove the three MCP server entries from ~/.claude.json (via jq)
#   - remove ~/.claude/state/nlr_root
#
# Flags:
#   --dry-run       Print what would happen without doing it
#   --purge-data    Also remove docker VOLUMES (DESTRUCTIVE; asks again)
#   --purge-models  Also remove models/ GGUF files
#   --purge-secrets Also remove secrets/.env
#   --yes           Skip all confirmation prompts (non-interactive)
#   -h, --help      Show this message

set -euo pipefail

NLR_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export NLR_ROOT

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

DRY_RUN=0
PURGE_DATA=0
PURGE_MODELS=0
PURGE_SECRETS=0
ASSUME_YES=0

print_help() {
  sed -n '2,25p' "$0" | sed 's/^# //; s/^#//'
  exit 0
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run)       DRY_RUN=1 ;;
    --purge-data)    PURGE_DATA=1 ;;
    --purge-models)  PURGE_MODELS=1 ;;
    --purge-secrets) PURGE_SECRETS=1 ;;
    --yes|-y)        ASSUME_YES=1 ;;
    -h|--help)       print_help ;;
    *) warn "unknown flag: $1" ;;
  esac
  shift
done

confirm() {
  local prompt="$1"
  [[ $ASSUME_YES -eq 1 ]] && return 0
  [[ $DRY_RUN -eq 1 ]] && return 0
  printf "%b%s%b [y/N] " "$YELLOW" "$prompt" "$NC"
  read -r r
  [[ "$r" =~ ^[Yy]$ ]]
}

run() {
  if [[ $DRY_RUN -eq 1 ]]; then
    dry "$*"
    return 0
  fi
  "$@"
}

run_sh() {
  if [[ $DRY_RUN -eq 1 ]]; then
    dry "sh -c: $*"
    return 0
  fi
  bash -c "$*"
}

OS_NAME="$(uname -s)"

echo ""
printf "%bneuro-link-recursive uninstaller%b\n" "$BOLD" "$NC"
printf "  root:     %s\n" "$NLR_ROOT"
printf "  os:       %s\n" "$OS_NAME"
printf "  dry-run:  %s\n" "$([[ $DRY_RUN -eq 1 ]] && echo yes || echo no)"
echo ""

if [[ $ASSUME_YES -ne 1 && $DRY_RUN -ne 1 ]]; then
  confirm "Proceed with uninstall? This keeps your data/vault intact." || {
    info "Aborted."
    exit 0
  }
fi

# ── 1. Stop llama-server service ────────────────────────────────────────────
step "1/6  Stop llama-server service"
case "$OS_NAME" in
  Darwin)
    PLIST="$HOME/Library/LaunchAgents/com.neurolink.llama-server.plist"
    if [[ -f "$PLIST" ]]; then
      run_sh "launchctl bootout gui/$UID/com.neurolink.llama-server 2>/dev/null || launchctl unload '$PLIST' 2>/dev/null || true"
      run rm -f "$PLIST"
      ok "Removed LaunchAgent com.neurolink.llama-server"
    else
      info "No LaunchAgent at $PLIST"
    fi
    ;;
  Linux)
    UNIT="$HOME/.config/systemd/user/neurolink-llama-server.service"
    if [[ -f "$UNIT" ]]; then
      run_sh "systemctl --user disable --now neurolink-llama-server.service 2>/dev/null || true"
      run rm -f "$UNIT"
      run_sh "systemctl --user daemon-reload"
      ok "Removed systemd user unit neurolink-llama-server"
    else
      info "No systemd user unit at $UNIT"
    fi
    ;;
  *)
    warn "Unsupported OS for service teardown; skipping"
    ;;
esac

# ── 2. Stop data services ───────────────────────────────────────────────────
step "2/6  Stop data services (Qdrant + Neo4j)"
if command -v docker >/dev/null 2>&1 && [[ -f "$NLR_ROOT/docker-compose.yml" ]]; then
  if [[ $PURGE_DATA -eq 1 ]]; then
    if confirm "DESTRUCTIVE: remove docker VOLUMES for qdrant + neo4j? This deletes all embeddings and graph data."; then
      run_sh "cd '$NLR_ROOT' && docker compose down -v qdrant neo4j"
      ok "Containers + volumes removed"
    else
      info "Keeping volumes; removing containers only"
      run_sh "cd '$NLR_ROOT' && docker compose down qdrant neo4j"
    fi
  else
    run_sh "cd '$NLR_ROOT' && docker compose down qdrant neo4j"
    ok "Containers stopped (volumes preserved; use --purge-data to remove)"
  fi
else
  info "docker or docker-compose.yml missing; skipping"
fi

# ── 3. Remove neuro-link skills from ~/.claude/skills/ ──────────────────────
step "3/6  Remove neuro-link skills"
SKILLS_DIR="$HOME/.claude/skills"
NLR_SKILLS=(
  neuro-link-setup
  neuro-link
  recursive-self-improvement
  neuro-scan
  neuro-surgery
  hyper-sleep
  crawl-ingest-update
  main-codebase-tools
  adjacent-tools-code-docs
  forked-repos-with-changes
)
removed=0
for s in "${NLR_SKILLS[@]}"; do
  if [[ -e "$SKILLS_DIR/$s" ]]; then
    run rm -rf "$SKILLS_DIR/$s"
    removed=$((removed + 1))
  fi
done
if [[ $removed -gt 0 ]]; then
  ok "Removed $removed neuro-link skills from $SKILLS_DIR"
else
  info "No neuro-link skills present in $SKILLS_DIR"
fi

# ── 4. Remove MCP server entries from ~/.claude.json ────────────────────────
step "4/6  De-register MCP servers"
CLAUDE_JSON="$HOME/.claude.json"
if [[ -f "$CLAUDE_JSON" ]] && command -v jq >/dev/null 2>&1; then
  BACKUP="$CLAUDE_JSON.bak.uninstall.$(date +%s)"
  run cp "$CLAUDE_JSON" "$BACKUP"
  if [[ $DRY_RUN -eq 0 ]]; then
    jq 'del(.mcpServers["neuro-link-recursive"])
        | del(.mcpServers["neuro-link-http"])
        | del(.mcpServers["turbovault"])' "$CLAUDE_JSON" > "$CLAUDE_JSON.new"
    mv "$CLAUDE_JSON.new" "$CLAUDE_JSON"
  else
    dry "jq delete mcpServers.{neuro-link-recursive,neuro-link-http,turbovault} from $CLAUDE_JSON"
  fi
  ok "De-registered 3 MCP entries (backup: $BACKUP)"
else
  info "$CLAUDE_JSON or jq missing; skipping"
fi

# ── 5. Remove persisted state ───────────────────────────────────────────────
step "5/6  Remove persisted state"
NLR_ROOT_FILE="$HOME/.claude/state/nlr_root"
if [[ -f "$NLR_ROOT_FILE" ]]; then
  run rm -f "$NLR_ROOT_FILE"
  ok "Removed $NLR_ROOT_FILE"
else
  info "No state file at $NLR_ROOT_FILE"
fi

# ── 6. Optional purges ──────────────────────────────────────────────────────
step "6/6  Optional purges"

if [[ $PURGE_MODELS -eq 1 ]]; then
  if confirm "DESTRUCTIVE: delete all GGUFs under $NLR_ROOT/models/ and ~/.cache/qmd/models/? (~10 GB)"; then
    run rm -rf "$NLR_ROOT/models/"*.gguf "$HOME/.cache/qmd/models/"*.gguf
    ok "Models removed"
  fi
else
  info "Models preserved ($NLR_ROOT/models/ + ~/.cache/qmd/models/). Use --purge-models to remove."
fi

if [[ $PURGE_SECRETS -eq 1 ]]; then
  if confirm "DESTRUCTIVE: delete $NLR_ROOT/secrets/.env (API keys + tokens)?"; then
    run rm -f "$NLR_ROOT/secrets/.env"
    ok "Secrets removed"
  fi
else
  info "Secrets preserved ($NLR_ROOT/secrets/.env). Use --purge-secrets to remove."
fi

echo ""
info "Uninstall complete."
info "Vault content under $NLR_ROOT's NN-*/ directories was not touched."
info "The turbovault cargo binary at ~/.cargo/bin/turbovault is still installed."
info "To fully remove: cargo uninstall turbovault"
echo ""
