---
version: 1
auto_index_context7: true
auto_index_auggie: true
auto_rag_inject: true
repos: []
---

# main-codebase-tools Configuration

Register your main codebases here. These get auto-indexed by Context7 + Augment Code and their docs are injected via auto-rag hooks.

## Registered Codebases

Add your repos below. Each entry triggers:
1. Context7 indexing (upstream library name for code doc lookup)
2. Auggie semantic understanding (full repo analysis)
3. Auto-RAG hook registration (context injected when working in these repos)

| Repo Path | Upstream Library | Context7 Name | Status | Last Indexed |
|-----------|-----------------|---------------|--------|--------------|
| ~/Desktop/HyperFrequency | — | — | active | — |
| ~/Desktop/HyperFrequency/alpha-factory | alpha-factory | — | active | — |
| ~/Desktop/auto-brain | — | — | active | — |

## How It Works

When you add a repo to this table:
1. The `neuro-scan` skill detects new entries and creates indexing tasks
2. Context7 is queried with the `Upstream Library` name for API docs
3. Auggie performs a full semantic scan of the repo
4. Results are stored in `08-code-docs/my-repos/` as deepwiki-style documentation
5. The auto-rag hook includes relevant code docs when you're working in these repos

## Adding a New Repo

Add a row to the table above with:
- **Repo Path**: absolute path to the local clone
- **Upstream Library**: the library name to query Context7 with (leave blank for custom repos)
- **Context7 Name**: override if the Context7 library name differs from upstream
- **Status**: `pending` (will be indexed on next scan) or `active` (already indexed)
