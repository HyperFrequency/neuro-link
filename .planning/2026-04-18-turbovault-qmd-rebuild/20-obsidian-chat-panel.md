---
title: Phase 6/7 — Obsidian Chat Panel + @neuro Agent Mode (Design)
status: draft
created: 2026-04-18
owner: Dan Repaci
supersedes_sections_of: 10-turbovault-fork-chat-ngrok.md (Phases 6 & 7)
---

# Phase 6/7 Design — Chat Panel & @neuro Agent

Design-only. No implementation in this doc. Consumed by the next PR that
lands the right-side chat view + agent mode on top of the provider
abstraction and MCP subscription work from Phase 4C+5.

## What's already in place (Phase 4C/5 merge)

- `LLMProvider` interface + OpenRouter/Anthropic/OpenAI/local-llama adapters
  behind dynamic import. Single entry point `plugin.llm`.
- Settings UI for provider keys, base URLs, default models, priority list.
- `VaultSubscriptionClient` connects to TurboVault MCP via WebSocket,
  subscribes with a glob filter, dispatches events to a handler.
- `NewSpecDispatcher` consumes those events and writes task specs under
  `00-neuro-link/tasks/` — already uses `plugin.llm.tool_use(...)`.
- `plugin.lifetimeSignal` — central AbortController reset on `onunload`.
- `schemaVersion` bumped to 2 with a `migrateSettings()` that preserves
  legacy top-level keys for one release.

## Phase 6 — Chat Panel (Right-side view)

### Goals

1. Right-side `ItemView` that streams assistant responses with markdown
   rendering and citation chips.
2. Plain `<textarea>` composer — **no Lexical**. Regex-based typeahead for
   `@`-mentions (files, skills, agent names). Matches the existing
   `chatbot.ts` style so users have muscle memory.
3. Keyboard shortcut `cmd+shift+k` to toggle.
4. File-drop attachment: drag a `.md` file into the composer → its vault
   path becomes a mention + the content attaches to the next user turn.
5. Conversation history stored in `state/chat_history/<date>.jsonl` so the
   user's threads survive vault reloads. Per-day rotation keeps file size
   bounded.

### Files to adapt (or rebuild cleanly) from `logancyang/obsidian-copilot`

Upstream is **AGPL-3.0-only**. Any adapted file MUST carry:

```ts
// SPDX-License-Identifier: AGPL-3.0-only
// Portions adapted from logancyang/obsidian-copilot
//   Copyright (c) 2023 Logan Yang
// See THIRD_PARTY/obsidian-copilot/LICENSE for the full license text.
```

Proposed adaptation scope (only the parts with clear reusable value):

| Upstream path                                   | Local path                                    | Strategy                                                                                    |
| ----------------------------------------------- | --------------------------------------------- | ------------------------------------------------------------------------------------------- |
| `src/components/Chat/ChatInput.tsx`             | `src/views/chat/composer.ts`                  | **Rewrite, do not adapt**. Upstream uses Lexical; we use `<textarea>`. Take UX patterns only. |
| `src/components/Chat/ChatMessages.tsx`          | `src/views/chat/message-list.ts`              | Adapt. Markdown rendering via `MarkdownRenderer.render()`. Adds AGPL header.                |
| `src/components/Chat/ChatStreamingIndicator.tsx`| `src/views/chat/streaming-indicator.ts`       | Adapt. Simple, short.                                                                       |
| `src/commands/copyMessage.ts`                   | `src/views/chat/copy-utils.ts`                | Adapt trivially.                                                                            |
| `src/utils/formatDateTime.ts`                   | inline in `src/views/chat/message-list.ts`    | Don't adapt — trivial `new Date().toLocaleTimeString()` suffices.                           |
| `src/components/modals/ChatModeToggle.tsx`      | rebuild                                       | Our mode set differs (chat vs @neuro-agent); rebuild.                                       |

The hot path (composer → LLM → render) is **built from scratch** because
Lexical adds ~300KB to the bundle and we don't need its feature set. Only
the parts that are genuinely useful (markdown rendering helpers, copy-to-
clipboard wiring, streaming indicators) get adapted with attribution.

### Composer — typeahead design

Plain `<textarea>`. Keypress handler tracks cursor position; on `@` we
show an overlay with three tabs:

1. **Files** — `app.vault.getMarkdownFiles()` filtered by fuzzy match.
2. **Skills** — read `.claude/skills/<name>/SKILL.md` frontmatter.
3. **Agents** — currently only `@neuro`; expandable.

Overlay is a plain `<div>` absolutely positioned relative to the textarea.
Close on `Esc`, `click outside`, or selection. Arrow keys + Enter select.

No framework. No virtual DOM. ~150 lines.

### Streaming

`plugin.llm.chatStream(...)` yields `LLMStreamChunk`s. The message list
keeps a buffer for the in-progress assistant message; on each `contentDelta`
we append and re-render the last bubble only. `MarkdownRenderer.render()`
is re-called on the full accumulated content — this is fine at normal
stream rates and lets us preserve markdown fidelity without incremental
parsing.

Tool-call chunks materialise as separate message bubbles ("Calling
`tv_search` …") that update in-place when the result comes back. This is
where @neuro's transparency lives.

### Abort

Each send gets a per-turn `AbortController`. The "Stop" button aborts it.
`plugin.lifetimeSignal` is composed in via an
`AbortSignal.any([perTurn.signal, plugin.lifetimeSignal])` (polyfilled for
older Node/Electron: combine by wiring both to a third controller).

## Phase 7 — @neuro Agent Mode

### Trigger

- Explicit: user types `@neuro` anywhere in the composer.
- Explicit button: mode toggle in the chat header switches Chat ↔ Agent.

In agent mode the composer is re-themed, a tool-call trace panel slides in
on the right, and the assistant is instructed to call tools freely.

### System prompt

Read from `.claude/agents/neuro.md`. If missing, emit a default
"You are the neuro-link agent embedded in an Obsidian vault..." and log a
`Notice` suggesting the user generate one via `/skill-creator`.

### Tool surface

1. **TurboVault tools** — fetched once from the subscription client's
   connection via `tools/list`. Prefixed `tv_*`. Cached 5 minutes; reload
   on explicit user "Refresh tools" button.
2. **Skill shims** — load each `.claude/skills/<name>/SKILL.md` frontmatter,
   expose `run_skill_<name>` with parameters derived from the skill's
   declared inputs. Execution shells out to `neuro-link skill run` via the
   existing `runNlrCommand` helper.
3. **Local helpers** — `get_current_note`, `open_file`, `create_task`,
   `notice`. Plain in-plugin implementations.

Tools are passed to `plugin.llm.tool_use(...)` per turn. The agent loop is
bounded:

```
for (let turn = 0; turn < MAX_TURNS; turn++) {
  const result = await plugin.llm.tool_use({ ..., messages, tools });
  messages.push(assistantMessage(result));
  if (!result.tool_calls?.length) break;           // agent produced text → done
  for (const tc of result.tool_calls) {
    const toolResult = await executeTool(tc);      // SAFETY GATES here
    messages.push(toolMessage(tc.id, toolResult));
  }
}
```

`MAX_TURNS` default 12 (configurable). Token budget tracked per
conversation; soft limit warns the user, hard limit aborts.

### Safety gates (MUST-HAVES)

- **Allowed-paths enforcement**: before any `tv_write_note` or `tv_edit_note`
  call, check the target path against `config/neuro-link.md:allowed_paths`.
  On violation, short-circuit with a tool-error response (do NOT let the
  raw MCP error back-propagate as the LLM may retry).
- **Schema enforcement on `02-KB-main/` writes**: route through
  `nlr_wiki_*` instead of `tv_write_note` when the target is under
  `02-KB-main/`. Same invariant as the Claude Code side.
- **Per-turn budget**: hard cap tokens per turn (configurable, default
  8k input / 4k output). Rejections surface as a Notice and stop the loop.
- **Prompt-injection hardening**: vault content included in the system
  prompt MUST be clearly delimited (`<vault-content>...</vault-content>`)
  and the system prompt explicitly tells the model to treat it as data,
  not instructions. Tool descriptions never include dynamic vault content.

### Trace panel

While the agent runs, a right-side panel lists every tool call + result
with collapsible JSON previews. Users can:

- Copy a tool call to the clipboard for reproduction.
- Pause the loop at the next turn boundary.
- Roll back a write (via `tv_batch_execute` reverse).

Log the trace to `state/agent_traces/<conversation>.jsonl` for post-hoc
debugging.

## Testing plan

Unit tests already landing in Phase 4C/5 cover:

- SSE parser correctness
- Provider error normalisation
- Subscription handle extraction
- Dispatcher slug sanitisation + spec validation

Phase 6/7 adds:

- **Composer typeahead** — pure function tests around the `@`-mention
  regex + list filtering.
- **Agent loop** — fake `LLMProvider` that scripts tool_use sequences; run
  the agent loop and assert final state, trace, budget enforcement.
- **Safety gate** — mock `tv_write_note` receiving a path outside
  `allowed_paths` → assert short-circuit.
- **Integration** (gated on TurboVault tool availability): end-to-end
  send-user-message → assistant-tool-call → tv_read_note → render. Marked
  `skip_if_server_missing` until the Rust side lands the upstream tool.

## Out of scope for Phase 6/7

- Multi-chat tabs
- Image / audio input
- Voice output
- Cross-device sync (user's existing sync plugin handles chat_history
  since it's in `state/`)

## Open questions

1. **Persistence format for chat history** — JSONL vs a single markdown
   file per thread? JSONL for streaming-friendly appends, markdown for
   human-readable. Default: JSONL; add an "Export to markdown" button.
2. **Provider override per conversation** — currently one priority list
   per vault. Should each thread optionally pin a specific provider?
   Probably yes; add a dropdown next to the model selector.
3. **Token pricing display** — read from the LLM usage telemetry and show
   a running cost estimate in the header? Would need a hard-coded pricing
   table per model, which rots quickly.

## Next steps (ordered)

1. Land Phase 6 composer + message list against the existing provider
   abstraction. No agent mode yet.
2. Add `.claude/agents/neuro.md` with a first-draft system prompt.
3. Land the agent loop against a mock LLM (unit tests only).
4. Wire the mock to a real LLM + TurboVault once the `tv_*` tool surface
   is reliably served from the fork (tracked separately).
5. Adversarial review `/codex:adversarial-review --effort xhigh` after
   Phase 6 UI ships, and again after Phase 7 agent ships.
