---
name: forked-repos-with-changes
description: Track repos the user has forked under the HyperFrequency org or personal account, maintaining semantic understanding of the functional diff between upstream and fork, and generating enriched docs for fork-specific functionality. Use this whenever the user says /forked-repos-with-changes, asks to track a fork, add a fork, review functional differences between upstream and fork, or hands off a forked repo expecting it documented. Also trigger when the user says "track this fork", "what did we change upstream", "generate fork docs", or mentions a fork repository in the context of adding it to the brain. Outputs to 08-code-docs/forked-up/<repo>/ and, for HyperFrequency org forks, feeds the shared Devin DeepWiki via doc-sync-embed-verify on gh push. Uses LLM-based semantic diff rather than git diff to produce readable functional differences, and pins the fork_point + last_merge_base in frontmatter so resync catches upstream changes that haven't been rebased.
---

# /forked-repos-with-changes

Fork tracking with two-tier documentation: the upstream goes through `/deep-tool-wiki` or `/adjacent-tools-code-docs`, and this skill generates a *supplementary* layer that only covers the fork's deltas.

## Why a separate skill

Forks have special needs. A plain `git diff` between fork and upstream is noisy — renames, import-order changes, and formatting differences dominate. What the user actually wants to know: "what functional differences exist between our fork and the upstream, and what new capabilities does the fork add?"

That's a semantic diff. You compute it by summarizing the changed-file set with an LLM, not by diff-parsing.

## Flow per fork

### Step 1 — Collect metadata

- Fork URL (must be under an org/user the user controls)
- Upstream URL
- Fork theme — one sentence on *why* this fork exists (e.g., "Hyperliquid perps + funding-rate awareness")
- Is this a HyperFrequency org fork? (affects DeepWiki integration)

If the user claims something is a fork but it's actually not (no common ancestor with a declared upstream), bail with an error.

### Step 2 — Compute merge base and fork point

```bash
cd <fork_clone>
git remote add upstream <upstream_url>
git fetch upstream
FORK_POINT=$(git merge-base HEAD upstream/main)
LAST_MERGE_BASE=$(git merge-base HEAD upstream/main)  # alias at first; drifts on upstream merges
```

Store both. `fork_point` is the historical divergence; `last_merge_base` moves forward each time the fork merges from upstream.

### Step 3 — Enumerate changed files

```bash
git diff --name-only $LAST_MERGE_BASE HEAD > changed-files.txt
```

Categorize via `scripts/categorize_changes.py`:

- `new` — files added in the fork
- `modified` — files changed vs upstream
- `deleted` — files removed in the fork
- Ignore: vendored deps, generated code, lockfiles, formatting-only diffs (detected via `diff --stat` threshold)

### Step 4 — LLM-based semantic diff

For each modified file, spawn a subagent with:

```
You are diffing a fork against its upstream. Read both versions.
Upstream: <upstream content>
Fork:     <fork content>

Summarize in 3 sentences what the fork changes *functionally* — not formatting, not imports, not renaming. Focus on:
- What new capability is added?
- What existing behavior is changed?
- What surface is removed?

If the diff is purely cosmetic, respond with "COSMETIC" and nothing else.
```

Collect summaries. Drop all COSMETIC files. Aggregate remaining summaries into per-feature buckets via a second LLM pass.

See `references/semantic-diff.md` for the full prompt + example outputs.

### Step 5 — Generate supplementary wiki

Create `08-code-docs/forked-up/<repo>/` with:

```
forked-up/<repo>/
├── README.md            # overview: fork_theme + top 5 changes
├── functional-diff.md   # per-feature deltas from Step 4
├── code-examples.md     # fork-specific usage patterns (LLM-generated from Step 4 + test files)
├── upstream-link.md     # pointer to /deep-tool-wiki or /adjacent-tools-code-docs page
└── new-files.md         # summary of files that are entirely new to the fork
```

All 5 have frontmatter:

```yaml
---
fork_url: https://github.com/HyperFrequency/<repo>
upstream_url: https://github.com/<upstream-org>/<repo>
fork_point: <sha>
last_merge_base: <sha>
changed_files_count: <N>
fork_theme: "..."
last_diffed: YYYY-MM-DD
confidence: 0.8
---
```

### Step 6 — DeepWiki integration (HyperFrequency forks only)

If the fork is under `HyperFrequency/`, invoke `/doc-sync-embed-verify` to push the new wiki to the shared Devin DeepWiki repo. This is what makes the fork's docs discoverable across the org, not just this user's vault.

For non-HyperFrequency forks (personal forks), skip Step 6 — docs live locally only.

### Step 7 — Register for sync

Append to `config/fork-watch.yml`:

```yaml
forks:
  - slug: <repo>
    fork_url: ...
    upstream_url: ...
    last_merge_base: <sha>
    watch_cadence: weekly
    deepwiki_target: <if HyperFrequency>
```

## Sync on change

Two trigger types:

- **Fork commits** — detected by a git hook on the fork's local clone. Re-run Steps 3–6 for changed files only.
- **Upstream merges** — detected weekly via `git fetch upstream`. If the fork hasn't pulled upstream yet, flag in `05-insights-HITL/` as "upstream has N new commits; consider merging". If the fork has pulled but `last_merge_base` is stale in frontmatter, re-run Steps 2–6.

## Listing forks

`/forked-repos-with-changes list` → table of registered forks with delta counts.

## Unforking

If the user deletes a fork or stops contributing to it:

1. Move `08-code-docs/forked-up/<repo>/` to `archive/`
2. Remove from `config/fork-watch.yml`
3. Don't delete DeepWiki entries — they stay for historical reference

## References

- `references/semantic-diff.md` — LLM-based diff prompts + example outputs
- `references/deepwiki-integration.md` — when and how to push HyperFrequency fork docs to DeepWiki
- `references/auto-rag-integration.md` — inherit from adjacent-tools-code-docs

## Scripts

- `scripts/compute_fork_diff.py` — end-to-end Step 3–4 runner
- `scripts/categorize_changes.py` — new/modified/deleted/cosmetic classifier
- `scripts/init_fork.sh` — new fork registration end-to-end
