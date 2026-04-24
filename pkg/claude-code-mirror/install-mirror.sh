#!/usr/bin/env bash
# install-mirror.sh — mirror the user's Claude Code config into ~/.claude/
#
# What it does:
#   1. Copies hooks/*.sh → ~/.claude/hooks/ (merge mode: never overwrite
#      unless NLR_MIRROR_FORCE=1; existing files with same content no-op,
#      divergent files get a warning + .new side-file).
#   2. Substitutes ${HOME} in settings.template.json and jq-merges the
#      result into ~/.claude/settings.json (preserves your own permissions
#      and plugin config).
#   3. Invokes the existing install_mcp_servers.sh for the three core
#      neuro-link MCP servers (neuro-link-http, neuro-link-recursive,
#      serena). Optional entries in mcp-servers.yaml (turbovault, context7,
#      infranodus) are registered if their prerequisites are met.
#
# Safety:
#   - Creates a backup of ~/.claude/settings.json before any mutation.
#   - Never reads or prints secret values from settings.json.
#   - NLR_MIRROR_DRY_RUN=1 prints all actions without executing them.

set -euo pipefail

MIRROR_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="${NLR_ROOT:-$(cd "$MIRROR_DIR/../.." && pwd)}"
CLAUDE_HOME="${CLAUDE_HOME:-$HOME/.claude}"
SETTINGS="$CLAUDE_HOME/settings.json"
HOOKS_DIR="$CLAUDE_HOME/hooks"
DRY_RUN="${NLR_MIRROR_DRY_RUN:-0}"
FORCE="${NLR_MIRROR_FORCE:-0}"

log() { printf "[claude-code-mirror] %s\n" "$*" >&2; }
run() {
  if [[ "$DRY_RUN" == "1" ]]; then
    log "DRY: $*"
  else
    "$@"
  fi
}

# --- Pre-flight ---
for bin in jq envsubst; do
  if ! command -v "$bin" >/dev/null 2>&1; then
    log "ERROR: '$bin' not on PATH. brew install jq gettext (gettext provides envsubst)."
    exit 1
  fi
done

# --- 1. Warn before touching an existing ~/.claude/ ---
if [[ -d "$CLAUDE_HOME" && "$FORCE" != "1" ]]; then
  log "Existing ~/.claude/ detected at $CLAUDE_HOME."
  log "  settings.json: $([[ -f $SETTINGS ]] && echo present || echo missing)"
  log "  hooks/:       $([[ -d $HOOKS_DIR ]] && echo "$(find "$HOOKS_DIR" -maxdepth 1 -name '*.sh' | wc -l | tr -d ' ') hook(s)" || echo missing)"
  log "This installer MERGES — existing entries preserved. Set NLR_MIRROR_FORCE=1 to overwrite divergent files."
fi

run mkdir -p "$HOOKS_DIR"

# --- 2. Merge hooks/ — never overwrite divergent files unless forced ---
log "Merging hooks/ → $HOOKS_DIR"
for src in "$MIRROR_DIR"/hooks/*.sh; do
  [[ -f "$src" ]] || continue
  name="$(basename "$src")"
  dst="$HOOKS_DIR/$name"
  if [[ -f "$dst" ]]; then
    if cmp -s "$src" "$dst"; then
      log "  = $name (identical, skip)"
      continue
    fi
    if [[ "$FORCE" == "1" ]]; then
      log "  ! $name (divergent, OVERWRITE via NLR_MIRROR_FORCE=1)"
      run cp "$src" "$dst"
    else
      log "  ! $name (divergent, writing $name.new — merge by hand or re-run with NLR_MIRROR_FORCE=1)"
      run cp "$src" "$dst.new"
    fi
  else
    log "  + $name"
    run cp "$src" "$dst"
  fi
  run chmod +x "$dst"
done

# --- 3. Substitute ${HOME} in settings.template.json and jq-merge ---
log "Applying settings.template.json → $SETTINGS"
TEMPLATE="$MIRROR_DIR/settings.template.json"
if [[ ! -f "$TEMPLATE" ]]; then
  log "ERROR: missing $TEMPLATE"
  exit 1
fi

TMP_RENDERED=$(mktemp -t nlr-mirror-settings.XXXXXX.json)
trap "rm -f '$TMP_RENDERED'" EXIT

# envsubst expands ${HOME}. The template also contains literal JSON that
# we must not mangle, so restrict to explicit vars.
HOME="$HOME" envsubst '${HOME}' < "$TEMPLATE" > "$TMP_RENDERED"

# Strip the top-level _comment key — it exists for template readability only.
jq 'del(._comment)' "$TMP_RENDERED" > "$TMP_RENDERED.clean" && mv "$TMP_RENDERED.clean" "$TMP_RENDERED"

if [[ -f "$SETTINGS" ]]; then
  BACKUP="$SETTINGS.bak.$(date +%s)"
  log "  backing up $SETTINGS → $BACKUP"
  run cp "$SETTINGS" "$BACKUP"
  # Merge: template entries extend existing ones. permissions.allow is
  # concatenated + deduped; hook event arrays are merged per-matcher
  # (grouping entries by `.matcher` so custom + template entries that
  # target the same matcher collapse into a single block with their
  # nested `.hooks` arrays concatenated and deduped by `.command`).
  # The earlier simpler `unique` on the outer array left duplicate
  # matcher blocks, causing shared commands to fire twice on retry.
  MERGED=$(mktemp -t nlr-mirror-merged.XXXXXX.json)
  jq -s '
    .[0] as $existing | .[1] as $template |
    ($existing.permissions.allow // []) as $a |
    ($template.permissions.allow // []) as $b |
    ($existing.hooks // {}) as $eh |
    ($template.hooks // {}) as $th |
    (($eh | keys_unsorted) + ($th | keys_unsorted) | unique) as $events |
    ($existing * $template)
    | .permissions.allow = (($a + $b) | unique)
    | .hooks = (
        reduce $events[] as $e ({};
          .[$e] = (
            (($eh[$e] // []) + ($th[$e] // []))
            | group_by(.matcher)
            | map(
                { matcher: (.[0].matcher),
                  hooks: (map(.hooks // []) | add | unique_by(.command)) }
                | if .matcher == null then del(.matcher) else . end
              )
          )
        )
      )
    ' "$SETTINGS" "$TMP_RENDERED" > "$MERGED" || {
    log "  jq merge failed — leaving $SETTINGS untouched; rendered template at $TMP_RENDERED"
    exit 1
  }
  run mv "$MERGED" "$SETTINGS"
else
  log "  no existing settings.json — installing fresh"
  run cp "$TMP_RENDERED" "$SETTINGS"
fi

# --- 4. Register MCP servers ---
log "Registering MCP servers via install_mcp_servers.sh"
MCP_INSTALLER="$REPO_ROOT/.claude/skills/neuro-link-setup/scripts/install_mcp_servers.sh"
if [[ -x "$MCP_INSTALLER" ]]; then
  run bash "$MCP_INSTALLER"
else
  log "  WARNING: $MCP_INSTALLER not found or not executable; skipping MCP registration."
fi

# --- 5. Post-install MCP validation ---
# Validate every REQUIRED MCP entry in ~/.claude.json is structurally
# sound (object with non-empty .command string) AND operationally viable
# (the resolved command is executable). Key-presence alone is not
# enough: a stale entry left behind by an earlier install will pass
# presence but fail at runtime, leaving the gate green and the stack
# broken. Malformed/missing serena gets normalized to the canonical
# shape from mcp-servers.yaml (~/.local/bin/serena-mcp, stdio, no args).
CLAUDE_JSON="${HOME}/.claude.json"
REQUIRED_MCP=("neuro-link-recursive" "neuro-link-http" "serena")

_expand_cmd() {
  # Expand the env-var placeholders we actually write into config
  # (${HOME}, $HOME, leading ~). Exotic forms are intentionally left
  # alone — they'll fail the -x check below, which is correct.
  local v="$1"
  v="${v//\$\{HOME\}/$HOME}"
  v="${v//\$HOME/$HOME}"
  v="${v/#~/$HOME}"
  printf '%s' "$v"
}

is_mcp_entry_valid() {
  local srv="$1"
  jq -e --arg s "$srv" '
    .mcpServers[$s] as $e
    | if ($e | type) != "object" then false
      elif ($e.command | type) != "string" then false
      elif ($e.command | length) == 0 then false
      else true end
    ' "$CLAUDE_JSON" >/dev/null 2>&1 || return 1
  local cmd
  cmd=$(jq -r --arg s "$srv" '.mcpServers[$s].command' "$CLAUDE_JSON")
  cmd="$(_expand_cmd "$cmd")"
  if [[ "$cmd" == /* ]]; then
    [[ -x "$cmd" ]]
  else
    command -v "$cmd" >/dev/null 2>&1
  fi
}

collect_missing_mcp() {
  local out=()
  for srv in "${REQUIRED_MCP[@]}"; do
    is_mcp_entry_valid "$srv" || out+=("$srv")
  done
  echo "${out[*]}"
}

if [[ "$DRY_RUN" == "1" ]]; then
  log "DRY: would validate MCP servers (structural + operational): ${REQUIRED_MCP[*]}"
elif [[ ! -f "$CLAUDE_JSON" ]]; then
  log "ERROR: $CLAUDE_JSON does not exist — MCP registration failed upstream."
  exit 3
else
  MISSING="$(collect_missing_mcp)"
  if [[ " $MISSING " == *" serena "* ]]; then
    log "  serena MCP missing or malformed in $CLAUDE_JSON — normalizing to canonical"
    cp "$CLAUDE_JSON" "$CLAUDE_JSON.bak.$(date +%s)"
    SERENA_ENTRY_JSON=$(jq -n --arg home "$HOME" '{
      type: "stdio",
      command: ($home + "/.local/bin/serena-mcp"),
      args: []
    }')
    jq --argjson entry "$SERENA_ENTRY_JSON" '.mcpServers.serena = $entry' \
      "$CLAUDE_JSON" > "$CLAUDE_JSON.new" && mv "$CLAUDE_JSON.new" "$CLAUDE_JSON"
    MISSING="$(collect_missing_mcp)"
  fi
  if [[ -n "$MISSING" ]]; then
    log "ERROR: required MCP servers missing or not executable in $CLAUDE_JSON: $MISSING"
    log "  For each failing server, confirm the .command path resolves to an executable file."
    log "  Canonical serena binary: \$HOME/.local/bin/serena-mcp (install via 'pipx install serena-mcp')."
    log "  Canonical neuro-link binaries: \$NLR_ROOT/server/target/release/neuro-link (cargo build --release in server/)."
    exit 3
  fi
  log "  MCP validation OK (structural + operational): ${REQUIRED_MCP[*]}"
fi

log "DONE. Review $SETTINGS and restart Claude Code to pick up hook changes."
