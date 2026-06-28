#!/usr/bin/env bash
# Shared helpers for neuro-link-recursive CLI client hooks.
# Sourced by per-client hook scripts in hooks/clients/<client>/*.sh
#
# Provides:
#   nlr_resolve_token     — echo the API token from env or secrets file (may be empty)
#   nlr_resolve_endpoint  — echo the /hooks/event URL
#   nlr_post_event        — POST a JSON payload built from stdin + args
#   nlr_build_payload     — build the canonical payload JSON from args + raw stdin data
#
# All scripts must be fast (<100ms typical) and fail-silent. Never block the host CLI.

set -u  # note: not -e — callers handle errors

nlr_resolve_endpoint() {
  # Priority: env NLR_HOOKS_URL > default localhost
  if [[ -n "${NLR_HOOKS_URL:-}" ]]; then
    printf '%s' "$NLR_HOOKS_URL"
  else
    printf '%s' "http://localhost:8080/api/v1/hooks/event"
  fi
}

nlr_resolve_token() {
  # Priority: NLR_API_TOKEN env > secrets/.env (under resolved NLR_ROOT) > ~/.claude/state/nlr_root/secrets/.env
  if [[ -n "${NLR_API_TOKEN:-}" ]]; then
    printf '%s' "$NLR_API_TOKEN"
    return 0
  fi

  local root=""
  if [[ -n "${NLR_ROOT:-}" ]]; then
    root="$NLR_ROOT"
  elif [[ -f "${HOME}/.claude/state/nlr_root" ]]; then
    root="$(cat "${HOME}/.claude/state/nlr_root" 2>/dev/null || true)"
  fi

  # Try $NLR_ROOT/secrets/.env
  local env_file=""
  if [[ -n "$root" && -f "${root}/secrets/.env" ]]; then
    env_file="${root}/secrets/.env"
  elif [[ -f "${HOME}/.claude/state/nlr_root/secrets/.env" ]]; then
    env_file="${HOME}/.claude/state/nlr_root/secrets/.env"
  fi

  if [[ -n "$env_file" ]]; then
    # Extract NLR_API_TOKEN value (strip quotes, whitespace)
    local tok
    tok="$(grep -E '^[[:space:]]*NLR_API_TOKEN[[:space:]]*=' "$env_file" 2>/dev/null \
      | head -n1 \
      | sed -E 's/^[[:space:]]*NLR_API_TOKEN[[:space:]]*=[[:space:]]*//' \
      | sed -E 's/^["'\'']//;s/["'\''][[:space:]]*$//')"
    printf '%s' "$tok"
    return 0
  fi

  printf '%s' ""
}

# nlr_build_payload <event_type> <client> <session_id> <raw_data_json>
# Emits canonical JSON body for POST /hooks/event.
nlr_build_payload() {
  local event_type="$1"
  local client="$2"
  local session_id="${3:-}"
  local raw_data="${4:-}"

  python3 - "$event_type" "$client" "$session_id" <<PY
import json, sys
event_type = sys.argv[1]
client = sys.argv[2]
session_id = sys.argv[3] or None
raw = sys.stdin.read() or ""
try:
    data = json.loads(raw) if raw.strip() else {}
except Exception:
    data = {"_raw": raw}
print(json.dumps({
    "event_type": event_type,
    "client": client,
    "session_id": session_id,
    "data": data,
}))
PY
}

# nlr_post_event <event_type> <client> <session_id>
# Reads raw data JSON from stdin, POSTs to neuro-link, echoes response body on stdout.
# Exit code 0 on network success (even if server error); non-zero only on local error.
nlr_post_event() {
  local event_type="$1"
  local client="$2"
  local session_id="${3:-}"

  local url token payload
  url="$(nlr_resolve_endpoint)"
  token="$(nlr_resolve_token)"

  local raw_data
  raw_data="$(cat)"

  payload="$(printf '%s' "$raw_data" | nlr_build_payload "$event_type" "$client" "$session_id" "")"

  if [[ -z "$token" ]]; then
    # No token configured — skip silently. Hooks are best-effort.
    return 0
  fi

  # Short timeout so hook never blocks the host CLI.
  curl -sS --max-time 2 \
    -H "Authorization: Bearer ${token}" \
    -H "Content-Type: application/json" \
    -X POST "$url" \
    --data "$payload" 2>/dev/null
}
