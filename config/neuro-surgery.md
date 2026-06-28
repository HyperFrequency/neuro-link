---
version: 1
hitl_required:
  - wiki_page_delete
  - ontology_restructure
  - source_of_truth_update
  - auto_tune_proposal
  - harness_config_change
  - bulk_staleness_resolution
auto_approved:
  - wiki_page_create
  - wiki_page_update_minor
  - ontology_append
  - task_status_update
  - log_append
  - index_regenerate
escalation_channel: terminal
approval_timeout_minutes: 0
fix_categories:
  - stale_page
  - broken_ontology
  - failed_ingest
  - contradicted_claim
  - missing_source
  - hook_failure
  - skill_error
---

# neuro-surgery Configuration

HITL escalation rules. Defines which operations require human approval before execution.

## Approval Matrix

### Requires Human Approval
- **wiki_page_delete** — removing a wiki page from 02-KB-main/
- **ontology_restructure** — changing the high-level structure of a reasoning ontology
- **source_of_truth_update** — modifying content in 02-KB-main/ that other pages cite
- **auto_tune_proposal** — self-improvement proposals for skills, hooks, or prompts
- **harness_config_change** — modifying harness bridge settings
- **bulk_staleness_resolution** — updating >5 stale pages in one operation

### Auto-Approved (no confirmation needed)
- **wiki_page_create** — creating new wiki pages
- **wiki_page_update_minor** — small updates (typo fixes, date updates, adding a source)
- **ontology_append** — adding new triples to an existing ontology
- **task_status_update** — changing task frontmatter status
- **log_append** — writing to log.md or state/*.jsonl
- **index_regenerate** — rebuilding 02-KB-main/index.md

## Fix Workflow

When neuro-surgery processes a fix:
1. Read the deviation from `state/deviation_log.jsonl`
2. Categorize into one of `fix_categories`
3. Check if the fix requires HITL approval
4. If approved (or auto-approved): execute the fix
5. Log the result to `state/deviation_log.jsonl` with resolution status
6. If the fix creates downstream staleness: queue staleness propagation

## Escalation

`escalation_channel: terminal` means HITL prompts appear in the Claude Code terminal.
`approval_timeout_minutes: 0` means no timeout — wait indefinitely for user response.
