#!/usr/bin/env bash
# End-to-end registration of a main codebase repo:
#   1. Collect metadata
#   2. Context7 indexing (resolve library id)
#   3. Auggie indexing (local)
#   4. Auto-RAG route registration
#   5. CLAUDE.md addendum generation
#   6. Write spec file
#
# Usage:
#   register_repo.sh <repo-url> <local-path> <primary-lang> "<comma-separated-topics>"

set -euo pipefail

REPO_URL="${1:?repo URL required}"
LOCAL_PATH="${2:?local path required}"
LANG="${3:?language required}"
TOPICS="${4:?comma-separated topics required}"

REPO_ROOT="${NLR_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)}"
SKILL_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OWNER_REPO="$(echo "$REPO_URL" | sed -E 's#https://github.com/([^/]+/[^/.]+).*#\1#')"
SLUG="$(echo "$OWNER_REPO" | tr '/' '-' | tr '[:upper:]' '[:lower:]')"

echo "Registering $OWNER_REPO as main codebase"
echo "  local:    $LOCAL_PATH"
echo "  language: $LANG"
echo "  topics:   $TOPICS"
echo

# Verify local path
if [[ ! -d "$LOCAL_PATH/.git" ]]; then
  echo "ERROR: $LOCAL_PATH is not a git repo" >&2
  exit 1
fi

# Check if already registered
SPEC_FILE="$SKILL_DIR/$SLUG.md"
if [[ -f "$SPEC_FILE" ]]; then
  echo "WARN: $SPEC_FILE already exists. Use /main-codebase-tools reindex $SLUG to refresh." >&2
  exit 1
fi

# Step 2: Context7 — note we can't resolve programmatically from a shell;
# the user or the skill runner will do this via MCP. Placeholder the id.
CONTEXT7_ID=""
echo "Step 2: Context7 indexing"
echo "  Run in Claude Code: mcp__context7__resolve-library-id \"$REPO_URL\""
echo "  Then re-run this script with CONTEXT7_ID=<id> prepended."

# Step 3: Auggie — requires auggie installed locally
if command -v auggie >/dev/null 2>&1; then
  echo "Step 3: Auggie indexing"
  (cd "$LOCAL_PATH" && auggie index --path . --project "$SLUG" &) > /dev/null 2>&1
  echo "  backgrounded — check with: auggie status $SLUG"
else
  echo "Step 3: Auggie not installed; skipping"
fi

# Step 4: Auto-RAG route
ROUTES_FILE="$REPO_ROOT/config/auto-rag-routes.yml"
mkdir -p "$(dirname "$ROUTES_FILE")"
if [[ ! -f "$ROUTES_FILE" ]]; then
  echo "routes: []" > "$ROUTES_FILE"
fi

# Use yq if available, else manual
if command -v yq >/dev/null 2>&1; then
  yq eval -i ".routes += [{\"repo\":\"$SLUG\",\"keywords\":[\"$(echo "$TOPICS" | sed 's/,/\",\"/g')\"],\"index_sources\":{\"context7\":\"$CONTEXT7_ID\",\"auggie\":\"$SLUG\"},\"weight\":1.0}]" "$ROUTES_FILE"
else
  cat >> "$ROUTES_FILE" <<EOF

- repo: $SLUG
  keywords: [$(echo "$TOPICS" | sed 's/,/, /g')]
  index_sources:
    context7: "$CONTEXT7_ID"
    auggie: "$SLUG"
  weight: 1.0
EOF
fi
echo "Step 4: auto-RAG route registered in $ROUTES_FILE"

# Step 5: CLAUDE.md addendum
CLAUDE_MD="$LOCAL_PATH/CLAUDE.md"
if [[ -f "$CLAUDE_MD" ]]; then
  echo "Step 5: CLAUDE.md already exists; not overwriting."
  echo "  Manually add: '# neuro-link registration: $SLUG' for cross-reference."
else
  cat > "$CLAUDE_MD" <<EOF
# $OWNER_REPO — agent-readable context

Registered with neuro-link-recursive as main codebase \`$SLUG\` on $(date -I).

## Primary topics
$(echo "$TOPICS" | tr ',' '\n' | sed 's/^/- /')

## Cross-references
- Spec file: \`$(basename "$SPEC_FILE")\` in neuro-link-recursive's \`.claude/skills/main-codebase-tools/\`
- Auto-RAG: triggers on keywords listed above
EOF
  echo "Step 5: wrote $CLAUDE_MD"
fi

# Step 6: Spec file
cat > "$SPEC_FILE" <<EOF
---
repo_url: $REPO_URL
local_path: $LOCAL_PATH
language: [$LANG]
primary_topics: [$(echo "$TOPICS" | sed 's/,/, /g')]
index_context7: true
context7_id: "$CONTEXT7_ID"
index_auggie: true
auggie_project: $SLUG
auto_rag_trigger_keywords: [$(echo "$TOPICS" | sed 's/,/, /g')]
weight: 1.0
last_indexed: $(date -I)
registered: $(date -I)
---

# $OWNER_REPO

See repo-local CLAUDE.md for architecture and gotchas.
EOF

echo "Step 6: wrote $SPEC_FILE"
echo
echo "Registration complete. /auto-rag will now inject context for keywords: $TOPICS"
