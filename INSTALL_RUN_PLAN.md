# Comprehensive install plan — neuro-quant + neuro-link from empty
Generated: 2026-04-25 (revision 2 — addresses codex review #1 findings)

Goal: Take SHIP-approved branches end-to-end on this Mac (M4 Max arm64) from empty, no destructive mutation without explicit user approval, sandbox-first.

## Critical clarifications (review-1 fix)
- The PARENT `neuro-quant/Makefile` has the full local installer flow (init→submodules→deps→venvs→pkg→pinelsp→models→pre-proof→mcp→hooks→proof). It is NOT pkg/Makefile.
- pkg/Makefile's `make all` includes cloud-modal / cloud-lambda / cloud-ray which **actually deploy** when AWS / Modal / Ray creds are present in env. NEVER call pkg/Makefile `all` without first auditing.
- "Non-destructive" is conditional: install-mirror.sh purges serena-hooks references when binary absent, install_mcp_servers.sh deep-merge replaces existing neuro-link-* entries. Backups are a recovery path, NOT the primary safety mechanism.

## Phase 0 — inventory + backups + USER STATE DIFF

0.1 Snapshot user-global state (preserve, don't delete):
- `cp ~/.claude.json ~/.claude.json.bak.<ts>`
- `cp -r ~/.claude/hooks ~/.claude/hooks.bak.<ts>` (or tar; preserve permissions)
- `cp ~/.claude/settings.json ~/.claude/settings.json.bak.<ts>`

0.2 Inventory existing state:
- branch+dirty status of `/Users/DanBot/hyperfrequency/neuro-quant`
- ~/.local/bin symlink graph (serena, serena-mcp, pine-lsp, pine-lsp-rust)
- pre-existing models in repo `models/` and `~/.cache/qmd/models/`
- /Users/DanBot/.claude/state/nlr_root content

0.3 **PRE-MUTATION DIFF** (gate-before-installer):
- Read current `~/.claude.json` mcpServers
- For each of {neuro-link-recursive, neuro-link-http, serena}:
  - If absent → safe to install canonical entry
  - If present and matches canonical shape → no-op
  - If present and DIVERGES (custom transport, args, env, headers) → STOP and present to user. Don't auto-merge. User must explicitly opt in to overwrite.
- Read current `~/.claude/settings.json` hooks
  - If contains references to `serena-hooks` AND `~/.local/bin/serena-hooks` is missing → present to user before purge

0.4 Prereq gate: arm64 host, arm64 brew, rustc, python3.12 arm64 via uv, uv, docker, buildx, gh auth, node22+, pnpm, huggingface-cli, HF_TOKEN. Stop if missing.

0.5 Cloud creds audit (review-1 H1): print which of {AWS_ACCESS_KEY_ID, AWS_PROFILE, MODAL_TOKEN_ID, MODAL_TOKEN_SECRET, RAY_TOKEN, RAY_PROJECT} are set. **None of these will be cleared**, but they make pkg/Makefile cloud targets active. Plan never invokes pkg/Makefile `all`/`cloud*` directly.

## Phase 1 — sandbox install (truly non-mutating)

1.1 Fresh checkout in `/tmp/install-sandbox-<ts>/`:
```
gh repo clone HyperFrequency/neuro-quant -- -b test/monorepo-install-docs-parent
cd neuro-quant && git submodule update --init --recursive
```

1.2 **Targeted dry exercise** (NOT `make all`, NOT pkg/Makefile cloud targets):
- `make init`             — verify host prereqs only
- `make submodules`       — populate submodules
- `make deps`             — `arch -arm64 brew install` system-deps.txt; idempotent
- `make venvs`            — toolbox/.venvs/* bootstrap; uv-managed
- `make -C neuro-link -f pkg/Makefile local` — explicit local-only (mac+linux+docker proof aggregation), excludes cloud
- `make pinelsp`          — pnpm + cargo build, symlinks land in `~/.local/bin` (this DOES mutate `~/.local/bin` symlinks). If user wants pure dry: `SKIP_RUST=1 SKIP_FOLKNOR=1` short-circuits.
- `make models`           — idempotent (skips already-downloaded files)
- `make pre-proof`        — verify.py with mcp/hooks/serena skipped; no global mutation; emits INSTALL_COMPLETE.incomplete.json

1.3 **NEVER** run sandbox `mcp` or `hooks` targets — those mutate user-global state. Validate them via DRY-RUN preview INSTEAD (read mcp-servers.yaml and settings.template.json to compute proposed change).

1.4 Sandbox validation checklist:
- 10 `toolbox/.venvs/*/bin/python` are arm64 cpython-3.12
- `server/target/aarch64-apple-darwin/release/neuro-link` is Mach-O arm64
- `models/Octen-Embedding-8B.f16.gguf` (~15 GB), correct sha256, manifest env_file present
- `models/qwen3-reranker-0.6b-q8_0.gguf` + `qmd-query-expansion-1.7B-q4_k_m.gguf`
- pinelsp dist artifacts (or skipped per env)
- `pkg/.proof/INSTALL_COMPLETE.incomplete.json` written, state=incomplete (skips intentional)
- `pkg/.proof/ALL.ready.json` aggregates per-platform builder proofs

1.5 If any sandbox check fails → STOP, report.

## Phase 2 — pre-flight on real directories

2.1 `cd /Users/DanBot/hyperfrequency/neuro-quant`. If `git status --porcelain` non-empty → ASK USER, no auto-reset.

2.2 `git fetch origin && git checkout test/monorepo-install-docs-parent && git submodule update --init --recursive`

2.3 Verify submodule pointer = `833f1ad`.

2.4 Re-run Phase 0.3 PRE-MUTATION DIFF against current `~/.claude.json`. Same gating logic. If user has divergent entries from step 1.4 sandbox preview → ASK USER, don't auto-merge.

## Phase 3 — real install (with explicit gates)

3.1 Run installer phase by phase, NOT `make all` in one shot:
- `make init && make submodules && make deps && make venvs`  — repo-local
- `make pkg`                                                   — submodule build
- `make pinelsp`                                               — pinelsp + symlink (preserves existing)
- `make models`                                                — idempotent
- `make pre-proof`                                             — verify.py without mcp+hooks+serena. Required exit 0.
- **STOP HERE if pre-proof exit != 0.** No further mutation.
- `make mcp`                                                   — mutates `~/.claude.json`. install_mcp_servers.sh deep-merge.
- `make hooks`                                                 — mutates `~/.claude/hooks/` + `~/.claude/settings.json`. install-mirror.sh.
- `make proof`                                                 — final verify.py (offline, skipped heavy components)

3.2 If any step fails: install-mirror.sh has trap that restores `$SETTINGS.bak.<ts>` on failure. install_mcp_servers.sh wrote backup before mutation. Manual restore path documented.

3.3 Acceptance: `pkg/.proof/INSTALL_COMPLETE.<state>.json` MUST be either `ready` (no skips, ideal) or `incomplete` (intentional skips of post-rag-up checks). State=fail blocks acceptance regardless of exit code.

## Phase 4 — post-install live audit

4.1 `make rag-up`. Health-check qdrant :6333, llama-embed :8400 (Octen f16), llama-rerank :8401, llama-qexpand :8402.

4.2 Tombstone-on-blank live test (gate-43 III1):
- Create `vaults/_smoketest/blank-test.md` with content "verify-tombstone"
- Run `nlr_rag_embed` (recreate=false), confirm point lands in qdrant
- Truncate the file to whitespace
- Re-run `nlr_rag_embed`
- `tail` the server log → expect "tombstoning stale point for vaults/_smoketest/blank-test.md (id=...)"
- Vector search for "verify-tombstone" → 0 results

4.3 `make proof-full` for the comprehensive ship gate. **MUST** produce `INSTALL_COMPLETE.ready.json` with `state="ready"`. Anything else (incomplete/fail) is a blocking failure.

4.4 Validate user-global state diff (post-install vs pre-install backup):
- `diff <(jq -S . ~/.claude.json.bak.<ts>) <(jq -S . ~/.claude.json) | head -50`
- Must show ADDS only: 3 mcpServers entries (or fewer, if user already had some)
- Must NOT show REMOVALS of any user-supplied keys

## Phase 5 — close

5.1 Memory snapshot of install state with diffs.
5.2 Confirm backups intact: `~/.claude.json.bak.*`, `~/.claude/hooks.bak.*`, `~/.claude/settings.json.bak.*`.
5.3 Optional PR open (after user confirmation only).

## Risk register (review-1 hardening)

| ID | Risk | Mitigation |
|---|---|---|
| R1 | mcp+hooks mutate user-global state | Phase 0.1 backup; Phase 0.3+2.4 PRE-MUTATION DIFF gate; gate-9 AAAA2 ordering; .NOTPARALLEL hard-enforces |
| R2 | ~20 GB model download | Phase 0.2 inventory pre-existing; gate-G3 idempotent skip-if-present |
| R3 | pinelsp needs node22+pnpm | Phase 0.4 prereq gate; SKIP_RUST/SKIP_FOLKNOR escapes |
| R4 | real /neuro-quant has uncommitted work | Phase 2.1 ASK USER, no auto-reset |
| R5 | existing serena MCP shape diverges | Gate-10 BB1 + Phase 0.3 DIFF gate: ASK USER before any merge that could overwrite |
| R6 | "Don't delete files" claim was overbroad | This revision: backups are recovery only. install-mirror.sh purge of serena-hooks AND install_mcp_servers.sh deep-merge of neuro-link-* are real mutations. Phase 0.3+2.4 DIFF gate is the actual safety mechanism. |
| R7 (review-1 H1) | pkg/Makefile `all` includes cloud-modal/lambda/ray | Plan never calls pkg/Makefile `all` or `cloud*`. Phase 1.2 explicit `make -C neuro-link -f pkg/Makefile local` only. |
| R8 (review-1 M4) | exit 0 with state=incomplete masquerades as success | Phase 3.3 + Phase 4.3 acceptance keyed on STATE field, not exit code |

## Success criteria (review-1 hardened)

✅ Sandbox dry exercise green (no global mutation occurred)
✅ Pre-mutation DIFF gate showed no divergent user entries (or user explicitly approved overwrite)
✅ Real `make pre-proof` exits 0 BEFORE mcp/hooks run
✅ Real `make mcp` + `make hooks` + `make proof` complete; `INSTALL_COMPLETE.<state>.json` is ready or incomplete (not fail)
✅ `make proof-full` after rag-up produces `INSTALL_COMPLETE.ready.json` with state="ready" — this is the ONLY acceptable terminal state
✅ Tombstone-on-blank live test confirmed
✅ Diff of `~/.claude.json` against pre-install backup shows ADDS only, no REMOVALS of user-supplied keys
