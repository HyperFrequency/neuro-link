#!/usr/bin/env bash
# Copy all neuro-link skills from this repo's .claude/skills/ into
# ~/.claude/skills/. Plain copy — no symlinks — so the user's Claude Code
# installation is self-contained and won't break if the repo moves.
#
# Idempotent: re-running overwrites existing copies with the latest repo
# version. Use --dry-run to preview without writing.

set -euo pipefail

REPO_ROOT="${NLR_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)}"
SRC_DIR="$REPO_ROOT/.claude/skills"
DEST_DIR="${HOME}/.claude/skills"

DRY_RUN=0
if [[ "${1:-}" == "--dry-run" ]]; then
  DRY_RUN=1
fi

if [[ ! -d "$SRC_DIR" ]]; then
  echo "ERROR: source directory not found: $SRC_DIR" >&2
  echo "Make sure NLR_ROOT points at the neuro-link-recursive repo root." >&2
  exit 1
fi

mkdir -p "$DEST_DIR"

SKILLS=(
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

echo "Installing ${#SKILLS[@]} neuro-link skills"
echo "  source: $SRC_DIR"
echo "  dest:   $DEST_DIR"
[[ $DRY_RUN -eq 1 ]] && echo "  mode:   DRY-RUN"
echo

for skill in "${SKILLS[@]}"; do
  SRC="$SRC_DIR/$skill"
  DEST="$DEST_DIR/$skill"

  if [[ ! -d "$SRC" ]]; then
    echo "  SKIP $skill (not present in $SRC_DIR)"
    continue
  fi

  if [[ $DRY_RUN -eq 1 ]]; then
    echo "  WOULD COPY $skill -> $DEST"
    continue
  fi

  # Remove any existing symlink from an older install before copying.
  if [[ -L "$DEST" || -L "$DEST/SKILL.md" ]]; then
    echo "  PURGE old symlink at $DEST"
    rm -rf "$DEST"
  fi

  # Plain recursive copy. Preserves references/, scripts/, assets/.
  # rsync chosen over cp because it handles partial existing dirs cleanly.
  rsync -a --delete "$SRC/" "$DEST/"

  # Make scripts executable (rsync -a preserves perms but source tree may
  # have lost the +x bit if it travelled through zip/tarball).
  if [[ -d "$DEST/scripts" ]]; then
    find "$DEST/scripts" -type f \( -name "*.sh" -o -name "*.py" \) -exec chmod +x {} \;
  fi

  echo "  OK   $skill"
done

echo
echo "Done. Restart Claude Code or run /help to see the new skills."
