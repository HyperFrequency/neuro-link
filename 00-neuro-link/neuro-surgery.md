---
skill_name: neuro-surgery
trigger: /neuro-surgery
invokes: [wiki-curate, reasoning-ontology, infranodus-cli]
HITL: required
---

# /neuro-surgery

HITL-driven repair of the knowledge base. Operates on the latest `/neuro-scan`
report in `06-Recursive/daily.md`.

## Responsibilities

1. **Fix failure reports** — one by one, propose a fix, show diff, wait for
   approval, apply.
2. **Coordinate recursive self-improvement** — pick up approved proposals from
   `/recursive-self-improvement` and sequence their execution.
3. **Resolve ontological inconsistencies** — cross-reference
   `03-Ontology-main/{workflow,agents}/`. When two ontologies disagree, surface
   the disagreement in `05-insights-HITL/`, wait for a canonical decision.
4. **Re-synthesize source-of-truth wikis** — when a wiki page is flagged as
   out-of-date or contradictory with a newly ingested source, re-run
   `/wiki-curate` against the updated `01-raw/` material. Human approves before
   overwriting `02-KB-main/`.
5. **Propose deep reasoning tasks** — review synthesis logs for topics that
   warrant a deeper follow-up (e.g., "three papers contradict on X"). Draft a
   task spec in `00-neuro-link/tasks/` with `type: deep-reasoning`.
6. **Job report** — on completion, append a "surgery log" entry to
   `06-Recursive/daily.md` describing what changed and why.

## HITL protocol

Every surgical action requires explicit approval. The skill never writes to
`02-KB-main/` or `03-Ontology-main/` without a thumb-up from the user.

## Inputs

- `06-Recursive/daily.md` — latest scan report
- `05-insights-HITL/daily.md` — HITL review queue
- `state/llm_logs/` — raw interaction context

## Outputs

- Edits to `02-KB-main/` (schema-preserving, via `nlr_wiki_update`)
- Edits to `03-Ontology-main/` (via `/reasoning-ontology`)
- Surgery-log entry in `06-Recursive/daily.md`
