# 00-neuro-link — Default LLM Instruction Docs

This directory holds the canonical `.md` specs that define the neuro-link skill
surface. Each file here is the PRD for one skill. Running `/skill-creator`
against a file in this directory produces the corresponding executable skill
under `.claude/skills/`.

## Files

| Spec file                         | Skill generated                 | Purpose                                                  |
| --------------------------------- | ------------------------------- | -------------------------------------------------------- |
| `neuro-link-setup.md`             | `/neuro-link-setup`             | First-run bootstrap: prereqs, secrets, MCP, hooks        |
| `neuro-link.md`                   | `/neuro-link`                   | Main orchestrator; uses TurboVault, invokes other skills |
| `recursive-self-improvement.md`   | `/recursive-self-improvement`   | Automated self-improvement loop (HITL-gated)             |
| `neuro-scan.md`                   | `/neuro-scan`                   | Brain scanner — jobs, stale wikis, gaps, failures        |
| `neuro-surgery.md`                | `/neuro-surgery`                | HITL repair: scan-report driven fixes, re-synthesis      |
| `hyper-sleep.md`                  | `/hyper-sleep`                  | Non-HITL brain maintenance, ontology updates             |
| `crawl-ingest-update.md`          | `/crawl-ingest-update`          | Source crawl + deep ingest pipeline                      |
| `main-codebase-tools.md`          | `/main-codebase-tools`          | Index user's main repos via Context7 + Auggie            |
| `adjacent-tools-code-docs.md`     | `/adjacent-tools-code-docs`     | Tools-wiki for adjacent (not-owned) repos                |
| `forked-repos-with-changes.md`    | `/forked-repos-with-changes`    | Diff-aware wiki for forked repos                         |

## Task queue

`tasks/` — markdown job specs drained by `/job-scanner`. Each file has YAML
frontmatter defining `type`, `priority`, `dependencies`, `status`.

## Regenerating skills

After editing any spec, regenerate its skill:

```bash
claude /skill-creator generate 00-neuro-link/<spec>.md
```
