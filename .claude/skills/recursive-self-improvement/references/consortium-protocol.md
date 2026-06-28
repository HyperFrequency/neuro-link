# Consortium grading protocol

Three (or more) independent graders produce grades on the same period. A
single grader is biased by its own priors; three with different system
prompts cross-check each other. Majority vote only forwards proposals
that survive cross-examination.

## Grader personas

Each grader has a distinct system prompt to avoid homogenized opinions.

### Grader A — The strict compliance auditor

```
You are a compliance auditor. Your job is to check that every action
recorded in the logs followed its skill's written protocol. You cite
specific log entries and specific skill clauses. You are unsympathetic to
"the intent was good" excuses — if the protocol said X and Y happened,
that's a violation to flag.
```

### Grader B — The results-oriented reviewer

```
You are a results-oriented reviewer. You care about outcomes, not process.
For each significant action in the logs, ask: did it produce the intended
result? If yes, you're happy regardless of whether every checkbox was
ticked. If no, explain why and what would have produced a better outcome.
```

### Grader C — The red-team adversary

```
You are a red-team adversary. For every action that looked fine, look
harder for what might be subtly wrong: did it silently skip something
important? Did it create a problem that won't surface until later? Did
it over-reach beyond its scope? Be skeptical and specific.
```

These three don't agree by default. That's the point. When they do agree
on a proposal, it has survived three different evaluation lenses.

## Grade artifacts

Each grader produces three files, saved to
`05-self-improvement-HITL/grades/<period>/<grader-id>/`:

1. **`orchestrator-agent-grading.md`** — evaluates the `/neuro-link`
   dispatcher
2. **`each-agent-grade.md`** — per-downstream-skill grade
3. **`learning-grade.md`** — did yesterday's changes produce their
   intended improvement?

Each grade file has:

```markdown
# Grade: <grader-id> — <period>

## Overall score: X/10

## Strengths
- <citation-backed positive>
- ...

## Issues
- **<severity>:** <description>
  - Citation: [log:2026-04-18T14:23:01Z]
  - Cause: <hypothesis>
  - Recommendation: <specific, actionable>
- ...

## Proposed changes (0-5)
### Proposal: <slug>
**Target:** <skill | ontology | prompt>
**Change:** <one-sentence>
**Rationale:** <why>
**Risk:** low | medium | high
**Rollback:** <specific command or procedure>
```

## Validation gates

Grader output is rejected and the grader is re-spawned if any of:

- **Zero citations.** A grade without citations is the grader's prior,
  not an audit.
- **Fewer citations than claims.** Every claim needs at least one citation.
- **Citations don't resolve.** If `[log:2026-04-18T14:23:01Z]` doesn't
  exist in the logs, the grader hallucinated.
- **Proposals without rollback.** Every proposal must have a rollback.
- **Proposals without a target path.** "Change the prompt" is too vague.

## Vote aggregation

Deduplicate proposals across graders by slug similarity (Jaccard on
stemmed tokens, threshold 0.6). For each deduplicated proposal:

- **3/3 graders agree** → high-consensus; flag for HITL approval first
- **2/3 graders agree** → consensus; include in HITL batch
- **1/3 grader agrees** → singleton; send to
  `05-insights-HITL/low-consensus-suggestions.md` as informational only,
  never auto-applied

Ties (e.g., the three proposals are slightly different variants of the
same idea) are resolved by a fourth-pass LLM that reads all three and
produces a merged proposal. The merged version goes to HITL with a note
about its composition.

## Safeguards against collusion

The three grader prompts are version-pinned in
`references/grader-personas.md`. Any change to the prompts requires its
own `/recursive-self-improvement` cycle with `target: grader-persona-A`
etc. — so the graders can't drift into agreement over time.

## When to run with more than 3 graders

Default is 3. Increase to 5 when:

- Cumulative grade scores have been trending up for >7 days
  (possible collusion signal)
- A recent change to any grader's prompt
- The period under grading includes a major change to the system

The skill reads `config/neuro-link.md` → `consortium_size` for the
current count.

## Performance

Each grader takes 2–5 minutes depending on log size. Three in parallel:
5 minutes wall clock. Plus 1–2 minutes for the vote aggregator. The full
consortium cycle runs in ~7 minutes.
