#!/usr/bin/env bash
# UserPromptSubmit hook: auto-inject relevant neuro-link-recursive wiki context
# into every prompt based on keyword matching against 02-KB-main/index.md and wiki pages.
#
# Design: FAST (<100ms). No LLM calls. Pure keyword grep against the wiki index.
# Falls back to scanning wiki page titles and frontmatter domains if no index exists.

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

KB_MAIN="${NLR_ROOT}/02-KB-main"
RAG_INDEX="${NLR_ROOT}/state/auto-rag-index.json"

# Read prompt from stdin JSON
input="$(cat)"
prompt="$(printf '%s' "$input" | python3 -c 'import sys,json; d=json.load(sys.stdin); print(d.get("prompt",""))' 2>/dev/null || echo "")"

if [[ -z "$prompt" ]]; then
  exit 0
fi

# Check if auto-rag is enabled in config
config="${NLR_ROOT}/config/neuro-link.md"
if [[ -f "$config" ]]; then
  auto_rag="$(python3 -c "
import sys, re
with open('$config') as f:
    content = f.read()
m = re.search(r'auto_rag:\s*(true|false)', content)
print(m.group(1) if m else 'true')
" 2>/dev/null || echo "true")"
  if [[ "$auto_rag" == "false" ]]; then
    exit 0
  fi
fi

# Lowercase prompt for matching
lc="$(printf '%s' "$prompt" | tr '[:upper:]' '[:lower:]')"

# Strategy 1: Use pre-built index if available
if [[ -f "$RAG_INDEX" ]]; then
  matches="$(python3 -c "
import json, sys

prompt = sys.argv[1].lower()
words = set(prompt.split())

with open('$RAG_INDEX') as f:
    index = json.load(f)

hits = []
for kw, pages in index.get('keywords', {}).items():
    if kw in words or kw in prompt:
        for page in pages:
            info = index.get('pages', {}).get(page, {})
            hits.append({
                'page': page,
                'title': info.get('title', ''),
                'overview': info.get('overview', ''),
                'keyword': kw
            })

# Deduplicate by page, keep top 3
seen = set()
unique = []
for h in hits:
    if h['page'] not in seen:
        seen.add(h['page'])
        unique.append(h)
    if len(unique) >= 3:
        break

if unique:
    parts = []
    for h in unique:
        parts.append(f\"[{h['title']}] {h['overview']}\")
    print('NEURO-LINK-RECURSIVE AUTO-RAG: Relevant wiki context for this prompt:\\n' + '\\n---\\n'.join(parts))
" "$lc" 2>/dev/null || echo "")"

  if [[ -n "$matches" ]]; then
    cat <<EOF
{
  "hookSpecificOutput": {
    "hookEventName": "UserPromptSubmit",
    "additionalContext": "$(printf '%s' "$matches" | python3 -c 'import sys,json; print(json.dumps(sys.stdin.read()))'  | sed 's/^"//;s/"$//')"
  }
}
EOF
    exit 0
  fi
fi

# Strategy 2: Fallback — grep wiki page titles against prompt keywords
if [[ -d "$KB_MAIN" ]]; then
  matches="$(python3 -c "
import os, sys, re

prompt = sys.argv[1].lower()
kb = '$KB_MAIN'
skip = {'schema.md', 'index.md', 'log.md'}
hits = []

for root, dirs, files in os.walk(kb):
    for f in files:
        if f in skip or not f.endswith('.md'):
            continue
        # Match filename (kebab-case) against prompt
        name = f.replace('.md', '').replace('-', ' ')
        words = name.split()
        if any(w in prompt for w in words if len(w) > 3):
            path = os.path.join(root, f)
            # Read first 10 lines for overview
            with open(path) as fh:
                lines = []
                in_frontmatter = False
                for line in fh:
                    if line.strip() == '---':
                        in_frontmatter = not in_frontmatter
                        continue
                    if not in_frontmatter and line.strip():
                        lines.append(line.strip())
                    if len(lines) >= 5:
                        break
            overview = ' '.join(lines)[:300]
            hits.append(f'[{name}] {overview}')
            if len(hits) >= 3:
                break
    if len(hits) >= 3:
        break

if hits:
    print('NEURO-LINK-RECURSIVE AUTO-RAG: Relevant wiki context:\\n' + '\\n---\\n'.join(hits))
" "$lc" 2>/dev/null || echo "")"

  if [[ -n "$matches" ]]; then
    cat <<EOF
{
  "hookSpecificOutput": {
    "hookEventName": "UserPromptSubmit",
    "additionalContext": "$(printf '%s' "$matches" | python3 -c 'import sys,json; print(json.dumps(sys.stdin.read()))' | sed 's/^"//;s/"$//')"
  }
}
EOF
    exit 0
  fi
fi

# No matches — exit silently
exit 0
