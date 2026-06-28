---
version: 1
enabled: false
bridge_mode: mcp2cli
harnesses:
  claude-code:
    type: local
    status: active
    role: primary
    skills_dir: ~/.claude/skills
    hooks_dir: ~/.claude/hooks
    statusline_name: "@neuro-link"
  k-dense-byok:
    type: api
    status: disabled
    role: research
    url: ""
    api_key_env: KDENSE_API_KEY
    capabilities:
      - scientific_review
      - consortium_dispatch
      - experiment_design
      - literature_synthesis
  k-dense-web:
    type: api
    status: disabled
    role: research
    url: https://k-dense.ai
    api_key_env: KDENSE_WEB_API_KEY
    capabilities:
      - web_research
      - automl_design
      - agent_topology_design
  forgecode:
    type: local
    status: disabled
    role: implementation
    command: forge
    capabilities:
      - code_generation
      - refactoring
      - testing
  claw-code:
    type: local
    status: disabled
    role: implementation
    command: claw
    capabilities:
      - code_generation
      - system_ops
routing_rules:
  - pattern: "research|paper|literature|review"
    route_to: k-dense-byok
  - pattern: "code|implement|build|fix|test"
    route_to: claude-code
  - pattern: "experiment|automl|feature engineering"
    route_to: k-dense-byok
  - pattern: "consortium|expert panel"
    route_to: k-dense-byok
---

# harness-harness-comms Configuration

Defines how neuro-link-recursive bridges between different agent harnesses.

## Bridge Mode

- `mcp2cli` — Each harness exposes an MCP server; mcp2cli-rs converts to CLI commands
- `ngrok_api` — Harnesses communicate over HTTPS via Ngrok tunnels (Phase 3)
- `direct` — Local process communication (stdin/stdout)

## Harness Definitions

### Claude Code (Primary)
Local CLI harness. Always active. Runs skills, hooks, and direct tool calls.
The `statusline_name` appears in the Claude Code status bar when neuro-link is active.

### K-Dense BYOK (Research)
Localhost K-Dense instance for:
- Scientific review and consortium dispatch
- Experiment design (e.g., AutoML pipelines)
- Literature synthesis with expert agent panels

**Setup**: Install K-Dense BYOK locally, set `url` to localhost endpoint, add API key to secrets/.env.

### K-Dense Web (Research)
Cloud K-Dense for web-accessible research tasks.

### ForgeCode (Implementation)
CLI code generation harness. Delegates implementation tasks.

### Claw-Code (Implementation)
Alternative CLI harness for code generation and system operations.

## Routing Rules

When a task matches a `pattern`, the `harness-bridge` skill suggests routing to the specified harness. The user confirms before dispatch.

## Communication Protocol

Each harness-to-harness message follows this format:
```json
{
  "from": "claude-code",
  "to": "k-dense-byok",
  "type": "task_dispatch",
  "task": {
    "description": "Design an AutoML feature engineering pipeline for exchange order book data",
    "context_refs": ["04-KB-agents-workflows/automl.md", "02-KB-main/feature-engineering.md"],
    "output_target": "07-neuro-link-task/automl-pipeline-design.md",
    "priority": 2
  },
  "timestamp": "2026-04-15T10:00:00Z"
}
```

The receiving harness processes the task and writes output to `output_target`. The sending harness monitors completion via `neuro-scan`.
