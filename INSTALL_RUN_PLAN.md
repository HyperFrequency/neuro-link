# Comprehensive install plan — neuro-quant + neuro-link from empty
Generated: 2026-04-25
Goal: Take the SHIP-approved branches end-to-end on this Mac (M4 Max arm64) from empty, no file deletion, sandbox-first.

## Phase 0 — inventory + backups
- 0.1 Snapshot user-global state: cp ~/.claude.json ~/.claude.json.bak.<ts>; tar -czf ~/claude-hooks.bak.<ts>.tar.gz ~/.claude/hooks/; cp ~/.claude/settings.json ~/.claude/settings.json.bak.<ts>
- 0.2 Inventory existing state: branch+dirty status of /Users/DanBot/hyperfrequency/neuro-quant; ~/.local/bin symlink graph (serena, serena-mcp, pine-lsp, pine-lsp-rust); pre-existing models in <repo>/models and ~/.cache/qmd/models; /Users/DanBot/.claude/state/nlr_root
- 0.3 Prereq gate: arm64 host, arm64 brew, rustc, python3.12 arm64 via uv, uv, docker, buildx, gh auth, node22+, pnpm, huggingface-cli, HF_TOKEN

## Phase 1 — sandbox install (full dry-run)
- 1.1 mkdir /tmp/install-sandbox-<ts>; gh repo clone HyperFrequency/neuro-quant -- -b test/monorepo-install-docs-parent; git submodule update --init --recursive
- 1.2 NLR_MIRROR_DRY_RUN=1 NLR_VERIFY_OFFLINE=1 make all 2>&1 | tee /tmp/sandbox-<ts>.log
- 1.3 Validate: 10 toolbox venvs arm64 cpython-3.12; server/target/aarch64-apple-darwin/release/neuro-link is arm64 Mach-O; models/Octen-Embedding-8B.f16.gguf (~15 GB) + qwen3-reranker-0.6b-q8_0 + qmd-query-expansion-1.7B-q4_k_m + .octen-manifest.env; pinelsp dist artifacts; pkg/.proof/INSTALL_COMPLETE.<state>.json exists; ALL.ready.json aggregates; DRY-mode mcp/hooks preview correct
- 1.4 Tombstone-on-blank smoke test deferred to phase 4 (needs live qdrant)
- 1.5 Stop conditions: any sandbox failure halts before phase 2

## Phase 2 — pre-flight on real directories
- 2.1 cd /Users/DanBot/hyperfrequency/neuro-quant; git status --porcelain → if dirty, ASK USER, no auto-reset
- 2.2 git fetch origin; git checkout test/monorepo-install-docs-parent; git submodule update --init --recursive
- 2.3 Verify submodule pointer = 833f1ad

## Phase 3 — real install
- 3.1 cd /Users/DanBot/hyperfrequency/neuro-quant; make all 2>&1 | tee /tmp/install-real-<ts>.log
- 3.2 pre-proof fails-closed before any global mutation if build broken; install_mcp_servers.sh uses host-arch-aware binary probe; install-mirror.sh non-destructive on existing serena
- 3.3 proof at end runs verify.py offline=1 + skip set qmd,multilspy,pyright,vaults

## Phase 4 — post-install live audit
- 4.1 make rag-up; health-check qdrant :6333, llama-embed :8400, llama-rerank :8401, llama-qexpand :8402
- 4.2 Tombstone-on-blank live test: write vaults/_smoketest/blank-test.md; embed; truncate to whitespace; re-embed; verify DELETE on deterministic point ID; vector search for original term returns 0 results
- 4.3 make proof-full → expect state=ready, INSTALL_COMPLETE.ready.json
- 4.4 Validate ~/.claude.json has 3 required MCP entries (serena, neuro-link-recursive, neuro-link-http); existing serena preserved per gate-10 BB1; ~/.claude/hooks/ has 10 mirrored scripts; backups intact

## Phase 5 — close
- 5.1 Memory snapshot
- 5.2 Optional PR open (after user confirmation)

## Risks
- R1: mcp+hooks mutate ~/.claude.json → backup + gate-9 AAAA2 ordering + .NOTPARALLEL
- R2: ~20 GB download → phase 0.2 inventory + gate-G3 idempotent
- R3: pinelsp needs node22+pnpm → phase 0.3 prereq + SKIP_RUST/SKIP_FOLKNOR escape
- R4: real /neuro-quant has uncommitted work → phase 2.1 ASK USER
- R5: existing serena MCP shape diverges from canonical → gate-10 BB1 preserves
- R6: don't delete files → every op is append/backup

## Success criteria
- sandbox dry install green
- real make all reaches proof with zero hard fails
- ~/.claude.json + hooks mirrored, existing entries preserved
- proof-full after rag-up state=ready
- backups exist; no user data deleted
- tombstone-on-blank smoke test confirms gate-43 behavior
