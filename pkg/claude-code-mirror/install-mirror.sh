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

# --- MCP validation helpers + pre-mutation guard ---
# Runs BEFORE sections 1-3 (hooks merge, settings merge) so if a
# required MCP entry is present-but-malformed we exit 3 with ZERO
# mutations anywhere: not hooks, not settings.json, not ~/.claude.json.
# install_mcp_servers.sh (called by section 4) does a destructive
# `jq -s '.[0] * .[1]'` merge that would overwrite a user's custom
# neuro-link-http or neuro-link-recursive entries, so catching the
# invalid-but-present case before any write is the only way to
# guarantee atomic retries.
CLAUDE_JSON="${HOME}/.claude.json"
# Gate-10 BB1: serena moved from REQUIRED to OPTIONAL. Clean clones on
# a real pristine host don't have ~/.local/bin/serena-mcp; install-mirror
# used to exit 3 at post-install MCP validation on that host, blocking
# `make all`. serena is genuinely high-value but not a hard dep — it's
# installed separately via `pipx install serena-mcp`. Required servers
# are only the two neuro-link ones that install_mcp_servers.sh itself
# creates. Optional servers are auto-registered when missing AND their
# binary is on disk; when the binary is absent, the installer skips
# them with a notice instead of failing.
REQUIRED_MCP=("neuro-link-recursive" "neuro-link-http")
OPTIONAL_MCP=("serena")

_expand_cmd() {
  # Expand the env-var placeholders we actually write into config
  # (${HOME}, $HOME, leading ~). Exotic forms fall through and fail
  # the -x check below, which is the correct outcome.
  local v="$1"
  v="${v//\$\{HOME\}/$HOME}"
  v="${v//\$HOME/$HOME}"
  v="${v/#~/$HOME}"
  printf '%s' "$v"
}

is_mcp_entry_valid() {
  # Validate by transport type. stdio entries need an executable
  # .command; http/sse entries need a string .url matching ^https?://.
  local srv="$1"
  local entry_type
  entry_type=$(jq -r --arg s "$srv" \
    '.mcpServers[$s].type // (if (.mcpServers[$s].url | type) == "string" then "http" else "stdio" end) // "stdio"' \
    "$CLAUDE_JSON" 2>/dev/null)
  case "$entry_type" in
    stdio)
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
      ;;
    http|sse)
      jq -e --arg s "$srv" '
        .mcpServers[$s] as $e
        | if ($e | type) != "object" then false
          elif ($e.url | type) != "string" then false
          elif ($e.url | test("^https?://")) then true
          else false end
        ' "$CLAUDE_JSON" >/dev/null 2>&1
      ;;
    *)
      return 1
      ;;
  esac
}

classify_mcp() {
  local srv="$1"
  jq -e --arg s "$srv" '(.mcpServers // {}) | has($s)' "$CLAUDE_JSON" >/dev/null 2>&1 \
    || { echo "absent"; return; }
  if is_mcp_entry_valid "$srv"; then echo "valid"; else echo "malformed"; fi
}

# Atomic pre-mutation guard: if ~/.claude.json is unparseable OR any
# required entry is present but invalid, bail out before any write
# touches the filesystem. A broken file would otherwise trick
# classify_mcp into reporting 'absent' (jq returns non-zero on parse
# error, which our caller treats as missing key) and the script would
# then run sections 1-3 before install_mcp_servers.sh finally hit the
# invalid JSON.
if [[ "$DRY_RUN" != "1" && -f "$CLAUDE_JSON" ]]; then
  if ! jq empty "$CLAUDE_JSON" >/dev/null 2>&1; then
    log "ERROR: $CLAUDE_JSON is not valid JSON."
    log "  Exiting BEFORE any hook/settings/mcp mutation to preserve your Claude config."
    log "  Back up the file (cp $CLAUDE_JSON $CLAUDE_JSON.broken) then fix or reset it and re-run."
    exit 3
  fi
  PREMUT_MALFORMED=()
  for srv in "${REQUIRED_MCP[@]}"; do
    [[ "$(classify_mcp "$srv")" == "malformed" ]] && PREMUT_MALFORMED+=("$srv")
  done
  if (( ${#PREMUT_MALFORMED[@]} > 0 )); then
    log "ERROR: required MCP entries present in $CLAUDE_JSON but failed validation:"
    log "       ${PREMUT_MALFORMED[*]}"
    log "  install-mirror.sh exits BEFORE any hook/settings/mcp mutation so your"
    log "  existing config is preserved. Fix each entry by hand to retain any custom"
    log "  args/env/transport choices, then re-run. Validators:"
    log "    stdio entries: .command must be a non-empty string resolving to an executable file"
    log "    http/sse entries: .url must be a string matching ^https?://"
    exit 3
  fi
fi

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
                # Preserve the FULL first object in the group: it is
                # always the existing user block when one exists (the
                # concat above puts existing before template), so any
                # per-block fields the user added (timeout, active
                # flags, etc.) survive instead of being stripped down
                # to a bare {matcher, hooks}.
                .[0]
                + { hooks: (map(.hooks // []) | add | unique_by(.command)) }
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
# At this point the pre-mutation guard above guaranteed any required
# entries that EXISTED were valid; install_mcp_servers.sh just added
# the missing canonical entries. Re-classify to confirm + auto-create
# canonical serena (the installer doesn't register serena itself), and
# exit 3 if anything is still missing or somehow malformed post-merge.
if [[ "$DRY_RUN" == "1" ]]; then
  log "DRY: would validate MCP servers — required: ${REQUIRED_MCP[*]}; optional: ${OPTIONAL_MCP[*]}"
elif [[ ! -f "$CLAUDE_JSON" ]]; then
  log "ERROR: $CLAUDE_JSON does not exist — MCP registration failed upstream."
  exit 3
else
  ABSENT_MCP=()
  MALFORMED_MCP=()
  for srv in "${REQUIRED_MCP[@]}"; do
    case "$(classify_mcp "$srv")" in
      absent)    ABSENT_MCP+=("$srv") ;;
      malformed) MALFORMED_MCP+=("$srv") ;;
    esac
  done

  # Optional servers: try to auto-register when ABSENT and their binary
  # is on disk. When the binary is absent (e.g. clean clone without
  # `pipx install serena-mcp`), skip silently with a notice. When
  # present-but-malformed, warn but don't fail the installer.
  for opt in "${OPTIONAL_MCP[@]}"; do
    case "$(classify_mcp "$opt")" in
      absent)
        case "$opt" in
          serena)
            if [[ -x "$HOME/.local/bin/serena-mcp" ]]; then
              log "  optional serena MCP absent — canonical binary present, registering"
              cp "$CLAUDE_JSON" "$CLAUDE_JSON.bak.$(date +%s)"
              SERENA_ENTRY_JSON=$(jq -n --arg home "$HOME" '{
                type: "stdio",
                command: ($home + "/.local/bin/serena-mcp"),
                args: []
              }')
              jq --argjson entry "$SERENA_ENTRY_JSON" '.mcpServers.serena = $entry' \
                "$CLAUDE_JSON" > "$CLAUDE_JSON.new" && mv "$CLAUDE_JSON.new" "$CLAUDE_JSON"
            else
              log "  optional serena MCP absent — canonical binary missing, skipping (install via 'pipx install serena-mcp' then re-run)"
            fi
            ;;
        esac
        ;;
      malformed)
        log "  WARNING: optional MCP entry '$opt' present but failed validation — NOT auto-rewritten."
        log "    Review \$CLAUDE_JSON.mcpServers.$opt; stdio needs executable .command, http/sse needs string .url matching ^https?://."
        ;;
    esac
  done

  if (( ${#ABSENT_MCP[@]} > 0 )); then
    log "ERROR: required MCP servers absent from $CLAUDE_JSON: ${ABSENT_MCP[*]}"
    log "  Re-run install_mcp_servers.sh after fixing NLR_BIN/TV_BIN paths, or run 'claude mcp add' manually."
  fi
  if (( ${#MALFORMED_MCP[@]} > 0 )); then
    log "ERROR: required MCP servers present but failed validation in $CLAUDE_JSON: ${MALFORMED_MCP[*]}"
    log "  These entries were NOT auto-rewritten — they may be intentional non-canonical configs."
    log "  For each, check: stdio entries need an executable .command; http/sse entries need a string .url matching ^https?://"
    log "  Canonical neuro-link binaries: \$NLR_ROOT/server/target/release/neuro-link (cargo build --release in server/)."
    log "  Edit \$CLAUDE_JSON by hand to correct, then re-run."
  fi
  if (( ${#ABSENT_MCP[@]} > 0 )) || (( ${#MALFORMED_MCP[@]} > 0 )); then
    exit 3
  fi
  log "  MCP validation OK (structural + operational): required=[${REQUIRED_MCP[*]}] optional=[${OPTIONAL_MCP[*]}]"
fi

log "DONE. Review $SETTINGS and restart Claude Code to pick up hook changes."
