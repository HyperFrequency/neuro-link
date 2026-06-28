# hyper-sleep safety bounds

These are the hard gates that separate safe autonomous maintenance from
dangerous autonomous "improvement". Every bound is tuned to a real failure
mode seen in practice.

## Bound 1 — No deletions

**Rule:** hyper-sleep never deletes files, Qdrant collection entries, or
Neo4j nodes. If a finding would require a delete, convert it to a task
spec for `/neuro-surgery` (HITL) and skip.

**Why:** deletes are easy to auto-decide and impossible to auto-recover.
A failed re-ingestion that thinks the wiki page is "stale beyond repair"
could wipe a human-curated page at 3am. The cost of missing one deletion
overnight is zero (surgery will catch it tomorrow). The cost of one wrong
deletion is potentially irrecoverable work.

## Bound 2 — Confidence ceiling 0.6

**Rule:** any wiki page written during sleep has frontmatter `confidence
<= 0.6`. Pages computed at higher confidence go to
`05-insights-HITL/pending-curation/YYYY-MM-DD/<slug>.md` for morning review,
not directly to `02-KB-main/`.

**Why:** confidence > 0.6 means the synthesis is claiming high authority.
High-authority content should be reviewed by a human before it joins the
canonical brain. Low-confidence content can flow freely because downstream
consumers know to discount it.

The 0.6 threshold isn't arbitrary. Below 0.5 is "draft / exploratory".
0.5–0.6 is "solid but not canonical". >0.6 is "I'd cite this in a paper".
The overnight path fills the first two tiers; humans fill the third.

## Bound 3 — Network allowlist

**Rule:** only the ingest sources listed in
`config/neuro-link.md` → `hyper_sleep_sources` may be fetched during sleep.
No arbitrary URL fetching, no following links off-allowlist.

**Why:** overnight runs are unsupervised. An LLM that decides to follow a
link to a random blog post adds unverified content to the raw store. The
allowlist is a "sources I trust enough to ingest without review" list.

**Default allowlist:**

- arxiv.org + ar5iv.labs.arxiv.org
- stacks.math.columbia.edu
- en.wikipedia.org (math articles only)
- nlab mirror (local)
- GitHub API (for repos in `08-code-docs/`)
- The user's own CLAUDE.md-registered repos

**Not on allowlist:**

- Medium, Substack, random blogs
- Forum posts (Reddit, HN, etc.)
- Any URL the LLM "thinks might be useful"

## Bound 4 — 4-hour wall clock

**Rule:** from start, the skill has 4 hours total. When exceeded, it:

1. Cleanly stops the current task (doesn't leave half-written files)
2. Checkpoints state (what was done, what was interrupted)
3. Writes a partial morning report
4. Exits

**Why:** unbounded overnight runs are how costs spiral and how subtle
bugs compound. A 4-hour cap is long enough to do meaningful work (dozens
of gap-fill synthesis, a full ontology refresh) but short enough that a
runaway loop gets caught by morning.

Enforcement via `scripts/sleep_watchdog.sh` — a sidecar process that
checks elapsed wall-clock every minute and sends SIGTERM on expiry.

## Bound 5 — Writable paths

**Rule:** hyper-sleep only writes under these paths:

- `01-raw/` — ingested source material
- `01-sorted/` — symlinks from sorting step
- `02-KB-main/` — subject to confidence ceiling (Bound 2)
- `04-Agent-Memory/` — logs and compaction output
- `06-Recursive/` — the morning report
- `05-insights-HITL/pending-curation/` — staged high-confidence pages
- `state/` — heartbeat, cache, checkpoint state

**Not writable during sleep:**

- `config/` — humans change config, not agents
- `03-Ontology-main/` (except via `/reasoning-ontology` which has its own
  bounds)
- `.claude/` — skill / agent / hook definitions
- `secrets/` — obvious
- `server/`, `scripts/`, `hooks/` — infrastructure

Enforcement: path-prefix check before every write. Violations halt the
skill with a failure entry in logs.

## Bound 6 — No concurrent runs

**Rule:** check `state/hyper-sleep.pid` before starting. If present and
the PID is alive, halt with a warning. Otherwise write the PID file and
clear it on exit (including abnormal exit via trap).

**Why:** two hyper-sleep runs concurrently can race on Qdrant upserts and
on log file appends. Easier to enforce single-instance than to make the
work idempotent under concurrency.

## Bound 7 — LLM call budget

**Rule:** budget a max of 500 LLM calls (across all models) per sleep run.
Tracked via hook logs.

**Why:** costs aside, 500 calls is enough for ~50 gap-fill syntheses + a
few ontology refreshes. Exceeding that means the skill has escalated
beyond its intended role. Stop and alert in the morning report.

## What violations do

When any bound trips, the skill:

1. Halts immediately (no partial completion of the offending task)
2. Writes a failure entry to `04-Agent-Memory/logs.md` with fingerprint
3. Adds a section "Bound violations" to the morning report
4. Exits non-zero so cron / launchd captures the failure

The user reviews bound violations in the morning. Persistent violations
are a signal to tighten the skill's scope, not to loosen the bounds.
