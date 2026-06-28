---
skill_name: forked-repos-with-changes
trigger: /forked-repos-with-changes
invokes: [deep-tool-wiki, doc-sync-embed-verify]
output_path: 08-code-docs/forked-up/
---

# /forked-repos-with-changes

Track repos the user has forked under the HyperFrequency org (or personal).
Maintains semantic understanding of the diff between upstream and fork, and
generates enriched docs for the forked-specific functionality.

## Scope

For each forked repo:

1. Pull upstream + fork.
2. Compute semantic diff — not just `git diff`, but a descriptive summary of
   functional differences (via LLM on the changed file set).
3. Generate supplementary wiki in `08-code-docs/forked-up/<repo-name>/`:
   - `README.md` — overview of what the fork changes
   - `functional-diff.md` — per-feature breakdown
   - `code-examples.md` — fork-specific usage patterns
   - `upstream-link.md` — pointer to the deep-tool-wiki page for the upstream
4. Keep synced — on fork commits or upstream merges, update the diff docs.

## Integration with `/deep-tool-wiki`

For HyperFrequency forks specifically, this skill's output feeds into the
shared `deep-tool-wiki` Devin DeepWiki (via `doc-sync-embed-verify` on
`gh push` to the fork).

## Tracked fork spec

```yaml
---
fork_url: https://github.com/HyperFrequency/nautilus_trader
upstream_url: https://github.com/nautechsystems/nautilus_trader
fork_point: abc123
last_merge_base: def456
changed_files_count: 47
fork_theme: "Hyperliquid perps + funding-rate awareness"
---
```

## Out of scope

Repos you use but haven't forked — those go to `/adjacent-tools-code-docs`.
