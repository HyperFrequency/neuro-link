---
name: adjacent-tools-code-docs
description: Maintain a comprehensive toolbox wiki for third-party repos the user or their agents use often but do not own (vectorbt.pro, nautilus_trader, qmd, turbovault, optuna, ray, pufferlib, polars, etc). Use this whenever the user says /adjacent-tools-code-docs, asks to document a third-party tool, mentions a new dependency they will rely on regularly, or wants a local searchable wiki for an external library. Also trigger when the user says "add X to the toolbox", "document this library", "keep track of this tool", or the user asks questions about a library that isn't yet in 08-code-docs/toolbox/. Follows the Karpathy LLM-Wiki pattern with YAML frontmatter, citations, open questions, and contradictions. Keeps synced with upstream via doc-sync-embed-verify on releases. Outputs to 08-code-docs/toolbox/<tool-name>/.
---

# /adjacent-tools-code-docs

Build and maintain a local, up-to-date wiki for third-party tools the user relies on. The wiki lives under `08-code-docs/toolbox/<tool>/` and is indexed by `/auto-rag` so that when the user asks a library-specific question, the toolbox wiki pages land in the prompt alongside any Context7 / Auggie results.

## When this skill vs `/deep-tool-wiki`

- `/deep-tool-wiki` → indexed by Devin DeepWiki for HyperFrequency forks specifically, living in a separate shared repo
- **This skill** → everything else — unforked third-party tools, docs live locally under `08-code-docs/toolbox/`

The duplication is intentional: HyperFrequency forks are shared across machines via the DeepWiki, while toolbox wikis stay local so the user can iterate on them without external sync overhead.

## Wiki page format — Karpathy LLM-Wiki pattern

Each tool gets a directory, not a single file. Break the tool's surface into chunks of roughly 1500 lines of logical content per wiki page. For a library like `qmd`, that usually means:

```
08-code-docs/toolbox/qmd/
├── README.md                 # top-level index
├── 01-architecture.md        # core concepts, design
├── 02-api-reference.md       # public API surface
├── 03-configuration.md       # env vars, config files
├── 04-integration.md         # how we integrate into neuro-link
└── 05-changelog-watch.md     # version history + breaking changes to track
```

Each `.md` has frontmatter:

```yaml
---
tool: qmd
page_slug: architecture
upstream: https://github.com/tobi/qmd
version_indexed: 0.4.2
last_synced: 2026-04-18
confidence: 0.9
primary_sources:
  - README
  - src/llm.ts
  - CHANGELOG.md
open_questions:
  - Does the reranker cache survive restart?
contradictions: []
---
```

And sections (enforced): Overview → Conceptual Model → Details → Contradictions → Open Questions → Sources.

See `references/karpathy-wiki-pattern.md` for the full template + examples.

## Registration flow

### Step 1 — Collect tool metadata

- Tool name (slug)
- Upstream URL
- Tool kind (library | CLI | service | plugin)
- Language(s)
- Primary use cases in the neuro-link context (1–3 sentences)

### Step 2 — Initial ingestion

Clone upstream read-only to `state/mirrors/toolbox/<slug>/` at a tagged version (HEAD if no tags). Don't fetch from Github on every query — local mirror, periodically refreshed.

### Step 3 — Generate pages

Use `/wiki-curate` with `source: local-mirror` to produce the 5+ wiki pages from the cloned source. Target 1500 lines per page logically; a large tool may need 10+ pages, a small one 3.

### Step 4 — Cross-link

Add `[[wikilinks]]` between pages. Add `[[../my-repos/<repo>]]` links where the tool is used in one of the user's own codebases (helps `/auto-rag` inject related-project context together).

### Step 5 — Register for sync

Append to `config/toolbox-watch.yml`:

```yaml
tools:
  - slug: qmd
    upstream: https://github.com/tobi/qmd
    last_synced: 2026-04-18
    watch_paths:
      - README.md
      - CHANGELOG.md
      - src/
    sync_cadence: weekly
```

### Step 6 — Auto-RAG integration

Same as `/main-codebase-tools` Step 4 but with lower default weight (0.6 vs 1.0) — third-party tool pages should inject alongside but yield to user's own code when both match.

## Sync cadence

`/neuro-scan` Pass 6 checks `last_synced` against upstream releases. Flagged tools get picked up by `/hyper-sleep` for re-sync. Re-sync logic:

1. Fetch new `upstream` HEAD (or latest release tag)
2. Diff `CHANGELOG.md` since `version_indexed`
3. Identify which wiki pages are affected (by matching changelog keywords against page `primary_sources`)
4. Regenerate affected pages via `/wiki-curate`
5. Bump `version_indexed` + `last_synced`

See `references/sync-patterns.md` for the detailed diff-to-affected-pages mapping.

## Listing and querying

`/adjacent-tools-code-docs list` → table of registered tools.

`/adjacent-tools-code-docs query <tool> <question>` → routes to `/auto-rag preview` scoped to `08-code-docs/toolbox/<tool>/`.

## When to graduate a tool

If the user starts contributing to a tool (PRs upstream), or forks it, this skill dispatches to `/forked-repos-with-changes` and archives the toolbox entry (moves to `archive/`, preserves history).

## References

- `references/karpathy-wiki-pattern.md` — full template + concrete examples
- `references/sync-patterns.md` — upstream-change-to-affected-pages mapping
- `references/auto-rag-integration.md` — inherit from main-codebase-tools; weight differs

## Scripts

- `scripts/sync_upstream.sh` — single-tool sync
- `scripts/init_tool.sh` — new tool registration end-to-end
- `scripts/diff_affected_pages.py` — compute which pages need regen from a CHANGELOG diff
