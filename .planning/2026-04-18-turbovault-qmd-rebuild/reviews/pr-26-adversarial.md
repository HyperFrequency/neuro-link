# Adversarial review — PR #26 (merged)

**PR:** [#26](https://github.com/HyperFrequency/neuro-link-recursive/pull/26) — providers + MCP subscription + dispatcher. Merged 2026-04-18 08:23 UTC at `59a5e2a`. Effort: xhigh. Post-merge — findings are follow-up work, not merge gates.

---

### Blockers

**1. Bearer token in WS query string.** `mcp-subscription.ts:83` appends `?token=<t>`. Even over TLS this leaks into Caddy access logs, Electron DevTools Network panel, and proxies. Subprotocol `bearer.<t>` (:84) is already sufficient. Fix: drop `?token=`; if the server can't read subprotocols pre-upgrade, gate on a short-lived ticket (`GET /mcp/ticket` with bearer header → `?ticket=<t>`).

**2. Auth-failure reconnect storm.** `mcp-subscription.ts:230-239` backs off but never distinguishes auth errors. Bad token = ~120 reconnects/hr forever, silently. Fix: on `CloseEvent.code === 1008` or app codes 4401/4403, set `stopped = true` and fire one `Notice("NLR subscription auth failed")`. Same for `err.code in [-32001,-32002]` at :172-182.

**3. Stale-content race vs LLM round-trip.** `new-spec.ts:89-104` gates per-path concurrency but doesn't re-check content after the 8-20 s LLM call. User drops `plan.md`, LLM runs, user edits mid-flight → dispatcher writes stale content. FileModified isn't watched. Fix: hash at read time; re-read before write, abort if changed.

**4. `loadSecretsEnv` doesn't sync to `llm.providers`.** `settings.ts:1074-1105` mutates `apiKeys[...]` only. `llm.providers.*.apiKey` (what providers read) stays stale until full plugin reload. Fix: map `OPENROUTER_API_KEY → llm.providers.openrouter.apiKey` (same for anthropic/openai); call `refreshLLM()`.

---

### Should-fix

**5. MCP surface is swap-ready but misnamed.** Dispatcher only sees `VaultEvent` (:28-35). Nothing in `dispatcher/` or `main.ts:88-102` depends on close events, ping/pong, or subprotocols — swap is localized. File name will mislead after the pull/long-poll pivot. Rename to `mcp-vault-events.ts`; extract `VaultEventSource { start(); stop(); onEvent }`.

**6. Unbounded SSE buffer.** `providers/sse.ts:47` appends chunks with no cap — a misbehaving server can OOM the renderer. Cap at 4 MB; throw `LLMProviderError("sse","bad_request",...)`. Same for `mcp-subscription.ts:185-193`: size-check `ev.data` before `JSON.parse`.

**7. `pendingRequests` unbounded + no RPC timeout.** `mcp-subscription.ts:57-60,241-255`. Only 2 RPCs fire today; once Phase 4D issues tool calls over the socket, a non-responsive server accumulates entries forever. Cap at 256, add 30 s per-call timeout that rejects with `timeout` kind.

**8. Settings-migration has zero test coverage.** `migrateSettings()`+`mergeLLMSettings()` at `settings.ts:136-189` is the most behavior-sensitive change. Add `tests/settings-migration.test.ts`: (a) legacy-only keys lifted into `llm.providers.*.apiKey`; (b) both set with different values → new wins (:170-185); (c) empty input → defaults. Extract the migration functions into a sibling module so tests don't pull the `obsidian` runtime.

**9. `onload` doesn't await subscription connect.** `main.ts:98` is fire-and-forget. TurboVault handshake is 50-300 ms locally, longer over network — files dropped in that window are silently missed server-side. Fixes: await connect (~2 s acceptable) OR server-side catchup via `tv_recent_vault_events(since)`. Gets worse with long-poll pivot.

**10. Timer leak in streaming paths.** `openrouter.ts:194` returns without calling `cleanup` for streams (punts to "upstream reader cancellation"); `streamOpenAIChunks` at :243-306 doesn't call it either. On plugin unload mid-stream, the timeout timer is never cleared. Propagate `cleanup` out of `fetchWithTimeout`, invoke in the `finally` of the stream iterators.

**11. Brittle `[DONE]` check.** `openrouter.ts:252`: exact `data === "[DONE]"` misses `[DONE] ` with trailing whitespace. Use `data.trim() === "[DONE]"`.

**12. Keys plaintext in `data.json`.** Obsidian offers no encrypted plugin storage — inherent. But the plugin now **doubles** exposure: keys sit in both `data.json` and `secrets/.env` after `saveSecretsEnv()`. After `settings.ts:1044-1072` succeeds, offer "Clear keys from plugin settings?" confirm. Document trade-offs in README. File a long-lived issue for OS-keychain integration.

**13. Slug-collision race on concurrent drops.** `new-spec.ts:242-252` pre-checks existence, then `vault.create` — TOCTOU. Parallel drops with the same LLM slug both see `-1` free, one creates, the other's `vault.create` throws "file exists" and is swallowed as a warning. Fix: catch that specific error from `vault.create` and retry the whole loop with the next suffix.

**14. `FileCreated` filter duplicated — server + client.** `mcp-subscription.ts:161` vs `new-spec.ts:57`. Keep both (defense in depth) but comment the intentional duplication.

**15. No retry on `tool_use` JSON failure.** `new-spec.ts:196-209` makes one call; bad JSON in args → `parseSpec` falls through to fence fallback, then silently drops. Add 1 retry with stricter prompt suffix ("previous response wasn't valid JSON — retry with exactly the tool call"). Cap at 1.

**16. Trailing-newline readiness false-negatives `echo -n` / non-eol-adding editors.** `new-spec.ts:122-142`. Combine the newline check with "mtime stable for 500 ms"; accept if mtime is unchanged across two reads regardless of trailing newline.

---

### Nits

**17.** `openrouter.ts:249` `emittedToolCallIndices` never reassigned — `let` → `const`.
**18.** `new-spec-helpers.ts:63-65` `escapeYaml` only handles `\` and `"`. LLM-produced title with literal `\n` breaks YAML. Use `JSON.stringify` for quoted strings.
**19.** `new-spec.ts:81` `isWatchedPath` survives backslashes but not URL-encoded paths. Make pure + unit-test.
**20.** `new-spec.ts:151-162` re-reads prompt template every dispatch. Cache in memory; invalidate on unload.
**21. AGPL preflight — pass.** No `SPDX: AGPL-3.0-only` headers in `src/` (correct; nothing adapted yet). `THIRD_PARTY/obsidian-copilot/README.md:2-8` states clean-room. Tests MIT. Phase 6 audit required per file.
**22. AbortController — no cross-call leak, confirmed.** `main.ts:31` owns one controller; `combineSignals` wraps it in per-call inner controllers. Aborting one call does NOT cascade to others; aborting lifetime DOES cascade via `{once: true}` listeners.
**23. Dynamic-import dedup — partial.** `providers/index.ts:131-149` caches instances, but `esbuild.config.mjs` bundles all four providers into one chunk — "lazy parse" in the docstring (:153-158) is overstated. Enable code-splitting or soften the claim.
**24. Sweep carryover.** `stats.ts`, `chatbot.ts`, `commands.ts`, `api-router.ts` still read `settings.apiKeys.OPENROUTER_API_KEY`. Sweep to `plugin.llm.chat(...)` in the release that drops the legacy fallback (`settings.ts:131-133`).

---

Open 1-4 as issues this week before Phase 6 chat panel lands on top. 5-16 are pre-release. 17-20/23 are micro-cleanups. 21-22 pass. 24 is a sweep.
