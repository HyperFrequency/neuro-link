---
skill_name: hyper-sleep
trigger: /hyper-sleep, scheduled nightly
invokes: [wiki-curate, reasoning-ontology, crawl-ingest-update]
HITL: optional
---

# /hyper-sleep

Non-HITL brain maintenance. Runs while the user sleeps; handles everything
from `/neuro-scan` that doesn't require approval.

## Scope

1. **Gap filling** — for each knowledge gap identified by `/neuro-scan`:
   - Auto-ingest proposed sources via `/crawl-ingest-update` (with
     `LLM_assisted=Y` for source selection)
   - Synthesize via `/wiki-curate` with a confidence cap of 0.6 (higher
     confidence requires HITL)
   - Surface high-confidence proposals to `05-insights-HITL/` for morning review
2. **Ontology updates** — re-run `/reasoning-ontology` on topics that have
   gained ≥3 new wiki pages since last ontology refresh.
3. **Folder-delegated self-improvement** — auto-scan each `NN-*` folder for
   local TODOs (files with `status: todo` in frontmatter), pick the lowest-risk
   ones, execute, log.
4. **Cache warming** — pre-embed any new pages from `02-KB-main/` into qmd's
   index (`qmd embed`).
5. **Compaction** — roll `04-Agent-Memory/logs.md` entries older than 7 days
   into `04-Agent-Memory/consolidated/{agent,workflow}/` summaries.

## Safety limits

- No deletions.
- No edits to `02-KB-main/` at confidence > 0.6 without HITL.
- No network access beyond the configured ingest source list.
- 4-hour hard timeout.

## Output

Morning report appended to `06-Recursive/daily.md` plus any
high-confidence proposals queued in `05-insights-HITL/daily.md`.
