#!/usr/bin/env bash
# inception-clear-relay.sh — UserPromptSubmit hook.
#
# Behavior when the pasted prompt starts with "# GIGAPROMPT-INBOX":
#  1. Save the body to ~/.claude/lateral-pass/pending-gigaprompt.md
#  2. Block the current prompt with a status message
#  3. Spawn a detached daemon that, into the same tmux pane:
#       sleep 1s   → tmux send-keys "/clear" Enter       (fires Claude Code's /clear)
#       sleep 5s   → paste the gigaprompt (sentinel line stripped) + Enter
#     When the daemon's paste re-triggers this hook, the sentinel is gone,
#     so Case 1 doesn't refire. Case 2 (pending file) also doesn't fire
#     because the daemon moves the file to archive BEFORE sending Enter.
#
# Manual fallback when no tmux: block with "type /clear then any char" message.
# Safe for non-matching prompts: passthrough.

set -euo pipefail

INBOX=~/.claude/lateral-pass/pending-gigaprompt.md
ARCHIVE=~/.claude/lateral-pass/archive
LOG=~/.claude/lateral-pass/relay.log

# Force these on every auto-relay. Can be overridden via env before starting
# the tmux pane (e.g. GIGAPROMPT_MODEL=claude-sonnet-4-6) but default is the
# max-reasoning Opus 1M variant the gigaprompt skill pins to.
FORCE_MODEL="${GIGAPROMPT_MODEL:-claude-opus-4-7[1m]}"

mkdir -p "$(dirname "$INBOX")" "$ARCHIVE"

payload=$(cat)

if command -v jq >/dev/null 2>&1; then
  prompt=$(printf '%s' "$payload" | jq -r '.prompt // ""' 2>/dev/null || echo "")
else
  prompt=""
fi

first_line=$(printf '%s' "$prompt" | head -n1)

ts() { date -u +%Y%m%dT%H%M%SZ; }
log() { printf '[%s] %s\n' "$(ts)" "$*" >> "$LOG"; }

# Case 1 — GIGAPROMPT-INBOX sentinel detected
if [[ "$first_line" == "# GIGAPROMPT-INBOX" ]]; then
  printf '%s' "$prompt" > "$INBOX"
  log "inbox captured ($(wc -c < "$INBOX") bytes)"

  # Attempt auto-fire path via tmux
  SESSION=""
  if [[ -n "${TMUX:-}" ]] && command -v tmux >/dev/null 2>&1; then
    SESSION=$(tmux display-message -p '#S' 2>/dev/null || echo "")
  fi

  if [[ -n "$SESSION" ]]; then
    ARCHIVED="$ARCHIVE/injected-$(ts).md"
    # Detached daemon — survives this hook's exit.
    setsid bash -c "
      set -e
      sleep 1
      # Fire /clear in the current pane
      tmux send-keys -t '$SESSION' '/clear' Enter

      # Give Claude Code time to clear context
      sleep 5

      # Force max-reasoning model before pasting the gigaprompt body.
      # /model is a Claude Code slash command; accepts model aliases with
      # brackets (e.g. claude-opus-4-7[1m]).
      tmux send-keys -t '$SESSION' '/model $FORCE_MODEL' Enter
      sleep 3

      # Move INBOX to archive FIRST so the paste's re-trigger
      # of this hook does not find a pending file in Case 2.
      mv '$INBOX' '$ARCHIVED'

      # Strip the sentinel + blank-line-after from the injected body
      # so the re-trigger does not see GIGAPROMPT-INBOX on line 1.
      tail -n +3 '$ARCHIVED' | tmux load-buffer -
      tmux paste-buffer -t '$SESSION'
      sleep 0.3
      tmux send-keys -t '$SESSION' Enter
    " >> "$LOG" 2>&1 </dev/null &
    disown || true

    if command -v jq >/dev/null 2>&1; then
      jq -nc --arg reason "Gigaprompt staged. Auto-firing /clear now — injection in ~6s (1s settle + 5s after clear). Do NOT type anything." \
        '{decision:"block", reason:$reason}'
    else
      printf '%s\n' '{"decision":"block","reason":"Gigaprompt staged. Auto-firing /clear — injection in ~6s."}'
    fi
    log "auto-fire daemon spawned for session $SESSION"
    exit 0
  fi

  # Manual fallback — no tmux detected. For fullauto runs, the user MUST
  # restart inside tmux; the explicit banner nags them to do so.
  bytes=$(wc -c < "$INBOX")
  manual_reason="Gigaprompt staged at $INBOX ($bytes bytes).

tmux NOT detected — fullauto rehydrate can only fire inside tmux.
To run autonomously without babysitting, do ONE of:

  A. Keep this terminal interactive:
     1. /clear
     2. /model $FORCE_MODEL
     3. Type any char + Enter to inject the staged prompt.

  B. Restart under tmux (recommended for fullauto):
     1. Ctrl-C to exit this claude session.
     2. tmux new-session -s gigaprompt-fullauto
     3. claude --dangerously-skip-permissions --model $FORCE_MODEL --effort max
     4. Paste the gigaprompt again — the auto-relay daemon will take over.

Effort must be max: CLI flag --effort max overrides the settings default (xhigh)."
  if command -v jq >/dev/null 2>&1; then
    jq -nc --arg reason "$manual_reason" '{decision:"block", reason:$reason}'
  else
    printf '%s\n' '{"decision":"block","reason":"Gigaprompt staged. tmux absent — /clear then /model then paste, or restart under tmux with --effort max."}'
  fi
  log "manual-mode fallback (no tmux)"
  exit 0
fi

# Case 2 — manual-mode injection (only fires when no tmux auto-fire ran)
if [[ -f "$INBOX" ]]; then
  content=$(cat "$INBOX")
  ARCHIVED="$ARCHIVE/injected-$(ts).md"
  mv "$INBOX" "$ARCHIVED"
  if command -v jq >/dev/null 2>&1; then
    jq -nc --arg ctx "$content" \
      '{hookSpecificOutput:{hookEventName:"UserPromptSubmit", additionalContext:$ctx}}'
  fi
  log "manual-mode inject ($(wc -c < "$ARCHIVED") bytes)"
  exit 0
fi

# Case 3 — passthrough
exit 0
