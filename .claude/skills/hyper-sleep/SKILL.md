---
name: hyper-sleep
description: Non-HITL nightly brain maintenance. Use this whenever the user says /hyper-sleep, schedules overnight maintenance, or when triggered by cron during the configured sleep window. Also trigger when the user says "run maintenance", "do background cleanup", "catch up while I sleep", or hands off work expecting it done by morning. Handles the subset of findings from /neuro-scan that don't need human judgment: auto-ingests proposed sources, synthesizes with a hard confidence cap at 0.6, refreshes ontologies for topics with ≥3 new pages, executes folder-delegated low-risk improvements, warms the qmd embedding cache, and compacts agent memory logs older than 7 days. Hard 4-hour timeout, no deletions, no network access outside the configured source list, no edits above confidence=0.6 without HITL.
---

# /hyper-sleep

Autonomous maintenance pass. Runs while the user sleeps, handles everything that `/neuro-scan` flagged as safe-to-auto-fix, and leaves a morning report for the user.

Design principle: hyper-sleep trades coverage for safety. It only does work where the cost of getting it wrong is low and recovery is trivial. If a finding sits anywhere close to the HITL threshold, `hyper-sleep` skips it and defers to `/neuro-surgery`.

## Safety bounds — enforced

These are hard gates. If any is violated, the skill halts and writes a failure entry to `04-Agent-Memory/logs.md`.

1. **No deletions.** Not of files, not of Qdrant collection entries, not of Neo4j nodes. If maintenance would require a delete, convert to a task spec for `/neuro-surgery` and skip.
2. **Confidence ceiling: 0.6.** Any wiki page written during sleep has frontmatter `confidence: <= 0.6`. Pages needing higher confidence get staged in `05-insights-HITL/pending-curation/` for morning review.
3. **Network allowlist.** Only the ingest sources listed in `config/neuro-link.md` → `hyper_sleep_sources`. No arbitrary URL fetching.
4. **4-hour hard timeout.** Absolute wall-clock limit from start. If exceeded, cleanly stop current task, checkpoint state, write what was finished to the morning report, exit.
5. **No writes outside allowed paths.** Allowed: `01-raw/`, `01-sorted/`, `02-KB-main/` (cap 0.6), `04-Agent-Memory/`, `06-Recursive/`, `state/`. Writes elsewhere are a bug; halt if detected.

## Workload

Read the latest `/neuro-scan` report. Filter findings to ones that match a `hyper-sleep`-eligible type:

### Gap filling

For each knowledge gap (isolated cluster or stale hub):

1. Delegate to `/crawl-ingest-update` with `LLM_assisted=Y` to propose 1–3 authoritative sources from the allowlist.
2. Ingest via the right pipeline (arXiv / ar5iv / Stacks / Marker depending on source type — see that skill's references).
3. Synthesize via `/wiki-curate` with `confidence_ceiling: 0.6` flag.
4. If the synthesis's computed confidence exceeds 0.6, don't write to `02-KB-main/` — stage in `05-insights-HITL/pending-curation/YYYY-MM-DD/<slug>.md` with the full draft, citations, and a decision prompt.

### Ontology refresh

For each topic that has gained ≥3 new wiki pages in `02-KB-main/<topic>/` since last ontology refresh:

1. Invoke `/reasoning-ontology <topic>` with `--refresh --mode=append-only`.
2. The `append-only` mode forbids the ontology skill from *deleting* existing triples — it can only add new ones or raise their confidence. Contradiction resolution (which might require a delete) is HITL-only.

### Folder-delegated improvements

Scan each `NN-*/` directory for files with frontmatter `status: todo`. Filter to `risk: low` (if unset, skip). For each:

1. Read the todo's `action` field.
2. If it matches a known pattern in `references/safe-auto-actions.md`, execute.
3. Otherwise skip — unknown actions are not hyper-sleep territory.

Safe auto-actions include: regenerating `index.md` files, fixing known broken-link patterns, normalizing frontmatter date formats, compacting log files.

### qmd cache warmup

For every page in `02-KB-main/` that doesn't have a corresponding qmd vector (qmd stores vectors in SQLite at `~/.cache/qmd/vectors.db`), run:

```
qmd embed --collection kb --incremental
```

`--incremental` only processes new/modified files. Skip if qmd isn't installed (report as warning in morning report, don't fail).

### Memory log compaction

Roll old entries from `04-Agent-Memory/logs.md`:

- Entries older than 7 days → summarize and append to `04-Agent-Memory/consolidated/<period>/<YYYY-WW>.md`
- Keep raw entries from last 7 days intact
- Consolidation grouping: by-agent (`consolidated/agent/<agent-name>.md`) and by-workflow (`consolidated/workflow/<workflow-slug>.md`)

Consolidation is a summarization task, not a deletion. The raw `logs.md` append-only file just gets a new "Archived up to <date>" marker; the old entries stay but are superseded by the consolidated version for read purposes.

See `references/compaction.md` for the exact consolidation prompt and format.

## Morning report

At the end of the sleep window (or when work runs out), append to `06-Recursive/daily.md`:

```markdown
## Hyper-sleep YYYY-MM-DD (HH:MM–HH:MM, N minutes)

### Gaps filled
- <topic> — <source> → 02-KB-main/<path> (confidence 0.5)
- ...

### Staged for HITL (in 05-insights-HITL/pending-curation/)
- <slug> — ingested + drafted, confidence 0.7 (above ceiling)
- ...

### Ontology refreshes
- <topic> — added N triples

### Folder improvements executed
- <pattern> on <path> — ok

### qmd index
- +N new embeddings

### Memory compaction
- consolidated <date>–<date>, N entries → consolidated/

### Incidents
- <timestamp> — <what went wrong, what was checkpointed>
```

## When not to run

- If `state/heartbeat.json` status != `ready`: halt. Setup is incomplete.
- If `config/neuro-link.md` → `hyper_sleep: disabled`: halt silently (user opted out).
- If a previous `hyper-sleep` is still running (check pid file in `state/`): halt and report.

## References

- `references/safety-bounds.md` — full list of safety checks and why each exists
- `references/compaction.md` — memory consolidation prompt + format
- `references/safe-auto-actions.md` — whitelisted folder-delegated action patterns

## Scripts

- `scripts/embed_warmup.sh` — idempotent qmd cache warm
- `scripts/compact_memory.py` — log consolidation
- `scripts/sleep_watchdog.sh` — enforces the 4-hour timeout
