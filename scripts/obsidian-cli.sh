#!/usr/bin/env bash
# obsidian-cli.sh — modded-Obsidian CLI for neuro-link-recursive
# Banner: ASCII brain + 3 blue circuit traces reaching terminal edges.

set -u

VAULT="/Users/DanBot/Vaults/neuro-quant-vault"
ENV_FILE="/Users/DanBot/Desktop/HyperFrequency/neuro-link/secrets/.env"
API_BASE="${NLR_API_URL:-http://127.0.0.1:8080}"

# ─── env / token ───
if [[ -f "$ENV_FILE" ]]; then
  # shellcheck disable=SC1090
  set -a; source "$ENV_FILE"; set +a
fi
TOKEN="${NLR_API_TOKEN:-}"

# ─── ANSI ───
C_BRAIN=$'\033[97m'    # bright white
C_FOLD=$'\033[37m'     # dim white for sulci
C_WIRE=$'\033[94m'     # bright blue
C_PIN=$'\033[96m'      # cyan endpoint
C_STATUS=$'\033[36m'   # cyan
C_DIM=$'\033[2m'
C_OK=$'\033[92m'
C_RED=$'\033[91m'
C_RESET=$'\033[0m'

# ─── terminal dims ───
COLS=$( (TERM=${TERM:-xterm-256color} tput cols) 2>/dev/null || echo 80)
LINES=$( (TERM=${TERM:-xterm-256color} tput lines) 2>/dev/null || echo 24)
[[ -z "$COLS"  || "$COLS"  -lt 1 ]] && COLS=80
[[ -z "$LINES" || "$LINES" -lt 1 ]] && LINES=24

# ─── ASCII brain (12 rows, ~38 cols) ───
read -r -d '' BRAIN_RAW <<'BRAIN' || true
       _.--""--._
     ,'  _.::._  `.
    /  ,'~/cc\~`. \
   ;  ( (~|cc|~) )  ;
   |  ,'.~`cc'~.`.  |
   ;  '~.( cc ).~'  ;
    \ ((~`vv'~)) /
     `. `~~~~~' ,'
       `-.,_,.-'
        |||  |||
       _|||__|||_
      `~~~~~~~~~~`
BRAIN

# Render brain at horizontal center, return start row + width.
render_brain() {
  local pad lines max=0 line w
  while IFS= read -r line; do
    w=${#line}
    (( w > max )) && max=$w
  done <<< "$BRAIN_RAW"
  pad=$(( (COLS - max) / 2 ))
  (( pad < 0 )) && pad=0
  local row=2
  printf '\n'
  while IFS= read -r line; do
    # color the cortex tissue, sulci as dim
    local colored
    colored=$(printf '%s' "$line" \
      | sed -E "s/(cc|vv)/${C_FOLD}\1${C_BRAIN}/g")
    printf '%*s%s%s%s\n' "$pad" "" "$C_BRAIN" "$colored" "$C_RESET"
    row=$((row+1))
  done <<< "$BRAIN_RAW"
  # export geometry for circuit drawing
  BRAIN_TOP=2
  BRAIN_BOTTOM=$((row-1))
  BRAIN_MIDROW=$(( (BRAIN_TOP + BRAIN_BOTTOM) / 2 ))
  BRAIN_LEFT=$pad
  BRAIN_RIGHT=$((pad + max - 1))
  BRAIN_MIDCOL=$(( (BRAIN_LEFT + BRAIN_RIGHT) / 2 ))
  BRAIN_WIDTH=$max
  BRAIN_HEIGHT=$((BRAIN_BOTTOM - BRAIN_TOP + 1))
}

# Compact fallback for narrow terminals.
render_compact_brain() {
  printf '\n%s   .-"""-.\n  /  o o  \\\n |  (___)  |\n  \\  ~~~  /\n   `-...-'"'"'%s\n\n' "$C_BRAIN" "$C_RESET"
}

# Move cursor to (row,col) using ANSI CUP. 1-indexed.
goto() { printf '\033[%d;%dH' "$1" "$2"; }

# Draw the 3 circuits using cursor positioning. Brain center is (BRAIN_MIDROW, BRAIN_MIDCOL).
draw_circuits() {
  local mr=$BRAIN_MIDROW mc=$BRAIN_MIDCOL
  local left_x=$BRAIN_LEFT right_x=$BRAIN_RIGHT bot_y=$BRAIN_BOTTOM
  local i

  # ── LEFT circuit: brain mid-row, jog up by 2 then run to col 1 ──
  local lrow=$((mr - 2))
  (( lrow < 1 )) && lrow=1
  # vertical hop up from brain edge
  for ((i=mr; i>=lrow; i--)); do
    goto "$i" "$left_x"; printf '%s│%s' "$C_WIRE" "$C_RESET"
  done
  # corner
  goto "$lrow" "$left_x"; printf '%s┘%s' "$C_WIRE" "$C_RESET"
  # horizontal run to col 2, with a junction in the middle
  local lmid=$(( (left_x + 1) / 2 ))
  for ((i=left_x-1; i>=2; i--)); do
    goto "$lrow" "$i"
    if (( i == lmid )); then printf '%s┬%s' "$C_WIRE" "$C_RESET"
    else                     printf '%s─%s' "$C_WIRE" "$C_RESET"
    fi
  done
  # endpoint pin
  goto "$lrow" 1; printf '%s◆%s' "$C_PIN" "$C_RESET"
  # short stub off the junction going up 1 row, capped with •
  if (( lmid >= 2 && lrow-1 >= 1 )); then
    goto "$((lrow-1))" "$lmid"; printf '%s•%s' "$C_PIN" "$C_RESET"
  fi

  # ── RIGHT circuit: brain mid-row, jog down by 2 then run to last col ──
  local rrow=$((mr + 2))
  (( rrow > LINES )) && rrow=$LINES
  for ((i=mr; i<=rrow; i++)); do
    goto "$i" "$right_x"; printf '%s│%s' "$C_WIRE" "$C_RESET"
  done
  goto "$rrow" "$right_x"; printf '%s└%s' "$C_WIRE" "$C_RESET"
  local rmid=$(( (right_x + COLS) / 2 ))
  for ((i=right_x+1; i<=COLS-1; i++)); do
    goto "$rrow" "$i"
    if (( i == rmid )); then printf '%s┴%s' "$C_WIRE" "$C_RESET"
    else                     printf '%s─%s' "$C_WIRE" "$C_RESET"
    fi
  done
  goto "$rrow" "$COLS"; printf '%s◆%s' "$C_PIN" "$C_RESET"
  if (( rmid <= COLS-1 && rrow+1 <= LINES )); then
    goto "$((rrow+1))" "$rmid"; printf '%s•%s' "$C_PIN" "$C_RESET"
  fi

  # ── BOTTOM circuit: from brain bottom-center, drop ──
  local target_row=$LINES
  # don't overrun the status line we'll print after; reserve last 2 rows
  if (( LINES > BRAIN_BOTTOM + 4 )); then
    target_row=$((BRAIN_BOTTOM + 3))
  else
    target_row=$LINES
  fi
  for ((i=bot_y+1; i<=target_row-1; i++)); do
    goto "$i" "$mc"; printf '%s│%s' "$C_WIRE" "$C_RESET"
  done
  # T-junction: split into two short legs
  goto "$target_row" "$mc"; printf '%s┴%s' "$C_WIRE" "$C_RESET"
  # left leg
  for ((i=mc-1; i>=mc-6 && i>=1; i--)); do
    goto "$target_row" "$i"; printf '%s─%s' "$C_WIRE" "$C_RESET"
  done
  local lleg=$((mc-6)); (( lleg < 1 )) && lleg=1
  goto "$target_row" "$lleg"; printf '%s◆%s' "$C_PIN" "$C_RESET"
  # right leg
  for ((i=mc+1; i<=mc+6 && i<=COLS; i++)); do
    goto "$target_row" "$i"; printf '%s─%s' "$C_WIRE" "$C_RESET"
  done
  local rleg=$((mc+6)); (( rleg > COLS )) && rleg=$COLS
  goto "$target_row" "$rleg"; printf '%s◆%s' "$C_PIN" "$C_RESET"

  # park cursor below the banner
  local park=$((target_row + 1))
  (( park > LINES )) && park=$LINES
  goto "$park" 1
}

# ─── banner ───
banner() {
  if (( COLS < 60 )); then
    render_compact_brain
    return
  fi
  # Clear the banner area only (top BRAIN_HEIGHT+6 rows). Use printf newlines first
  # so we don't blow away the user's whole scrollback.
  render_brain
  # Add a few buffer rows so circuits have space below the brain.
  local need=$((BRAIN_BOTTOM + 5))
  local printed=$BRAIN_BOTTOM
  while (( printed < need )); do printf '\n'; printed=$((printed+1)); done
  # cursor is now somewhere below; draw circuits using absolute positioning
  draw_circuits
}

# ─── data helpers ───
count_pages() {
  local n
  n=$(find "$VAULT" -maxdepth 4 -type f -name '*.md' 2>/dev/null | wc -l | tr -d ' ')
  echo "${n:-0}"
}

count_tasks() {
  local task_dir="/Users/DanBot/Desktop/HyperFrequency/neuro-link/07-neuro-link-task"
  if [[ -d "$task_dir" ]]; then
    find "$task_dir" -maxdepth 2 -type f -name '*.md' 2>/dev/null | wc -l | tr -d ' '
  else
    echo 0
  fi
}

status_line() {
  local pages tasks
  pages=$(count_pages)
  tasks=$(count_tasks)
  printf '%sneuro-link%s · vault: %s · wiki pages: %s · pending tasks: %s%s\n' \
    "$C_STATUS" "$C_RESET" "$VAULT" "$pages" "$tasks" "$C_RESET"
}

# ─── subcommands ───
cmd_help() {
  banner
  echo
  cat <<EOF
${C_STATUS}Usage:${C_RESET} nl-cli [SUBCOMMAND] [args]

Subcommands:
  ${C_OK}vault${C_RESET}            list top-level pages in $VAULT
  ${C_OK}wiki${C_RESET}  <query>    keyword search across the wiki
  ${C_OK}tasks${C_RESET}            show pending neuro-link tasks
  ${C_OK}rag${C_RESET}   <query>    semantic RAG query (top 3 hits)
  ${C_OK}open${C_RESET}  <page>     open a wiki page in \$EDITOR
  ${C_OK}status${C_RESET}           system health
  ${C_OK}--help${C_RESET}           this help

Default: prints brain banner + status line.
EOF
}

cmd_vault() {
  banner; echo
  printf '%sTop-level pages in %s:%s\n\n' "$C_STATUS" "$VAULT" "$C_RESET"
  ( cd "$VAULT" 2>/dev/null && ls -1 | grep -Ev '^\.' ) | awk '{printf "  • %s\n", $0}'
}

cmd_wiki() {
  banner; echo
  local q="${*:-}"
  if [[ -z "$q" ]]; then
    printf '%susage:%s nl-cli wiki <query>\n' "$C_RED" "$C_RESET"; return 1
  fi
  /usr/local/bin/neuro-link search "$q"
}

cmd_tasks() {
  banner; echo
  /usr/local/bin/neuro-link tasks 2>&1 || true
}

cmd_status() {
  banner; echo
  /usr/local/bin/neuro-link status 2>&1 || true
  echo
  status_line
}

cmd_open() {
  banner; echo
  local page="${*:-}"
  if [[ -z "$page" ]]; then
    printf '%susage:%s nl-cli open <page>\n' "$C_RED" "$C_RESET"; return 1
  fi
  local match
  match=$(find "$VAULT" -type f -iname "*${page}*.md" 2>/dev/null | head -1)
  if [[ -z "$match" ]]; then
    printf '%sno match for%s "%s"\n' "$C_RED" "$C_RESET" "$page"; return 1
  fi
  printf '%sopening:%s %s\n' "$C_STATUS" "$C_RESET" "$match"
  ${EDITOR:-vim} "$match"
}

cmd_rag() {
  banner; echo
  local q="${*:-}"
  if [[ -z "$q" ]]; then
    printf '%susage:%s nl-cli rag <query>\n' "$C_RED" "$C_RESET"; return 1
  fi
  if [[ -z "$TOKEN" ]]; then
    printf '%sno NLR_API_TOKEN in %s%s\n' "$C_RED" "$ENV_FILE" "$C_RESET"; return 1
  fi
  local payload resp
  payload=$(jq -nc --arg q "$q" '{
    jsonrpc:"2.0", id:1, method:"tools/call",
    params:{ name:"nlr_rag_query", arguments:{ query:$q, top_k:3 } }
  }')
  resp=$(curl -sS -m 15 \
    -H "Authorization: Bearer ${TOKEN}" \
    -H "Content-Type: application/json" \
    -d "$payload" \
    "${API_BASE}/mcp" 2>&1) || {
      printf '%scurl failed:%s %s\n' "$C_RED" "$C_RESET" "$resp"; return 1; }
  # Try to pretty-print. The response shape is JSON-RPC with content array.
  if ! echo "$resp" | jq -e . >/dev/null 2>&1; then
    printf '%sraw response:%s\n%s\n' "$C_DIM" "$C_RESET" "$resp"; return 0
  fi
  printf '%sTop 3 hits for:%s "%s"\n\n' "$C_STATUS" "$C_RESET" "$q"
  echo "$resp" | jq -r '
    (.result.content[0].text // .result // .) as $t |
    (try ($t | fromjson) catch $t) as $obj |
    ($obj.hits // $obj.results // $obj // []) as $hits |
    if ($hits | type) == "array" then
      $hits[:3] | to_entries[] |
        "  \u001b[96m[\(.key+1)]\u001b[0m score=\(.value.score // .value.distance // "?") \u001b[2msrc=\(.value.source // .value.path // .value.id // "?")\u001b[0m\n     \((.value.text // .value.content // .value.snippet // "") | gsub("\\s+"; " ") | .[0:160])\n"
    else
      "  (no structured hits — raw:)\n  \($obj | tostring | .[0:400])"
    end
  '
}

# ─── dispatch ───
sub="${1:-}"; shift || true
case "$sub" in
  ""|default) banner; echo; status_line ;;
  -h|--help|help) cmd_help ;;
  vault)   cmd_vault  "$@" ;;
  wiki)    cmd_wiki   "$@" ;;
  tasks)   cmd_tasks  "$@" ;;
  rag)     cmd_rag    "$@" ;;
  open)    cmd_open   "$@" ;;
  status)  cmd_status "$@" ;;
  *)
    printf '%sunknown subcommand:%s %s\n' "$C_RED" "$C_RESET" "$sub"
    cmd_help
    exit 2
    ;;
esac

printf '%s' "$C_RESET"
