---
skill_name: adjacent-tools-code-docs
trigger: /adjacent-tools-code-docs
invokes: [doc-sync-embed-verify, deep-tool-wiki, docs-dual-lookup]
output_path: 08-code-docs/toolbox/
---

# /adjacent-tools-code-docs

Maintain a "toolbox wiki" for third-party repos you or your agents use all the
time but don't own (e.g., `vectorbt.pro`, `nautilus_trader`, `qmd`, `turbovault`,
`optuna`, `ray`, `pufferlib`).

## Scope

For each adjacent tool:

1. Clone/track upstream (read-only).
2. Generate a comprehensive wiki page per 1500+ line chunk, following the
   Karpathy LLM-Wiki pattern (YAML frontmatter, citations, open questions,
   conceptual model, details, contradictions).
3. Output to `08-code-docs/toolbox/<tool-name>/`.
4. Keep synced with upstream via `doc-sync-embed-verify` — on upstream releases,
   re-ingest changed docs and regenerate the affected wiki sections.

## Wiki page schema

```yaml
---
tool: qmd
upstream: https://github.com/tobi/qmd
version_indexed: 0.4.2
last_synced: 2026-04-18
confidence: 0.9
primary_sources: [README, src/llm.ts, CHANGELOG.md]
---
```

## Diff against deep-tool-wiki

`/deep-tool-wiki` is for **HyperFrequency forks** (queries Devin DeepWiki of
the user's own forks). This skill is for **unforked third-party tools** —
content goes to local `08-code-docs/toolbox/`, not the shared deepwiki.

## Maintenance cadence

- Upstream release detected → re-ingest affected sections
- `/neuro-scan` flags tools >14 days without a sync check
- `/hyper-sleep` performs the syncs
