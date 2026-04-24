#!/usr/bin/env bash
# PostToolUse(Bash) hook: when a successful `gh repo fork` of a HyperFrequency-targeted
# repo lands, kick off the deep-tool-wiki ingestion in the background and append a
# system reminder so the next response surfaces the new wiki page.
#
# This is the AUTOMATION half — when you fork a new repo into HyperFrequency, this
# hook detects it and queues the ingestion. The actual LLM synthesis (Sonnet ingest +
# Opus ontology) happens via a follow-up agent task that the user (or Claude) triggers
# from the queued state — the hook itself only does the mechanical clone + InfraNodus
# call, then writes a stub page that the next LLM run can fill in.

set -euo pipefail

input="$(cat)"
cmd="$(printf '%s' "$input" | python3 -c 'import sys,json; d=json.load(sys.stdin); print(d.get("tool_input",{}).get("command",""))' 2>/dev/null || echo "")"
exit_code="$(printf '%s' "$input" | python3 -c 'import sys,json; d=json.load(sys.stdin); print(d.get("tool_response",{}).get("exit_code",0))' 2>/dev/null || echo "0")"

# Only proceed on successful gh repo fork commands.
if [[ "$cmd" != *"gh repo fork"* ]] || [[ "$exit_code" != "0" ]]; then
  exit 0
fi

# Extract the upstream spec from the command. Heuristic: the first arg after `fork`.
upstream="$(printf '%s' "$cmd" | python3 -c '
import sys, re
c = sys.stdin.read()
m = re.search(r"gh\s+repo\s+fork\s+([^\s]+)", c)
print(m.group(1) if m else "")
' 2>/dev/null || echo "")"

if [[ -z "$upstream" ]]; then
  exit 0
fi

# Tool name = repo basename, lowercased
tool="$(printf '%s' "$upstream" | awk -F/ '{print tolower($NF)}' | tr '.' '-')"

# Queue an ingestion job in the background. Log to a file so we don't lose output.
queue_dir="$HOME/.claude/deep-tool-wiki-queue"
mkdir -p "$queue_dir"
log="$queue_dir/${tool}-$(date +%s).log"

(
  exec >"$log" 2>&1
  echo "[$(date)] Auto-ingest started for $upstream → $tool"

  # 1. Clone + extract raw doc material
  python3 "$HOME/.claude/scripts/deep-tool-wiki.py" clone "$upstream" \
    --tool "$tool" --out "/tmp/dtw-${tool}-raw.md" || {
      echo "Clone failed"; exit 1;
    }

  # 2. Run InfraNodus on the raw material (cleaner pass will happen when the LLM
  #    synthesizes the body and re-runs InfraNodus on the body)
  python3 "$HOME/.claude/scripts/deep-tool-wiki.py" infranodus "/tmp/dtw-${tool}-raw.md" \
    --name "$tool" --out "/tmp/dtw-${tool}-graph-raw.md" || {
      echo "InfraNodus failed"; exit 1;
    }

  # 3. Write a STUB page in the vault so the new tool is discoverable; the LLM
  #    will fill in the body and re-run InfraNodus on the cleaner body next time
  #    /deep-tool-wiki is invoked or on next /deep-tool-wiki refresh.
  stub_body="$(mktemp)"
  cat <<STUB > "$stub_body"
## Overview

> ⚠️ STUB — auto-generated on fork. Run \`/deep-tool-wiki refresh ${tool}\` to fill in the body via Sonnet ingest + Opus ontology.

Raw doc material is at \`/tmp/dtw-${tool}-raw.md\`.

## Pending sections

- [ ] Overview
- [ ] Installation
- [ ] Core Concepts
- [ ] Quick Start
- [ ] Usage Examples
- [ ] API Surface
- [ ] Common Patterns
- [ ] Integrations
- [ ] Reasoning Ontology (Opus 4.6 task — fresh context)
STUB

  python3 "$HOME/.claude/scripts/deep-tool-wiki.py" write-page "$tool" \
    --upstream "$upstream" \
    --fork "HyperFrequency/$(basename "$upstream")" \
    --body "$stub_body" \
    --infranodus-block "/tmp/dtw-${tool}-graph-raw.md"

  python3 "$HOME/.claude/scripts/deep-tool-wiki.py" update-index

  echo "[$(date)] Auto-ingest stub written for $tool"
) &

# Return additionalContext to inform Claude that ingestion was queued.
cat <<EOF
{
  "hookSpecificOutput": {
    "hookEventName": "PostToolUse",
    "additionalContext": "AUTO-INGEST QUEUED: A fork of '${upstream}' was just created. Background ingestion is running — a stub wiki page will be at /Library/Obsidian-Vault/Auto-Quant/DeepTools/${tool}.md. To complete the full ingest (Sonnet body synthesis + Opus reasoning ontology + clean InfraNodus pass), invoke the /deep-tool-wiki skill with: 'refresh ${tool}'. Logs: ${log}"
  }
}
EOF
