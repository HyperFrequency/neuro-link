# .claude — Project-level Claude Code config

Project-scoped Claude Code configuration for the neuro-link-recursive vault.

## Structure

- `agents/` — custom subagent definitions (one `.md` per agent)
  - `neuro.md` — the `@neuro` orchestrator subagent
- `skills/` — project-local skill overrides (user-level skills live in
  `~/.claude/skills/`)
- `hooks/` — project-local hook scripts referenced from `settings.json`
- `settings.json` — Claude Code settings (permissions, hooks, env vars)
- `settings.local.json` — local-only settings, gitignored

## Skills listed for this vault

Generated via `/skill-creator` from specs in `00-neuro-link/`:

- `/neuro-link-setup` — bootstrap
- `/neuro-link` — main orchestrator
- `/recursive-self-improvement` — improvement loop
- `/neuro-scan` — brain scanner
- `/neuro-surgery` — HITL repair
- `/hyper-sleep` — non-HITL maintenance
- `/crawl-ingest-update` — deep-ingest pipeline
- `/main-codebase-tools` — user's main repos
- `/adjacent-tools-code-docs` — third-party toolbox
- `/forked-repos-with-changes` — fork tracking

## Hooks

- `UserPromptSubmit`: `hooks/auto-rag-inject.sh` — qmd / docs-dual-lookup router
- `PostToolUse`: `hooks/neuro-grade.sh` — append to `04-Agent-Memory/logs.md`

## Subagents

- `@neuro` — orchestrator with restricted tool set and vault-specific
  system prompt. See `agents/neuro.md`.

## Regenerating skills from specs

```bash
for spec in 00-neuro-link/*.md; do
  [ "$(basename "$spec")" = "README.md" ] && continue
  claude /skill-creator generate "$spec"
done
```
