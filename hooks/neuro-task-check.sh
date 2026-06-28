#!/usr/bin/env bash
# UserPromptSubmit hook: check for high-priority pending tasks in neuro-link-recursive
# and inject a brief reminder if any exist.
#
# Lightweight check — just counts files with priority-1 status in frontmatter.
# Only fires if there are priority-1 tasks to avoid noise.

set -euo pipefail

# Resolve NLR_ROOT: env var > persisted root file > exit
_NLR_ROOT_FILE="${HOME}/.claude/state/nlr_root"
if [[ -n "${NLR_ROOT:-}" ]]; then
  : # use env var
elif [[ -f "$_NLR_ROOT_FILE" ]]; then
  NLR_ROOT="$(cat "$_NLR_ROOT_FILE")"
else
  exit 0  # not configured — skip silently
fi

TASK_DIR="${NLR_ROOT}/07-neuro-link-task"

# Skip if task directory doesn't exist or is empty
if [[ ! -d "$TASK_DIR" ]]; then
  exit 0
fi

# Count priority-1 pending tasks (fast: grep frontmatter only)
count="$(python3 -c "
import os, re, sys

task_dir = '$TASK_DIR'
count = 0
total_pending = 0

for f in os.listdir(task_dir):
    if not f.endswith('.md'):
        continue
    path = os.path.join(task_dir, f)
    with open(path) as fh:
        content = fh.read(500)  # Only read frontmatter

    # Check status
    if re.search(r'status:\s*pending', content):
        total_pending += 1
        # Check priority
        m = re.search(r'priority:\s*(\d+)', content)
        if m and int(m.group(1)) == 1:
            count += 1

if count > 0:
    print(f'{count}:{total_pending}')
" 2>/dev/null || echo "")"

if [[ -z "$count" ]]; then
  exit 0
fi

p1="${count%%:*}"
total="${count##*:}"

if [[ "$p1" -gt 0 ]]; then
  cat <<EOF
{
  "hookSpecificOutput": {
    "hookEventName": "UserPromptSubmit",
    "additionalContext": "NEURO-LINK-RECURSIVE: ${p1} high-priority task(s) pending (${total} total). Run /neuro-link tasks or /job-scanner to review and process."
  }
}
EOF
fi

exit 0
