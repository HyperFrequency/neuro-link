---
name: main-codebase-tools
description: Register the user's main codebases for context-aware retrieval and auto-RAG injection. Use this whenever the user says /main-codebase-tools, /main-codebase-tools add <repo>, asks to register a repo for context injection, or adds a new project they'll be working on regularly. Also trigger when the user says "track this repo", "add this to auto-rag", "index my codebase for Claude", or mentions a new project they expect Claude to know about. Performs full indexing via Context7 (API surface / doc snippets) and Augment Code / Auggie (semantic understanding), registers the repo's primary topics with the /auto-rag keyword router, subscribes fork repos to doc-sync-embed-verify for upstream change tracking, and generates a short CLAUDE.md addendum with architecture and gotchas. Each registered repo gets one spec file in this skill's directory with frontmatter pinning the indexed version.
---

# /main-codebase-tools

Register the user's actively-worked repos for context injection. The goal: when the user asks Claude a question about a project they own, the right context lands in the prompt automatically via the `/auto-rag` UserPromptSubmit hook.

## When to register

The user's own repos qualify. Third-party tools used often go to `/adjacent-tools-code-docs`. Forks go to `/forked-repos-with-changes`. The distinction matters because:

- User's own repos → indexed deeply (Context7 + Auggie), tightly wired to auto-RAG keyword triggers
- Third-party → public-doc-only indexing, looser RAG integration
- Forks → diff tracking between upstream and fork, specialized doc generation

## Registration flow

### Step 1 — Collect repo metadata

Ask the user (or read from their argument):

- Repo URL (`https://github.com/<owner>/<repo>`)
- Local clone path
- Primary language(s)
- Top 3–5 primary topics (for auto-RAG keyword routing)
- Whether it's a fork (if yes, bail and dispatch to `/forked-repos-with-changes`)

### Step 2 — Context7 indexing

Call Context7 via its MCP:

```
mcp__context7__resolve-library-id(<repo>)
mcp__context7__query-docs(<resolved-id>, "full index")
```

Context7 holds a pre-computed doc/code snippet index. If the repo isn't in Context7 yet, the skill queues a task to request indexing rather than failing — Context7 picks up new repos periodically.

### Step 3 — Augment Code / Auggie indexing

Invoke `auggie` locally on the repo path:

```bash
auggie index --path <local_path> --project <repo-slug>
```

Auggie produces a semantic embedding layer over the full source tree — complements Context7's doc-focused index with code-focused semantics. This takes 2–20 minutes depending on repo size; run in background, report completion.

### Step 4 — Register auto-RAG keywords

Edit the auto-RAG keyword router config at `config/auto-rag-routes.yml`:

```yaml
routes:
  - repo: <repo-slug>
    keywords: [keyword1, keyword2, keyword3]  # from Step 1 topics
    index_sources:
      - context7: <resolved-id>
      - auggie: <project>
    weight: 1.0       # higher means more aggressive context injection
```

See `references/auto-rag-integration.md` for the full route schema.

### Step 5 — Subscribe to doc-sync-embed-verify (if fork)

Already handled by `/forked-repos-with-changes` dispatch. Not applicable for non-forks.

### Step 6 — Generate CLAUDE.md addendum

Produce a short addendum at `<local_path>/CLAUDE.md` (append, don't overwrite):

- One-paragraph architecture summary (LLM-generated from README + top-level file structure)
- 3–5 gotchas (LLM-generated from issues labeled `gotcha` or the last 20 closed issues)
- Link back to the neuro-link spec file so the two stay in sync

If the repo already has a CLAUDE.md, diff the proposed additions and ask for approval before appending.

### Step 7 — Write the spec file

Create `.claude/skills/main-codebase-tools/<repo-slug>.md` with frontmatter:

```yaml
---
repo_url: https://github.com/<owner>/<repo>
local_path: /absolute/path
language: [rust, python]
primary_topics: [...]
index_context7: true
context7_id: <resolved-id>
index_auggie: true
auggie_project: <project>
auto_rag_trigger_keywords: [...]
weight: 1.0
last_indexed: YYYY-MM-DD
registered: YYYY-MM-DD
---
```

## Listing registered repos

`/main-codebase-tools list` reads all `<repo-slug>.md` spec files in this directory and prints a table: repo, last_indexed, weight, keyword count.

## Re-indexing

`/main-codebase-tools reindex <slug>` runs Steps 2, 3, 6 again for one repo. Bumps `last_indexed`.

`/neuro-scan` auto-flags any repo where `last_indexed > 7 days`. `/hyper-sleep` picks up the flagged ones and runs re-index overnight.

## Removing

`/main-codebase-tools remove <slug>`:

1. Delete the auto-RAG route entry
2. Keep the spec file for reference; move to `archive/` subdirectory
3. Don't touch Context7 / Auggie indexes (those are shared)

## References

- `references/context7-indexing.md` — when Context7 helps vs doesn't, how to query
- `references/auggie-indexing.md` — Auggie setup + known gotchas
- `references/auto-rag-integration.md` — full route schema + weight tuning

## Scripts

- `scripts/register_repo.sh` — all-in-one wrapper that runs steps 1–7
- `scripts/reindex.sh` — re-run indexing for a single repo
