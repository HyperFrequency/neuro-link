---
version: 1
scan_targets:
  task_queue: true
  wiki_staleness: true
  knowledge_gaps: true
  failure_log: true
  upstream_docs: true
staleness_threshold_days: 30
gap_analysis_method: infranodus
gap_graph_name: neuro-link-recursive-main
failure_retry_limit: 3
upstream_check_repos:
  - nautilus-trader
  - vectorbtpro
  - optuna
  - mlflow
  - qlib
notify_on:
  - priority_1_tasks
  - critical_failures
  - knowledge_gaps
---

# neuro-scan Configuration

Controls what the brain scanner checks and how it reports.

## Scan Targets

| Target | What it checks | Action on finding |
|--------|---------------|-------------------|
| `task_queue` | `07-neuro-link-task/*.md` with `status: pending` | Queue for job-scanner |
| `wiki_staleness` | Pages in `02-KB-main/` older than `staleness_threshold_days` | Flag as needs_review |
| `knowledge_gaps` | InfraNodus `generate_content_gaps` on union ontology | Create task in 07-neuro-link-task/ |
| `failure_log` | `state/deviation_log.jsonl` for unresolved failures | Re-queue up to `failure_retry_limit` times |
| `upstream_docs` | Repos in `upstream_check_repos` for new releases/commits | Create ingest task |

## Staleness Propagation

When a wiki page is updated, all pages that link to it (found via `wikilinks[]` frontmatter) are flagged as `needs_review` in their frontmatter. This is the Cornelius Brain "staleness propagation" pattern.

## Notifications

The `notify_on` array controls which findings inject an `additionalContext` reminder into the next prompt via the `neuro-task-check.sh` hook.
