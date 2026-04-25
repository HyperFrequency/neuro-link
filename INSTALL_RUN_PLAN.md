# Comprehensive install plan — neuro-quant + neuro-link from empty
Generated: 2026-04-25 (rev-4 — addresses codex reviews #1+#2+#3)

Goal: Take SHIP-approved branches end-to-end on this Mac (M4 Max arm64) from empty, with explicit operator gates before host-mutating steps, sandbox-first.

## Critical clarifications carried from review iterations

- The PARENT `neuro-quant/Makefile` has the full local installer flow. pkg/Makefile's `make all` includes cloud-modal/lambda/ray which actually deploy when creds are present.
- `make proof` is defined IN THE PARENT MAKEFILE (with PROOF_SKIP=qmd,multilspy,pyright,vaults) and runs `verify.py` (rev-3 review-3 H1 fix). The submodule pkg/Makefile's `proof` is builder-aggregation-only.
- `make deps` is HOST-MUTATING (calls `brew install`).
- The PRE-MUTATION DIFF gate is OPERATOR-ENFORCED. install_mcp_servers.sh deep-merges UNCONDITIONALLY in code.
- "Rollback" is **manual** via Phase 0.1 backups.
- Serena ships as THREE distinct binaries: `serena-mcp`, `serena-hooks`, plain `serena`.
- Layout (rev-3 review-3 H2): NQ_ROOT = parent neuro-quant repo root. NLR_ROOT = neuro-link submodule root. Detect dynamically; do NOT hardcode `/Users/DanBot/hyperfrequency/...` in the plan execution.

## Phase 0 — inventory + backups (no mutation)

0.1 Snapshot user-global state:
- `cp ~/.claude.json ~/.claude.json.bak.<ts>`
- `cp -r ~/.claude/hooks ~/.claude/hooks.bak.<ts>`
- `cp ~/.claude/settings.json ~/.claude/settings.json.bak.<ts>`

0.2 Resolve workspace layout:
- `NQ_ROOT` = parent neuro-quant repo (has top-level `Makefile` with `pre-proof` + `mcp` + `hooks` + `proof` + `proof-full` targets). Resolve via: search for a checkout with `Makefile` containing `pre-proof:` AND a `neuro-link/` submodule.
- `NLR_ROOT` = neuro-link submodule root, found at `$NQ_ROOT/neuro-link`.
- If user's actual checkout doesn't have a parent neuro-quant: clone via `gh repo clone HyperFrequency/neuro-quant -b test/monorepo-install-docs-parent <chosen-path>` (with explicit user approval).

0.3 SERENA TRIPLE INVENTORY:
- 0.3a `~/.local/bin/serena-mcp` — REQUIRED for canonical MCP registration via install-mirror.sh; if missing AND `~/.claude.json.mcpServers.serena.command` doesn't already resolve to an executable → install-mirror exits 3.
- 0.3b `~/.local/bin/serena-hooks` — REQUIRED for `settings.template.json` hook injection. If missing → gate-11 CC2 strips serena-hooks references from rendered template AND existing `~/.claude/settings.json`.
- 0.3c `~/.local/bin/serena` — INFORMATIONAL.

0.4 PRE-MUTATION DIFF gate (operator-enforced; protected set: serena, neuro-link-recursive, neuro-link-http, turbovault):
- For each protected MCP: absent OR matches canonical → safe; diverges → STOP, ask user.
- Read `~/.claude/settings.json.hooks` for serena-hooks references; if present + binary missing → ask user before allowing purge.

0.5 Prereq gate: arm64 host, arm64 brew, rustc, python3.12 arm64 via uv, uv, docker, buildx, gh auth, node22+, pnpm, huggingface-cli, HF_TOKEN.

0.6 Pre-existing model inventory (rev-3 review-3 M7 fix):
- `find $NLR_ROOT/models ~/.cache/qmd/models -name '*.gguf' 2>/dev/null` — list pre-existing artifacts; gate-G3 idempotent skip will preserve.
- HF_TOKEN export plan: `export HF_TOKEN=<value-from-secrets-or-prior-shell>` BEFORE `make models`. Do not rely on secrets/.env being auto-sourced — `download_models.sh` doesn't source it.

## Phase 1a — read-only preview (zero host mutation)

1a.1 Fresh checkout in `/tmp/install-sandbox-<ts>/`:
```
gh repo clone HyperFrequency/neuro-quant -- -b test/monorepo-install-docs-parent
cd /tmp/install-sandbox-<ts>/neuro-quant && git submodule update --init --recursive
```
SAND_NQ_ROOT = `/tmp/install-sandbox-<ts>/neuro-quant`.

1a.2 `cd $SAND_NQ_ROOT && make -n init submodules deps venvs pkg pinelsp models pre-proof mcp hooks proof | head -200` — see exact commands without execution.

1a.3 Read what `make hooks` would strip from current `~/.claude/settings.json` if 0.3b binary absent.

1a.4 STOP gates: any 0.3 / 0.4 mismatch unaccepted by operator → halt.

## Phase 1b — sandbox host-mutating exercise

⚠ HOST MUTATIONS in this phase:
- `make deps` calls `arch -arm64 brew install` (system-deps.txt entries; idempotent)
- `make pinelsp` symlinks `pine-lsp` / `pine-lsp-rust` into `~/.local/bin/`
- `make models` writes ~20 GB into `$SAND_NQ_ROOT/neuro-link/models/` and `~/.cache/qmd/models/` (gate-G3 idempotent — pre-existing files preserved)

1b.1 In sandbox:
```
cd $SAND_NQ_ROOT
make init && make submodules && make deps && make venvs
make pkg
make pinelsp     # or SKIP_RUST=1 SKIP_FOLKNOR=1 to dry
HF_TOKEN=<token> make models
make pre-proof
```

1b.2 NEVER run `make mcp` or `make hooks` in sandbox.

1b.3 Sandbox validation:
- 10 toolbox venvs arm64 cpython-3.12 verified via `file(1)`
- `$SAND_NQ_ROOT/neuro-link/server/target/aarch64-apple-darwin/release/neuro-link` Mach-O arm64
- 3 model files present with sha256 confirmed
- `$SAND_NQ_ROOT/neuro-link/pkg/.proof/INSTALL_COMPLETE.incomplete.json` written, state=incomplete
- `$SAND_NQ_ROOT/neuro-link/pkg/.proof/ALL.ready.json` aggregates per-platform builder proofs

1b.4 If pre-proof fails (rev-3 review-3 M5 fix):
- Inspect `INSTALL_COMPLETE.fail.json` for the specific failed check
- If failure is on `qmd` / `pyright` / `multilspy` / `vaults` → that's an unrelated dev-tool gap; pre-proof's skip set should already cover. If not skipping, document why and proceed with awareness.
- If failure is on `serena-arch` / `mcp-servers` / `hooks` → that's the post-mutation contract; should be skipped by pre-proof. If not, the contract drifted; STOP and investigate.
- If failure is on a build artifact (octen-manifest missing, server binary missing) → real build problem, STOP and fix.

## Phase 2 — pre-flight on real directories

2.1 cd $NQ_ROOT (resolved per 0.2). If `git status --porcelain` non-empty → ASK USER.

2.2 `git fetch origin && git checkout test/monorepo-install-docs-parent && git submodule update --init --recursive`

2.3 Verify submodule pointer = `833f1ad`.

2.4 Re-run Phase 0.3+0.4 against current `~/.claude.json` and `~/.claude/settings.json`.

## Phase 3 — real install (atomic per-step gates)

⚠ Each step below is its own checkpoint. On failure mid-way, the rollback procedure for THIS specific step is documented inline.

3.1 `cd $NQ_ROOT && make init`
- Failure mode: prereq missing. No mutation. Fix prereq, retry.

3.2 `make submodules`
- Failure mode: git fetch fail. No mutation. Retry.

3.3 `make deps`
- Failure mode: brew install fails partway. Partial host mutation possible.
- Recovery: re-run `make deps` (idempotent); investigate broken brew formulas.
- Disk-full / SIGTERM: retry.

3.4 `make venvs`
- Failure mode: uv venv create fails. Affects only `$NQ_ROOT/toolbox/.venvs/` (repo-local).
- Recovery: `rm -rf toolbox/.venvs/<broken>` and re-run.

3.5 `make pkg`
- Failure mode: cargo build fails. Affects only `$NLR_ROOT/server/target/`.
- Recovery: `cargo clean` and retry.

3.6 `make pinelsp`
- Failure mode: pnpm install / cargo build fails. Affects pinelsp/ build dir; symlinks land at end.
- Recovery: SKIP_RUST=1 SKIP_FOLKNOR=1 to skip; retry individual variant.

3.7 `make models`
- Failure mode: HF download fails (auth, network, disk-full).
- Recovery: HF_TOKEN check + retry. gate-G3 skip-if-present means re-run is safe.

3.8 `make pre-proof`
- Required: exit 0 AND state field in `INSTALL_COMPLETE.<state>.json` ∈ {ready, incomplete}
- If state=fail: triage per 1b.4
- **Hard stop here if pre-proof fails.** No mcp/hooks mutation.

3.9 `make mcp`
- Mutates `~/.claude.json`. Gate-9 AAAA2 only allows this AFTER pre-proof succeeded.
- Failure mode: install_mcp_servers.sh deep-merge fails OR registers wrong binary (gate-62 BBBB1 should pick triple-specific path).
- Recovery: `cp ~/.claude.json.bak.<ts> ~/.claude.json` (manual restore from Phase 0.1 backup).

3.10 `make hooks`
- Mutates `~/.claude/hooks/` + `~/.claude/settings.json`.
- Failure mode: install-mirror.sh trap cleans temp files but NOT user-global state.
- Recovery (rev-3 review-3 M4 fix - explicit per-failure):
  - For partial hook copies: `diff -r ~/.claude/hooks.bak.<ts>/ ~/.claude/hooks/` to find diffs; `rm` only files added by this run.
  - For settings.json: `cp ~/.claude/settings.json.bak.<ts> ~/.claude/settings.json`.
  - mcp side already done in 3.9; if hooks fails AFTER mcp succeeded: see 3.9 recovery for ~/.claude.json.

3.11 `make proof`
- Runs verify.py (in PARENT Makefile, with offline=1 + skips). 
- Required: state field in `INSTALL_COMPLETE.<state>.json` ∈ {ready, incomplete}; state=fail blocks acceptance.
- This is the final gate of phase 3.

## Phase 4 — post-install live audit

4.1 `cd $NLR_ROOT && make secrets-bootstrap` (in parent; idempotent — gate-7 DD1+FF2 regex).
- This writes NEO4J_PASSWORD + LITELLM_MASTER_KEY + NLR_API_TOKEN into `$NLR_ROOT/secrets/.env`.
- gate-7 DD1+FF2 regex `^KEY=.+$` correctly skips already-set + non-whitespace values; pre-existing values preserved.

4.2 `cd $NQ_ROOT && make rag-up`. Health-check qdrant :6333, llama-embed :8400, llama-rerank :8401, llama-qexpand :8402.

4.3 Boot neuro-link MCP HTTP server (rev-3 review-3 M6 fix — with cleanup):
```
cd $NLR_ROOT
set -a; source secrets/.env; set +a
nohup neuro-link serve > /tmp/neuro-link-serve-<ts>.log 2>&1 &
SERVER_PID=$!
sleep 3
curl -s -H "Authorization: Bearer $NLR_API_TOKEN" http://127.0.0.1:8787/health
```
- Trap cleanup: `trap "kill -TERM $SERVER_PID 2>/dev/null; lsof -i :8787 | grep LISTEN && lsof -ti :8787 | xargs kill -9" EXIT INT TERM`
- After phase 4 complete: `kill -TERM $SERVER_PID; lsof -i :8787` to verify port free.

4.4 Tombstone-on-blank live test (rev-3 review-3 H3 fix — Qdrant query, not log text):
- `mkdir -p $NLR_ROOT/vaults/_smoketest && echo 'verify-tombstone-2026' > $NLR_ROOT/vaults/_smoketest/blank-test.md`
- Trigger embed via MCP: `curl -s -X POST -H "Authorization: Bearer $NLR_API_TOKEN" -H "Content-Type: application/json" http://127.0.0.1:8787/mcp -d '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"nlr_rag_embed","arguments":{"recreate":false}}}'`
- Confirm point in Qdrant: `curl -s http://127.0.0.1:6333/collections/nlr_wiki/points/scroll -H "Content-Type: application/json" -d '{"limit":50,"with_payload":true}' | jq '.result.points[] | select(.payload.path == "_smoketest/blank-test.md")'`
- Truncate to whitespace: `: > $NLR_ROOT/vaults/_smoketest/blank-test.md`
- Re-trigger embed via MCP
- Verify tombstone via Qdrant query: same scroll, expect 0 results matching path "_smoketest/blank-test.md"
- (Optional log corroboration: grep "/tmp/neuro-link-serve-<ts>.log" for "Qdrant tombstone of stale point for _smoketest/blank-test.md")
- Cleanup: `rm $NLR_ROOT/vaults/_smoketest/blank-test.md`

4.5 `make proof-full` for the comprehensive ship gate. **Required**: `INSTALL_COMPLETE.ready.json` with `state="ready"`. State=incomplete or fail blocks acceptance.

4.6 Validate user-global state diff:
- `diff <(jq -S . ~/.claude.json.bak.<ts>) <(jq -S . ~/.claude.json) | head -50`
- Must show ADDS only

4.7 **Trap cleanup**: kill server PID; verify port 8787 free.

## Phase 5 — close

5.1 Memory snapshot of install state with diffs.
5.2 Confirm backups intact.
5.3 Optional PR to merge SHIP branches (after user confirmation only).

## Risk register (rev-4 hardened)

| ID | Risk | Mitigation |
|---|---|---|
| R1 | mcp+hooks mutate user-global state | Phase 0.1 backup; Phase 0.4+2.4 PRE-MUTATION DIFF; gate-9 AAAA2; .NOTPARALLEL |
| R2 | ~20 GB model download | Phase 0.6 inventory; gate-G3 idempotent |
| R3 | pinelsp needs node22+pnpm | Phase 0.5 prereq gate; SKIP_RUST/SKIP_FOLKNOR escapes |
| R4 | real /neuro-quant has uncommitted work | Phase 2.1 ASK USER |
| R5 | existing serena MCP shape diverges | Phase 0.3+0.4 DIFF gate covering serena AND turbovault |
| R6 | Hook stripping deletes user serena-hooks references | Phase 0.4 explicit gate |
| R7 | pkg/Makefile cloud deploys | Plan never calls pkg `all`/`cloud*` |
| R8 | exit 0 with state=incomplete looks like success | Acceptance keyed on STATE field |
| R9 | `make deps` is host-mutating | Phase 1 split: 1a read-only, 1b host-mutating |
| R10 | Recovery section was inaccurate | Phase 3.x per-step recovery procedures |
| R11 (rev-3 H1) | `make proof` ambiguity (parent vs pkg) | Plan now explicit: parent Makefile's proof runs verify.py; pkg/Makefile is unused after `make pkg` |
| R12 (rev-3 H2) | Layout assumptions | Phase 0.2 dynamic NQ_ROOT/NLR_ROOT resolution |
| R13 (rev-3 H3) | Tombstone log-text mismatch | Phase 4.4 validates via Qdrant query, not log text |
| R14 (rev-3 M5) | pre-proof legitimate non-MCP failures | Phase 1b.4 + 3.8 triage by failed check |
| R15 (rev-3 M6) | nohup server orphan | Phase 4.3 explicit trap + 4.7 cleanup |
| R16 (rev-3 M7) | HF_TOKEN not auto-exported | Phase 0.6 explicit `export HF_TOKEN=...` step |

## Success criteria (rev-4 hardened)

✅ Phase 0 backups exist and are restorable
✅ Phase 0.4 DIFF gate showed no divergent user entries OR user explicitly approved overwrite per protected entry
✅ Phase 1a read-only preview clean
✅ Phase 1b sandbox host mutation green per per-artifact checks
✅ Real `make pre-proof` exits 0 BEFORE mcp/hooks run
✅ Real `make mcp` + `make hooks` + `make proof` complete with state ∈ {ready, incomplete}
✅ Phase 4.5 `make proof-full` produces `INSTALL_COMPLETE.ready.json` with state="ready" — ONLY acceptable terminal state
✅ Phase 4.4 tombstone-on-blank live test confirmed via Qdrant query (no reliance on log text)
✅ Phase 4.6 `~/.claude.json` diff shows ADDs only, no REMOVALs of user keys
✅ Phase 4.7 server cleanup: PID terminated, port 8787 free
