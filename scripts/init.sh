#!/usr/bin/env bash
# neuro-link-recursive init script
# Creates directory structure, installs skills + hooks, registers hooks, verifies setup.
set -euo pipefail

NLR_ROOT="${NLR_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
CLAUDE_DIR="${HOME}/.claude"
SKILLS_DIR="${CLAUDE_DIR}/skills"
HOOKS_DIR="${CLAUDE_DIR}/hooks"
SETTINGS="${CLAUDE_DIR}/settings.json"

echo "=== neuro-link-recursive init ==="
echo "Root: ${NLR_ROOT}"

# 0. Ensure Claude Code directories exist
mkdir -p "${SKILLS_DIR}" "${HOOKS_DIR}" "${CLAUDE_DIR}/state"

# Bootstrap settings.json if it doesn't exist (fresh install)
if [[ ! -f "$SETTINGS" ]]; then
  echo '{}' > "$SETTINGS"
  echo "  Created ${SETTINGS}"
fi

# 1. Create directory tree
echo "[1/7] Creating directory structure..."
dirs=(
  00-raw
  01-sorted/books 01-sorted/arxiv 01-sorted/medium 01-sorted/huggingface 01-sorted/github 01-sorted/docs
  02-KB-main
  03-ontology-main/workflow/SOT 03-ontology-main/agents/by-agent 03-ontology-main/agents/by-workflow-state 03-ontology-main/agents/by-auto-HITL
  04-KB-agents-workflows
  05-insights-gaps/knowledge 05-insights-gaps/ontology 05-insights-gaps/goals
  06-progress-reports
  07-neuro-link-task
  08-code-docs/my-repos 08-code-docs/common-tools 08-code-docs/my-forks
  09-business-docs
  config state secrets scripts
)
for d in "${dirs[@]}"; do
  mkdir -p "${NLR_ROOT}/${d}"
done

# 2. Create .gitignore for secrets
echo "[2/7] Setting up secrets..."
cat > "${NLR_ROOT}/secrets/.gitignore" << 'GITIGNORE'
*
!.gitignore
!.env.example
GITIGNORE

if [[ ! -f "${NLR_ROOT}/secrets/.env" ]]; then
  cp "${NLR_ROOT}/secrets/.env.example" "${NLR_ROOT}/secrets/.env" 2>/dev/null || true
fi

# 3. Initialize state files + persist NLR_ROOT for hooks
echo "[3/7] Initializing state..."
[[ -f "${NLR_ROOT}/state/heartbeat.json" ]] || echo '{"status":"initialized","last_check":"'"$(date -u +%Y-%m-%dT%H:%M:%SZ)"'","errors":[]}' > "${NLR_ROOT}/state/heartbeat.json"
[[ -f "${NLR_ROOT}/state/session_log.jsonl" ]] || touch "${NLR_ROOT}/state/session_log.jsonl"
[[ -f "${NLR_ROOT}/state/score_history.jsonl" ]] || touch "${NLR_ROOT}/state/score_history.jsonl"
[[ -f "${NLR_ROOT}/state/deviation_log.jsonl" ]] || touch "${NLR_ROOT}/state/deviation_log.jsonl"

# Persist NLR_ROOT so installed hooks can find the correct tree
mkdir -p "${CLAUDE_DIR}/state"
printf '%s' "${NLR_ROOT}" > "${CLAUDE_DIR}/state/nlr_root"
echo "  Persisted NLR_ROOT → ${CLAUDE_DIR}/state/nlr_root"

# 4. Install skills (symlink or copy)
echo "[4/7] Installing skills..."
nlr_skills=(neuro-link neuro-scan wiki-curate crawl-ingest auto-rag job-scanner reasoning-ontology neuro-link-setup)
for skill in "${nlr_skills[@]}"; do
  src="${NLR_ROOT}/skills/${skill}/SKILL.md"
  dst="${SKILLS_DIR}/${skill}/SKILL.md"
  if [[ -f "$src" ]]; then
    mkdir -p "$(dirname "$dst")"
    ln -sf "$src" "$dst" 2>/dev/null || cp "$src" "$dst"
    echo "  Installed: ${skill}"
  else
    echo "  SKIP (not found): ${skill}"
  fi
done

# 5. Install hooks
echo "[5/7] Installing hooks..."
nlr_hooks=(auto-rag-inject.sh neuro-log-tool-use.sh neuro-task-check.sh)
for hook in "${nlr_hooks[@]}"; do
  src="${NLR_ROOT}/hooks/${hook}"
  dst="${HOOKS_DIR}/${hook}"
  if [[ -f "$src" ]]; then
    cp "$src" "$dst"
    chmod +x "$dst"
    echo "  Installed: ${hook}"
  else
    echo "  SKIP (not found): ${hook}"
  fi
done

# 6. Register hooks in settings.json (idempotent)
echo "[6/7] Registering hooks in settings.json..."
if [[ -f "$SETTINGS" ]]; then
  python3 -c "
import json, sys

settings_path = '$SETTINGS'
hooks_dir = '$HOOKS_DIR'

with open(settings_path) as f:
    settings = json.load(f)

hooks = settings.setdefault('hooks', {})

# Hook definitions: event -> list of scripts to register
nlr_hooks = {
    'UserPromptSubmit': [
        hooks_dir + '/auto-rag-inject.sh',
        hooks_dir + '/neuro-task-check.sh',
    ],
    'PostToolUse': [
        hooks_dir + '/neuro-log-tool-use.sh',
    ],
}

changed = False

for event, scripts in nlr_hooks.items():
    entries = hooks.setdefault(event, [])

    # Collect all already-registered commands across all entries for this event
    existing_cmds = set()
    for entry in entries:
        for h in entry.get('hooks', []):
            existing_cmds.add(h.get('command', ''))

    for script in scripts:
        if script not in existing_cmds:
            # Find an entry without a matcher (generic) to append to, or create one
            target_entry = None
            for entry in entries:
                if 'matcher' not in entry:
                    target_entry = entry
                    break
            if target_entry is None:
                target_entry = {'hooks': []}
                entries.append(target_entry)
            target_entry['hooks'].append({'type': 'command', 'command': script})
            changed = True
            print(f'  Registered: {event} -> {script.split(\"/\")[-1]}')

if changed:
    with open(settings_path, 'w') as f:
        json.dump(settings, f, indent=2)
    print('  settings.json updated')
else:
    print('  All hooks already registered')
" || {
    echo "  ERROR: Failed to register hooks in settings.json"
    echo "  Run /neuro-link-setup for manual registration"
    exit 1
  }
else
  echo "  ERROR: ${SETTINGS} not found — cannot register hooks"
  exit 1
fi

# 7. Verify
echo "[7/7] Verifying..."
errors=0

# Check critical files
for f in CLAUDE.md SETUP.md README.md 02-KB-main/schema.md config/neuro-link.md; do
  if [[ ! -f "${NLR_ROOT}/${f}" ]]; then
    echo "  MISSING: ${f}"
    errors=$((errors + 1))
  fi
done

# Check hooks are executable
for hook in "${nlr_hooks[@]}"; do
  if [[ -f "${HOOKS_DIR}/${hook}" ]] && [[ ! -x "${HOOKS_DIR}/${hook}" ]]; then
    echo "  NOT EXECUTABLE: ${hook}"
    errors=$((errors + 1))
  fi
done

# Check NLR_ROOT is persisted
if [[ ! -f "${CLAUDE_DIR}/state/nlr_root" ]]; then
  echo "  MISSING: ${CLAUDE_DIR}/state/nlr_root"
  errors=$((errors + 1))
fi

# Verify hooks are in settings.json
if [[ -f "$SETTINGS" ]]; then
  for hook in "${nlr_hooks[@]}"; do
    if ! grep -q "$hook" "$SETTINGS" 2>/dev/null; then
      echo "  NOT REGISTERED: ${hook}"
      errors=$((errors + 1))
    fi
  done
fi

if [[ $errors -eq 0 ]]; then
  echo ""
  echo "=== neuro-link-recursive initialized successfully ==="
  echo "Next: run /neuro-link status to verify all components"
else
  echo ""
  echo "=== Completed with ${errors} errors ==="
  echo "Run the setup skill (/neuro-link-setup) for guided configuration"
  exit 1
fi
