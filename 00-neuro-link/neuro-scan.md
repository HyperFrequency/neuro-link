---
skill_name: neuro-scan
trigger: /neuro-scan, scheduled hourly via /schedule
invokes: [infranodus-cli, job-scanner]
mcp: [turbovault, neuro-link-recursive]
---

# /neuro-scan

Brain scanner. Produces a structured health report and queues remediation tasks.

## Scan targets

1. **Pending jobs** — `00-neuro-link/tasks/*.md` with `status: pending`. Design
   agent skills / hooks / rules, set execution schedule, register completion
   scan hooks.
2. **Recursive self-improvement proposals** — elevate approved proposals to
   task commands in `00-neuro-link/tasks/`.
3. **Failure logs** — scan `state/llm_logs/`, hook logs, cron jobs, and
   harness-to-harness comms for failures in skills/hooks/chron/h2h.
4. **Semantic scan**:
   - `tv_get_hub_notes` + `tv_get_isolated_clusters` — identify knowledge islands
   - `tv_get_broken_links` — dangling wikilinks
   - InfraNodus `content_gap` + `adversarial_review` on ontologies and workflows
   - Propose new sources to fill gaps
   - Detect reasoning inconsistencies across `03-Ontology-main/`
5. **Stale wikis** — `tv_query_frontmatter_sql "SELECT path WHERE last_updated < now() - interval 30d"`

## Output

`06-Recursive/daily.md` — append a dated scan report section with:

- Pending jobs count + one-line descriptions
- Proposed improvements (elevated or queued)
- Failure summary (hook / skill / cron / h2h)
- Knowledge gap report (top 10 orphan clusters)
- Stale wiki list (>30 days without update)

Remediation tasks auto-queued to `00-neuro-link/tasks/` with `source: neuro-scan`
in the frontmatter.

## Cron cadence

Default: hourly. Configurable in `config/neuro-link.md` frontmatter
`scan_interval`.
