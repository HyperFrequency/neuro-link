---
title: TurboVault Fork + @neuro Chat + ngrok Exposure — Master Plan
status: draft
created: 2026-04-18
owner: Dan Repaci
---

# TurboVault Fork + @neuro Chat + ngrok — System Plan

Extends the earlier PRD (`00-PRD.md`) with:

1. **Fork TurboVault** under HyperFrequency. Add `subscribe_vault_events` MCP
   tool with glob filter + `FileRenamed` emit fix in the same file.
2. **Install full-feature TurboVault** (`cargo install --features full`) —
   STDIO + HTTP + WebSocket + TCP + Unix.
3. **Obsidian plugin chat panel** on the right — borrows UI patterns from
   `logancyang/obsidian-copilot` (license permitting).
4. **`@neuro` agent** — full vault manipulation via TurboVault + access to
   the 10 neuro-link skills.
5. **ngrok public exposure** of the whole MCP (vault + RAG) behind Caddy
   bearer-auth so the user can reach the system remotely.

Adversarial review (`/codex:adversarial-review --effort xhigh`) runs after
each phase. This is a hard gate — no progression to next phase until the
review lands and is resolved.

## System plumbing map

```
                     ┌────────────────────────────────────────┐
                     │ External client (user, another harness)│
                     └──────────────────┬─────────────────────┘
                                        │ HTTPS + Bearer
                                        ▼
                             ┌────────────────────┐
                             │      ngrok         │
                             │ (reserved domain)  │
                             └──────────┬─────────┘
                                        │
                                        ▼
                             ┌────────────────────┐
                             │  Caddy (auth gate) │
                             │  bearer → proxy    │
                             └──────────┬─────────┘
                                        │ 127.0.0.1
                                        ▼
    ┌──────────────────────────────────────────────────────────────┐
    │                    TurboVault (HF fork)                      │
    │  - 47 base tools + subscribe_vault_events (new)              │
    │  - WebSocket notifications for vault events                  │
    │  - HTTP MCP endpoints                                        │
    │  - `--features full`                                         │
    └───────────────────┬────────────────────┬─────────────────────┘
                        │                    │
             TurboVault MCP        File watch events (new)
                        │                    │
                        ▼                    ▼
      ┌───────────────────────────────────────────────┐
      │       Obsidian plugin (TypeScript)            │
      │  - Chat panel (right-side view)               │
      │  - @neuro agent (tool-using)                  │
      │  - Provider abstraction (OpenRouter / ...)    │
      │  - Subscription handler                       │
      │  - File-drop-to-task dispatcher               │
      └───────────────────────────────────────────────┘
                        │
                        │ reads
                        ▼
              .claude/skills/<name>/
              00-neuro-link/<specs>.md
              config/neuro-link.md
```

## Phased execution

Each phase ends with an adversarial review (`/codex:adversarial-review
--effort xhigh`). Do not start Phase N+1 until Phase N's review findings
are either addressed or explicitly deferred.

### Phase 4A — TurboVault fork + subscribe_vault_events

**Scope:**
- `gh repo fork Epistates/turbovault HyperFrequency/turbovault`
- Branch `feat/subscribe-vault-events`
- Patch `crates/turbovault-vault/src/watcher.rs`:
  - Emit `VaultEvent::FileRenamed` from `EventKind::Modify(ModifyKind::Name(_))`
- New module `crates/turbovault-tools/src/subscribe.rs`:
  - `subscribe_vault_events(filter: Option<EventFilter>) -> SubscriptionHandle`
  - `unsubscribe_vault_events(id: SubscriptionHandle) -> ()`
  - `EventFilter { globs: Vec<String>, kinds: Option<Vec<EventKind>> }`
  - Registry with per-subscription `VaultWatcher` + WebSocket notification forwarder
- Wire into `crates/turbovault/src/tools.rs` tool registry
- `Cargo.toml` dep: `globset = "0.4"` for glob matching
- Tests: subscribe → drop file → receive event; filter matches; unsubscribe releases watcher
- Version bump + CHANGELOG entry

**Deliverable:** green tests on the fork; a running `turbovault --features full` that exposes the new tool.

**Adversarial review focus:**
- Race conditions on subscription cancellation
- WebSocket backpressure / slow consumer
- Filter correctness (edge cases: `**`, negation, trailing slashes)
- Resource leak if client drops without unsubscribe

### Phase 4B — Install unification

**Scope:**
- `install.sh` switches TurboVault source from Epistates upstream to
  HyperFrequency fork.
- `cargo install --git https://github.com/HyperFrequency/turbovault --features full`
- Writes `config/Caddyfile.tmpl` and `config/ngrok.yml.tmpl` for Phase 8.
- Updated dry-run output matches new behavior.

**Deliverable:** `./install.sh --dry-run` lists the fork build + full
features; `./install.sh` on a clean machine builds the fork and installs
the binary.

**Adversarial review focus:**
- Backward compat: existing users on upstream get a clean migration path
- Failure modes: fork offline, feature build breaks, stale `~/.cargo` bin

### Phase 4C — Obsidian plugin MCP subscription + file-drop dispatcher

**Scope:**
- `obsidian-plugin/src/mcp-subscription.ts` — WebSocket client that calls
  `subscribe_vault_events` on startup with glob `00-neuro-link/*.md`
- `obsidian-plugin/src/dispatcher/new-spec.ts` — on `FileCreated` under
  `00-neuro-link/`, reads frontmatter, invokes LLM, writes task spec to
  `00-neuro-link/tasks/<slug>.md`
- Uses provider abstraction (Phase 5)
- Subscribes at plugin load, unsubscribes on unload

**Deliverable:** dropping a file into `00-neuro-link/` fires the plugin,
writes a task spec within ~2 seconds.

**Adversarial review focus:**
- Plugin reload races with active subscription
- LLM call timeouts / retries
- Task spec collisions (two files same slug)

### Phase 5 — Provider abstraction

**Scope:**
- `obsidian-plugin/src/providers/` module:
  - `base.ts` — `LLMProvider` interface (chat, stream, tool use)
  - `openrouter.ts` — default provider
  - `anthropic.ts` — direct API
  - `openai.ts` — direct API
  - `local-llama.ts` — for qmd/llama-server localhost
- Settings UI: key input, model picker, stream toggle, timeout
- Used by chat panel, new-spec dispatcher, and @neuro agent

**Deliverable:** user pastes OpenRouter key in settings; `@neuro` chat
successfully calls any supported provider.

**Adversarial review focus:**
- API key storage (Obsidian encrypted storage vs plain settings.json)
- Timeout handling
- Provider-specific quirks (OpenAI tools vs Anthropic tool_use)

### Phase 6 — Chat panel UI (right-side view)

**Scope:**
- Borrow UI patterns from `logancyang/obsidian-copilot` (MIT or
  AGPL? — Phase 4A research agent confirms, sets copy strategy).
- `obsidian-plugin/src/views/chat-view.ts` — right-side ItemView
- Markdown rendering, streaming, citation rendering, file-drop attachment
- Keyboard shortcut `cmd+shift+k` to toggle

**Deliverable:** `@neuro` chat window appears on the right; streams LLM
responses; supports file mentions.

**Adversarial review focus:**
- UI jank on long streams
- Accessibility / keyboard traps
- Memory leaks on repeated open/close
- License + attribution correctness

### Phase 7 — @neuro agent with full tool access

**Scope:**
- `obsidian-plugin/src/agent/neuro-agent.ts`:
  - Reads `.claude/agents/neuro.md` for system prompt
  - Loads tool manifest: all `tv_*` tools (from TurboVault MCP) + skill
    invocation shims (loads `SKILL.md` from `.claude/skills/<name>/`)
  - Tool-use loop: LLM calls tool → plugin executes → result → back to
    LLM
  - Safety gates: schema-aware writes enforce same invariants as the
    Claude Code side
- `@neuro` detection: when user types `@neuro` in chat, switch to agent
  mode

**Deliverable:** in chat window, "@neuro scan the vault for broken
wikilinks and propose fixes" produces a coherent plan with citations.

**Adversarial review focus:**
- Prompt injection (adversarial vault content reaches agent prompt)
- Infinite tool loops
- Scope creep — agent writes outside allowed paths
- Cost explosion (no per-turn budget)

### Phase 8 — ngrok public exposure + Caddy

**Scope:**
- `config/Caddyfile`: bearer-check reverse proxy `:443 → 127.0.0.1:3001`
- `config/ngrok.yml`: tunnel config, reserved domain (if configured)
- `scripts/start_public_tunnel.sh`: orchestrates Caddy + ngrok startup
- Health check: `/healthz` endpoint returns 200 with valid bearer
- Settings UI: show the public URL + "Rotate Token" button

**Deliverable:** `curl -H "Authorization: Bearer $TOKEN" https://<ngrok>/mcp`
returns the MCP tool list.

**Adversarial review focus:**
- Bearer token leakage (logs, Obsidian plugin settings export)
- Token rotation: does it invalidate existing connections?
- Cloudflare/ngrok WAF interactions
- DoS surface: unlimited subscribe_vault_events subscriptions

### Phase 9 — End-to-end integration test

**Scope:**
- Fresh macOS install via `curl | bash`
- All phases exercised in one run
- Measurements: boot time, p50 tool-call latency, chat stream throughput
- Teardown verified

**Deliverable:** reproducible happy path on a clean machine in <30 min.

**Adversarial review focus:**
- End-to-end security: secret at rest, in transit, in logs
- Failure recovery (Qdrant down, llama-server crashes, ngrok disconnects)

## Testing strategy

**Unit tests** per phase:
- Rust: `cargo test --all-features` in the fork
- TypeScript: `bun test` in `obsidian-plugin/`

**Integration tests** per phase:
- Phase 4A: spawn `turbovault --transport websocket`, connect Node WS
  client, subscribe, touch file, assert event received in <500ms
- Phase 4C: Obsidian plugin in vault fixture, drop file, assert task
  spec created
- Phase 6+7: scripted chat session hitting a mock LLM provider
- Phase 8: curl-based smoke covering auth failures, tool listing,
  subscription

**Adversarial reviews** via `/codex:adversarial-review --effort xhigh`:
- After every phase
- Produces a report saved under `.planning/2026-04-18-.../reviews/phase-<N>.md`
- Findings classified: blocker | should-fix | nit
- Blockers gate progression; should-fix get tickets; nits noted

## Parallel agents dispatched in Phase 4A kickoff

1. **obsidian-copilot source analysis** — license, reusable UI code
2. **subscribe tool API design** — MCP tool surface, subscription
   registry, WebSocket notification plumbing
3. **Caddy + ngrok auth proxy design** — Caddyfile shape, ngrok
   config, health-check endpoint
4. **Obsidian plugin extension audit** — current code map, safe
   injection points, existing bearer-auth pattern reuse
5. **Provider abstraction design** — interface shape, quirks between
   OpenRouter / Anthropic / OpenAI / local

All agents spawn with full capability; each returns a concrete
design+implementation-notes artifact the main thread consumes.
