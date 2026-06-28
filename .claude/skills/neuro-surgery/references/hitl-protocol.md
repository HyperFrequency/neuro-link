# HITL protocol — the rules behind every surgery

Every edit to `02-KB-main/` or `03-Ontology-main/` requires explicit user
approval per change. This document is the why and how.

## Why per-change, never batch

Batching approvals ("approve all low-risk changes") sounds efficient, but
it trades trust for speed. A batch approval means the user didn't look at
each change individually. The next time the brain emits weird knowledge,
the user has to investigate "did I actually approve this, or did some
batch sneak it in?" The answer can't be confidently recovered.

Per-change approval:

- Takes ~5 seconds per change with a diff view
- Forces the user to actually see the edit
- Keeps the approval log auditable (each approval has a timestamp)
- Prevents silent drift in the brain's canonical content

The efficiency loss is real but worth it. If you need to batch because you
have 50 changes queued, that's a signal that something upstream is producing
too many low-value proposals — fix the upstream, not the review bar.

## The 6-step loop

Every surgical action follows this exact flow:

### 1. Diagnose

Read the item. Read the citations. Read the target file at its current
version. Form a hypothesis of what's wrong and why. Write the hypothesis
down before proposing a fix — it keeps the next steps honest.

### 2. Propose

Tell the user 5 things:

- **What** — one sentence
- **Why** — causal story grounded in citations (not speculation or the
  LLM's prior)
- **Fix** — the exact change as a unified diff
- **Risk** — low / medium / high + one-sentence reasoning
- **Rollback** — the specific command that undoes this change

If you can't write a rollback, you can't propose. That's a hard gate —
it forces you to think about reversibility before you ask for approval.

### 3. Wait

The user types one of:

- `apply` — execute
- `skip` — record in log, move on
- `modify <instruction>` — update proposal per the instruction, re-show, loop back to step 3
- `stop` — exit cleanly

Don't assume anything else. If the user's response is unclear (e.g., they
ask a clarifying question), answer, then re-prompt.

### 4. Execute

Route to the right executor:

| Target | Executor | Why |
|---|---|---|
| `02-KB-main/*.md` body | `nlr_wiki_update` | Preserves frontmatter schema |
| `02-KB-main/*.md` frontmatter | `nlr_wiki_update --frontmatter-only` | Validates against schema |
| `03-Ontology-main/**/*.md` | `/reasoning-ontology update` | Maintains both tiers + InfraNodus sync |
| Broken wikilink only | `tv_edit_note` w/ SEARCH/REPLACE | Safe; only touches link text |
| `00-neuro-link/tasks/*.md` | direct `Edit` | No schema |
| `config/*.md` frontmatter | direct `Edit`, frontmatter only | Body is docs, not config |

Never `Write` a wholesale replacement of a schema-bound file. That's how
frontmatter gets silently dropped.

### 5. Verify

After the write, re-read the target and run these checks:

- **Schema intact** for `02-KB-main/`: every required fm field present,
  sections in order, no obvious corruption
- **Wikilinks resolve**: `tv_get_broken_links` scoped to the edited path
  returns none that weren't there before
- **No accidental deletion**: compare byte count before/after against your
  expected delta (e.g., if you added 100 bytes, the file should be +100)

If any check fails, roll back using the command you wrote in Step 2. Then
report the failure to the user — don't attempt a second auto-fix.

### 6. Log

Append to `06-Recursive/daily.md` under today's "Surgery log" subsection:

```markdown
- HH:MM <item> — applied | skipped | rolled-back
  - Target: <path>
  - Change: <one-line diff summary>
  - Rollback: <command used in rollback step if failed; else the command you prepared>
  - Approver: <user handle>   # in HITL_override=agent mode, this is the reviewer agent's id
```

## When to refuse

Refuse the surgery and escalate to a task if any of:

- **The change requires deleting content the user didn't explicitly mention.**
  Propose the deletion separately as its own per-change approval.
- **The change would touch >3 files.** Convert to a `batch_execute` proposal
  with the full plan, get approval on the plan, then execute as a single
  atomic transaction.
- **The risk is marked high and the rollback is shaky.** Queue a
  deep-reasoning task and punt.
- **The user says "apply all" or similar.** Refuse; offer instead to change
  the HITL_override to `agent` mode where a reviewer agent auto-approves
  within safety bounds.

## HITL_override modes

Three modes in `config/neuro-link.md`:

### `off` (dev-only, dangerous)

Every approval auto-granted. Use only in a dev branch when iterating on
the surgery skill itself. Never on master.

### `manual` (default, production)

Every approval asks the human. This is what this doc describes.

### `agent` (automation path)

A reviewer agent substitutes for the human. The reviewer's safety bounds:

- May auto-approve: typo fixes, frontmatter date bumps, broken-link
  repairs, index regenerations
- May NOT auto-approve: content additions/deletions, ontology node
  changes, confidence bumps, domain reclassifications

Anything outside the auto-approve list escalates to `manual` for the human,
even in `agent` mode.

## Recording trust

Every approval (regardless of mode) goes into the log. The log is what
lets `/recursive-self-improvement` grade the surgery skill tomorrow: did
approved changes hold up? Were there rollbacks within 24 hours of an
approval? The log is the audit trail.
