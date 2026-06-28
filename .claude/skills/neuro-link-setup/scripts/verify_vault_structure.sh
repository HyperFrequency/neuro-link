#!/usr/bin/env bash
# Verify the post-2026-04-18 vault structure. Reports missing dirs; exits
# non-zero if any required directory is absent.

set -uo pipefail

REPO_ROOT="${NLR_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)}"
cd "$REPO_ROOT"

REQUIRED_DIRS=(
  "00-neuro-link"
  "00-neuro-link/tasks"
  "01-raw"
  "01-sorted"
  "02-KB-main"
  "03-Ontology-main"
  "03-Ontology-main/workflow"
  "03-Ontology-main/workflow/SOT"
  "03-Ontology-main/agents"
  "03-Ontology-main/agents/by-agent"
  "03-Ontology-main/agents/by-workflow-state"
  "03-Ontology-main/agents/by-auto-HITL"
  "04-Agent-Memory"
  "04-Agent-Memory/consolidated"
  "04-Agent-Memory/consolidated/agent"
  "04-Agent-Memory/consolidated/workflow"
  "05-insights-HITL"
  "06-Recursive"
  "07-self-improvement-HITL"
  "08-code-docs"
  "08-code-docs/my-repos"
  "08-code-docs/toolbox"
  "08-code-docs/forked-up"
  ".claude"
  ".claude/agents"
  ".claude/skills"
  "config"
  "hooks"
  "models"
  "scripts"
  "secrets"
  "server"
  "state"
)

missing=0
for d in "${REQUIRED_DIRS[@]}"; do
  if [[ ! -d "$d" ]]; then
    echo "  MISSING $d"
    missing=$((missing + 1))
  fi
done

if [[ $missing -eq 0 ]]; then
  echo "Vault structure OK (${#REQUIRED_DIRS[@]} dirs verified)."
  exit 0
else
  echo
  echo "ERROR: $missing directory(s) missing." >&2
  echo "Run: cd $REPO_ROOT && mkdir -p <missing-dirs>" >&2
  exit 1
fi
