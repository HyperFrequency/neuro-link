---
skill_name: main-codebase-tools
trigger: /main-codebase-tools, /main-codebase-tools add <repo>
invokes: [auto-rag, docs-dual-lookup]
mcp: [context7, augment-code]
---

# /main-codebase-tools

Register the user's main codebases for context-aware retrieval. Files in this
directory are pointers (repo URL + local path + primary language) to repos
actively worked on.

## Registration flow

For each repo added:

1. **Index via Context7** — full doc indexing (API surface, code examples).
2. **Index via Augment Code / Auggie** — semantic understanding of the codebase.
3. **Register with `/auto-rag`** — add the repo's primary topics to the keyword
   router so relevant sections inject when a prompt mentions them.
4. **Subscribe to doc updates** — if the repo is a fork or third-party with an
   upstream, subscribe to `doc-sync-embed-verify` to re-ingest on upstream
   changes.
5. **Generate CLAUDE.md addendum** — a short summary of the repo's architecture
   and gotchas, auto-linked from the repo's CLAUDE.md.

## Per-repo spec

Each registered repo gets one file in this dir with frontmatter:

```yaml
---
repo_url: https://github.com/user/repo
local_path: /Users/DanBot/src/repo
language: rust
primary_topics: [trading, nautilus, backtesting]
index_context7: true
index_auggie: true
auto_rag_trigger_keywords: [nautilus, backtest, strategy]
last_indexed: 2026-04-18
---
```

## Maintenance

- `/neuro-scan` flags any repo whose `last_indexed` is >7 days old.
- `/hyper-sleep` re-indexes flagged repos overnight.
- `doc-sync-embed-verify` fires on upstream PR merges for forks.
