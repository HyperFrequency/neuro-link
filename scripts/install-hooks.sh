#!/usr/bin/env bash
# install-hooks.sh — register neuro-link-recursive hooks in a CLI client's config.
#
# Usage: ./scripts/install-hooks.sh <client> [--link-project]
#   client ∈ {claude-code}  (cline/forge-code/claw-code/openclaw removed)
#
# Options:
#   --link-project   For claude-code only: create symlinks from
#                    ~/.claude/hooks/<name>.sh -> <repo>/hooks/<name>.sh for
#                    the top-level project hooks (auto-rag-inject.sh,
#                    harness-bridge-check.sh, neuro-grade.sh,
#                    neuro-log-tool-use.sh, neuro-task-check.sh). This is
#                    de-shadow mode: edits in the repo propagate without a
#                    re-copy. Default (omitting this flag) keeps the
#                    existing per-client settings.json registration behavior.
#
# Behaviour:
#   1. Validates the hook scripts exist in hooks/clients/<client>/
#   2. Locates the client's config file
#   3. Backs up the config (timestamped) before editing
#   4. Adds hooks only if not already present (idempotent)
#
# Never overwrites. Never removes user-defined hooks. Always reports what changed.
set -euo pipefail

# --- Colors ---
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

ok()   { printf "${GREEN}[OK]${NC}   %s\n" "$1"; }
warn() { printf "${YELLOW}[WARN]${NC} %s\n" "$1"; }
fail() { printf "${RED}[FAIL]${NC} %s\n" "$1"; }
info() { printf "${CYAN}[INFO]${NC} %s\n" "$1"; }
step() { printf "\n${BOLD}=== %s ===${NC}\n" "$1"; }

die() { fail "$1"; exit 1; }

usage() {
  cat <<EOF
Usage: $0 <client> [--link-project]

Registers neuro-link-recursive hooks in the target client's config.

Supported clients:
  claude-code   ~/.claude/settings.json

(Previously supported cline, forge-code, claw-code, openclaw were removed —
see hooks/clients/README.md for the replacement pattern.)

Options:
  --link-project   (claude-code only) symlink project hooks in hooks/*.sh
                   into ~/.claude/hooks/ so edits in-repo propagate without
                   copying. Default behavior (no flag) is the existing
                   settings.json registration.

The script is idempotent — re-running will not duplicate hooks.
EOF
  exit 1
}

[[ $# -ge 1 ]] || usage
CLIENT="$1"
shift || true
LINK_PROJECT=0
for arg in "$@"; do
  case "$arg" in
    --link-project) LINK_PROJECT=1 ;;
    *) die "Unknown option: $arg" ;;
  esac
done

# Resolve repo root = parent of scripts/
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
HOOKS_DIR="${REPO_ROOT}/hooks/clients/${CLIENT}"

# --link-project only touches top-level hooks/*.sh (no per-client dir needed).
# The per-client sanity check is deferred to the dispatch (below) so that
# removed clients can emit a clear "client removed" error rather than a
# "no hook scripts found" one.
if [[ "$LINK_PROJECT" -eq 0 && "$CLIENT" == "claude-code" ]]; then
  [[ -d "$HOOKS_DIR" ]] || die "No hook scripts found for client '${CLIENT}' at ${HOOKS_DIR}"

  # Check required scripts exist
  for script in pre_tool.sh post_tool.sh user_prompt.sh session_start.sh session_end.sh; do
    [[ -f "${HOOKS_DIR}/${script}" ]] || die "Missing hook script: ${HOOKS_DIR}/${script}"
  done
fi

backup_file() {
  local target="$1"
  if [[ -f "$target" ]]; then
    local ts
    ts="$(date -u +%Y%m%dT%H%M%SZ)"
    cp -a "$target" "${target}.bak.${ts}"
    info "Backed up ${target} -> ${target}.bak.${ts}"
  fi
}

ensure_config_dir() {
  local cfg="$1"
  mkdir -p "$(dirname "$cfg")"
}

# === Claude Code / Claw-Code / OpenClaw (share the same settings.json format) ===
install_claude_like() {
  local cfg="$1"
  ensure_config_dir "$cfg"
  backup_file "$cfg"

  if [[ ! -f "$cfg" ]]; then
    echo '{}' > "$cfg"
  fi

  python3 - "$cfg" "$HOOKS_DIR" <<'PY'
import json, sys, os

cfg_path = sys.argv[1]
hooks_dir = sys.argv[2]

try:
    with open(cfg_path) as f:
        cfg = json.load(f)
except Exception:
    cfg = {}

cfg.setdefault("hooks", {})

# Map hook events -> (event key in settings.json, script filename)
event_map = [
    ("PreToolUse",       "pre_tool.sh"),
    ("PostToolUse",      "post_tool.sh"),
    ("UserPromptSubmit", "user_prompt.sh"),
    ("SessionStart",     "session_start.sh"),
    ("SessionEnd",       "session_end.sh"),
]

added = []
for event, script in event_map:
    cmd = os.path.join(hooks_dir, script)
    existing = cfg["hooks"].get(event, [])
    if not isinstance(existing, list):
        existing = []
    # Check if a matcher block for this script already exists
    already = any(
        any(
            h.get("type") == "command" and h.get("command") == cmd
            for h in (block.get("hooks") or [])
        )
        for block in existing
    )
    if already:
        continue
    existing.append({
        "matcher": "*",
        "hooks": [
            {"type": "command", "command": cmd},
        ],
    })
    cfg["hooks"][event] = existing
    added.append(event)

with open(cfg_path, "w") as f:
    json.dump(cfg, f, indent=2)

if added:
    print("ADDED:" + ",".join(added))
else:
    print("NOCHANGE")
PY
}

# === Cline ===
install_cline() {
  local cfg="$1"
  ensure_config_dir "$cfg"
  backup_file "$cfg"

  if [[ ! -f "$cfg" ]]; then
    echo '{}' > "$cfg"
  fi

  python3 - "$cfg" "$HOOKS_DIR" <<'PY'
import json, sys, os

cfg_path = sys.argv[1]
hooks_dir = sys.argv[2]

try:
    with open(cfg_path) as f:
        cfg = json.load(f)
except Exception:
    cfg = {}

cfg.setdefault("hooks", {})

event_map = [
    ("preToolUse",       "pre_tool.sh"),
    ("postToolUse",      "post_tool.sh"),
    ("userPromptSubmit", "user_prompt.sh"),
    ("sessionStart",     "session_start.sh"),
    ("sessionEnd",       "session_end.sh"),
]

added = []
for event, script in event_map:
    cmd = os.path.join(hooks_dir, script)
    existing = cfg["hooks"].get(event, [])
    if not isinstance(existing, list):
        existing = []
    if any(h.get("command") == cmd for h in existing if isinstance(h, dict)):
        continue
    existing.append({"command": cmd})
    cfg["hooks"][event] = existing
    added.append(event)

with open(cfg_path, "w") as f:
    json.dump(cfg, f, indent=2)

print("ADDED:" + ",".join(added) if added else "NOCHANGE")
PY
}

# === Forge-Code (YAML config) ===
install_forge() {
  local cfg="$1"
  ensure_config_dir "$cfg"
  backup_file "$cfg"

  if [[ ! -f "$cfg" ]]; then
    echo '' > "$cfg"
  fi

  python3 - "$cfg" "$HOOKS_DIR" <<'PY'
import sys, os
try:
    import yaml
except Exception:
    print("ERROR: PyYAML not installed. Run: pip3 install pyyaml")
    sys.exit(1)

cfg_path = sys.argv[1]
hooks_dir = sys.argv[2]

try:
    with open(cfg_path) as f:
        cfg = yaml.safe_load(f) or {}
except Exception:
    cfg = {}

if not isinstance(cfg, dict):
    cfg = {}

cfg.setdefault("hooks", {})

# Forge calls hook events via bash command strings. We use the documented
# forge hook keys; unrecognised keys are still parsed but will no-op at runtime.
event_map = [
    ("on_before_tool",    "pre_tool.sh"),
    ("on_after_tool",     "post_tool.sh"),
    ("on_user_prompt",    "user_prompt.sh"),
    ("on_session_start",  "session_start.sh"),
    ("on_session_end",    "session_end.sh"),
]

added = []
for event, script in event_map:
    cmd = f"bash {os.path.join(hooks_dir, script)}"
    existing = cfg["hooks"].get(event)
    if existing == cmd:
        continue
    if isinstance(existing, list) and cmd in existing:
        continue
    cfg["hooks"][event] = cmd
    added.append(event)

with open(cfg_path, "w") as f:
    yaml.safe_dump(cfg, f, sort_keys=False)

print("ADDED:" + ",".join(added) if added else "NOCHANGE")
PY
}

# === Project-hook symlinks (claude-code --link-project) ===
# Symlinks ~/.claude/hooks/<name>.sh -> <repo>/hooks/<name>.sh for every
# top-level *.sh file under hooks/. De-shadows the dead project hooks by
# routing the global config's ~/.claude/hooks/ references through the repo.
link_project_hooks() {
  local repo_hooks="${REPO_ROOT}/hooks"
  local target_dir="${HOME}/.claude/hooks"
  mkdir -p "$target_dir"
  local linked=0 skipped=0
  for src in "${repo_hooks}"/*.sh; do
    [[ -f "$src" ]] || continue
    local name
    name="$(basename "$src")"
    local dst="${target_dir}/${name}"
    if [[ -L "$dst" ]]; then
      local cur
      cur="$(readlink "$dst")"
      if [[ "$cur" == "$src" ]]; then
        skipped=$((skipped+1))
        continue
      fi
      info "Replacing stale symlink: $dst -> $cur"
      rm -f "$dst"
    elif [[ -e "$dst" ]]; then
      backup_file "$dst"
      rm -f "$dst"
    fi
    ln -s "$src" "$dst"
    ok "Linked ${name} -> ${src}"
    linked=$((linked+1))
  done
  info "Project-hook symlinks: linked=${linked} unchanged=${skipped}"
}

if [[ "$LINK_PROJECT" -eq 1 ]]; then
  if [[ "$CLIENT" != "claude-code" ]]; then
    die "--link-project is only supported for client 'claude-code'"
  fi
  step "Linking project hooks from ${REPO_ROOT}/hooks/ -> ~/.claude/hooks/"
  link_project_hooks
  step "Done (--link-project)"
  info "The global ~/.claude/settings.json can now reference hooks at"
  info "~/.claude/hooks/<name>.sh and they resolve to the in-repo scripts."
  exit 0
fi

# --- Dispatch ---
step "Installing neuro-link-recursive hooks for: ${CLIENT}"

case "$CLIENT" in
  claude-code)
    CFG="${HOME}/.claude/settings.json"
    result="$(install_claude_like "$CFG")"
    ;;
  claw-code|openclaw|cline|forge-code)
    die "Client '${CLIENT}' was removed — see hooks/clients/README.md. Only 'claude-code' is supported."
    ;;
  *)
    die "Unknown client '${CLIENT}'. Run '$0' without args for usage."
    ;;
esac

info "Config: ${CFG}"

case "$result" in
  ADDED:*)
    added_events="${result#ADDED:}"
    ok "Registered hooks for events: ${added_events}"
    ;;
  NOCHANGE)
    ok "All hooks already present — no changes made."
    ;;
  ERROR:*)
    fail "${result#ERROR:}"
    exit 1
    ;;
  *)
    warn "Unexpected installer output: ${result}"
    ;;
esac

step "Next steps"
cat <<EOF
  1. Ensure the neuro-link server is running:        neuro-link serve
  2. Export NLR_API_TOKEN or write it to secrets/.env
     (export NLR_API_TOKEN="<your-token>")
  3. Optionally override the hooks URL:
     export NLR_HOOKS_URL="http://localhost:8080/api/v1/hooks/event"
  4. Start ${CLIENT} as usual — hooks will POST events and receive RAG context.

  To inspect captured events:   neuro-link mcp then call nlr_hooks_log_list
                                or:   tail -f state/hooks_log.jsonl
EOF
