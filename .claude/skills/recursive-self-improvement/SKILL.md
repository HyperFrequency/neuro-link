---
name: recursive-self-improvement
description: Automated self-improvement loop with consortium grading and HITL gates. Use this whenever the user says /recursive-self-improvement, asks to run the improvement cycle, review consortium grades, check performance delta, or when this is triggered by scheduled cron. Also trigger when the user says "grade the agents", "see how we're doing", "what can be improved", or is about to make significant changes to prompts/skills/ontologies and wants a data-backed recommendation. Runs a 3+ agent consortium (never single-grader — that's overfit to one reviewer's biases), majority-votes on proposals, surfaces the proposal batch to the user for HITL approval (default manual), and only then executes approved adjustments to agent topology, ontologies, skills, or prompts. Never auto-applies changes to 02-KB-main or 03-Ontology-main without explicit user approval.
---

# /recursive-self-improvement

Closed-loop improvement of the neuro-link system based on its own operational logs. The loop is deliberately conservative — every change that touches agent behavior, skill content, or ontology structure requires human approval by default.

## Config (read from `config/neuro-link.md` frontmatter)

```yaml
scheduled: daily | weekly | off
require_consortium: Y            # if N, single grader — not recommended
HITL_override: off | manual | agent
harness_cli: claude | forgecode | claw-code
full_session_logs: true
```

The *why* for consortium-required-by-default: a single grader inherits the grader's biases (e.g., recency bias, preference for verbose explanations). Three independent graders with different system prompts cross-check each other. Majority vote only approves proposals that survive cross-examination.

## Inputs

- `04-Agent-Memory/logs.md` — append-only event stream
- `04-Agent-Memory/perf-grade.md` — rolling grade history
- `06-Recursive/daily.md` — yesterday's report (delta source)
- `state/llm_logs/<token>/<date>.jsonl` — raw LLM interactions (bucketed by bearer token)

## Flow

### Phase 1 — Consortium grading

Spawn three grader subagents in parallel. Each reads the period's logs independently. They produce three grade documents:

1. `orchestrator-agent-grading.md` — evaluates the orchestrator (the `/neuro-link` dispatcher) on: correct subcommand routing, tool-choice quality, output-contract adherence.
2. `each-agent-grade.md` — per-downstream-skill grade (`neuro-scan`, `neuro-surgery`, etc.) on: completion rate, HITL-protocol adherence, log-hygiene.
3. `learning-grade.md` — did yesterday's changes improve the metrics they were meant to improve? (Explicit A/B: flag the change, find invocations before/after, compare.)

Each grader must cite specific log entries. Grades without citations are rejected and re-spawned.

See `references/consortium-protocol.md` for the full grader system prompts and cross-check rubric.

### Phase 2 — Proposal extraction

Each grader produces 0–5 proposed changes. Deduplicate across graders. For each unique proposal, count graders in agreement. Proposals with `count >= 2` pass to HITL. Singleton proposals go to `05-insights-HITL/` as "low-consensus suggestions" — informational only, never auto-applied.

### Phase 3 — HITL gate

Surface the HITL-eligible proposal batch in `05-insights-HITL/daily.md` under today's date. Format each proposal as:

```markdown
### Proposal: <slug>
**Target:** <agent | skill | ontology-node | prompt>
**Change:** <one-sentence summary>
**Rationale:** <why graders agreed>
**Citations:** [log:YYYY-MM-DD-HH:MM:SS], [log:...], [log:...]
**Risk:** low | medium | high
**Rollback:** <command or procedure to undo>

**Decision:** [ ] approve  [ ] reject  [ ] defer
```

Wait for user to fill in decisions. Loop until all proposals have a decision or the user says "skip the rest".

`HITL_override=agent` bypasses this by spawning a reviewer agent whose only job is to approve/reject within safety bounds (no writes to `02-KB-main/`, no tool permission elevation). `HITL_override=off` skips HITL entirely — dev-only; print a warning.

### Phase 4 — Execute approved changes

For each approved proposal, route to the appropriate executor:

| Target type | Executor |
|---|---|
| Agent topology | Write to `03-Ontology-main/agents/by-agent/<name>.md` via `/reasoning-ontology` |
| Workflow ontology | Write to `03-Ontology-main/workflow/<file>.md` via `/reasoning-ontology` |
| Skill body | `/skill-creator modify` against `.claude/skills/<name>/SKILL.md` |
| Prompt template | Edit file in `07-self-improvement-HITL/prompts/` |
| Config knob | Edit frontmatter of `config/neuro-link.md` (frontmatter only — never body) |

All executions go through their schema-aware tool. Never edit target files with bare `Write` — this breaks the auto-RAG index.

### Phase 5 — Change-log

Append a dated entry to `06-Recursive/daily.md`:

```markdown
## YYYY-MM-DD
**Consortium:** 3 graders, <total citations> citations
**Proposals:** <total>, approved <N>, rejected <M>, deferred <K>
**Executed:**
- <slug>: <target> — <one-line what changed>
...
**Rollback manifest:** `06-Recursive/rollback/YYYY-MM-DD.sh`
```

Generate the rollback manifest concurrently — a bash script that undoes every approved change in reverse order. This is what turns "recursive self-improvement" from a scary phrase into a debuggable system.

## Scheduled invocation

When run via cron (not interactive), skip Phase 3 if `HITL_override=agent`; otherwise queue a task in `00-neuro-link/tasks/` with `type: recursive-self-improvement-review` and exit. The task gets picked up next time the user opens the system, at which point the HITL gate runs interactively.

## References

- `references/consortium-protocol.md` — grader system prompts and rubric
- `references/grading-criteria.md` — what "good" looks like per target type
- `references/rollback-patterns.md` — how to write a rollback manifest that actually works

## Scripts

- `scripts/spawn_consortium.py` — spawns N graders in parallel, collects results
- `scripts/dedup_proposals.py` — merges identical proposals across graders
- `scripts/generate_rollback.py` — builds the rollback manifest from approved proposals
