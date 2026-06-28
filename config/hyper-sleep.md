---
version: 1
enabled: false
schedule:
  garbage_collect_raw:
    interval_hours: 168
    description: Remove raw files older than 90 days that have been fully curated
    max_age_days: 90
  reindex_ontologies:
    interval_hours: 24
    description: Rebuild InfraNodus graph indexes for all persisted ontologies
  check_upstream_docs:
    interval_hours: 12
    description: Check upstream repos for doc changes since last ingest
  compact_logs:
    interval_hours: 72
    description: Archive old session_log.jsonl entries (>30 days) to compressed files
  regenerate_indexes:
    interval_hours: 6
    description: Rebuild 02-KB-main/index.md and verify all wikilinks resolve
  health_check:
    interval_hours: 1
    description: Update state/heartbeat.json with MCP server connectivity status
  gap_analysis:
    interval_hours: 48
    description: Run InfraNodus content gap analysis on full ontology corpus
hitl_for_deletions: true
hitl_for_ontology_updates: true
---

# hyper-sleep Configuration

Background maintenance tasks. Non-HITL work from neuro-scan findings.

## Enabling

Set `enabled: true` in frontmatter, then create a scheduled trigger:
```
/schedule create --name "neuro-hyper-sleep" --cron "0 */6 * * *" --prompt "/neuro-scan"
```

Or use the loop command for self-paced execution:
```
/loop /neuro-scan
```

## Maintenance Tasks

Each task in `schedule` runs at its specified interval. The `description` field explains what it does.

### Garbage Collection
Removes raw files from `00-raw/` that:
- Are older than `max_age_days`
- Have a corresponding wiki page in `02-KB-main/` (i.e., fully curated)
- Are NOT the sole source for any active wiki page

### Log Compaction
Moves old JSONL entries from `state/session_log.jsonl` and `state/score_history.jsonl` to compressed archives in `state/archive/`. Keeps the active files small for fast reads.

### Gap Analysis
Runs InfraNodus `generate_content_gaps` across all ontologies in `03-ontology-main/`. Creates task files in `07-neuro-link-task/` for topics with gaps. This drives the "auto-ingest & synthesize" behavior — the system finds what it doesn't know and proposes sources to fill it.

## HITL Guards

Even in background mode:
- `hitl_for_deletions: true` — never delete files without user confirmation
- `hitl_for_ontology_updates: true` — propose ontology changes, don't auto-apply
