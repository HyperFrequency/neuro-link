# Comprehensive install plan — neuro-quant + neuro-link from empty
Generated: 2026-04-25 (rev-3 — addresses codex review #1+#2 findings)

Goal: Take SHIP-approved branches end-to-end on this Mac (M4 Max arm64) from empty, with explicit user gates before host-mutating steps, sandbox-first.

## Critical clarifications (rev-2 + rev-3 fixes)

- The PARENT `neuro-quant/Makefile` has the full local installer flow. pkg/Makefile's `make all` includes cloud-modal/lambda/ray which actually deploy when creds are present. Plan never invokes pkg/Makefile `all` or `cloud*`.
- `make deps` is HOST-MUTATING (calls `brew install`). It is NOT part of any "preview" phase.
- The PRE-MUTATION DIFF gate is OPERATOR-ENFORCED, not code-enforced. install_mcp_servers.sh deep-merges UNCONDITIONALLY in code; the gate is a manual safety check the operator MUST verify before allowing `make mcp` to run.
- "Rollback" is **manual** via the Phase 0.1 backups. install-mirror.sh's trap only cleans temp files, NOT user-global state.
- Serena ships as THREE distinct binaries with distinct roles: `serena-mcp`, `serena-hooks`, plain `serena`. Each gates a different installer behavior.

## Phase 0 — inventory + backups (no mutation; plan can stop cleanly here)

0.1 Snapshot user-global state (preserve, don't delete):
- `cp ~/.claude.json ~/.claude.json.bak.<ts>`
- `cp -r ~/.claude/hooks ~/.claude/hooks.bak.<ts>`
- `cp ~/.claude/settings.json ~/.claude/settings.json.bak.<ts>`

0.2 Inventory existing state:
- branch+dirty status of `/Users/DanBot/hyperfrequency/neuro-quant`
- pre-existing models in repo `models/` and `~/.cache/qmd/models/`
- `~/.claude/state/nlr_root` content
- which cloud creds are set (AWS_ACCESS_KEY_ID, AWS_PROFILE, MODAL_*, RAY_*) — informational only; never invoked

0.3 SERENA TRIPLE INVENTORY (rev-3 review-2 M5 fix):
- 0.3a `~/.local/bin/serena-mcp` — REQUIRED for `install-mirror.sh` post-install MCP validation. If missing: install-mirror exits 3 unless the existing `serena` MCP entry in `~/.claude.json` already passes `is_mcp_entry_valid`.
- 0.3b `~/.local/bin/serena-hooks` — REQUIRED for `settings.template.json` hook injection to land. If missing: gate-11 CC2 actively STRIPS serena-hooks entries from both rendered template AND existing `~/.claude/settings.json` during `make hooks`. Document explicitly which serena-hooks references will be removed.
- 0.3c `~/.local/bin/serena` — INFORMATIONAL. gate-11 CC3 reads `.command` from `~/.claude.json.mcpServers.serena`; any executable path there passes.

0.4 PRE-MUTATION DIFF gate (operator-enforced):
- Read `~/.claude.json.mcpServers` for {neuro-link-recursive, neuro-link-http, serena, turbovault}  ← rev-3 review-2 H2: turbovault added to protected set
- For each: 
  - absent → safe
  - present + matches canonical → no-op
  - present + DIVERGES → STOP; explicit operator approval required before `make mcp` (which deep-merges and overwrites)
- Read `~/.claude/settings.json.hooks` for any `serena-hooks` references AND determine if 0.3b binary exists. If references exist + binary missing → `make hooks` will purge those references. STOP; explicit operator approval required.

0.5 Prereq gate: arm64 host, arm64 brew, rustc, python3.12 arm64 via uv, uv, docker, buildx, gh auth, node22+, pnpm, huggingface-cli, HF_TOKEN. Stop if missing.

## Phase 1a — read-only preview (zero host mutation)

1a.1 Fresh checkout in `/tmp/install-sandbox-<ts>/`:
```
gh repo clone HyperFrequency/neuro-quant -- -b test/monorepo-install-docs-parent
cd /tmp/install-sandbox-<ts>/neuro-quant && git submodule update --init --recursive
```

1a.2 Inspect (no execution): `make -n init submodules deps venvs pkg pinelsp models pre-proof mcp hooks proof | head -200` to see exact commands the real install will run.

1a.3 Inspect what `make hooks` would delete by simulating the strip filter on the user's actual `~/.claude/settings.json` against the rendered template. (gate-11 CC2 / gate-12 EE1).

1a.4 Inspect `pkg/.proof/INSTALL_COMPLETE.<state>.json` from prior runs (if any) to baseline.

1a.5 STOP gates: if any 0.3 / 0.4 mismatch unaccepted by operator → halt before Phase 1b.

## Phase 1b — sandbox host-mutating exercise (CONSCIOUS host mutation)

⚠ This phase MUTATES the host: `make deps` calls `brew install`, `make pinelsp` symlinks into `~/.local/bin`, `make models` downloads ~20 GB into `~/.cache/qmd/models/` and sandbox `models/`. Existing files preserved by gate-G3 idempotency.

1b.1 In sandbox checkout:
- `make init`
- `make submodules`
- `make deps` ← brew install side-effects begin here
- `make venvs`
- `make -C neuro-link -f pkg/Makefile local` (LOCAL-ONLY; cloud targets explicitly excluded)
- `make pinelsp` (or with SKIP_RUST=1 SKIP_FOLKNOR=1 to dry it)
- `make models` (idempotent)
- `make pre-proof`

1b.2 NEVER run `make mcp` or `make hooks` in sandbox. Validate via DRY-RUN: read mcp-servers.yaml + settings.template.json against the rendered targets.

1b.3 Validate sandbox artifacts:
- 10 toolbox venvs arm64 cpython-3.12
- `server/target/aarch64-apple-darwin/release/neuro-link` Mach-O arm64
- 3 models present with correct sha256
- `pkg/.proof/INSTALL_COMPLETE.incomplete.json` written, state=incomplete (skips intentional)
- `pkg/.proof/ALL.ready.json` aggregates per-platform builder proofs

1b.4 If any check fails → STOP, report.

## Phase 2 — pre-flight on real directories

2.1 cd `/Users/DanBot/hyperfrequency/neuro-quant`. If `git status --porcelain` non-empty → ASK USER, no auto-reset.

2.2 `git fetch origin && git checkout test/monorepo-install-docs-parent && git submodule update --init --recursive`

2.3 Verify submodule pointer = `833f1ad`.

2.4 Re-run Phase 0.3+0.4 against current `~/.claude.json` and `~/.claude/settings.json`. Same gating logic with operator approval required for any divergence.

## Phase 3 — real install (with explicit acceptance gates)

3.1 Run installer phase by phase:
- `make init && make submodules && make deps && make venvs`  — repo-local + brew side-effects
- `make pkg`                                                   — submodule build
- `make pinelsp`                                               — pinelsp + symlink (preserves existing)
- `make models`                                                — idempotent
- `make pre-proof`                                             — required exit 0
- **STOP HERE if pre-proof exit ≠ 0.** No further mutation.
- `make mcp`                                                   — mutates `~/.claude.json`
- `make hooks`                                                 — mutates `~/.claude/hooks/` + `~/.claude/settings.json`
- `make proof`                                                 — final verify.py (offline + skips)

3.2 Acceptance: `pkg/.proof/INSTALL_COMPLETE.<state>.json` MUST be `ready` (no skips, ideal post-rag-up) or `incomplete` (intentional skips of post-rag-up checks). State=fail blocks acceptance regardless of exit code.

3.3 If 3.1 mid-phase fails AFTER `make mcp` succeeded but BEFORE `make hooks` finished:
- ⚠ Manual rollback only (rev-3 review-2 H4 honest framing):
  - `cp ~/.claude.json.bak.<ts> ~/.claude.json`
  - `rm -rf ~/.claude/hooks/<partially-copied-files>` only those that were just added (compare against backup)
  - `cp ~/.claude/settings.json.bak.<ts> ~/.claude/settings.json`
  - Restart Claude Code to pick up reverted state

## Phase 4 — post-install live audit

4.1 Generate secrets if missing: `make secrets-bootstrap` writes NEO4J_PASSWORD + LITELLM_MASTER_KEY + NLR_API_TOKEN to `neuro-link/secrets/.env` (idempotent).

4.2 `make rag-up`. Health-check qdrant :6333, llama-embed :8400, llama-rerank :8401, llama-qexpand :8402.

4.3 Boot neuro-link MCP HTTP server (rev-3 review-2 H3 fix):
- Source `neuro-link/secrets/.env` so NLR_API_TOKEN is in env
- `cd /Users/DanBot/hyperfrequency/neuro-quant/neuro-link`
- Start the server with logs captured: `nohup neuro-link serve > /tmp/neuro-link-serve-<ts>.log 2>&1 &`
- Verify: `curl -s -H "Authorization: Bearer $NLR_API_TOKEN" http://127.0.0.1:8787/health` returns 200
- Save the PID for cleanup.

4.4 Tombstone-on-blank live test (gate-43 III1):
- `mkdir -p vaults/_smoketest && echo 'verify-tombstone' > vaults/_smoketest/blank-test.md`
- Trigger nlr_rag_embed via MCP: `curl -s -X POST -H "Authorization: Bearer $NLR_API_TOKEN" -H "Content-Type: application/json" http://127.0.0.1:8787/mcp -d '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"nlr_rag_embed","arguments":{"recreate":false}}}'`
- Confirm: `tail /tmp/neuro-link-serve-<ts>.log` shows the embed log line for blank-test.md
- `: > vaults/_smoketest/blank-test.md` (truncate to whitespace)
- Re-trigger nlr_rag_embed via MCP
- Confirm: `tail /tmp/neuro-link-serve-<ts>.log` contains "tombstoning stale point for vaults/_smoketest/blank-test.md (id=...)"
- Vector search via nlr_rag_query for "verify-tombstone" → 0 results

4.5 `make proof-full` for the comprehensive ship gate. **Required**: produces `INSTALL_COMPLETE.ready.json` with `state="ready"`. Anything else (incomplete/fail) is a blocking failure.

4.6 Validate user-global state diff (post-install vs Phase 0.1 backup):
- `diff <(jq -S . ~/.claude.json.bak.<ts>) <(jq -S . ~/.claude.json) | head -50`
- Must show ADDS only: missing-then-added mcpServers entries
- Any REMOVAL of user-supplied keys → STOP, restore from backup, investigate

## Phase 5 — close

5.1 Memory snapshot of install state with diffs.
5.2 Confirm backups intact at `~/.claude.json.bak.*`, `~/.claude/hooks.bak.*`, `~/.claude/settings.json.bak.*`.
5.3 Optional: open PRs to merge SHIP branches to master/main (only after user confirmation).

## Risk register (rev-3 hardened)

| ID | Risk | Mitigation |
|---|---|---|
| R1 | mcp+hooks mutate user-global state | Phase 0.1 backup; Phase 0.4+2.4 PRE-MUTATION DIFF; gate-9 AAAA2 ordering; .NOTPARALLEL |
| R2 | ~20 GB model download | Phase 0.2 inventory; gate-G3 idempotent skip-if-present |
| R3 | pinelsp needs node22+pnpm | Phase 0.5 prereq gate; SKIP_RUST/SKIP_FOLKNOR escapes |
| R4 | real /neuro-quant has uncommitted work | Phase 2.1 ASK USER, no auto-reset |
| R5 | existing serena MCP shape diverges | Phase 0.3+0.4 DIFF gate covering serena AND turbovault |
| R6 | Hook stripping deletes user serena-hooks references | Phase 0.4 explicit gate; operator must approve before purge |
| R7 (rev-2 H1) | pkg/Makefile `all` includes cloud deploys | Plan never calls pkg `all`/`cloud*`; Phase 1b uses explicit `local` |
| R8 (rev-2 M4) | exit 0 with state=incomplete masquerades as success | Phase 3.2 + Phase 4.5 acceptance keyed on STATE field |
| R9 (rev-3 H1) | `make deps` is host-mutating | Phase 1 split into 1a (read-only) + 1b (consciously host-mutating) |
| R10 (rev-3 H4) | Recovery section was inaccurate | Phase 3.3 honest manual restore procedure |

## Success criteria (rev-3 hardened)

✅ Phase 0 backups exist and are restorable
✅ Phase 0.4 DIFF gate showed no divergent user entries OR user explicitly approved overwrite for each
✅ Phase 1a read-only preview clean (no host mutation)
✅ Phase 1b sandbox host mutation green (per the explicit acceptance criteria)
✅ Real `make pre-proof` exits 0 BEFORE mcp/hooks run
✅ Real `make mcp` + `make hooks` + `make proof` complete; `INSTALL_COMPLETE.<state>.json` is ready or incomplete (not fail)
✅ Phase 4.5 `make proof-full` produces `INSTALL_COMPLETE.ready.json` with state="ready" — ONLY acceptable terminal state
✅ Phase 4.4 tombstone-on-blank live test confirmed via server log + vector-search probe
✅ Phase 4.6 `~/.claude.json` diff shows ADDs only, no REMOVALs of user keys
