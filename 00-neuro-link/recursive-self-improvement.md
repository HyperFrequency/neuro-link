---
skill_name: recursive-self-improvement
trigger: /recursive-self-improvement, scheduled via /schedule
config:
  scheduled: daily
  completed: append-only log
  performance: autoresearch stats
  require_consortium: Y
  HITL_override: manual    # off | manual | agent
  harness_cli: claude | forgecode | claw-code
  full_session_logs: true
---

# /recursive-self-improvement

Automated self-improvement loop for user / agent / tool interactions.

## Inputs

- `04-Agent-Memory/logs.md` — append-only event stream
- `04-Agent-Memory/perf-grade.md` — grading agent output
- `06-Recursive/daily.md` — yesterday's report (delta source)
- `state/llm_logs/` — raw token-bucketed LLM interactions

## Process

1. **Consortium**: spawn 3+ grading agents (default Y). Each reads the period's
   logs independently and produces an opinion on:
   - `orchestrator-agent-grading.md`
   - `each-agent-grade.md`
   - `learning-grade.md`
2. **Majority vote** on proposed adjustments. Ties → HITL.
3. **HITL gate** (default `manual`): surface the proposal in
   `05-insights-HITL/daily.md`; wait for thumb up/down.
4. **Execute approved adjustments**:
   - Agent topology changes → `03-Ontology-main/agents/by-agent/`
   - Ontology edits → `03-Ontology-main/workflow/`
   - Skill edits → `.claude/skills/` via `/skill-creator`
   - Prompt changes → `07-self-improvement-HITL/prompts/`
5. **Change-log** append to `06-Recursive/daily.md`.

## Outputs

- `06-Recursive/{daily,weekly,all-time}.md` — cumulative report stream
- `04-Agent-Memory/perf-grade.md` — rolling performance grade

## HITL override semantics

- `off` — changes applied without user intervention (dangerous; dev-only)
- `manual` — user must approve each proposal (default)
- `agent` — a designated reviewer agent auto-approves within safety bounds
