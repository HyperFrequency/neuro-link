---
name: neuro-scan
description: Brain scanner that produces a structured health report and queues remediation tasks. Use this whenever the user says /neuro-scan, asks to check pending jobs, find knowledge gaps, scan for stale wikis, audit failures in hooks/skills/cron/harness-to-harness comms, or when scheduled hourly by cron. Also trigger when the user says "what's broken", "audit the brain", "what should I work on next", or is about to start a work session and wants situational awareness. Runs six independent scan passes — pending jobs, self-improvement proposals, failure logs, knowledge-graph gaps, stale wikis, and upstream doc drift — then writes a dated report to 06-Recursive/daily.md and auto-queues remediation tasks with source=neuro-scan to 00-neuro-link/tasks/.
---

# /neuro-scan

Scan the brain and produce an actionable health report. This skill does not fix anything — it inventories what needs attention and delegates fix-work to `/neuro-surgery` (HITL) or `/hyper-sleep` (non-HITL).

Design principle: the scan must be *fast* (<60s total) and *non-mutating*. The report goes to `06-Recursive/daily.md`. Any proposed fix goes to `00-neuro-link/tasks/` as a task spec, not as an immediate edit.

## Six scan passes

Run these in parallel where possible — they are independent.

### Pass 1 — Pending job queue

List every `.md` file in `00-neuro-link/tasks/` with frontmatter `status: pending`. Sort by `priority` (1 highest). For each:

- Read the type (`ingest | curate | scan | repair | report | ontology | deep-reasoning`)
- Check if dependencies (`depends_on: [task-slug]`) are all `status: completed`
- Confirm `assigned_harness` is available (most will be `claude-code`)

Include in the report: count + one-line description + blocking-dependency flag per task.

### Pass 2 — Approved self-improvement proposals

Read `07-self-improvement-HITL/overview.md` for proposals with `status: approved` that haven't been executed yet. These were approved by `/recursive-self-improvement` but never applied. Convert each to a task spec via `scripts/elevate_proposal_to_task.sh` so `/job-scanner` can pick them up.

### Pass 3 — Failure logs

Scan in this order, newest first:

1. Hook logs: `state/hooks/*.log` — look for non-zero exit codes in the last 24h
2. LLM logs: `state/llm_logs/<token>/<date>.jsonl` — look for `error` fields or status 5xx
3. Cron logs: `state/cron/*.log` — same
4. Harness-to-harness comms: `06-self-improvement-recursive/harness-to-harness-comms/*.json` with `status: failed`

Deduplicate: if the same error fingerprint (command + exit code + first line of stderr) appears more than once in 24h, report as a recurring failure with a count.

### Pass 4 — Semantic knowledge graph

Run three checks via TurboVault and InfraNodus:

1. `tv_get_hub_notes` — top-20 concepts by centrality. If any hub has last_updated older than 30 days, flag as "stale hub".
2. `tv_get_isolated_clusters` — disconnected subgraphs in `02-KB-main/`. Each cluster is a knowledge island that doesn't link to the main body — probably a gap.
3. InfraNodus `content_gap` + `adversarial_review` on each ontology in `03-Ontology-main/`. Look for contradictions between workflow and agent ontologies, and for topics the ontologies mention but that have no wiki page.

For each finding, propose a remediation task: `ingest` task to fill gaps, `ontology` task to resolve contradictions.

See `references/gap-analysis.md` for how to write these task specs.

### Pass 5 — Stale wikis

```sql
-- executed via tv_query_frontmatter_sql
SELECT path, domain, last_updated, confidence
FROM notes
WHERE path LIKE '02-KB-main/%'
  AND last_updated < DATE_SUB(CURRENT_DATE, INTERVAL 30 DAY)
ORDER BY confidence DESC, last_updated ASC
LIMIT 50
```

High-confidence *and* stale pages are the most valuable to refresh — they're load-bearing but haven't been audited recently. Report top 10.

### Pass 6 — Upstream doc drift

For every repo tracked in `08-code-docs/toolbox/` and `08-code-docs/forked-up/`, check:

- Has upstream released since `last_synced`? (use `gh api repos/<owner>/<repo>/releases?per_page=1`)
- If yes, propose a `curate` task for `/adjacent-tools-code-docs` or `/forked-repos-with-changes` to pick up the delta.

See `references/failure-patterns.md` for the common failure fingerprints to recognize and the right remediation task type for each.

## Report format

Append to `06-Recursive/daily.md`:

```markdown
## Scan YYYY-MM-DD HH:MM

### Pending jobs (N)
- [priority 1] <slug>: <description> — blocked-by: <dep> | ready
- ...

### Elevated proposals (N)
- <slug>: <target> — queued to 00-neuro-link/tasks/<slug>.md

### Failures (N recurring, M new)
- <fingerprint> x <count> — first seen YYYY-MM-DD, last seen YYYY-MM-DD

### Knowledge gaps
- Stale hubs (N): <wiki>, <wiki>, ...
- Isolated clusters (N): <cluster-topic>, ...
- Ontology contradictions (N): <ontology-a> vs <ontology-b> on <node>

### Stale wikis top 10
| Path | Domain | Last updated | Confidence |
| ... | ... | ... | ... |

### Upstream doc drift
- <repo>: last_synced YYYY-MM-DD, upstream released YYYY-MM-DD — queued to <tasks/>
```

## Auto-queued tasks

Every actionable finding produces a task file in `00-neuro-link/tasks/<slug>.md` with:

```yaml
---
type: ...
status: pending
priority: <inherited from finding severity>
created: <today>
depends_on: []
assigned_harness: claude-code
source: neuro-scan
scan_date: YYYY-MM-DD HH:MM
---
```

## Cadence

Default hourly via cron. Override in `config/neuro-link.md` → `scan_interval`. Also runs ad-hoc when the user invokes `/neuro-scan`.

## References

- `references/gap-analysis.md` — InfraNodus `content_gap` / `adversarial_review` interpretation + task-spec generation
- `references/failure-patterns.md` — common error fingerprints and remediation mappings

## Scripts

- `scripts/scan_failures.sh` — aggregates hook/cron/h2h logs and dedupes by fingerprint
- `scripts/stale_wikis.sh` — runs the frontmatter-SQL query and formats results
- `scripts/elevate_proposal_to_task.sh` — converts approved self-improvement proposals to task specs
