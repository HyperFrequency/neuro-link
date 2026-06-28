---
title: File Structure Migration Plan
status: draft
created: 2026-04-18
---

# Vault File Structure Migration

## Old → New Path Mapping

| Old path                              | New path                          | Action   |
| ------------------------------------- | --------------------------------- | -------- |
| `00-raw/`                             | `01-raw/`                         | rename   |
| `01-sorted/`                          | `01-sorted/`                      | keep     |
| `02-KB-main/`                         | `02-KB-main/`                     | keep (content curation TBD) |
| `02-KB-main/math/*.md` (stubs)        | `.archive/2026-04-18/math-stubs/` | quarantine (shallow clippings) |
| `03-ontology-main/`                   | `03-Ontology-main/`               | rename (case) |
| `04-KB-agents-workflows/`             | `04-Agent-Memory/`                | rename + restructure (see below) |
| `05-insights-gaps/`                   | `05-insights-HITL/`               | merge into one |
| `05-self-improvement-HITL/`           | `07-self-improvement-HITL/`       | rename |
| `06-progress-reports/`                | *removed*                         | merged into `06-Recursive/` |
| `06-self-improvement-recursive/`      | `06-Recursive/`                   | rename |
| `07-neuro-link-task/`                 | `00-neuro-link/tasks/`            | move (job queue is a subdir of the new root) |
| `08-code-docs/`                       | `08-code-docs/`                   | restructure (see below) |
| `09-business-docs/`                   | *removed from vault root*         | archive or move out of vault |
| *(new)*                               | `00-neuro-link/`                  | NEW — default LLM instruction docs |

## New `00-neuro-link/` layout

```
00-neuro-link/
├── README.md                            ← index of the 10 default .md files
├── neuro-link-setup.md                  ← spawns /neuro-link-setup skill
├── neuro-link.md                        ← spawns /neuro-link skill (orchestrator)
├── recursive-self-improvement.md        ← spawns /recursive-self-improvement skill
├── neuro-scan.md                        ← spawns /neuro-scan skill
├── neuro-surgery.md                     ← spawns /neuro-surgery skill
├── hyper-sleep.md                       ← spawns /hyper-sleep skill
├── crawl-ingest-update.md               ← spawns /crawl-ingest-update skill
├── main-codebase-tools.md               ← spawns /main-codebase-tools skill
├── adjacent-tools-code-docs.md          ← spawns /adjacent-tools-code-docs skill
├── forked-repos-with-changes.md         ← spawns /forked-repos-with-changes skill
└── tasks/                               ← job queue (was 07-neuro-link-task)
```

## New `03-Ontology-main/` layout

```
03-Ontology-main/
├── workflow/
│   ├── SOT/                             ← source-of-truth canonical files
│   ├── state-definitions.md
│   ├── phase-gating.md
│   └── goal-hierarchical.md
└── agents/
    ├── by-agent/
    ├── by-workflow-state/
    └── by-auto-HITL/
```

## New `04-Agent-Memory/` layout

```
04-Agent-Memory/
├── logs.md                              ← append-only event log
├── consolidated.md                      ← rolling summary
├── consolidated/
│   ├── agent/
│   └── workflow/
└── perf-grade.md                        ← grading agent output
```

## New `05-insights-HITL/` layout

```
05-insights-HITL/
├── daily.md
├── weekly.md
└── all-time.md
```

## New `06-Recursive/` layout

```
06-Recursive/
├── daily.md
├── weekly.md
└── all-time.md
```

## New `07-self-improvement-HITL/` layout

```
07-self-improvement-HITL/
├── overview.md
├── models/
├── hyperparameters/
├── prompts/
├── features/
└── code-changes/
```

## New `08-code-docs/` layout

```
08-code-docs/
├── my-repos/                            ← user's own repos
├── toolbox/                             ← third-party tools used often
└── forked-up/                           ← forked repos with diff tracking
```

## Symlink impact

`/Users/DanBot/neuro-link/` has symlinks into every `NN-*` folder in the old
structure. Every rename breaks a symlink. Two options:

1. **Update symlinks post-rename** — script replays the new map.
2. **Replace `/Users/DanBot/neuro-link/` entirely** with a fresh set of symlinks
   after the migration completes.

Decision: **option 2**, atomic swap, less partial-state risk. Script lives in
`scripts/migrate/rebuild_symlinks.sh` (to be written in Phase 2).

## Content quarantine

`.archive/2026-04-18/` at vault root holds everything being thrown out:

- `math-stubs/` — all existing `02-KB-main/math/*.md` files (user rejected
  as "shallow clippings")
- `old-embeddings/` — Qdrant snapshot of all collections before re-index
- `old-skills/` — tarball of deleted skills from `~/.claude/skills/`
- `old-rag-tools/` — copy of `server/src/tools/rag_*.rs` before rewrite

No `git rm`; everything moves to `.archive/` for rollback. The `.archive/`
directory is gitignored.

## Execution script (Phase 2, HITL-gated)

```bash
#!/usr/bin/env bash
# scripts/migrate/vault_rename.sh
# ONE-SHOT: not idempotent. Runs only after HITL confirmation.
set -euo pipefail
VAULT="/Users/DanBot/Desktop/HyperFrequency/neuro-link"
cd "$VAULT"

mkdir -p .archive/2026-04-18/{math-stubs,old-skills,old-rag-tools,old-embeddings}

# Quarantine shallow math stubs
mv 02-KB-main/math/*.md .archive/2026-04-18/math-stubs/ 2>/dev/null || true

# Rename top-level folders
git mv 00-raw 01-raw
git mv 03-ontology-main 03-Ontology-main
git mv 04-KB-agents-workflows 04-Agent-Memory
git mv 06-self-improvement-recursive 06-Recursive
git mv 05-self-improvement-HITL 07-self-improvement-HITL
git mv 07-neuro-link-task 00-neuro-link/tasks

# Merge insights
mkdir -p 05-insights-HITL
git mv 05-insights-gaps/* 05-insights-HITL/ 2>/dev/null || true
rmdir 05-insights-gaps

# Remove 06-progress-reports after manual review
# (NOT automated — user inspects first)

# Restructure 08-code-docs
mkdir -p 08-code-docs/{my-repos,toolbox,forked-up}

# New default docs dir (files populated in Phase 2)
mkdir -p 00-neuro-link

echo "Migration complete. Review 'git status' before committing."
```

Script lives in the repo but is never auto-run. User triggers manually.
