---
name: neuro-surgery
description: HITL-driven repair of the knowledge base, operating on the latest /neuro-scan report. Use this whenever the user says /neuro-surgery, asks to fix failures, resolve ontology inconsistencies, re-synthesize stale wiki pages, coordinate recursive-self-improvement execution, or review deep-reasoning proposals. Also trigger when the user says "fix the scan report", "clean up the failures", "audit contradictions", or wants to work through the backlog interactively. Every single write to 02-KB-main/ or 03-Ontology-main/ requires explicit user approval per-change — never batch approvals, never apply silently. Skilled at diffs: shows before/after side-by-side, waits for thumb up, then applies via schema-aware tools (nlr_wiki_update / reasoning-ontology — never bare Write or tv_edit_note on schema-bound paths).
---

# /neuro-surgery

Interactive repair skill. Pulls from the latest `/neuro-scan` report in `06-Recursive/daily.md` and walks through fixable findings with the user one at a time. Every knowledge-artifact edit is thumbs-up gated.

Design principle: the surgery is slow on purpose. The brain's quality depends on humans trusting that `02-KB-main/` and `03-Ontology-main/` aren't silently mutating. A per-change approval protocol trades speed for trust.

## Operating modes

The user invokes this skill after `/neuro-scan` has produced a report. The skill reads that report and offers options:

```
Scan report from YYYY-MM-DD HH:MM. What do you want to repair?

1. Failures (N recurring, M new)
2. Knowledge gaps (N stale hubs, N isolated clusters, N ontology contradictions)
3. Stale wikis (top 10)
4. Approved self-improvement proposals (N awaiting execution)
5. Upstream doc drift (N repos)
6. Deep-reasoning proposals (N awaiting review)

Type numbers (e.g., "1 3 5") or "all".
```

Work through selected sections one item at a time. Never batch-process.

## Per-item protocol

For each item, follow this exact loop:

### Step 1 — Diagnose

Read the item. Read related context (cited log entries, affected wiki pages, referenced ontology nodes). Form a hypothesis of what's wrong and why.

### Step 2 — Propose

Tell the user:

- **What:** one-sentence diagnosis
- **Why:** causal story (grounded in citations, not speculation)
- **Fix:** the exact change you'd make, as a diff
- **Risk:** low | medium | high + reasoning
- **Rollback:** how to undo if it makes things worse

### Step 3 — Wait for decision

The user types `apply`, `skip`, `modify <instruction>`, or `stop`. Honor whichever one they picked:

- `apply` — execute via schema-aware tool (next step)
- `skip` — record as skipped in surgery log, move on
- `modify X` — update the proposal per X, re-show, loop back to step 3
- `stop` — exit the surgery session cleanly

### Step 4 — Execute (apply only)

Route to the right executor:

| Target | Executor |
|---|---|
| `02-KB-main/*.md` content | `nlr_wiki_update` (preserves schema frontmatter) |
| `02-KB-main/*.md` frontmatter only | `nlr_wiki_update` with `--frontmatter-only` |
| `03-Ontology-main/**/*.md` | `/reasoning-ontology update <path>` |
| Broken wikilink in content | `tv_edit_note` with SEARCH/REPLACE block (safe: only touches link text) |
| Task spec in `00-neuro-link/tasks/` | direct Edit (these have no schema) |
| Config in `config/*.md` frontmatter | direct Edit, frontmatter only |

**Never** use `Write` or `tv_write_note` to replace a `02-KB-main/` or `03-Ontology-main/` file wholesale — that's how frontmatter gets silently dropped.

### Step 5 — Verify

After the write, re-read the target. Confirm:

- Frontmatter schema intact (for `02-KB-main/`)
- Wikilinks still resolve (run `tv_get_broken_links` scoped to the edited path)
- No accidental deletion (diff byte count vs pre-edit)

If any check fails, roll back using the rollback command from the proposal and report to the user.

### Step 6 — Log

Append to `06-Recursive/daily.md` under today's "Surgery log" subsection:

```markdown
- HH:MM <item> — applied | skipped | rolled-back
  - Target: <path>
  - Change: <one-line diff summary>
  - Rollback: <command>
```

## Deep-reasoning proposals

When `/neuro-scan` flags topics where 3+ sources disagree, it produces a `deep-reasoning` task. Surgery doesn't fix these in-place — they require genuine thought. Instead:

1. Present the contradiction clearly (cite each source)
2. Ask the user if they want to commission a research task
3. If yes, draft a task spec in `00-neuro-link/tasks/` with `type: deep-reasoning` and dispatch to `/scientific-critical-thinking` or `/hypothesis-generation` (choose based on whether the question is about evidence quality or mechanism)
4. The eventual output lands in `05-insights-HITL/`, not `02-KB-main/` — because it's a reasoning artifact, not consensus knowledge

See `references/resynthesis-guide.md` for how to frame contradiction reports and when to escalate to a commission vs resolve in-place.

## Approved self-improvement proposals

For items from `/recursive-self-improvement` with `status: approved`, the surgery step is usually trivial — the proposal already has a diff and rollback. Verify the diff still applies cleanly (base file hasn't changed since approval), then apply. If the base has drifted, route back to `/recursive-self-improvement review` to re-examine.

## HITL protocol — non-negotiable rules

These are absolute. If the user presses to bypass, refuse and explain why.

1. **Every edit to `02-KB-main/` or `03-Ontology-main/` requires explicit per-change approval.** Not batch. Not "all low-risk". One at a time.
2. **Never use `Write` on a schema-bound file.** Always route through the schema-aware tool. If the schema-aware tool refuses, the fix needs re-proposal, not bypass.
3. **Always write a rollback command before applying.** If you can't write one, the change isn't safe enough to apply.
4. **Log every surgery action**, even skips. Without a log, the brain can't grade itself tomorrow.

## References

- `references/hitl-protocol.md` — full protocol with examples of correct and incorrect handling
- `references/resynthesis-guide.md` — when to re-synthesize vs surface-patch vs commission deep reasoning
- `references/rollback-patterns.md` — writing rollback commands for each target type

## Scripts

- `scripts/verify_frontmatter.sh` — post-edit schema check
- `scripts/write_surgery_log.sh` — append a log entry in the canonical format
